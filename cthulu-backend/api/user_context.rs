//! Per-user directory and key helpers for multi-tenant data isolation.
//!
//! When AUTH_ENABLED=true, data is namespaced under
//! `~/.cthulu/users/{user_id}/`. When disabled (dev mode), uses the
//! flat legacy structure at `~/.cthulu/`.

use std::path::PathBuf;

use crate::api::clerk_auth::AuthUser;
use crate::api::AppState;

/// Returns the per-user data directory.
/// Dev mode: returns `state.data_dir` directly.
/// Auth mode: returns `state.data_dir/users/{user_id}`.
pub fn user_data_dir(state: &AppState, auth: &AuthUser) -> PathBuf {
    if state.auth_enabled {
        state.data_dir.join("users").join(&auth.user_id)
    } else {
        state.data_dir.clone()
    }
}

/// Prefix an in-memory key with user_id for multi-tenant isolation.
/// Dev mode: returns the key unchanged.
pub fn user_key(state: &AppState, auth: &AuthUser, key: &str) -> String {
    if state.auth_enabled {
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
    use std::path::PathBuf;

    #[test]
    fn user_data_dir_path_construction() {
        let base = PathBuf::from("/tmp/cthulu");
        let result = base.join("users").join("user_abc");
        assert_eq!(result, PathBuf::from("/tmp/cthulu/users/user_abc"));
    }

    #[test]
    fn user_key_format() {
        let result = format!("{}::{}", "user_abc", "agent::123");
        assert_eq!(result, "user_abc::agent::123");
    }

    #[test]
    fn ensure_dirs_creates_subdirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path().join("users").join("test_user");
        for sub in &["flows", "agents", "prompts"] {
            std::fs::create_dir_all(base.join(sub)).unwrap();
        }
        assert!(base.join("flows").exists());
        assert!(base.join("agents").exists());
        assert!(base.join("prompts").exists());
    }
}
