//! Per-user directory and key helpers for multi-tenant data isolation.
//!
//! When AUTH_ENABLED=true, data is namespaced under
//! `~/.cthulu/users/{user_id}/`. When disabled (dev mode), uses the
//! flat legacy structure at `~/.cthulu/`.

use std::path::{Path, PathBuf};

use crate::api::local_auth::{auth_enabled, AuthUser};
use crate::api::AppState;

/// Pure path computation — testable without AppState.
pub fn user_data_dir_path(base: &Path, auth_enabled: bool, user_id: &str) -> PathBuf {
    if auth_enabled {
        base.join("users").join(user_id)
    } else {
        base.to_path_buf()
    }
}

/// Pure key computation — testable without AppState.
pub fn user_key_string(auth_enabled: bool, user_id: &str, key: &str) -> String {
    if auth_enabled {
        format!("{user_id}::{key}")
    } else {
        key.to_string()
    }
}

/// Returns the per-user data directory.
/// Safety: user_id is validated to [a-zA-Z0-9_-] in AuthUser extractor,
/// preventing path traversal.
pub fn user_data_dir(state: &AppState, auth: &AuthUser) -> PathBuf {
    user_data_dir_path(&state.data_dir, auth_enabled(state), &auth.user_id)
}

/// Prefix an in-memory key with user_id for multi-tenant isolation.
pub fn user_key(state: &AppState, auth: &AuthUser, key: &str) -> String {
    user_key_string(auth_enabled(state), &auth.user_id, key)
}

/// Pure directory creation — testable without AppState.
pub fn ensure_dirs_at(base: &Path) -> std::io::Result<()> {
    for sub in &["flows", "agents", "prompts"] {
        std::fs::create_dir_all(base.join(sub))?;
    }
    Ok(())
}

/// Ensure the user's data directory and subdirectories exist.
pub fn ensure_user_dirs(state: &AppState, auth: &AuthUser) -> std::io::Result<()> {
    ensure_dirs_at(&user_data_dir(state, auth))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn user_data_dir_auth_enabled() {
        let result = user_data_dir_path(Path::new("/data"), true, "user_abc");
        assert_eq!(result, PathBuf::from("/data/users/user_abc"));
    }

    #[test]
    fn user_data_dir_auth_disabled() {
        let result = user_data_dir_path(Path::new("/data"), false, "user_abc");
        assert_eq!(result, PathBuf::from("/data"));
    }

    #[test]
    fn user_key_auth_enabled() {
        let result = user_key_string(true, "user_abc", "agent::123");
        assert_eq!(result, "user_abc::agent::123");
    }

    #[test]
    fn user_key_auth_disabled() {
        let result = user_key_string(false, "user_abc", "agent::123");
        assert_eq!(result, "agent::123");
    }

    #[test]
    fn ensure_dirs_at_creates_all_subdirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let user_dir = user_data_dir_path(tmp.path(), true, "test_user");
        ensure_dirs_at(&user_dir).unwrap();
        assert!(user_dir.join("flows").exists());
        assert!(user_dir.join("agents").exists());
        assert!(user_dir.join("prompts").exists());
    }
}
