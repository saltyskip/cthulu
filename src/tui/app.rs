use serde::Deserialize;

use super::claude_stream::{ClaudeEvent, ClaudeStream};

#[derive(Debug, Clone)]
pub enum Screen {
    FlowList,
    Session,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct FlowSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub node_count: usize,
    pub edge_count: usize,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct SessionInfo {
    pub flow_id: String,
    pub flow_name: String,
    pub prompt: String,
    pub permissions: Vec<String>,
    pub append_system_prompt: Option<String>,
    pub working_dir: String,
    pub sources_summary: String,
    pub sinks_summary: String,
}

#[derive(Debug, Clone)]
pub struct OutputLine {
    pub kind: OutputKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutputKind {
    System,
    Text,
    ToolUse,
    ToolResult,
    Result,
    Error,
    Cost,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Input,
    Output,
}

pub struct App {
    pub server_url: String,
    pub screen: Screen,
    pub should_quit: bool,

    // Flow list
    pub flows: Vec<FlowSummary>,
    pub flow_list_index: usize,
    pub flow_list_loading: bool,
    pub flow_list_error: Option<String>,

    // Session
    pub session: Option<SessionInfo>,
    pub session_loading: bool,

    // Input
    pub input: String,
    pub input_cursor: usize,
    pub focus: Focus,

    // Output
    pub output_lines: Vec<OutputLine>,
    pub output_scroll: usize,

    // Claude subprocess
    pub claude_stream: Option<ClaudeStream>,
    pub claude_running: bool,

    // HTTP client
    http_client: reqwest::Client,
}

impl App {
    pub fn new(server_url: String) -> Self {
        Self {
            server_url,
            screen: Screen::FlowList,
            should_quit: false,
            flows: Vec::new(),
            flow_list_index: 0,
            flow_list_loading: false,
            flow_list_error: None,
            session: None,
            session_loading: false,
            input: String::new(),
            input_cursor: 0,
            focus: Focus::Input,
            output_lines: Vec::new(),
            output_scroll: 0,
            claude_stream: None,
            claude_running: false,
            http_client: reqwest::Client::new(),
        }
    }

    // ── Flow List ───────────────────────────────────────────────

    pub async fn load_flows(&mut self) {
        self.flow_list_loading = true;
        self.flow_list_error = None;

        let url = format!("{}/api/flows", self.server_url);
        match self.http_client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(body) = resp.json::<serde_json::Value>().await {
                    if let Some(flows_arr) = body.get("flows") {
                        if let Ok(flows) = serde_json::from_value::<Vec<FlowSummary>>(flows_arr.clone()) {
                            self.flows = flows;
                            // Clamp index to new list length to prevent out-of-bounds panic
                            if !self.flows.is_empty() {
                                self.flow_list_index = self.flow_list_index.min(self.flows.len() - 1);
                            } else {
                                self.flow_list_index = 0;
                            }
                        }
                    }
                }
            }
            Err(e) => {
                self.flow_list_error = Some(format!("Failed to connect: {e}"));
            }
        }
        self.flow_list_loading = false;
    }

    pub fn flow_list_up(&mut self) {
        if self.flow_list_index > 0 {
            self.flow_list_index -= 1;
        }
    }

    pub fn flow_list_down(&mut self) {
        if !self.flows.is_empty() && self.flow_list_index < self.flows.len() - 1 {
            self.flow_list_index += 1;
        }
    }

    pub async fn select_flow_by_id(&mut self, id: &str) {
        // Try to find the flow in the loaded list, or load session directly
        if let Some(idx) = self.flows.iter().position(|f| f.id == id) {
            self.flow_list_index = idx;
        }
        self.enter_session_for_flow(id).await;
    }

    pub async fn enter_session(&mut self) {
        if self.flows.is_empty() {
            return;
        }
        let flow_id = self.flows[self.flow_list_index].id.clone();
        self.enter_session_for_flow(&flow_id).await;
    }

    async fn enter_session_for_flow(&mut self, flow_id: &str) {
        self.session_loading = true;
        self.screen = Screen::Session;
        self.output_lines.clear();
        self.output_scroll = 0;

        self.output_lines.push(OutputLine {
            kind: OutputKind::System,
            text: format!("Loading session for flow {}...", flow_id),
        });

        let url = format!("{}/api/flows/{}/session", self.server_url, flow_id);
        match self.http_client.get(&url).send().await {
            Ok(resp) => {
                if let Ok(session) = resp.json::<SessionInfo>().await {
                    self.output_lines.clear();
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::System,
                        text: format!("Flow: {}", session.flow_name),
                    });
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::System,
                        text: format!("Sources: {}", session.sources_summary),
                    });
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::System,
                        text: format!("Sinks: {}", session.sinks_summary),
                    });
                    let perms = if session.permissions.is_empty() {
                        "ALL (dangerously-skip-permissions)".to_string()
                    } else {
                        session.permissions.join(", ")
                    };
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::System,
                        text: format!("Permissions: {}", perms),
                    });
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::System,
                        text: format!("Working dir: {}", session.working_dir),
                    });
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::System,
                        text: "─".repeat(60),
                    });
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::System,
                        text: "Prompt pre-filled below. Edit and press Enter to send.".to_string(),
                    });

                    // Pre-fill the input with the rendered prompt
                    self.input = session.prompt.clone();
                    self.input_cursor = self.input.len();
                    self.session = Some(session);
                } else {
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::Error,
                        text: "Failed to parse session response".to_string(),
                    });
                }
            }
            Err(e) => {
                self.output_lines.push(OutputLine {
                    kind: OutputKind::Error,
                    text: format!("Failed to load session: {e}"),
                });
            }
        }

        self.session_loading = false;
    }

    pub fn leave_session(&mut self) {
        // Kill any running Claude process
        if let Some(stream) = self.claude_stream.take() {
            stream.kill();
        }
        self.claude_running = false;
        self.session = None;
        self.input.clear();
        self.input_cursor = 0;
        self.output_lines.clear();
        self.output_scroll = 0;
        self.screen = Screen::FlowList;
    }

    // ── Claude Session ──────────────────────────────────────────

    pub async fn send_prompt(&mut self) {
        if self.input.trim().is_empty() || self.claude_running {
            return;
        }

        let prompt = self.input.clone();
        self.input.clear();
        self.input_cursor = 0;

        // Show the sent prompt in output
        self.output_lines.push(OutputLine {
            kind: OutputKind::System,
            text: "─".repeat(60),
        });
        // Show a truncated version of what was sent
        let display_prompt = if prompt.len() > 200 {
            format!("{}...", &prompt[..200])
        } else {
            prompt.clone()
        };
        self.output_lines.push(OutputLine {
            kind: OutputKind::System,
            text: format!("▶ Sent prompt ({} chars): {}", prompt.len(), display_prompt),
        });

        // Build Claude args from session config
        let session = match &self.session {
            Some(s) => s.clone(),
            None => return,
        };

        match ClaudeStream::spawn(&prompt, &session) {
            Ok(stream) => {
                self.claude_stream = Some(stream);
                self.claude_running = true;
                self.output_lines.push(OutputLine {
                    kind: OutputKind::System,
                    text: "Claude Code session started...".to_string(),
                });
            }
            Err(e) => {
                self.output_lines.push(OutputLine {
                    kind: OutputKind::Error,
                    text: format!("Failed to start Claude: {e}"),
                });
            }
        }

        self.auto_scroll_output();
    }

    pub async fn poll_claude_events(&mut self) {
        // Collect events first to avoid borrow issues
        let events: Vec<ClaudeEvent> = {
            let stream = match &mut self.claude_stream {
                Some(s) => s,
                None => return,
            };
            let mut collected = Vec::new();
            while let Some(event) = stream.try_recv() {
                collected.push(event);
            }
            collected
        };

        if events.is_empty() {
            return;
        }

        let mut should_stop = false;

        for event in events {
            match event {
                ClaudeEvent::System(msg) => {
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::System,
                        text: format!("[system] {msg}"),
                    });
                }
                ClaudeEvent::Text(text) => {
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::Text,
                        text,
                    });
                }
                ClaudeEvent::ToolUse { tool, input } => {
                    let input_preview = if input.len() > 300 {
                        format!("{}...", &input[..300])
                    } else {
                        input
                    };
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::ToolUse,
                        text: format!("[tool] {tool}: {input_preview}"),
                    });
                }
                ClaudeEvent::ToolResult(text) => {
                    let preview = if text.len() > 500 {
                        format!("{}...", &text[..500])
                    } else {
                        text
                    };
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::ToolResult,
                        text: format!("[result] {preview}"),
                    });
                }
                ClaudeEvent::Result { text, cost, turns } => {
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::Result,
                        text,
                    });
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::Cost,
                        text: format!("Cost: ${cost:.4} | Turns: {turns}"),
                    });
                    should_stop = true;
                }
                ClaudeEvent::Error(e) => {
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::Error,
                        text: format!("[error] {e}"),
                    });
                    should_stop = true;
                }
                ClaudeEvent::Done(code) => {
                    self.output_lines.push(OutputLine {
                        kind: OutputKind::System,
                        text: format!("Process exited with code {code}"),
                    });
                    should_stop = true;
                }
            }
        }

        if should_stop {
            self.claude_running = false;
            self.claude_stream = None;
        }

        self.auto_scroll_output();
    }

    // ── Input helpers ───────────────────────────────────────────

    pub fn input_char(&mut self, c: char) {
        self.input.insert(self.input_cursor, c);
        self.input_cursor += c.len_utf8();
    }

    pub fn input_newline(&mut self) {
        self.input.insert(self.input_cursor, '\n');
        self.input_cursor += 1;
    }

    pub fn input_backspace(&mut self) {
        if self.input_cursor > 0 {
            // Find the previous char boundary
            let mut new_cursor = self.input_cursor - 1;
            while !self.input.is_char_boundary(new_cursor) {
                new_cursor -= 1;
            }
            self.input.remove(new_cursor);
            self.input_cursor = new_cursor;
        }
    }

    pub fn input_left(&mut self) {
        if self.input_cursor > 0 {
            let mut new_cursor = self.input_cursor - 1;
            while !self.input.is_char_boundary(new_cursor) {
                new_cursor -= 1;
            }
            self.input_cursor = new_cursor;
        }
    }

    pub fn input_right(&mut self) {
        if self.input_cursor < self.input.len() {
            let mut new_cursor = self.input_cursor + 1;
            while new_cursor < self.input.len() && !self.input.is_char_boundary(new_cursor) {
                new_cursor += 1;
            }
            self.input_cursor = new_cursor;
        }
    }

    pub fn input_cursor_line(&self) -> usize {
        self.input[..self.input_cursor].matches('\n').count()
    }

    // ── Output scroll ───────────────────────────────────────────

    pub fn scroll_output_up(&mut self) {
        self.output_scroll = self.output_scroll.saturating_sub(3);
    }

    pub fn scroll_output_down(&mut self) {
        self.output_scroll = self
            .output_scroll
            .saturating_add(3)
            .min(self.output_lines.len().saturating_sub(1));
    }

    fn auto_scroll_output(&mut self) {
        // Auto-scroll to bottom
        if self.output_lines.len() > 10 {
            self.output_scroll = self.output_lines.len().saturating_sub(10);
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Input => Focus::Output,
            Focus::Output => Focus::Input,
        };
    }
}
