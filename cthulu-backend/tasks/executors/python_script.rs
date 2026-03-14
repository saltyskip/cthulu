use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use super::{ExecutionResult, Executor, LineSink};

/// Default timeout for Python scripts: 5 minutes.
const DEFAULT_SCRIPT_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub struct PythonScriptExecutor {
    script: String,
    pub timeout: Duration,
}

impl PythonScriptExecutor {
    pub fn new(script: String, timeout_secs: Option<u64>) -> Self {
        Self {
            script,
            timeout: timeout_secs
                .map(Duration::from_secs)
                .unwrap_or(DEFAULT_SCRIPT_TIMEOUT),
        }
    }

    pub fn build_command(&self) -> (&str, Vec<&str>) {
        ("python3", vec!["-c", &self.script])
    }
}

#[async_trait]
impl Executor for PythonScriptExecutor {
    async fn execute(&self, prompt: &str, working_dir: &Path) -> Result<ExecutionResult> {
        self.execute_streaming(prompt, working_dir, None).await
    }

    async fn execute_streaming(
        &self,
        prompt: &str,
        working_dir: &Path,
        line_sink: Option<LineSink>,
    ) -> Result<ExecutionResult> {
        let mut child = Command::new("python3")
            .arg("-c")
            .arg(&self.script)
            .current_dir(working_dir)
            .env("CTHULU_INPUT", prompt)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn python3 process — is python3 installed?")?;

        // Write prompt/input to stdin then close it
        {
            let mut stdin = child.stdin.take().expect("stdin piped");
            let _ = stdin.write_all(prompt.as_bytes()).await;
            drop(stdin);
        }

        // Capture stderr
        let stderr = child.stderr.take().expect("stderr piped");
        let stderr_handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            let mut stderr_text = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.is_empty() {
                    tracing::debug!(source = "python-stderr", "{}", line);
                    stderr_text.push_str(&line);
                    stderr_text.push('\n');
                }
            }
            stderr_text
        });

        // Capture stdout, optionally stream lines
        let stdout = child.stdout.take().expect("stdout piped");
        let stdout_handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, BufReader};
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            let mut output = String::new();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(ref sink) = line_sink {
                    sink(line.clone());
                }
                output.push_str(&line);
                output.push('\n');
            }
            output
        });

        let status = match timeout(self.timeout, child.wait()).await {
            Ok(result) => result.context("failed to wait on python3 process")?,
            Err(_) => {
                tracing::error!(
                    "python script timed out after {}s, killing",
                    self.timeout.as_secs()
                );
                let _ = child.kill().await;
                stderr_handle.abort();
                stdout_handle.abort();
                anyhow::bail!(
                    "python script timed out after {}s",
                    self.timeout.as_secs()
                );
            }
        };

        let stderr_text = stderr_handle.await.unwrap_or_default();
        let stdout_text = stdout_handle.await.unwrap_or_default();

        if !status.success() {
            let code = status.code().unwrap_or(-1);
            anyhow::bail!(
                "python3 exited with code {code}\nstderr:\n{stderr_text}"
            );
        }

        Ok(ExecutionResult {
            text: stdout_text,
            cost_usd: 0.0,
            num_turns: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_build_command_basic() {
        let executor = PythonScriptExecutor::new("print('hello')".to_string(), None);
        let (program, args) = executor.build_command();
        assert_eq!(program, "python3");
        assert_eq!(args, vec!["-c", "print('hello')"]);
    }

    #[test]
    fn test_build_command_with_timeout() {
        let executor = PythonScriptExecutor::new("import time".to_string(), Some(30));
        assert_eq!(executor.timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_execute_hello_world() {
        let executor = PythonScriptExecutor::new("print('hello from python')".to_string(), None);
        let dir = PathBuf::from(".");
        let result = executor.execute("unused", &dir).await.unwrap();
        assert_eq!(result.text.trim(), "hello from python");
        assert_eq!(result.cost_usd, 0.0);
        assert_eq!(result.num_turns, 0);
    }

    #[tokio::test]
    async fn test_execute_reads_input_env() {
        let script = r#"
import os
data = os.environ.get('CTHULU_INPUT', '')
print(f'Got: {data}')
"#;
        let executor = PythonScriptExecutor::new(script.to_string(), None);
        let dir = PathBuf::from(".");
        let result = executor.execute("test input data", &dir).await.unwrap();
        assert!(result.text.contains("Got: test input data"));
    }

    #[tokio::test]
    async fn test_execute_failure() {
        let executor = PythonScriptExecutor::new("raise ValueError('boom')".to_string(), None);
        let dir = PathBuf::from(".");
        let result = executor.execute("", &dir).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_reads_stdin() {
        let script = r#"
import sys
data = sys.stdin.read()
print(f'stdin: {data}')
"#;
        let executor = PythonScriptExecutor::new(script.to_string(), None);
        let dir = PathBuf::from(".");
        let result = executor.execute("piped input", &dir).await.unwrap();
        assert!(result.text.contains("stdin: piped input"));
    }
}
