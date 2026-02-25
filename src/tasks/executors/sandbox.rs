use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_trait::async_trait;

use super::{ExecutionResult, Executor};
use crate::sandbox::provider::SandboxProvider;
use crate::sandbox::types::*;

const PROCESS_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// Executor that runs Claude CLI inside a sandbox.
///
/// Bridges the existing `Executor` trait (used by `FlowRunner`) to the
/// `SandboxProvider` / `SandboxHandle` API. Each `execute()` call:
/// 1. Provisions a sandbox (or reuses one for the workspace)
/// 2. Runs `claude` CLI inside it with the same args as `ClaudeCodeExecutor`
/// 3. Parses the stream-json output for cost/turns/result
/// 4. Returns `ExecutionResult`
pub struct SandboxExecutor {
    provider: Arc<dyn SandboxProvider>,
    permissions: Vec<String>,
    append_system_prompt: Option<String>,
}

impl SandboxExecutor {
    pub fn new(
        provider: Arc<dyn SandboxProvider>,
        permissions: Vec<String>,
        append_system_prompt: Option<String>,
    ) -> Self {
        Self {
            provider,
            permissions,
            append_system_prompt,
        }
    }

    fn build_claude_args(&self) -> Vec<String> {
        let mut args = vec![
            "claude".to_string(),
            "--print".to_string(),
            "--verbose".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];

        if let Some(prompt) = &self.append_system_prompt {
            args.push("--append-system-prompt".to_string());
            args.push(prompt.clone());
        }

        if self.permissions.is_empty() {
            args.push("--dangerously-skip-permissions".to_string());
        } else {
            args.push("--allowedTools".to_string());
            args.push(self.permissions.join(","));
        }

        args.push("-".to_string()); // read from stdin
        args
    }
}

#[async_trait]
impl Executor for SandboxExecutor {
    async fn execute(&self, prompt: &str, working_dir: &Path) -> Result<ExecutionResult> {
        // Use working_dir's last component as workspace_id
        let workspace_id = working_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "default".to_string());

        let spec = SandboxSpec {
            workspace_id,
            profile: SandboxProfile::Base,
            filesystem: FilesystemSpec::default(),
            resources: ResourceHints::default(),
            env: BTreeMap::new(),
            mounts: vec![],
            network: NetworkPolicy::default_safe(),
            lifecycle: LifecyclePolicy::default(),
            labels: BTreeMap::from([("executor".into(), "sandbox".into())]),
        };

        let handle = self
            .provider
            .provision(spec)
            .await
            .map_err(|e| anyhow::anyhow!("sandbox provision failed: {e}"))?;

        let args = self.build_claude_args();

        let exec_req = ExecRequest {
            command: args,
            cwd: None,
            env: BTreeMap::new(),
            stdin: Some(prompt.as_bytes().to_vec()),
            timeout: Some(PROCESS_TIMEOUT),
            tty: false,
            detach: false,
        };

        let result = handle
            .exec(exec_req)
            .await
            .map_err(|e| anyhow::anyhow!("sandbox exec failed: {e}"))?;

        if result.timed_out {
            let _ = handle.destroy().await;
            anyhow::bail!(
                "claude process timed out after {}s",
                PROCESS_TIMEOUT.as_secs()
            );
        }

        // Parse stream-json output (same logic as ClaudeCodeExecutor)
        let stdout_str = String::from_utf8_lossy(&result.stdout);
        let mut result_text: Option<String> = None;
        let mut total_cost: f64 = 0.0;
        let mut total_turns: u64 = 0;

        for line in stdout_str.lines() {
            if line.is_empty() {
                continue;
            }
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(line) {
                let event_type = event
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                if event_type == "result" {
                    total_cost = event
                        .get("total_cost_usd")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    total_turns = event
                        .get("num_turns")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    result_text = event
                        .get("result")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
            }
        }

        if let Some(code) = result.exit_code {
            if code != 0 {
                let stderr_str = String::from_utf8_lossy(&result.stderr);
                let _ = handle.destroy().await;
                anyhow::bail!("claude exited with code {code}: {stderr_str}");
            }
        }

        // Destroy the sandbox to release resources (workspace dirs, VM state, TAP devices).
        // Each execute() provisions a fresh sandbox, so there's nothing to preserve.
        let _ = handle.destroy().await;

        Ok(ExecutionResult {
            text: result_text.unwrap_or_default(),
            cost_usd: total_cost,
            num_turns: total_turns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_args_with_permissions() {
        let executor = SandboxExecutor::new(
            Arc::new(crate::sandbox::backends::dangerous::DangerousHostProvider::new(
                DangerousConfig {
                    root_dir: std::path::PathBuf::from("/tmp/test-sandbox"),
                    ..DangerousConfig::default()
                },
            ).unwrap()),
            vec!["Bash".into(), "Read".into()],
            None,
        );
        let args = executor.build_claude_args();
        assert!(args.contains(&"claude".to_string()));
        assert!(args.contains(&"--print".to_string()));
        assert!(args.contains(&"--allowedTools".to_string()));
        assert!(args.contains(&"Bash,Read".to_string()));
        assert!(!args.contains(&"--dangerously-skip-permissions".to_string()));
    }

    #[test]
    fn build_args_no_permissions() {
        let executor = SandboxExecutor::new(
            Arc::new(crate::sandbox::backends::dangerous::DangerousHostProvider::new(
                DangerousConfig {
                    root_dir: std::path::PathBuf::from("/tmp/test-sandbox2"),
                    ..DangerousConfig::default()
                },
            ).unwrap()),
            vec![],
            Some("You are a helpful agent".into()),
        );
        let args = executor.build_claude_args();
        assert!(args.contains(&"--dangerously-skip-permissions".to_string()));
        assert!(args.contains(&"--append-system-prompt".to_string()));
        assert!(args.contains(&"You are a helpful agent".to_string()));
    }

    #[test]
    fn build_args_reads_stdin() {
        let executor = SandboxExecutor::new(
            Arc::new(crate::sandbox::backends::dangerous::DangerousHostProvider::new(
                DangerousConfig {
                    root_dir: std::path::PathBuf::from("/tmp/test-sandbox3"),
                    ..DangerousConfig::default()
                },
            ).unwrap()),
            vec![],
            None,
        );
        let args = executor.build_claude_args();
        assert_eq!(args.last().unwrap(), "-");
    }
}
