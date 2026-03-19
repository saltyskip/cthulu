//! Self-hosted authentication: signup, login, JWT issuance + verification.
//!
//! Users stored in `{data_dir}/users.json`. Passwords hashed with bcrypt.
//! JWTs signed with a server-generated secret stored in `{data_dir}/jwt_secret`.
//!
//! When `AUTH_ENABLED` env var is not "true" (default), auth is bypassed
//! and all requests get a hardcoded "dev_user" identity.

use axum::extract::FromRequestParts;
use axum::routing::post;
use axum::{Json, Router};
use hyper::header::AUTHORIZATION;
use hyper::http::request::Parts;
use hyper::StatusCode;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::api::AppState;

// ── User Store ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredUser {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub created_at: String,
}

/// In-memory user store backed by `{data_dir}/users.json`.
pub struct UserStore {
    pub users: HashMap<String, StoredUser>, // email -> StoredUser
}

impl UserStore {
    pub fn load(data_dir: &PathBuf) -> Self {
        let path = data_dir.join("users.json");
        let users = match std::fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => HashMap::new(),
        };
        Self { users }
    }

    pub fn save(&self, data_dir: &PathBuf) -> std::io::Result<()> {
        let path = data_dir.join("users.json");
        let json = serde_json::to_string_pretty(&self.users)?;
        // Atomic write: temp file + rename (project rule #9)
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, &path)
    }

    pub fn find_by_email(&self, email: &str) -> Option<&StoredUser> {
        self.users.get(email)
    }

    pub fn insert(&mut self, user: StoredUser) {
        self.users.insert(user.email.clone(), user);
    }
}

// ── JWT ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String, // user_id
    email: String,
    exp: usize,
}

const JWT_EXPIRY_HOURS: i64 = 24;

/// Load or generate the JWT signing secret.
/// Panics if the secret cannot be persisted to disk (server should not start
/// with an ephemeral secret that would invalidate all JWTs on restart).
pub fn load_or_create_jwt_secret(data_dir: &PathBuf) -> String {
    let path = data_dir.join("jwt_secret");
    match std::fs::read_to_string(&path) {
        Ok(secret) if !secret.trim().is_empty() => secret.trim().to_string(),
        _ => {
            let secret = uuid::Uuid::new_v4().to_string();
            std::fs::write(&path, &secret)
                .unwrap_or_else(|e| panic!("Failed to write JWT secret to {}: {e}", path.display()));
            secret
        }
    }
}

fn issue_jwt(user: &StoredUser, secret: &str) -> Result<String, String> {
    let exp = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::hours(JWT_EXPIRY_HOURS))
        .ok_or("time overflow")?
        .timestamp() as usize;

    let claims = Claims {
        sub: user.id.clone(),
        email: user.email.clone(),
        exp,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| format!("JWT encode failed: {e}"))
}

fn verify_jwt(token: &str, secret: &str) -> Result<Claims, String> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_aud = false;

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|e| format!("JWT verify failed: {e}"))
}

// ── AuthUser Extractor ───────────────────────────────────────

/// Authenticated user extracted from JWT.
/// When auth is disabled (dev mode), user_id is "dev_user".
#[derive(Debug, Clone, Serialize)]
pub struct AuthUser {
    pub user_id: String,
}

impl AuthUser {
    pub fn dev_user() -> Self {
        Self {
            user_id: "dev_user".to_string(),
        }
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = (StatusCode, Json<Value>);

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let jwt_secret = state.jwt_secret.clone();
        let auth_on = state.auth_enabled;
        let token = extract_token(parts);

        async move {
            if !auth_on {
                return Ok(AuthUser::dev_user());
            }

            let token = token.ok_or_else(|| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Missing authentication token" })),
                )
            })?;

            let claims = verify_jwt(&token, &jwt_secret).map_err(|e| {
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": e })),
                )
            })?;

            // Validate user_id is safe for filesystem paths
            if !claims.sub.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "Invalid user ID format" })),
                ));
            }

            Ok(AuthUser { user_id: claims.sub })
        }
    }
}

/// Extract Bearer token from Authorization header or ?token query param.
fn extract_token(parts: &Parts) -> Option<String> {
    if let Some(auth_header) = parts.headers.get(AUTHORIZATION) {
        if let Ok(value) = auth_header.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    if let Some(query) = parts.uri.query() {
        for pair in query.split('&') {
            if let Some(token) = pair.strip_prefix("token=") {
                return Some(token.to_string());
            }
        }
    }

    None
}

// ── HTTP Handlers ────────────────────────────────────────────

#[derive(Deserialize)]
struct SignupRequest {
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/login", post(login))
}

async fn signup(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<SignupRequest>,
) -> (StatusCode, Json<Value>) {
    if body.email.trim().is_empty() || body.password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Email required, password must be 8+ characters" })),
        );
    }

    let email = body.email.trim().to_lowercase();

    // Hash password outside the lock (CPU-intensive, ~250ms at cost 12)
    let password = body.password.clone();
    let password_hash = match tokio::task::spawn_blocking(move || bcrypt::hash(&password, 12)).await {
        Ok(Ok(h)) => h,
        Ok(Err(e)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Hash failed: {e}") })),
            );
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("Hash task failed: {e}") })),
            );
        }
    };

    // Hold write lock for entire check-then-insert to prevent TOCTOU race
    let mut store = state.user_store.write().await;
    if store.find_by_email(&email).is_some() {
        return (
            StatusCode::CONFLICT,
            Json(json!({ "error": "User already exists" })),
        );
    }

    let user = StoredUser {
        id: uuid::Uuid::new_v4().to_string().replace('-', "_"),
        email: email.clone(),
        password_hash,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    // Issue token
    let token = match issue_jwt(&user, &state.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e })),
            );
        }
    };

    // Save user (still holding write lock)
    store.insert(user.clone());
    if let Err(e) = store.save(&state.data_dir) {
        tracing::error!(error = %e, "failed to persist user store after signup");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to save user account" })),
        );
    }
    drop(store);

    (
        StatusCode::CREATED,
        Json(json!({
            "token": token,
            "user": { "id": user.id, "email": user.email }
        })),
    )
}

async fn login(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<LoginRequest>,
) -> (StatusCode, Json<Value>) {
    let email = body.email.trim().to_lowercase();

    let store = state.user_store.read().await;
    let user = match store.find_by_email(&email) {
        Some(u) => u.clone(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid email or password" })),
            );
        }
    };
    drop(store);

    // Verify password (CPU-intensive, run off async runtime)
    let password = body.password.clone();
    let hash = user.password_hash.clone();
    let valid = tokio::task::spawn_blocking(move || bcrypt::verify(&password, &hash).unwrap_or(false))
        .await
        .unwrap_or(false);
    if !valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid email or password" })),
        );
    }

    // Issue token
    let token = match issue_jwt(&user, &state.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e })),
            );
        }
    };

    (
        StatusCode::OK,
        Json(json!({
            "token": token,
            "user": { "id": user.id, "email": user.email }
        })),
    )
}

// ── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_user_has_expected_id() {
        let user = AuthUser::dev_user();
        assert_eq!(user.user_id, "dev_user");
    }

    #[test]
    fn extract_bearer_token() {
        let parts = hyper::Request::builder()
            .header("Authorization", "Bearer test_token_123")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let token = extract_token(&parts);
        assert_eq!(token, Some("test_token_123".to_string()));
    }

    #[test]
    fn extract_query_token() {
        let parts = hyper::Request::builder()
            .uri("https://example.com/api/test?token=abc123&other=val")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let token = extract_token(&parts);
        assert_eq!(token, Some("abc123".to_string()));
    }

    #[test]
    fn extract_no_token() {
        let parts = hyper::Request::builder()
            .uri("https://example.com/api/test")
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let token = extract_token(&parts);
        assert!(token.is_none());
    }

    #[test]
    fn jwt_roundtrip() {
        let secret = "test_secret_123";
        let user = StoredUser {
            id: "user_abc".into(),
            email: "test@example.com".into(),
            password_hash: "fake".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let token = issue_jwt(&user, secret).unwrap();
        let claims = verify_jwt(&token, secret).unwrap();
        assert_eq!(claims.sub, "user_abc");
        assert_eq!(claims.email, "test@example.com");
    }

    #[test]
    fn jwt_wrong_secret_fails() {
        let user = StoredUser {
            id: "user_abc".into(),
            email: "test@example.com".into(),
            password_hash: "fake".into(),
            created_at: "2026-01-01T00:00:00Z".into(),
        };
        let token = issue_jwt(&user, "secret1").unwrap();
        let result = verify_jwt(&token, "secret2");
        assert!(result.is_err());
    }

    #[test]
    fn user_store_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().to_path_buf();

        let mut store = UserStore { users: HashMap::new() };
        store.insert(StoredUser {
            id: "u1".into(),
            email: "a@b.com".into(),
            password_hash: "hash".into(),
            created_at: "2026-01-01".into(),
        });
        store.save(&dir).unwrap();

        let loaded = UserStore::load(&dir);
        assert!(loaded.find_by_email("a@b.com").is_some());
        assert!(loaded.find_by_email("nope@b.com").is_none());
    }

    #[test]
    fn bcrypt_hash_and_verify() {
        let password = "my_secure_password";
        let hash = bcrypt::hash(password, 4).unwrap(); // low cost for test speed
        assert!(bcrypt::verify(password, &hash).unwrap());
        assert!(!bcrypt::verify("wrong_password", &hash).unwrap());
    }
}
