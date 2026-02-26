//! Host transport abstraction for running commands on the machine where
//! Firecracker runs.
//!
//! - `LocalLinux`: direct command execution (Linux host with /dev/kvm)
//! - `LimaSsh`: SSH into a Lima VM that has KVM support (macOS dev path)

use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;

use crate::sandbox::error::SandboxError;

/// Result of running a command on the host transport.
#[derive(Debug)]
pub struct HostCommandResult {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl HostCommandResult {
    /// Returns true if the command exited with code 0.
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    /// Returns an error if the command did not exit with code 0.
    pub fn check(&self) -> Result<(), SandboxError> {
        if self.success() {
            Ok(())
        } else {
            Err(SandboxError::CommandFailed {
                code: self.exit_code,
                stderr: String::from_utf8_lossy(&self.stderr).to_string(),
            })
        }
    }

    pub fn stdout_string(&self) -> String {
        String::from_utf8_lossy(&self.stdout).trim().to_string()
    }
}

/// Abstraction over where Firecracker commands run.
///
/// On Linux with /dev/kvm, commands run locally.
/// On macOS, commands are forwarded via SSH to a Lima VM.
#[async_trait]
pub trait HostTransport: Send + Sync {
    /// Run a shell command on the host where Firecracker will execute.
    async fn run_cmd(&self, args: &[&str]) -> Result<HostCommandResult, SandboxError>;

    /// Run a shell command as root (sudo on local, already root in Lima).
    async fn run_cmd_sudo(&self, args: &[&str]) -> Result<HostCommandResult, SandboxError>;

    /// Copy a file from the local machine to the FC host.
    /// For LocalLinux this is a no-op (or a `cp`).
    /// For LimaSsh this is `lima copy`.
    async fn copy_to_host(
        &self,
        local_path: &Path,
        remote_path: &Path,
    ) -> Result<(), SandboxError>;

    /// Copy a file from the FC host to the local machine.
    async fn copy_from_host(
        &self,
        remote_path: &Path,
        local_path: &Path,
    ) -> Result<(), SandboxError>;

    /// The path to the firecracker binary on the FC host.
    fn firecracker_bin(&self) -> &Path;

    /// The base directory for VM state on the FC host.
    fn state_dir(&self) -> &Path;

    /// Check that the transport is functional (FC binary exists, KVM accessible).
    async fn health_check(&self) -> Result<(), SandboxError>;
}

// ── LocalLinux ──────────────────────────────────────────────────────

/// Direct host execution — Linux with /dev/kvm.
pub struct LocalLinuxTransport {
    firecracker_bin: PathBuf,
    state_dir: PathBuf,
}

impl LocalLinuxTransport {
    pub fn new(firecracker_bin: PathBuf, state_dir: PathBuf) -> Self {
        Self {
            firecracker_bin,
            state_dir,
        }
    }
}

#[async_trait]
impl HostTransport for LocalLinuxTransport {
    async fn run_cmd(&self, args: &[&str]) -> Result<HostCommandResult, SandboxError> {
        if args.is_empty() {
            return Err(SandboxError::Exec("empty command".into()));
        }
        let output = tokio::process::Command::new(args[0])
            .args(&args[1..])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Ok(HostCommandResult {
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    async fn run_cmd_sudo(&self, args: &[&str]) -> Result<HostCommandResult, SandboxError> {
        let mut sudo_args: Vec<&str> = vec!["sudo"];
        sudo_args.extend_from_slice(args);
        self.run_cmd(&sudo_args).await
    }

    async fn copy_to_host(
        &self,
        local_path: &Path,
        remote_path: &Path,
    ) -> Result<(), SandboxError> {
        // Local — just copy if paths differ
        if local_path != remote_path {
            if let Some(parent) = remote_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::copy(local_path, remote_path).await?;
        }
        Ok(())
    }

    async fn copy_from_host(
        &self,
        remote_path: &Path,
        local_path: &Path,
    ) -> Result<(), SandboxError> {
        if remote_path != local_path {
            if let Some(parent) = local_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::copy(remote_path, local_path).await?;
        }
        Ok(())
    }

    fn firecracker_bin(&self) -> &Path {
        &self.firecracker_bin
    }

    fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    async fn health_check(&self) -> Result<(), SandboxError> {
        // Check firecracker binary exists
        if !self.firecracker_bin.exists() {
            return Err(SandboxError::Provision(format!(
                "firecracker binary not found at {}",
                self.firecracker_bin.display()
            )));
        }
        // Check /dev/kvm is accessible
        let kvm = Path::new("/dev/kvm");
        if !kvm.exists() {
            return Err(SandboxError::Provision(
                "/dev/kvm not found — KVM is required for Firecracker".into(),
            ));
        }
        Ok(())
    }
}

// ── LimaSsh ─────────────────────────────────────────────────────────

/// SSH into a Lima VM — macOS development path.
///
/// Lima provides a Linux VM with KVM support on Apple Silicon / Intel Macs.
/// We SSH into it to run Firecracker commands.
pub struct LimaSshTransport {
    /// Lima instance name (e.g. "firecracker" or "default")
    instance_name: String,
    /// Path to firecracker binary inside the Lima VM
    remote_firecracker_bin: String,
    /// State directory inside the Lima VM
    remote_state_dir: String,
}

impl LimaSshTransport {
    pub fn new(
        instance_name: String,
        remote_firecracker_bin: String,
        remote_state_dir: String,
    ) -> Self {
        Self {
            instance_name,
            remote_firecracker_bin,
            remote_state_dir,
        }
    }

    /// Run a command inside the Lima VM via `limactl shell`.
    async fn lima_shell(&self, cmd: &str) -> Result<HostCommandResult, SandboxError> {
        let output = tokio::process::Command::new("limactl")
            .args(["shell", &self.instance_name, "--", "bash", "-c", cmd])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Ok(HostCommandResult {
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

#[async_trait]
impl HostTransport for LimaSshTransport {
    async fn run_cmd(&self, args: &[&str]) -> Result<HostCommandResult, SandboxError> {
        if args.is_empty() {
            return Err(SandboxError::Exec("empty command".into()));
        }
        // Escape and join args into a single shell command
        let cmd = args
            .iter()
            .map(|a| shell_escape(a))
            .collect::<Vec<_>>()
            .join(" ");
        self.lima_shell(&cmd).await
    }

    async fn run_cmd_sudo(&self, args: &[&str]) -> Result<HostCommandResult, SandboxError> {
        if args.is_empty() {
            return Err(SandboxError::Exec("empty command".into()));
        }
        let cmd = args
            .iter()
            .map(|a| shell_escape(a))
            .collect::<Vec<_>>()
            .join(" ");
        self.lima_shell(&format!("sudo {cmd}")).await
    }

    async fn copy_to_host(
        &self,
        local_path: &Path,
        remote_path: &Path,
    ) -> Result<(), SandboxError> {
        let local = local_path.to_string_lossy();
        let remote = format!(
            "{}:{}",
            self.instance_name,
            remote_path.to_string_lossy()
        );

        let output = tokio::process::Command::new("limactl")
            .args(["copy", &local, &remote])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SandboxError::Exec(format!(
                "limactl copy failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    async fn copy_from_host(
        &self,
        remote_path: &Path,
        local_path: &Path,
    ) -> Result<(), SandboxError> {
        let remote = format!(
            "{}:{}",
            self.instance_name,
            remote_path.to_string_lossy()
        );
        let local = local_path.to_string_lossy();

        let output = tokio::process::Command::new("limactl")
            .args(["copy", &remote, &*local])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SandboxError::Exec(format!(
                "limactl copy failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    fn firecracker_bin(&self) -> &Path {
        Path::new(&self.remote_firecracker_bin)
    }

    fn state_dir(&self) -> &Path {
        Path::new(&self.remote_state_dir)
    }

    async fn health_check(&self) -> Result<(), SandboxError> {
        // Check Lima instance is running
        let result = self
            .lima_shell("echo ok")
            .await
            .map_err(|e| SandboxError::Provision(format!("Lima health check failed: {e}")))?;

        if !result.success() {
            return Err(SandboxError::Provision(format!(
                "Lima instance '{}' is not responding: {}",
                self.instance_name,
                String::from_utf8_lossy(&result.stderr)
            )));
        }

        // Check firecracker binary exists inside Lima
        let fc_check = self
            .lima_shell(&format!(
                "test -x {} && echo ok",
                self.remote_firecracker_bin
            ))
            .await?;

        if !fc_check.success() {
            return Err(SandboxError::Provision(format!(
                "firecracker binary not found at {} inside Lima VM '{}'",
                self.remote_firecracker_bin, self.instance_name
            )));
        }

        // Check /dev/kvm inside Lima
        let kvm_check = self.lima_shell("test -e /dev/kvm && echo ok").await?;
        if !kvm_check.success() {
            return Err(SandboxError::Provision(format!(
                "/dev/kvm not available inside Lima VM '{}'",
                self.instance_name
            )));
        }

        Ok(())
    }
}

// ── RemoteSsh ───────────────────────────────────────────────────────

/// SSH into a remote Linux server with real /dev/kvm.
///
/// This is the production path: Firecracker runs on a dedicated Linux server
/// accessible over SSH. Commands are run via `ssh user@host`.
pub struct RemoteSshTransport {
    /// SSH destination (e.g., "user@192.168.1.100")
    ssh_target: String,
    /// SSH port
    ssh_port: u16,
    /// Path to SSH private key (None = ssh-agent / default)
    ssh_key_path: Option<String>,
    /// Path to firecracker binary on the remote server
    remote_firecracker_bin: String,
    /// State directory on the remote server
    remote_state_dir: String,
}

impl RemoteSshTransport {
    pub fn new(
        ssh_target: String,
        ssh_port: u16,
        ssh_key_path: Option<String>,
        remote_firecracker_bin: String,
        remote_state_dir: String,
    ) -> Self {
        Self {
            ssh_target,
            ssh_port,
            ssh_key_path,
            remote_firecracker_bin,
            remote_state_dir,
        }
    }

    /// Build the base SSH command args.
    fn ssh_base_args(&self) -> Vec<String> {
        let mut args = vec![
            "-o".into(),
            "StrictHostKeyChecking=no".into(),
            "-o".into(),
            "UserKnownHostsFile=/dev/null".into(),
            "-o".into(),
            "ConnectTimeout=10".into(),
            "-p".into(),
            self.ssh_port.to_string(),
        ];
        if let Some(ref key) = self.ssh_key_path {
            args.push("-i".into());
            args.push(key.clone());
        }
        args
    }

    /// Run a command on the remote server via SSH.
    async fn ssh_exec(&self, cmd: &str) -> Result<HostCommandResult, SandboxError> {
        let mut args = self.ssh_base_args();
        args.push(self.ssh_target.clone());
        args.push("--".into());
        args.push("bash".into());
        args.push("-c".into());
        args.push(cmd.into());

        let output = tokio::process::Command::new("ssh")
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Ok(HostCommandResult {
            exit_code: output.status.code(),
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

#[async_trait]
impl HostTransport for RemoteSshTransport {
    async fn run_cmd(&self, args: &[&str]) -> Result<HostCommandResult, SandboxError> {
        if args.is_empty() {
            return Err(SandboxError::Exec("empty command".into()));
        }
        let cmd = args
            .iter()
            .map(|a| shell_escape(a))
            .collect::<Vec<_>>()
            .join(" ");
        self.ssh_exec(&cmd).await
    }

    async fn run_cmd_sudo(&self, args: &[&str]) -> Result<HostCommandResult, SandboxError> {
        if args.is_empty() {
            return Err(SandboxError::Exec("empty command".into()));
        }
        let cmd = args
            .iter()
            .map(|a| shell_escape(a))
            .collect::<Vec<_>>()
            .join(" ");
        self.ssh_exec(&format!("sudo {cmd}")).await
    }

    async fn copy_to_host(
        &self,
        local_path: &Path,
        remote_path: &Path,
    ) -> Result<(), SandboxError> {
        // Ensure parent dir exists on remote
        if let Some(parent) = remote_path.parent() {
            self.ssh_exec(&format!("mkdir -p {}", parent.display()))
                .await?;
        }

        let mut args = vec!["scp".to_string()];
        args.extend(self.ssh_base_args());
        args.push(local_path.to_string_lossy().to_string());
        args.push(format!(
            "{}:{}",
            self.ssh_target,
            remote_path.to_string_lossy()
        ));

        let output = tokio::process::Command::new(&args[0])
            .args(&args[1..])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SandboxError::Exec(format!(
                "scp to remote failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    async fn copy_from_host(
        &self,
        remote_path: &Path,
        local_path: &Path,
    ) -> Result<(), SandboxError> {
        if let Some(parent) = local_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let mut args = vec!["scp".to_string()];
        args.extend(self.ssh_base_args());
        args.push(format!(
            "{}:{}",
            self.ssh_target,
            remote_path.to_string_lossy()
        ));
        args.push(local_path.to_string_lossy().to_string());

        let output = tokio::process::Command::new(&args[0])
            .args(&args[1..])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SandboxError::Exec(format!(
                "scp from remote failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(())
    }

    fn firecracker_bin(&self) -> &Path {
        Path::new(&self.remote_firecracker_bin)
    }

    fn state_dir(&self) -> &Path {
        Path::new(&self.remote_state_dir)
    }

    async fn health_check(&self) -> Result<(), SandboxError> {
        // Check SSH connectivity
        let result = self.ssh_exec("echo ok").await.map_err(|e| {
            SandboxError::Provision(format!(
                "SSH to {} failed: {e}",
                self.ssh_target
            ))
        })?;

        if !result.success() {
            return Err(SandboxError::Provision(format!(
                "SSH to '{}' is not responding: {}",
                self.ssh_target,
                String::from_utf8_lossy(&result.stderr)
            )));
        }

        // Check firecracker binary exists on remote
        let fc_check = self
            .ssh_exec(&format!("test -x {} && echo ok", self.remote_firecracker_bin))
            .await?;

        if !fc_check.success() {
            return Err(SandboxError::Provision(format!(
                "firecracker binary not found at {} on remote '{}'",
                self.remote_firecracker_bin, self.ssh_target
            )));
        }

        // Check /dev/kvm on remote
        let kvm_check = self.ssh_exec("test -c /dev/kvm && echo ok").await?;
        if !kvm_check.success() {
            return Err(SandboxError::Provision(format!(
                "/dev/kvm not available on remote '{}'",
                self.ssh_target
            )));
        }

        Ok(())
    }
}

/// Shell escaping using the single-quote-with-replacement idiom.
///
/// Always wraps in single quotes for any input containing special characters.
/// Embedded single quotes are replaced with `'\''` (end quote, escaped quote,
/// restart quote). This is safe against all injection vectors: `$`, backtick,
/// `\`, `"`, etc. are all literal inside single quotes.
///
/// Used by host transports and guest agent to prevent shell injection
/// when interpolating user-supplied values into shell command strings.
pub fn shell_escape(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    // If only safe characters, return as-is for readability
    if s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.' || b == b'/') {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ── Factory ─────────────────────────────────────────────────────────

/// Build a HostTransport from the config enum.
pub fn build_transport(
    config: &crate::sandbox::types::FirecrackerHostTransportConfig,
    state_dir: &Path,
) -> Box<dyn HostTransport> {
    match config {
        crate::sandbox::types::FirecrackerHostTransportConfig::LocalLinux {
            firecracker_bin,
            ..
        } => Box::new(LocalLinuxTransport::new(
            firecracker_bin.clone(),
            state_dir.to_path_buf(),
        )),
        crate::sandbox::types::FirecrackerHostTransportConfig::LimaSsh {
            ssh_target,
            remote_firecracker_bin,
            remote_state_dir,
        } => Box::new(LimaSshTransport::new(
            ssh_target.clone(),
            remote_firecracker_bin.clone(),
            remote_state_dir.clone(),
        )),
        crate::sandbox::types::FirecrackerHostTransportConfig::LimaTcp {
            lima_instance,
            ..
        } => {
            // For LimaTcp, host commands go through Lima shell
            // The FC API is accessed directly over TCP (handled by VmApi)
            Box::new(LimaSshTransport::new(
                lima_instance.clone(),
                "firecracker".into(), // Binary name inside Lima
                state_dir.to_string_lossy().to_string(),
            ))
        }
        crate::sandbox::types::FirecrackerHostTransportConfig::RemoteSsh {
            ssh_target,
            ssh_port,
            ssh_key_path,
            remote_firecracker_bin,
            remote_state_dir,
            ..
        } => {
            // Remote Linux server with real /dev/kvm
            // FC API is accessed over TCP (handled by VmApi), host commands over SSH
            Box::new(RemoteSshTransport::new(
                ssh_target.clone(),
                *ssh_port,
                ssh_key_path.clone(),
                remote_firecracker_bin.clone(),
                remote_state_dir.clone(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_escape_basic() {
        assert_eq!(shell_escape("hello"), "hello");
        assert_eq!(shell_escape("hello world"), "'hello world'");
        assert_eq!(shell_escape("$HOME"), "'$HOME'");
        assert_eq!(shell_escape(""), "''");
        assert_eq!(shell_escape("/usr/bin/test"), "/usr/bin/test");
        assert_eq!(shell_escape("file_name.txt"), "file_name.txt");
    }

    #[test]
    fn shell_escape_single_quotes() {
        // Single quotes use the '\'' replacement idiom
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape("it's $HOME"), "'it'\\''s $HOME'");
    }

    #[test]
    fn shell_escape_injection_safe() {
        // These must NOT expand $HOME or execute commands
        assert_eq!(shell_escape("$(rm -rf /)"), "'$(rm -rf /)'");
        assert_eq!(shell_escape("`whoami`"), "'`whoami`'");
        assert_eq!(shell_escape("foo;bar"), "'foo;bar'");
        assert_eq!(shell_escape("a\"b"), "'a\"b'");
    }

    #[test]
    fn host_command_result_check() {
        let ok = HostCommandResult {
            exit_code: Some(0),
            stdout: b"ok".to_vec(),
            stderr: vec![],
        };
        assert!(ok.success());
        assert!(ok.check().is_ok());

        let fail = HostCommandResult {
            exit_code: Some(1),
            stdout: vec![],
            stderr: b"error".to_vec(),
        };
        assert!(!fail.success());
        assert!(fail.check().is_err());
    }

    #[tokio::test]
    async fn local_transport_run_echo() {
        let transport = LocalLinuxTransport::new(
            PathBuf::from("/usr/bin/echo"),
            PathBuf::from("/tmp/fc-test"),
        );
        let result = transport.run_cmd(&["echo", "hello"]).await.unwrap();
        assert!(result.success());
        assert_eq!(result.stdout_string(), "hello");
    }

    #[tokio::test]
    async fn local_transport_run_empty_errors() {
        let transport = LocalLinuxTransport::new(
            PathBuf::from("/usr/bin/echo"),
            PathBuf::from("/tmp/fc-test"),
        );
        let result = transport.run_cmd(&[]).await;
        assert!(result.is_err());
    }
}
