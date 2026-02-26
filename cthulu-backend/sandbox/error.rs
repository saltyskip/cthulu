use std::io;

/// Errors from sandbox operations.
///
/// Backends should map their internal errors into these variants.
/// `Unsupported` is the expected return for capability-gated operations
/// that a particular backend does not implement (e.g. checkpoints on
/// the dangerous-host backend).
#[derive(thiserror::Error, Debug)]
pub enum SandboxError {
    #[error("unsupported operation: {0}")]
    Unsupported(&'static str),

    #[error("sandbox not found: {0}")]
    NotFound(String),

    #[error("provision failed: {0}")]
    Provision(String),

    #[error("exec failed: {0}")]
    Exec(String),

    #[error("command failed: code={code:?}, stderr={stderr}")]
    CommandFailed { code: Option<i32>, stderr: String },

    #[error("timeout")]
    Timeout,

    #[error("io: {0}")]
    Io(#[from] io::Error),

    #[error("serialization: {0}")]
    Serde(String),

    #[error("backend error: {0}")]
    Backend(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_displays_message() {
        let err = SandboxError::Unsupported("checkpoint");
        assert_eq!(err.to_string(), "unsupported operation: checkpoint");
    }

    #[test]
    fn not_found_displays_id() {
        let err = SandboxError::NotFound("sbx-123".into());
        assert_eq!(err.to_string(), "sandbox not found: sbx-123");
    }

    #[test]
    fn command_failed_displays_code_and_stderr() {
        let err = SandboxError::CommandFailed {
            code: Some(1),
            stderr: "permission denied".into(),
        };
        assert_eq!(
            err.to_string(),
            "command failed: code=Some(1), stderr=permission denied"
        );
    }

    #[test]
    fn command_failed_with_no_code() {
        let err = SandboxError::CommandFailed {
            code: None,
            stderr: "killed".into(),
        };
        assert!(err.to_string().contains("code=None"));
    }

    #[test]
    fn timeout_displays() {
        let err = SandboxError::Timeout;
        assert_eq!(err.to_string(), "timeout");
    }

    #[test]
    fn io_error_converts_via_from() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file missing");
        let err: SandboxError = io_err.into();
        assert!(err.to_string().contains("file missing"));
        // Verify it's the Io variant
        assert!(matches!(err, SandboxError::Io(_)));
    }

    #[test]
    fn provision_exec_serde_backend_display() {
        assert_eq!(
            SandboxError::Provision("no docker".into()).to_string(),
            "provision failed: no docker"
        );
        assert_eq!(
            SandboxError::Exec("process died".into()).to_string(),
            "exec failed: process died"
        );
        assert_eq!(
            SandboxError::Serde("bad json".into()).to_string(),
            "serialization: bad json"
        );
        assert_eq!(
            SandboxError::Backend("connection refused".into()).to_string(),
            "backend error: connection refused"
        );
    }

    #[test]
    fn error_is_send_and_sync() {
        // SandboxError must be Send + Sync for use in async trait returns
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SandboxError>();
    }
}
