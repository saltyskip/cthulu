use axum::http::StatusCode;
use axum::Json;
use serde_json::json;
use std::path::{Path, PathBuf};

type ApiError = (StatusCode, Json<serde_json::Value>);

/// Validate a user-provided name (workspace, workflow, flow, agent).
/// Rejects empty, too long, path traversal, control chars, leading dots.
pub fn validate_name(s: &str, field: &str, max_len: usize) -> Result<(), ApiError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": format!("{field} must not be empty")}))));
    }
    if trimmed.len() > max_len {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": format!("{field} must be at most {max_len} characters")}))));
    }
    if trimmed.starts_with('.') {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": format!("{field} must not start with '.'")}))));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": format!("{field} contains invalid path characters")}))));
    }
    if trimmed.contains('\0') || trimmed.chars().any(|c| c.is_control()) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": format!("{field} contains invalid characters")}))));
    }
    Ok(())
}

/// Validate that a file path stays within a base directory (prevents path traversal).
pub fn validate_file_path(base: &Path, requested: &str) -> Result<PathBuf, ApiError> {
    // Reject obvious traversal attempts before even joining
    if requested.contains("..") {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Path traversal not allowed"}))));
    }

    let joined = base.join(requested);

    // Canonicalize base (it must exist)
    let canon_base = base.canonicalize().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to resolve base path: {e}")})))
    })?;

    // The joined path may not exist yet, so canonicalize its parent
    let canon_joined = if joined.exists() {
        joined.canonicalize().map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": format!("Failed to resolve path: {e}")})))
        })?
    } else {
        // For non-existent paths, canonicalize the parent and append the filename
        let parent = joined.parent().ok_or_else(|| {
            (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid file path"})))
        })?;
        let canon_parent = parent.canonicalize().map_err(|_| {
            (StatusCode::BAD_REQUEST, Json(json!({"error": "Parent directory does not exist"})))
        })?;
        let filename = joined.file_name().ok_or_else(|| {
            (StatusCode::BAD_REQUEST, Json(json!({"error": "Invalid file path"})))
        })?;
        canon_parent.join(filename)
    };

    if !canon_joined.starts_with(&canon_base) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({"error": "Path traversal not allowed"}))));
    }

    Ok(canon_joined)
}
