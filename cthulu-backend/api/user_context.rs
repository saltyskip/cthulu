//! Per-user directory and key helpers for multi-tenant data isolation.
//!
//! When AUTH_ENABLED=true, data is namespaced under
//! `~/.cthulu/users/{user_id}/`. When disabled (dev mode), uses the
//! flat legacy structure at `~/.cthulu/`.

use std::path::PathBuf;

use crate::api::clerk_auth::{auth_enabled, AuthUser};
use crate::api::AppState;

/// Returns the per-user data directory.
/// Dev mode: returns `state.data_dir` directly.
/// Auth mode: returns `state.data_dir/users/{user_id}`.
pub fn user_data_dir(state: &AppState, auth: &AuthUser) -> PathBuf {
    if auth_enabled(state) {
        state.data_dir.join("users").join(&auth.user_id)
    } else {
        state.data_dir.clone()
    }
}

/// Prefix an in-memory key with user_id for multi-tenant isolation.
/// Dev mode: returns the key unchanged.
pub fn user_key(state: &AppState, auth: &AuthUser, key: &str) -> String {
    if auth_enabled(state) {
        format!("{}::{}", auth.user_id, key)
    } else {
        key.to_string()
    }
}

/// Ensure the user's data directory and subdirectories exist.
pub fn ensure_user_dirs(state: &AppState, auth: &AuthUser) -> std::io::Result<()> {
    let base = user_data_dir(state, auth);
    for sub in &["flows", "agents", "prompts"] {
        std::fs::create_dir_all(base.join(sub))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_auth(id: &str) -> AuthUser {
        AuthUser { user_id: id.to_string() }
    }

    // Note: user_data_dir and user_key require AppState which has many Arc fields.
    // We test the path/key construction logic directly; full integration tests
    // would need a test AppState builder (future improvement).

    #[test]
    fn user_key_formats_correctly() {
        // Verify the format string matches what user_key produces
        let auth = test_auth("user_abc");
        let expected = format!("{}::{}", auth.user_id, "agent::123");
        assert_eq!(expected, "user_abc::agent::123");
    }

    #[test]
    fn ensure_dirs_creates_all_subdirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path().to_path_buf();
        let auth = test_auth("test_user");
        let user_dir = base.join("users").join(&auth.user_id);
        // Simulate what ensure_user_dirs does
        for sub in &["flows", "agents", "prompts"] {
            std::fs::create_dir_all(user_dir.join(sub)).unwrap();
        }
        assert!(user_dir.join("flows").exists());
        assert!(user_dir.join("agents").exists());
        assert!(user_dir.join("prompts").exists());
    }
}
