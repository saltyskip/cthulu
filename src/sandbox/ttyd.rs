//! ttyd WebSocket command execution engine.
//!
//! Provides a reusable `TtydSession` that connects to a ttyd web terminal
//! via WebSocket and can send commands and capture their output.
//!
//! Protocol:
//!   - Subprotocol: "tty"
//!   - Handshake: JSON `{"AuthToken": "...", "columns": N, "rows": N}`
//!   - INPUT frame:  Binary `[0x30] + command_bytes`   (byte '0' prefix)
//!   - OUTPUT frame: Binary `[0x30] + output_bytes`    (byte '0' prefix)
//!
//! Each command is bookended with a unique marker so we can detect when
//! the command has finished and extract just its output.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use crate::sandbox::error::SandboxError;

/// A live session to a ttyd web terminal over WebSocket.
pub struct TtydSession {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl TtydSession {
    /// Connect to a ttyd web terminal and wait for the shell prompt.
    ///
    /// `web_terminal_url` should be the HTTP URL (e.g., `http://host:7700`).
    pub async fn connect(web_terminal_url: &str) -> Result<Self, SandboxError> {
        let http_url = reqwest::Url::parse(web_terminal_url)
            .map_err(|e| SandboxError::Backend(format!("invalid web_terminal URL: {e}")))?;
        let ws_scheme = if http_url.scheme() == "https" {
            "wss"
        } else {
            "ws"
        };
        let ws_url = format!(
            "{}://{}:{}/ws",
            ws_scheme,
            http_url.host_str().unwrap_or("localhost"),
            http_url.port_or_known_default().unwrap_or(7681)
        );

        // Fetch ttyd auth token
        let auth_token = {
            let token_url = format!("{}/token", web_terminal_url.trim_end_matches('/'));
            match reqwest::get(&token_url).await {
                Ok(resp) if resp.status().is_success() => resp
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| v["token"].as_str().map(String::from))
                    .unwrap_or_default(),
                _ => String::new(),
            }
        };

        // Build WebSocket request with 'tty' subprotocol
        let request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&ws_url)
            .header("Sec-WebSocket-Protocol", "tty")
            .header("Host", http_url.host_str().unwrap_or("localhost"))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| SandboxError::Backend(format!("failed to build WS request: {e}")))?;

        let (mut ws, _resp) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| SandboxError::Backend(format!("WebSocket connect failed: {e}")))?;

        // Send handshake
        let handshake = serde_json::json!({
            "AuthToken": auth_token,
            "columns": 200,
            "rows": 50,
        });
        ws.send(Message::Text(handshake.to_string().into()))
            .await
            .map_err(|e| SandboxError::Backend(format!("WS handshake send failed: {e}")))?;

        // Wait for shell prompt (up to 10 seconds)
        let _ = tokio::time::timeout(Duration::from_secs(10), async {
            let mut buf = String::new();
            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Binary(data) = msg {
                    if !data.is_empty() && data[0] == b'0' {
                        if let Ok(text) = std::str::from_utf8(&data[1..]) {
                            buf.push_str(text);
                        }
                        if buf.contains('$') || buf.contains('#') || buf.contains("root@") {
                            return;
                        }
                    }
                }
            }
        })
        .await;

        Ok(Self { ws })
    }

    /// Send a shell command and wait for a unique marker in the output.
    ///
    /// Returns the full terminal output captured between sending the command
    /// and seeing the marker. The marker line itself is stripped from the result.
    ///
    /// The command is automatically wrapped with a unique end-marker:
    /// ```sh
    /// <your_command>; echo '__TTYD_DONE_<uuid>__'
    /// ```
    pub async fn exec(
        &mut self,
        command: &str,
        timeout: Duration,
    ) -> Result<String, SandboxError> {
        let marker = format!("__TTYD_DONE_{}__", &uuid::Uuid::new_v4().to_string()[..8]);

        // Send command first, then marker echo as a separate line.
        // This prevents the terminal from echoing the marker as part of
        // the command line itself (which would cause premature detection).
        let full_cmd = format!("{}\n", command);
        let marker_cmd = format!("echo '{}'\n", marker);

        // Send the actual command
        let mut frame = Vec::with_capacity(1 + full_cmd.len());
        frame.push(b'0');
        frame.extend_from_slice(full_cmd.as_bytes());
        self.ws
            .send(Message::Binary(frame.into()))
            .await
            .map_err(|e| SandboxError::Backend(format!("WS send failed: {e}")))?;

        // Small delay to let the command start before sending the marker
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Send the marker echo as a separate command — it will execute
        // after the main command finishes (bash executes sequentially).
        let mut marker_frame = Vec::with_capacity(1 + marker_cmd.len());
        marker_frame.push(b'0');
        marker_frame.extend_from_slice(marker_cmd.as_bytes());
        self.ws
            .send(Message::Binary(marker_frame.into()))
            .await
            .map_err(|e| SandboxError::Backend(format!("WS send marker failed: {e}")))?;

        // Collect output until the marker appears in OUTPUT (not in the echo of the command).
        // We count occurrences: the marker will appear once in the echo of `echo 'marker'`,
        // and once in the actual output. We wait for the second occurrence.
        let result = tokio::time::timeout(timeout, async {
            let mut output = String::new();
            while let Some(Ok(msg)) = self.ws.next().await {
                if let Message::Binary(data) = msg {
                    if !data.is_empty() && data[0] == b'0' {
                        if let Ok(text) = std::str::from_utf8(&data[1..]) {
                            output.push_str(text);
                        }
                        // The marker appears twice: once in the echoed command line,
                        // once as actual output from `echo`. We wait for at least 2 occurrences.
                        let count = output.matches(&marker).count();
                        if count >= 2 {
                            return Ok::<String, SandboxError>(output);
                        }
                    }
                }
            }
            // WebSocket closed before marker
            Err(SandboxError::Backend(
                "ttyd WebSocket closed before command completed".into(),
            ))
        })
        .await
        .map_err(|_| {
            SandboxError::Backend(format!("ttyd command timed out after {}s", timeout.as_secs()))
        })??;

        // Strip everything before the first newline after the echoed command
        // and the marker line from the end
        let cleaned = strip_terminal_noise(&result, command, &marker);
        Ok(cleaned)
    }

    /// Send a command with a specific marker string (no wrapping).
    /// Use this when you've already embedded the marker in your command.
    pub async fn exec_with_marker(
        &mut self,
        command: &str,
        marker: &str,
        timeout: Duration,
    ) -> Result<String, SandboxError> {
        // Send as INPUT frame
        let full_cmd = format!("{}\n", command);
        let mut frame = Vec::with_capacity(1 + full_cmd.len());
        frame.push(b'0');
        frame.extend_from_slice(full_cmd.as_bytes());
        self.ws
            .send(Message::Binary(frame.into()))
            .await
            .map_err(|e| SandboxError::Backend(format!("WS send failed: {e}")))?;

        // Collect output until marker
        let result = tokio::time::timeout(timeout, async {
            let mut output = String::new();
            while let Some(Ok(msg)) = self.ws.next().await {
                if let Message::Binary(data) = msg {
                    if !data.is_empty() && data[0] == b'0' {
                        if let Ok(text) = std::str::from_utf8(&data[1..]) {
                            output.push_str(text);
                        }
                        if output.contains(marker) {
                            return Ok::<String, SandboxError>(output);
                        }
                    }
                }
            }
            Err(SandboxError::Backend(
                "ttyd WebSocket closed before marker detected".into(),
            ))
        })
        .await
        .map_err(|_| {
            SandboxError::Backend(format!("ttyd command timed out after {}s", timeout.as_secs()))
        })??;

        Ok(result)
    }

    /// Close the WebSocket session.
    pub async fn close(mut self) {
        let _ = self.ws.close(None).await;
    }
}

/// Strip terminal noise from captured output.
///
/// Terminal output includes: ANSI escape codes, the echoed command itself,
/// prompt strings, and the marker. We do best-effort cleaning:
/// 1. Remove ANSI escape sequences
/// 2. Remove lines containing the original command echo
/// 3. Remove lines containing the marker
/// 4. Trim whitespace
fn strip_terminal_noise(raw: &str, command_hint: &str, marker: &str) -> String {
    let no_ansi = strip_ansi_escapes(raw);

    // Take the first significant portion of the command for matching
    let cmd_hint = command_hint
        .split(';')
        .next()
        .unwrap_or(command_hint)
        .trim();
    let cmd_short = if cmd_hint.len() > 30 {
        &cmd_hint[..30]
    } else {
        cmd_hint
    };

    let lines: Vec<&str> = no_ansi.lines().collect();
    let filtered: Vec<&str> = lines
        .into_iter()
        .filter(|line| {
            let trimmed = line.trim();
            // Skip empty lines
            if trimmed.is_empty() {
                return false;
            }
            // Skip lines that contain the marker or any stale marker from previous commands
            if trimmed.contains(marker) || trimmed.contains("__TTYD_DONE_") {
                return false;
            }
            // Skip lines that look like our echoed command
            // (terminal echoes the command after the prompt, e.g., "root@fc:~# claude ...")
            if !cmd_short.is_empty() && trimmed.contains(cmd_short) {
                // Only filter if it looks like a prompt line (contains @ or $ or # before the command)
                if trimmed.contains('@') || trimmed.starts_with('$') || trimmed.starts_with('#') {
                    return false;
                }
            }
            // Skip bare prompts (e.g., "root@host:~#", "$ ")
            if trimmed.ends_with('#') || trimmed.ends_with('$') {
                let content = trimmed.trim_end_matches(['#', '$']).trim();
                if content.is_empty() || content.contains("root@") || content.contains("@fc-") {
                    return false;
                }
            }
            true
        })
        .collect();

    filtered.join("\n").trim().to_string()
}

/// Strip ANSI escape sequences from a string (no regex dependency).
fn strip_ansi_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // CSI: ESC [
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Consume parameter bytes (0x30-0x3F) and intermediate bytes (0x20-0x2F)
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_digit() || c == ';' || c == '?' || (c >= ' ' && c <= '/') {
                        chars.next();
                    } else {
                        break;
                    }
                }
                // Consume final byte (0x40-0x7E)
                if let Some(&c) = chars.peek() {
                    if c >= '@' && c <= '~' {
                        chars.next();
                    }
                }
            }
            // OSC: ESC ]
            else if chars.peek() == Some(&']') {
                chars.next();
                // Consume until BEL (0x07) or ST (ESC \)
                while let Some(c) = chars.next() {
                    if c == '\x07' {
                        break;
                    }
                    if c == '\x1b' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
            }
            // Other ESC sequences (e.g., ESC ( A, ESC ) B): consume next byte
            else {
                chars.next();
            }
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_noise_basic() {
        let raw = "root@fc:~# echo hello; echo '__TTYD_DONE_abc__'\nhello\n__TTYD_DONE_abc__\nroot@fc:~#";
        let cleaned = strip_terminal_noise(raw, "echo hello", "__TTYD_DONE_abc__");
        assert_eq!(cleaned, "hello");
    }

    #[test]
    fn strip_noise_ansi() {
        let raw = "\x1b[32mhello\x1b[0m\n__TTYD_DONE_xyz__";
        let cleaned = strip_terminal_noise(raw, "echo hello", "__TTYD_DONE_xyz__");
        assert_eq!(cleaned, "hello");
    }

    #[test]
    fn strip_noise_empty() {
        let raw = "__TTYD_DONE_abc__\nroot@fc:~#";
        let cleaned = strip_terminal_noise(raw, "some_cmd", "__TTYD_DONE_abc__");
        assert_eq!(cleaned, "");
    }
}
