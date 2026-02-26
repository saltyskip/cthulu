use std::collections::BTreeMap;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use crate::sandbox::error::SandboxError;
use crate::sandbox::handle::ExecStream;
use crate::sandbox::types::{ExecEvent, ExecRequest, ExecResult};

/// Wraps `tokio::process::Command` with env filtering, output limits,
/// timeout, and kill. Used by `DangerousHostProvider`.
pub struct ProcessSupervisor {
    /// Environment variables to inherit from host (allowlist).
    pub env_allowlist: Vec<String>,
    /// Maximum stdout+stderr bytes before truncation.
    pub max_output_bytes: usize,
}

impl ProcessSupervisor {
    pub fn new(env_allowlist: Vec<String>, max_output_bytes: usize) -> Self {
        Self {
            env_allowlist,
            max_output_bytes,
        }
    }

    /// Build a filtered env map: only allowlisted host vars + request vars.
    fn build_env(&self, extra: &BTreeMap<String, String>) -> Vec<(String, String)> {
        let mut env: Vec<(String, String)> = Vec::new();
        for key in &self.env_allowlist {
            if let Ok(val) = std::env::var(key) {
                env.push((key.clone(), val));
            }
        }
        for (k, v) in extra {
            env.push((k.clone(), v.clone()));
        }
        env
    }

    /// Run a command to completion, capturing output.
    pub async fn exec(
        &self,
        req: &ExecRequest,
        working_dir: &std::path::Path,
    ) -> Result<ExecResult, SandboxError> {
        if req.command.is_empty() {
            return Err(SandboxError::Exec("empty command".into()));
        }

        let started_at = chrono::Utc::now().timestamp_millis();
        let env = self.build_env(&req.env);

        let mut cmd = Command::new(&req.command[0]);
        cmd.args(&req.command[1..]);
        cmd.current_dir(req.cwd.as_deref().unwrap_or(working_dir.to_str().unwrap_or(".")));
        cmd.env_clear();
        for (k, v) in &env {
            cmd.env(k, v);
        }
        cmd.stdin(if req.stdin.is_some() { Stdio::piped() } else { Stdio::null() });
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| SandboxError::Exec(format!("spawn failed: {e}")))?;

        // Write stdin if provided
        if let Some(input) = &req.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(input).await;
                drop(stdin);
            }
        }

        // Collect stdout
        let stdout_handle = child.stdout.take().unwrap();
        let max_bytes = self.max_output_bytes;
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let mut reader = BufReader::new(stdout_handle);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        if buf.len() + line.len() <= max_bytes {
                            buf.extend_from_slice(line.as_bytes());
                        }
                    }
                    Err(_) => break,
                }
            }
            buf
        });

        // Collect stderr
        let stderr_handle = child.stderr.take().unwrap();
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            let mut reader = BufReader::new(stderr_handle);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        if buf.len() + line.len() <= max_bytes {
                            buf.extend_from_slice(line.as_bytes());
                        }
                    }
                    Err(_) => break,
                }
            }
            buf
        });

        // Wait with timeout
        let timeout_dur = req.timeout.unwrap_or(Duration::from_secs(15 * 60));
        let (timed_out, exit_code) = match tokio::time::timeout(timeout_dur, child.wait()).await {
            Ok(Ok(status)) => (false, status.code()),
            Ok(Err(e)) => return Err(SandboxError::Exec(format!("wait failed: {e}"))),
            Err(_) => {
                let _ = child.kill().await;
                (true, None)
            }
        };

        let stdout = stdout_task.await.unwrap_or_default();
        let stderr = stderr_task.await.unwrap_or_default();
        let finished_at = chrono::Utc::now().timestamp_millis();

        Ok(ExecResult {
            exit_code,
            stdout,
            stderr,
            timed_out,
            started_at_unix_ms: started_at,
            finished_at_unix_ms: Some(finished_at),
            session_id: None,
        })
    }

    /// Start a streaming exec session.
    pub async fn exec_stream(
        &self,
        req: &ExecRequest,
        working_dir: &std::path::Path,
    ) -> Result<ProcessExecStream, SandboxError> {
        if req.command.is_empty() {
            return Err(SandboxError::Exec("empty command".into()));
        }

        let env = self.build_env(&req.env);

        let mut cmd = Command::new(&req.command[0]);
        cmd.args(&req.command[1..]);
        cmd.current_dir(req.cwd.as_deref().unwrap_or(working_dir.to_str().unwrap_or(".")));
        cmd.env_clear();
        for (k, v) in &env {
            cmd.env(k, v);
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| SandboxError::Exec(format!("spawn failed: {e}")))?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        // Multiplex stdout and stderr into a single event channel
        let (tx, rx) = mpsc::unbounded_channel();

        let tx_out = tx.clone();
        let stdout_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut data = line.into_bytes();
                data.push(b'\n');
                let _ = tx_out.send(ExecEvent::Stdout(data));
            }
        });

        let tx_err = tx.clone();
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut data = line.into_bytes();
                data.push(b'\n');
                let _ = tx_err.send(ExecEvent::Stderr(data));
            }
        });

        // Wait for exit in background — drain stdout/stderr before sending Exit
        let tx_exit = tx;
        tokio::spawn(async move {
            let status = child.wait().await;
            // Wait for stdout/stderr readers to finish draining before sending Exit
            let _ = stdout_handle.await;
            let _ = stderr_handle.await;
            match status {
                Ok(s) => {
                    let _ = tx_exit.send(ExecEvent::Exit {
                        code: s.code().unwrap_or(-1),
                    });
                }
                Err(_) => {
                    let _ = tx_exit.send(ExecEvent::Exit { code: -1 });
                }
            }
        });

        Ok(ProcessExecStream { rx, stdin })
    }
}

/// ExecStream implementation backed by a child process.
pub struct ProcessExecStream {
    rx: mpsc::UnboundedReceiver<ExecEvent>,
    stdin: tokio::process::ChildStdin,
}

#[async_trait::async_trait]
impl ExecStream for ProcessExecStream {
    async fn next_event(&mut self) -> Result<Option<ExecEvent>, SandboxError> {
        Ok(self.rx.recv().await)
    }

    async fn write_stdin(&mut self, data: &[u8]) -> Result<(), SandboxError> {
        self.stdin
            .write_all(data)
            .await
            .map_err(|e| SandboxError::Exec(format!("write stdin: {e}")))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| SandboxError::Exec(format!("flush stdin: {e}")))?;
        Ok(())
    }

    async fn close_stdin(&mut self) -> Result<(), SandboxError> {
        self.stdin
            .shutdown()
            .await
            .map_err(|e| SandboxError::Exec(format!("close stdin: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn supervisor() -> ProcessSupervisor {
        ProcessSupervisor::new(vec!["PATH".into()], 1024 * 1024)
    }

    #[tokio::test]
    async fn exec_echo() {
        let sup = supervisor();
        let req = ExecRequest {
            command: vec!["echo".into(), "hello sandbox".into()],
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            timeout: Some(Duration::from_secs(5)),
            tty: false,
            detach: false,
        };
        let result = sup.exec(&req, &PathBuf::from(".")).await.unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert!(!result.timed_out);
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).trim(),
            "hello sandbox"
        );
    }

    #[tokio::test]
    async fn exec_nonzero_exit() {
        let sup = supervisor();
        let req = ExecRequest {
            command: vec!["bash".into(), "-c".into(), "exit 42".into()],
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            timeout: Some(Duration::from_secs(5)),
            tty: false,
            detach: false,
        };
        let result = sup.exec(&req, &PathBuf::from(".")).await.unwrap();
        assert_eq!(result.exit_code, Some(42));
    }

    #[tokio::test]
    async fn exec_with_stdin() {
        let sup = supervisor();
        let req = ExecRequest {
            command: vec!["cat".into()],
            cwd: None,
            env: BTreeMap::new(),
            stdin: Some(b"piped input".to_vec()),
            timeout: Some(Duration::from_secs(5)),
            tty: false,
            detach: false,
        };
        let result = sup.exec(&req, &PathBuf::from(".")).await.unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(String::from_utf8_lossy(&result.stdout), "piped input");
    }

    #[tokio::test]
    async fn exec_timeout() {
        let sup = supervisor();
        let req = ExecRequest {
            command: vec!["sleep".into(), "60".into()],
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            timeout: Some(Duration::from_millis(200)),
            tty: false,
            detach: false,
        };
        let result = sup.exec(&req, &PathBuf::from(".")).await.unwrap();
        assert!(result.timed_out);
        assert!(result.exit_code.is_none());
    }

    #[tokio::test]
    async fn exec_env_filtering() {
        // Only PATH is in allowlist; HOME should NOT be inherited
        let sup = ProcessSupervisor::new(vec!["PATH".into()], 1024 * 1024);
        let mut extra = BTreeMap::new();
        extra.insert("MY_VAR".into(), "my_value".into());

        let req = ExecRequest {
            command: vec!["bash".into(), "-c".into(), "echo $MY_VAR".into()],
            cwd: None,
            env: extra,
            stdin: None,
            timeout: Some(Duration::from_secs(5)),
            tty: false,
            detach: false,
        };
        let result = sup.exec(&req, &PathBuf::from(".")).await.unwrap();
        assert_eq!(
            String::from_utf8_lossy(&result.stdout).trim(),
            "my_value"
        );
    }

    #[tokio::test]
    async fn exec_empty_command_errors() {
        let sup = supervisor();
        let req = ExecRequest {
            command: vec![],
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            timeout: None,
            tty: false,
            detach: false,
        };
        assert!(sup.exec(&req, &PathBuf::from(".")).await.is_err());
    }

    #[tokio::test]
    async fn exec_stream_echo() {
        let sup = supervisor();
        let req = ExecRequest {
            command: vec!["echo".into(), "streaming".into()],
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            timeout: None,
            tty: false,
            detach: false,
        };
        let mut stream = sup.exec_stream(&req, &PathBuf::from(".")).await.unwrap();

        let mut saw_stdout = false;
        let mut saw_exit = false;
        while let Ok(Some(event)) = stream.next_event().await {
            match event {
                ExecEvent::Stdout(data) => {
                    assert!(String::from_utf8_lossy(&data).contains("streaming"));
                    saw_stdout = true;
                }
                ExecEvent::Exit { code } => {
                    assert_eq!(code, 0);
                    saw_exit = true;
                    break;
                }
                _ => {}
            }
        }
        assert!(saw_stdout);
        assert!(saw_exit);
    }
}
