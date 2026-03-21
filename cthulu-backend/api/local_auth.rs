//! Self-hosted authentication: signup, login, JWT issuance + verification.
//!
//! Users stored in `{data_dir}/users.json`. Passwords hashed with bcrypt.
//! JWTs signed with a server-generated secret stored in `{data_dir}/jwt_secret`.
//!
//! When `AUTH_ENABLED` is not "true" (default), auth is bypassed
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
use base64::Engine;
use percent_encoding::percent_decode_str;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::api::AppState;

// ── User Store ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredUser {
    pub id: String,
    pub email: String,
    /// WARNING: Contains the bcrypt hash. Never expose in API responses.
    /// This field is serialized for file persistence only. All API handlers
    /// MUST use manual `json!()` construction to exclude it. Never use
    /// `Json(user)` or `serde_json::to_value(user)` in HTTP responses.
    pub password_hash: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
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
            Ok(contents) => match serde_json::from_str(&contents) {
                Ok(parsed) => parsed,
                Err(e) => {
                    tracing::error!(path = %path.display(), error = %e,
                        "corrupt users.json — starting with empty store (backup the file before next save)");
                    HashMap::new()
                }
            },
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                tracing::error!(path = %path.display(), error = %e, "failed to read users.json");
                HashMap::new()
            }
            Err(_) => HashMap::new(), // file doesn't exist yet — normal on first run
        };
        Self { users }
    }

    /// Atomic save: write to temp file, then rename (crash-safe).
    pub fn save(&self, data_dir: &PathBuf) -> std::io::Result<()> {
        let path = data_dir.join("users.json");
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(&self.users)?;
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &path)
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

/// Load or generate the JWT signing secret (256-bit CSPRNG, base64url-encoded).
/// The secret file is created with mode 0600 on Unix.
/// If the file is deleted, a new secret is generated and all existing JWTs are invalidated.
pub fn load_or_create_jwt_secret(data_dir: &PathBuf) -> String {
    let path = data_dir.join("jwt_secret");
    match std::fs::read_to_string(&path) {
        Ok(secret) if !secret.trim().is_empty() => secret.trim().to_string(),
        _ => {
            let mut buf = [0u8; 32];
            getrandom::fill(&mut buf).expect("CSPRNG failed");
            let secret = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&buf);
            if let Err(e) = std::fs::write(&path, &secret) {
                tracing::error!(path = %path.display(), error = %e,
                    "failed to persist JWT secret — tokens will not survive restart");
            } else {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
                }
            }
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

/// Check if auth is enabled via AppState (read once at startup).
pub fn auth_enabled(state: &AppState) -> bool {
    state.auth_enabled
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
/// Query-param support exists for SSE/WebSocket connections where headers can't be set.
fn extract_token(parts: &Parts) -> Option<String> {
    if let Some(auth_header) = parts.headers.get(AUTHORIZATION) {
        if let Ok(value) = auth_header.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    // Fallback: ?token= query param (URL-decoded for safety)
    if let Some(query) = parts.uri.query() {
        for pair in query.split('&') {
            if let Some(token) = pair.strip_prefix("token=") {
                return Some(percent_decode_str(token).decode_utf8_lossy().into_owned());
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
    let email = body.email.trim().to_lowercase();
    let email_parts: Vec<&str> = email.split('@').collect();
    if email_parts.len() != 2 || email_parts[0].is_empty() || email_parts[1].is_empty() || !email_parts[1].contains('.') {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Valid email required" })),
        );
    }
    if body.password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Password must be at least 8 bytes" })),
        );
    }
    let password = body.password;

    // Hash password outside lock (bcrypt is CPU-intensive)
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

    // Single write lock for check-and-insert to prevent TOCTOU race
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
        name: None,
        avatar_url: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let token = match issue_jwt(&user, &state.jwt_secret) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e })),
            );
        }
    };

    store.insert(user.clone());
    if let Err(e) = store.save(&state.data_dir) {
        tracing::error!(error = %e, "failed to persist user store");
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

    fn test_user(id: &str, email: &str) -> StoredUser {
        StoredUser {
            id: id.into(),
            email: email.into(),
            password_hash: "fake".into(),
            name: None,
            avatar_url: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn jwt_roundtrip() {
        let secret = "test_secret_123";
        let user = test_user("user_abc", "test@example.com");
        let token = issue_jwt(&user, secret).unwrap();
        let claims = verify_jwt(&token, secret).unwrap();
        assert_eq!(claims.sub, "user_abc");
        assert_eq!(claims.email, "test@example.com");
    }

    #[test]
    fn jwt_wrong_secret_fails() {
        let user = test_user("user_abc", "test@example.com");
        let token = issue_jwt(&user, "secret1").unwrap();
        let result = verify_jwt(&token, "secret2");
        assert!(result.is_err());
    }

    #[test]
    fn user_store_roundtrip() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().to_path_buf();

        let mut store = UserStore { users: HashMap::new() };
        store.insert(test_user("u1", "a@b.com"));
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
