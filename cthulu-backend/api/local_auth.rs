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
    /// Personal Anthropic API key. When set, personal agents use this key
    /// instead of the server-level ANTHROPIC_API_KEY env var.
    /// Stored as-is (not hashed) because we need the plaintext to call the API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_api_key: Option<String>,
    /// Personal Claude OAuth token (sk-ant-oat01-*). Used for Claude Code CLI sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_token: Option<String>,
    /// User's dedicated VM ID from VM Manager.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vm_id: Option<u32>,
    /// SSH port for the user's VM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_port: Option<u16>,
    /// Per-user environment variables (SLACK_WEBHOOK_URL, etc).
    /// Available on the VM (via .bashrc) and on the Cthulu backend (for sinks).
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub env_vars: HashMap<String, String>,
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

/// Load JWT signing secret. Priority:
/// 1. JWT_SECRET env var (for cloud — shared across replicas via Vault)
/// 2. File at {data_dir}/jwt_secret (for local dev)
/// 3. Generate new random secret (first run)
pub fn load_or_create_jwt_secret(data_dir: &PathBuf) -> String {
    // Cloud: use env var so all pods share the same secret
    if let Ok(secret) = std::env::var("JWT_SECRET") {
        if !secret.trim().is_empty() {
            tracing::info!("JWT secret loaded from env");
            return secret.trim().to_string();
        }
    }

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
    /// Hardcoded dev identity — used when auth is disabled.
    /// Note: this bypasses the user_id charset validation in FromRequestParts.
    /// If this ever accepts dynamic input, add the same [a-zA-Z0-9_-] check.
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
    use axum::routing::{get, put};
    Router::new()
        .route("/auth/signup", post(signup))
        .route("/auth/login", post(login))
        .route("/auth/vm-status", get(vm_status))
        .route("/auth/provision-vm", get(provision_vm_sse))
        .route("/auth/vm-env", post(set_vm_env))
        .route("/auth/google", post(google_oauth_callback))
        .route("/auth/me", get(get_profile).put(update_profile))
        .route("/auth/users/search", get(search_users))
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
            Json(json!({ "error": "Password too short" })),
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
        anthropic_api_key: None,
        oauth_token: None,
        vm_id: None,
        ssh_port: None,
        env_vars: HashMap::new(),
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
    state.save_user_store(&store);
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
    let user = store.find_by_email(&email).cloned();
    drop(store);

    // Constant-time: always run bcrypt verify to prevent user enumeration via timing.
    // If user doesn't exist, verify against a dummy hash (same cost as real verify).
    let dummy_hash = "$2b$12$000000000000000000000u2a5OJr0FkDxcMkGCuaLxMPqOIZJcMK";
    let password = body.password.clone();
    let hash = user.as_ref().map(|u| u.password_hash.clone())
        .unwrap_or_else(|| dummy_hash.to_string());
    let valid = tokio::task::spawn_blocking(move || bcrypt::verify(&password, &hash).unwrap_or(false))
        .await
        .unwrap_or(false);

    if !valid || user.is_none() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid email or password" })),
        );
    }
    let user = user.unwrap();

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

// ── VM Provisioning ──────────────────────────────────────────

#[derive(Deserialize)]
struct ProvisionVmRequest {
    /// Claude OAuth token to inject into the VM.
    oauth_token: String,
}

/// GET /api/auth/vm-status — check if user's VM is running.
/// Returns: { has_vm, vm_id, running, terminal_url } or { has_vm: false }
/// If VM exists in profile but is dead on VM Manager, clears vm_id from profile.
async fn vm_status(
    auth: AuthUser,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<Value> {
    let (vm_id, ssh_port) = {
        let store = state.user_store.read().await;
        let user = store.users.values().find(|u| u.id == auth.user_id);
        match user {
            Some(u) => (u.vm_id, u.ssh_port),
            None => return Json(json!({ "has_vm": false })),
        }
    };

    let Some(vm_id) = vm_id else {
        return Json(json!({ "has_vm": false }));
    };

    // Check if VM is actually running
    let vm_client = crate::vm_manager::VmManagerClient::new((*state.http_client).clone());
    match vm_client.get_vm(vm_id).await {
        Ok(Some(vm)) => {
            let terminal_url = vm.web_terminal.unwrap_or_else(|| {
                let host = vm_client.ssh_host();
                format!("http://{}:{}", host, vm.web_port)
            });
            Json(json!({
                "has_vm": true,
                "vm_id": vm.vm_id,
                "ssh_port": vm.ssh_port,
                "running": true,
                "terminal_url": terminal_url,
            }))
        }
        Ok(None) => {
            // VM was deleted externally — clear from profile
            tracing::warn!(vm_id, user_id = %auth.user_id, "VM not found, clearing from profile");
            {
                let mut store = state.user_store.write().await;
                if let Some(user) = store.users.values_mut().find(|u| u.id == auth.user_id) {
                    user.vm_id = None;
                    user.ssh_port = None;
                }
                state.save_user_store(&store);
            }
            Json(json!({ "has_vm": false, "was_deleted": true }))
        }
        Err(e) => {
            // Can't reach VM Manager — report unknown
            tracing::warn!(vm_id, error = %e, "VM Manager unreachable during status check");
            Json(json!({
                "has_vm": true,
                "vm_id": vm_id,
                "ssh_port": ssh_port,
                "running": false,
                "error": "VM Manager unreachable",
            }))
        }
    }
}

/// POST /api/auth/vm-env — set environment variables on the user's VM.
/// Writes to /root/.env on the VM (sourced by claude via .bashrc).
#[derive(Deserialize)]
struct SetVmEnvRequest {
    /// Key-value pairs to set on the VM.
    env: std::collections::HashMap<String, String>,
}

async fn set_vm_env(
    auth: AuthUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<SetVmEnvRequest>,
) -> (StatusCode, Json<Value>) {
    // Validate keys
    for key in body.env.keys() {
        if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Invalid env var name: {key}") })));
        }
    }

    // 1. Save to user profile in MongoDB (available to backend sinks)
    {
        let mut store = state.user_store.write().await;
        if let Some(user) = store.users.values_mut().find(|u| u.id == auth.user_id) {
            for (key, value) in &body.env {
                user.env_vars.insert(key.clone(), value.clone());
            }
        }
        state.save_user_store(&store);
    }

    // 2. Also write to VM via SSH (available to claude CLI on the VM)
    let ssh_port = {
        let store = state.user_store.read().await;
        store.users.values()
            .find(|u| u.id == auth.user_id)
            .and_then(|u| u.ssh_port)
    };

    if let Some(ssh_port) = ssh_port {
        let vm_client = crate::vm_manager::VmManagerClient::new((*state.http_client).clone());
        let mut env_lines = Vec::new();
        for (key, value) in &body.env {
            env_lines.push(format!("export {}='{}'", key, value.replace('\'', "'\\''")));
        }
        let cmd = format!(
            "cat >> /root/.bashrc << 'CTHULU_ENV'\n# Cthulu env vars\n{}\nCTHULU_ENV",
            env_lines.join("\n")
        );
        if let Ok(mut child) = vm_client.ssh_stream(ssh_port, &cmd).await {
            let _ = child.wait().await;
        }
    }

    let keys: Vec<&String> = body.env.keys().collect();
    tracing::info!(user_id = %auth.user_id, keys = ?keys, "env vars saved to profile + VM");
    (StatusCode::OK, Json(json!({ "ok": true, "keys": keys })))
}

/// POST /api/auth/provision-vm — create a dedicated VM for the user.
/// Called during onboarding after the user sets their OAuth token.
/// Stores the VM ID and SSH port in the user profile.
async fn provision_vm(
    auth: AuthUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<ProvisionVmRequest>,
) -> (StatusCode, Json<Value>) {
    // Check if user already has a VM
    {
        let store = state.user_store.read().await;
        if let Some(user) = store.users.values().find(|u| u.id == auth.user_id) {
            if user.vm_id.is_some() {
                return (StatusCode::CONFLICT, Json(json!({ "error": "VM already provisioned", "vm_id": user.vm_id })));
            }
        }
    }

    // Check total VM count (max 5)
    let vm_client = crate::vm_manager::VmManagerClient::new((*state.http_client).clone());
    match vm_client.list_vms().await {
        Ok(vms) if vms.len() >= 5 => {
            return (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "error": "Maximum 5 VMs reached. Contact admin." })));
        }
        Err(e) => {
            return (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("VM Manager unreachable: {e}") })));
        }
        _ => {}
    }

    // Create VM
    let vm = match vm_client.create_vm(&body.oauth_token, "nano").await {
        Ok(vm) => vm,
        Err(e) => {
            tracing::error!(error = %e, "VM creation failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("VM creation failed: {e}") })));
        }
    };

    tracing::info!(vm_id = vm.vm_id, ssh_port = ?vm.ssh_port, user_id = %auth.user_id, "VM provisioned for user");

    // Store VM info + OAuth token in user profile
    {
        let mut store = state.user_store.write().await;
        if let Some(user) = store.users.values_mut().find(|u| u.id == auth.user_id) {
            user.vm_id = Some(vm.vm_id);
            user.ssh_port = Some(vm.ssh_port);
            user.oauth_token = Some(body.oauth_token);
        }
        state.save_user_store(&store);
    }

    (StatusCode::CREATED, Json(json!({
        "vm_id": vm.vm_id,
        "ssh_port": vm.ssh_port,
        "status": "created",
    })))
}

/// GET /api/auth/provision-vm — SSE stream that creates a VM and sends progress events.
async fn provision_vm_sse(
    auth: AuthUser,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::response::sse::Sse<impl futures::stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    use axum::response::sse::Event;

    let stream = async_stream::stream! {
        // Step 1: Check if already has VM
        {
            let store = state.user_store.read().await;
            if let Some(user) = store.users.values().find(|u| u.id == auth.user_id) {
                if let Some(vm_id) = user.vm_id {
                    yield Ok(Event::default().event("done").data(
                        serde_json::to_string(&json!({"vm_id": vm_id, "ssh_port": user.ssh_port, "status": "already_exists"})).unwrap()
                    ));
                    return;
                }
            }
        }

        yield Ok(Event::default().event("progress").data(
            serde_json::to_string(&json!({"step": "checking", "message": "Checking VM availability..."})).unwrap()
        ));

        // Step 2: Check VM count
        let vm_client = crate::vm_manager::VmManagerClient::new((*state.http_client).clone());
        match vm_client.list_vms().await {
            Ok(vms) if vms.len() >= 5 => {
                yield Ok(Event::default().event("error").data(
                    serde_json::to_string(&json!({"message": "Maximum 5 VMs reached. Contact admin."})).unwrap()
                ));
                return;
            }
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    serde_json::to_string(&json!({"message": format!("VM Manager unreachable: {e}")})).unwrap()
                ));
                return;
            }
            _ => {}
        }

        yield Ok(Event::default().event("progress").data(
            serde_json::to_string(&json!({"step": "creating", "message": "Creating your VM..."})).unwrap()
        ));

        // Step 3: Create VM
        let vm = match vm_client.create_vm("pending", "nano").await {
            Ok(vm) => vm,
            Err(e) => {
                yield Ok(Event::default().event("error").data(
                    serde_json::to_string(&json!({"message": format!("VM creation failed: {e}")})).unwrap()
                ));
                return;
            }
        };

        tracing::info!(vm_id = vm.vm_id, ssh_port = ?vm.ssh_port, user_id = %auth.user_id, "VM provisioned");

        yield Ok(Event::default().event("progress").data(
            serde_json::to_string(&json!({"step": "saving", "message": format!("VM #{} created, saving...", vm.vm_id)})).unwrap()
        ));

        // Step 4: Store in user profile
        {
            let mut store = state.user_store.write().await;
            if let Some(user) = store.users.values_mut().find(|u| u.id == auth.user_id) {
                user.vm_id = Some(vm.vm_id);
                user.ssh_port = Some(vm.ssh_port);
            }
            state.save_user_store(&store);
        }

        // Step 5: Copy prompts to the VM
        yield Ok(Event::default().event("progress").data(
            serde_json::to_string(&json!({"step": "prompts", "message": "Copying workflow prompts to VM..."})).unwrap()
        ));

        let host = vm_client.ssh_host();
        let prompts_dir = std::path::Path::new("/app/prompts");
        if prompts_dir.exists() {
            // Create prompts directory on VM
            if let Ok(mut child) = vm_client.ssh_stream(vm.ssh_port, "mkdir -p /root/prompts").await {
                let _ = child.wait().await;
            }

            // SCP each prompt file
            if let Ok(entries) = std::fs::read_dir(prompts_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map_or(false, |e| e == "md") {
                        let filename = path.file_name().unwrap_or_default().to_string_lossy();
                        let scp_result = tokio::process::Command::new("scp")
                            .args([
                                "-o", "StrictHostKeyChecking=no",
                                "-o", "UserKnownHostsFile=/dev/null",
                                "-P", &vm.ssh_port.to_string(),
                                &path.to_string_lossy(),
                                &format!("root@{host}:/root/prompts/{filename}"),
                            ])
                            .output()
                            .await;

                        match scp_result {
                            Ok(out) if out.status.success() => {
                                tracing::info!(file = %filename, "copied prompt to VM");
                            }
                            Ok(out) => {
                                tracing::warn!(file = %filename, stderr = %String::from_utf8_lossy(&out.stderr), "scp failed");
                            }
                            Err(e) => {
                                tracing::warn!(file = %filename, error = %e, "scp error");
                            }
                        }
                    }
                }
            }
        }

        // Use terminal URL from VM Manager response, or build one
        let terminal_url = vm.web_terminal.unwrap_or_else(|| {
            format!("http://{}:{}", host, vm.web_port)
        });

        // Build prompt list for the user
        let prompt_files: Vec<String> = std::fs::read_dir("/app/prompts")
            .ok()
            .map(|entries| {
                entries.flatten()
                    .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();

        yield Ok(Event::default().event("done").data(
            serde_json::to_string(&json!({
                "vm_id": vm.vm_id,
                "ssh_port": vm.ssh_port,
                "status": "created",
                "terminal_url": terminal_url,
                "prompts_copied": prompt_files,
            })).unwrap()
        ));
    };

    axum::response::sse::Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(std::time::Duration::from_secs(5)))
}

// ── Google OAuth ─────────────────────────────────────────────

#[derive(Deserialize)]
struct GoogleOAuthRequest {
    /// Authorization code from Google's OAuth consent flow.
    code: String,
    /// The redirect_uri used in the frontend (must match exactly).
    redirect_uri: String,
}

/// POST /api/auth/google — exchange Google OAuth code for Cthulu JWT.
///
/// Env vars required:
///   GOOGLE_CLIENT_ID     — OAuth client ID from Google Cloud Console
///   GOOGLE_CLIENT_SECRET — OAuth client secret
async fn google_oauth_callback(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<GoogleOAuthRequest>,
) -> (StatusCode, Json<Value>) {
    let client_id = match std::env::var("GOOGLE_CLIENT_ID") {
        Ok(v) if !v.is_empty() => v,
        _ => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Google OAuth not configured (GOOGLE_CLIENT_ID missing)" }))),
    };
    let client_secret = match std::env::var("GOOGLE_CLIENT_SECRET") {
        Ok(v) if !v.is_empty() => v,
        _ => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Google OAuth not configured (GOOGLE_CLIENT_SECRET missing)" }))),
    };

    // 1. Exchange authorization code for tokens
    let token_resp = match state.http_client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", body.code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", body.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Google token exchange failed: {e}") }))),
    };

    let token_data: Value = match token_resp.json().await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Invalid Google token response: {e}") }))),
    };

    let access_token = match token_data["access_token"].as_str() {
        Some(t) => t.to_string(),
        None => {
            let err = token_data["error_description"].as_str().unwrap_or("unknown error");
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Google OAuth error: {err}") })));
        }
    };

    // 2. Fetch user profile from Google
    let userinfo_resp = match state.http_client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .header("Authorization", format!("Bearer {access_token}"))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Google userinfo failed: {e}") }))),
    };

    let userinfo: Value = match userinfo_resp.json().await {
        Ok(v) => v,
        Err(e) => return (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Invalid Google userinfo: {e}") }))),
    };

    let email = match userinfo["email"].as_str() {
        Some(e) => e.to_lowercase(),
        None => return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Google account has no email" }))),
    };
    let name = userinfo["name"].as_str().map(String::from);
    let avatar = userinfo["picture"].as_str().map(String::from);

    // 3. Find or create user in our store
    let mut store = state.user_store.write().await;

    let user = if let Some(existing) = store.find_by_email(&email).cloned() {
        // Update name/avatar from Google if not set locally
        if let Some(u) = store.users.get_mut(&email) {
            if u.name.is_none() { u.name = name.clone(); }
            if u.avatar_url.is_none() { u.avatar_url = avatar.clone(); }
        }
        existing
    } else {
        // Create new user (no password — Google-only account)
        let new_user = StoredUser {
            id: uuid::Uuid::new_v4().to_string().replace('-', "_"),
            email: email.clone(),
            password_hash: "GOOGLE_OAUTH".to_string(), // marker: no password login
            name,
            avatar_url: avatar,
            anthropic_api_key: None,
            oauth_token: None,
            vm_id: None,
            ssh_port: None,
            env_vars: HashMap::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        store.insert(new_user.clone());
        new_user
    };

    state.save_user_store(&store);
    drop(store);

    // 4. Issue Cthulu JWT
    let token = match issue_jwt(&user, &state.jwt_secret) {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))),
    };

    (StatusCode::OK, Json(json!({
        "token": token,
        "user": { "id": user.id, "email": user.email, "name": user.name, "avatar_url": user.avatar_url }
    })))
}

// ── Profile + Search ─────────────────────────────────────────

/// GET /api/auth/me — get current user profile
async fn get_profile(
    auth: AuthUser,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.user_store.read().await;
    let user = store
        .users
        .values()
        .find(|u| u.id == auth.user_id)
        .ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" })))
        })?;

    Ok(Json(json!({
        "id": user.id,
        "email": user.email,
        "name": user.name,
        "avatar_url": user.avatar_url,
        "has_api_key": user.anthropic_api_key.is_some(),
        "has_oauth_token": user.oauth_token.is_some(),
        "vm_id": user.vm_id,
        "ssh_port": user.ssh_port,
    })))
}

#[derive(Deserialize)]
struct UpdateProfileRequest {
    name: Option<String>,
    avatar_url: Option<String>,
    /// Set personal Anthropic API key. Send empty string to clear.
    anthropic_api_key: Option<String>,
    /// Set personal Claude OAuth token (sk-ant-oat01-*). Send empty string to clear.
    oauth_token: Option<String>,
}

/// PUT /api/auth/me — update current user profile
async fn update_profile(
    auth: AuthUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(body): Json<UpdateProfileRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut store = state.user_store.write().await;
    let user = store
        .users
        .values_mut()
        .find(|u| u.id == auth.user_id)
        .ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" })))
        })?;

    if let Some(name) = body.name {
        user.name = Some(name);
    }
    if let Some(avatar_url) = body.avatar_url {
        user.avatar_url = if avatar_url.is_empty() { None } else { Some(avatar_url) };
    }
    if let Some(api_key) = body.anthropic_api_key {
        user.anthropic_api_key = if api_key.is_empty() { None } else { Some(api_key) };
    }
    if let Some(token) = body.oauth_token {
        user.oauth_token = if token.is_empty() { None } else { Some(token) };
    }

    let updated = json!({
        "id": user.id,
        "email": user.email,
        "name": user.name,
        "avatar_url": user.avatar_url,
        "has_api_key": user.anthropic_api_key.is_some(),
        "has_oauth_token": user.oauth_token.is_some(),
        "vm_id": user.vm_id,
        "ssh_port": user.ssh_port,
    });

    state.save_user_store(&store);

    Ok(Json(updated))
}

/// GET /api/auth/users/search?q=query — search users by name or email
async fn search_users(
    _auth: AuthUser,
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<Value> {
    let query = params.get("q").map(|s| s.to_lowercase()).unwrap_or_default();
    if query.is_empty() {
        return Json(json!({ "users": [] }));
    }

    let store = state.user_store.read().await;
    let results: Vec<Value> = store
        .users
        .values()
        .filter(|u| {
            u.email.to_lowercase().contains(&query)
                || u.name.as_deref().unwrap_or("").to_lowercase().contains(&query)
        })
        .take(10)
        .map(|u| {
            json!({
                "id": u.id,
                "email": u.email,
                "name": u.name,
            })
        })
        .collect();

    Json(json!({ "users": results }))
}

// ── API Key Resolution ──────────────────────────────────────

/// Resolve which Anthropic API key to use for an agent session.
///
/// Priority:
/// 1. If the agent has a `team_id` → use the server env `ANTHROPIC_API_KEY` (team-level key)
/// 2. If the user has a personal `anthropic_api_key` set → use that
/// 3. Fall back to server env `ANTHROPIC_API_KEY`
/// 4. Fall back to the existing OAuth token from Keychain
///
/// Resolved credentials for an agent session.
pub struct ResolvedCredentials {
    /// API key (sk-ant-api03-*) — set as ANTHROPIC_API_KEY
    pub api_key: Option<String>,
    /// OAuth token (sk-ant-oat01-*) — set as CLAUDE_CODE_OAUTH_TOKEN
    pub oauth_token: Option<String>,
}

/// Resolve which credentials to use for an agent session.
///
/// Priority:
/// 1. User's OAuth token (personal agents)
/// 2. User's API key (personal agents)
/// 3. Server env CLAUDE_CODE_OAUTH_TOKEN
/// 4. Server env ANTHROPIC_API_KEY
pub async fn resolve_credentials(
    state: &AppState,
    user_id: &str,
    agent_team_id: Option<&str>,
) -> ResolvedCredentials {
    // Team agent → use server-level credentials
    if agent_team_id.is_some() {
        return ResolvedCredentials {
            api_key: std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty()),
            oauth_token: std::env::var("CLAUDE_CODE_OAUTH_TOKEN").ok().filter(|k| !k.is_empty()),
        };
    }

    // Personal agent → try user's credentials
    {
        let store = state.user_store.read().await;
        if let Some(user) = store.users.values().find(|u| u.id == user_id) {
            let has_oauth = user.oauth_token.as_ref().map_or(false, |t| !t.is_empty());
            let has_key = user.anthropic_api_key.as_ref().map_or(false, |k| !k.is_empty());

            if has_oauth || has_key {
                return ResolvedCredentials {
                    api_key: user.anthropic_api_key.clone().filter(|k| !k.is_empty()),
                    oauth_token: user.oauth_token.clone().filter(|t| !t.is_empty()),
                };
            }
        }
    }

    // Fall back to server-level credentials
    ResolvedCredentials {
        api_key: std::env::var("ANTHROPIC_API_KEY").ok().filter(|k| !k.is_empty()),
        oauth_token: std::env::var("CLAUDE_CODE_OAUTH_TOKEN").ok().filter(|k| !k.is_empty()),
    }
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
            anthropic_api_key: None,
            oauth_token: None,
            vm_id: None,
            ssh_port: None,
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
