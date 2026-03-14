use serde::Serialize;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Severity level for an environment check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckLevel {
    Info,
    Warn,
    Error,
}

/// Aggregate status derived from all checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    Pass,
    Warn,
    Fail,
}

/// A single environment check with code, level, message, and optional hint.
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentCheck {
    pub code: String,
    pub level: CheckLevel,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Full result of probing the Claude CLI environment.
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentTestResult {
    pub status: CheckStatus,
    pub checks: Vec<EnvironmentCheck>,
    pub tested_at: String,
}

/// Probe the local environment for Claude CLI readiness.
///
/// Performs the following checks in order:
///
/// 1. **`cli_resolvable`** — Is the `claude` binary on `PATH`?
///    Uses `which` on Unix / `where` on Windows.
///
/// 2. **`anthropic_api_key`** — Is `ANTHROPIC_API_KEY` set?
///    This is a *warning* because it overrides subscription-based auth, which
///    can cause surprising billing behaviour.
///
/// 3. **`hello_probe`** — Can we actually talk to Claude?
///    Spawns `claude --print - --output-format stream-json --verbose`, writes
///    `"Respond with hello.\n"` to stdin, and waits up to 30 s.  The response
///    is parsed with [`super::parse::parse_stream_json`] and checked for login
///    issues via [`super::parse::detect_login_required`].
pub async fn test_environment() -> EnvironmentTestResult {
    let mut checks: Vec<EnvironmentCheck> = Vec::new();
    let mut cli_found = false;

    // -----------------------------------------------------------------------
    // 1. Check that `claude` is on PATH
    // -----------------------------------------------------------------------
    {
        let lookup_cmd = if cfg!(target_os = "windows") {
            "where"
        } else {
            "which"
        };

        match Command::new(lookup_cmd).arg("claude").output().await {
            Ok(output) if output.status.success() => {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                checks.push(EnvironmentCheck {
                    code: "cli_resolvable".into(),
                    level: CheckLevel::Info,
                    message: format!("claude binary found at {path}"),
                    hint: None,
                });
                cli_found = true;
            }
            Ok(_) => {
                checks.push(EnvironmentCheck {
                    code: "cli_resolvable".into(),
                    level: CheckLevel::Error,
                    message: "claude binary not found on PATH".into(),
                    hint: Some(
                        "Install Claude Code CLI: https://docs.anthropic.com/en/docs/claude-code"
                            .into(),
                    ),
                });
            }
            Err(e) => {
                checks.push(EnvironmentCheck {
                    code: "cli_resolvable".into(),
                    level: CheckLevel::Error,
                    message: format!("failed to run `{lookup_cmd} claude`: {e}"),
                    hint: Some(
                        "Ensure the `which` (or `where` on Windows) command is available".into(),
                    ),
                });
            }
        }
    }

    // -----------------------------------------------------------------------
    // 2. Check ANTHROPIC_API_KEY
    // -----------------------------------------------------------------------
    {
        if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            checks.push(EnvironmentCheck {
                code: "anthropic_api_key".into(),
                level: CheckLevel::Warn,
                message: "ANTHROPIC_API_KEY is set — this overrides subscription-based auth"
                    .into(),
                hint: Some(
                    "If you intend to use Claude Pro/Team subscription auth, unset ANTHROPIC_API_KEY"
                        .into(),
                ),
            });
        } else {
            checks.push(EnvironmentCheck {
                code: "anthropic_api_key".into(),
                level: CheckLevel::Info,
                message: "ANTHROPIC_API_KEY not set (subscription auth will be used)".into(),
                hint: None,
            });
        }
    }

    // -----------------------------------------------------------------------
    // 3. Hello probe (only if CLI was found)
    // -----------------------------------------------------------------------
    if cli_found {
        run_hello_probe(&mut checks).await;
    } else {
        checks.push(EnvironmentCheck {
            code: "hello_probe".into(),
            level: CheckLevel::Error,
            message: "skipped hello probe because claude binary was not found".into(),
            hint: None,
        });
    }

    // -----------------------------------------------------------------------
    // Derive aggregate status
    // -----------------------------------------------------------------------
    let status = derive_status(&checks);
    let tested_at = chrono::Utc::now().to_rfc3339();

    EnvironmentTestResult {
        status,
        checks,
        tested_at,
    }
}

/// Spawn `claude --print - --output-format stream-json --verbose`, send a
/// trivial prompt, and validate the response.
async fn run_hello_probe(checks: &mut Vec<EnvironmentCheck>) {
    let spawn_result = Command::new("claude")
        .args(["--print", "-", "--output-format", "stream-json", "--verbose"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();

    let mut child = match spawn_result {
        Ok(c) => c,
        Err(e) => {
            checks.push(EnvironmentCheck {
                code: "hello_probe".into(),
                level: CheckLevel::Error,
                message: format!("failed to spawn claude process: {e}"),
                hint: Some("Ensure `claude` is executable and not blocked by permissions".into()),
            });
            return;
        }
    };

    // Write the probe prompt to stdin, then close it.
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(b"Respond with hello.\n").await {
            tracing::warn!(error = %e, "failed to write to claude stdin during probe");
            let _ = child.kill().await;
            checks.push(EnvironmentCheck {
                code: "hello_probe".into(),
                level: CheckLevel::Error,
                message: format!("failed to write prompt to claude stdin: {e}"),
                hint: None,
            });
            return;
        }
        // stdin is dropped here, closing the pipe
    }

    // Collect stdout/stderr before waiting, so we retain the ability to kill
    // the child on timeout (wait_with_output takes ownership).
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut r) = stdout_pipe {
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut r, &mut buf).await;
        }
        buf
    });
    let stderr_handle = tokio::spawn(async move {
        let mut buf = Vec::new();
        if let Some(mut r) = stderr_pipe {
            let _ = tokio::io::AsyncReadExt::read_to_end(&mut r, &mut buf).await;
        }
        buf
    });

    // Wait for the process with a 30-second timeout.
    let timeout_duration = Duration::from_secs(30);
    let exit_status = match tokio::time::timeout(timeout_duration, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            checks.push(EnvironmentCheck {
                code: "hello_probe".into(),
                level: CheckLevel::Error,
                message: format!("claude process error: {e}"),
                hint: None,
            });
            return;
        }
        Err(_elapsed) => {
            tracing::warn!("claude hello probe timed out after 30s");
            let _ = child.kill().await;
            checks.push(EnvironmentCheck {
                code: "hello_probe".into(),
                level: CheckLevel::Error,
                message: "claude hello probe timed out after 30 seconds".into(),
                hint: Some(
                    "Check network connectivity and Claude CLI configuration".into(),
                ),
            });
            return;
        }
    };

    let stdout_bytes = stdout_handle.await.unwrap_or_default();
    let stderr_bytes = stderr_handle.await.unwrap_or_default();

    let stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
    let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();

    // Check for login/auth issues first.
    let login = super::parse::detect_login_required(&stdout, &stderr);
    if login.requires_login {
        let mut msg = "Claude CLI requires login".to_string();
        if let Some(ref url) = login.login_url {
            msg.push_str(&format!(" — visit {url}"));
        }
        checks.push(EnvironmentCheck {
            code: "hello_probe".into(),
            level: CheckLevel::Error,
            message: msg,
            hint: Some("Run `claude login` to authenticate".into()),
        });
        return;
    }

    // Check exit status.
    if !exit_status.success() {
        let code = exit_status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".into());
        let stderr_preview = if stderr.len() > 300 {
            format!("{}…", &stderr[..300])
        } else {
            stderr.clone()
        };
        checks.push(EnvironmentCheck {
            code: "hello_probe".into(),
            level: CheckLevel::Error,
            message: format!("claude exited with code {code}: {stderr_preview}"),
            hint: None,
        });
        return;
    }

    // Parse the stream-json output.
    let parsed = super::parse::parse_stream_json(&stdout);

    // Check that we actually got a response containing "hello".
    let summary_lower = parsed.summary.to_lowercase();
    if summary_lower.contains("hello") {
        let model_info = if parsed.model.is_empty() {
            String::new()
        } else {
            format!(" (model: {})", parsed.model)
        };
        checks.push(EnvironmentCheck {
            code: "hello_probe".into(),
            level: CheckLevel::Info,
            message: format!("Claude responded successfully{model_info}"),
            hint: None,
        });
    } else if parsed.summary.is_empty() {
        checks.push(EnvironmentCheck {
            code: "hello_probe".into(),
            level: CheckLevel::Warn,
            message: "Claude process exited successfully but produced no text output".into(),
            hint: Some("The CLI may have changed its output format".into()),
        });
    } else {
        // Got a response, but it didn't contain "hello" — still basically okay.
        let preview = if parsed.summary.len() > 120 {
            format!("{}…", &parsed.summary[..120])
        } else {
            parsed.summary.clone()
        };
        checks.push(EnvironmentCheck {
            code: "hello_probe".into(),
            level: CheckLevel::Info,
            message: format!("Claude responded (content: \"{preview}\")"),
            hint: None,
        });
    }
}

/// Derive the aggregate [`CheckStatus`] from a list of checks.
///
/// * Any `Error`-level check → `Fail`
/// * Any `Warn`-level check (but no errors) → `Warn`
/// * Otherwise → `Pass`
fn derive_status(checks: &[EnvironmentCheck]) -> CheckStatus {
    let mut has_warn = false;
    for check in checks {
        match check.level {
            CheckLevel::Error => return CheckStatus::Fail,
            CheckLevel::Warn => has_warn = true,
            CheckLevel::Info => {}
        }
    }
    if has_warn {
        CheckStatus::Warn
    } else {
        CheckStatus::Pass
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_status_all_info() {
        let checks = vec![
            EnvironmentCheck {
                code: "a".into(),
                level: CheckLevel::Info,
                message: "ok".into(),
                hint: None,
            },
            EnvironmentCheck {
                code: "b".into(),
                level: CheckLevel::Info,
                message: "ok".into(),
                hint: None,
            },
        ];
        assert_eq!(derive_status(&checks), CheckStatus::Pass);
    }

    #[test]
    fn test_derive_status_with_warn() {
        let checks = vec![
            EnvironmentCheck {
                code: "a".into(),
                level: CheckLevel::Info,
                message: "ok".into(),
                hint: None,
            },
            EnvironmentCheck {
                code: "b".into(),
                level: CheckLevel::Warn,
                message: "hmm".into(),
                hint: None,
            },
        ];
        assert_eq!(derive_status(&checks), CheckStatus::Warn);
    }

    #[test]
    fn test_derive_status_with_error() {
        let checks = vec![
            EnvironmentCheck {
                code: "a".into(),
                level: CheckLevel::Warn,
                message: "hmm".into(),
                hint: None,
            },
            EnvironmentCheck {
                code: "b".into(),
                level: CheckLevel::Error,
                message: "bad".into(),
                hint: None,
            },
        ];
        assert_eq!(derive_status(&checks), CheckStatus::Fail);
    }

    #[test]
    fn test_derive_status_empty() {
        assert_eq!(derive_status(&[]), CheckStatus::Pass);
    }

    #[test]
    fn test_environment_check_serialization() {
        let check = EnvironmentCheck {
            code: "cli_resolvable".into(),
            level: CheckLevel::Info,
            message: "found".into(),
            hint: None,
        };
        let json = serde_json::to_string(&check).unwrap();
        assert!(json.contains("\"level\":\"info\""));
        // hint should be omitted
        assert!(!json.contains("hint"));
    }

    #[test]
    fn test_environment_check_serialization_with_hint() {
        let check = EnvironmentCheck {
            code: "test".into(),
            level: CheckLevel::Error,
            message: "missing".into(),
            hint: Some("install it".into()),
        };
        let json = serde_json::to_string(&check).unwrap();
        assert!(json.contains("\"hint\":\"install it\""));
        assert!(json.contains("\"level\":\"error\""));
    }

    #[test]
    fn test_check_status_serialization() {
        let result = EnvironmentTestResult {
            status: CheckStatus::Pass,
            checks: vec![],
            tested_at: "2026-01-01T00:00:00Z".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"status\":\"pass\""));
    }
}
