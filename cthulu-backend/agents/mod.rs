pub mod file_repository;
pub mod repository;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Well-known ID for the built-in Studio Assistant agent.
pub const STUDIO_ASSISTANT_ID: &str = "studio-assistant";

// ---------------------------------------------------------------------------
// Hook types — mirrors Claude Code's settings.json hook schema
// ---------------------------------------------------------------------------

/// A single hook handler. Either an HTTP callback (used by the Cthulu server)
/// or a shell command (used for CLI-spawned agents).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AgentHook {
    Http {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout: Option<u32>,
    },
    Command {
        command: String,
    },
}

/// A group of hooks that share a tool-name matcher regex.
/// Matches the Claude Code 3-level structure: event -> matcher group -> handlers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHookGroup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    pub hooks: Vec<AgentHook>,
}

/// Per-agent hook configuration keyed by Claude Code event name.
/// Valid event names: PreToolUse, PostToolUse, Stop, SessionStart,
/// UserPromptSubmit, PermissionRequest, PreCompact, PostToolUseFailure.
pub type AgentHooks = HashMap<String, Vec<AgentHookGroup>>;

// ---------------------------------------------------------------------------
// Agent
// ---------------------------------------------------------------------------

/// A reusable agent definition — owns the "what" (prompt, permissions, personality).
/// The execution environment ("where") stays on the executor node config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Inline prompt text or path to a .md file.
    pub prompt: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append_system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    /// Per-agent hooks merged into .claude/settings.local.json alongside system hooks.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub hooks: AgentHooks,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Agent {
    /// Start building an agent. The `id` is required up-front; call `.name()` next.
    pub fn builder(id: impl Into<String>) -> AgentBuilder<NeedsName> {
        AgentBuilder {
            id: id.into(),
            name: String::new(),
            description: String::new(),
            prompt: String::new(),
            permissions: Vec::new(),
            append_system_prompt: None,
            working_dir: None,
            hooks: HashMap::new(),
            _state: std::marker::PhantomData,
        }
    }
}

// --- Typestate markers ---

pub struct NeedsName;
pub struct Ready;

pub struct AgentBuilder<State = NeedsName> {
    id: String,
    name: String,
    description: String,
    prompt: String,
    permissions: Vec<String>,
    append_system_prompt: Option<String>,
    working_dir: Option<String>,
    hooks: AgentHooks,
    _state: std::marker::PhantomData<State>,
}

impl AgentBuilder<NeedsName> {
    /// Set the agent name (required). Transitions to `Ready` state.
    pub fn name(self, name: impl Into<String>) -> AgentBuilder<Ready> {
        AgentBuilder {
            id: self.id,
            name: name.into(),
            description: self.description,
            prompt: self.prompt,
            permissions: self.permissions,
            append_system_prompt: self.append_system_prompt,
            working_dir: self.working_dir,
            hooks: self.hooks,
            _state: std::marker::PhantomData,
        }
    }
}

impl<S> AgentBuilder<S> {
    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = d.into();
        self
    }

    pub fn prompt(mut self, p: impl Into<String>) -> Self {
        self.prompt = p.into();
        self
    }

    pub fn permissions(mut self, p: Vec<String>) -> Self {
        self.permissions = p;
        self
    }

    pub fn append_system_prompt(mut self, s: impl Into<String>) -> Self {
        self.append_system_prompt = Some(s.into());
        self
    }

    pub fn working_dir(mut self, w: impl Into<String>) -> Self {
        self.working_dir = Some(w.into());
        self
    }

    pub fn hooks(mut self, h: AgentHooks) -> Self {
        self.hooks = h;
        self
    }
}

impl AgentBuilder<Ready> {
    /// Consume the builder and produce an `Agent`. Sets `created_at` and `updated_at` to now.
    pub fn build(self) -> Agent {
        let now = Utc::now();
        Agent {
            id: self.id,
            name: self.name,
            description: self.description,
            prompt: self.prompt,
            permissions: self.permissions,
            append_system_prompt: self.append_system_prompt,
            working_dir: self.working_dir,
            hooks: self.hooks,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Create the default Studio Assistant agent.
pub fn default_studio_assistant() -> Agent {
    Agent::builder(STUDIO_ASSISTANT_ID)
        .name("Studio Assistant")
        .description("Built-in assistant for flow editing and Studio help")
        .prompt(include_str!("studio_assistant_prompt.md"))
        .permissions(vec![
            "Read".into(),
            "Grep".into(),
            "Glob".into(),
        ])
        .build()
}

pub const BUGS_BUNNY_ID: &str = "bugs-bunny";
pub const DAFFY_DUCK_ID: &str = "daffy-duck";
pub const TWEETY_BIRD_ID: &str = "tweety-bird";

pub fn default_bugs_bunny() -> Agent {
    // Bugs Bunny is read-only — PreToolUse denies Edit/Write as defense-in-depth
    // (permissions already restrict him, but hooks provide belt-and-suspenders safety).
    let mut hooks = AgentHooks::new();
    hooks.insert("PreToolUse".into(), vec![
        AgentHookGroup {
            matcher: Some("Edit|Write|MultiEdit|NotebookEdit".into()),
            hooks: vec![AgentHook::Command {
                command: r#"echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Bugs Bunny is read-only, doc. No editing allowed!"}}';"#.into(),
            }],
        },
    ]);

    Agent::builder(BUGS_BUNNY_ID)
        .name("Bugs Bunny")
        .description("Code reviewer. Finds bugs, code smells, and logic errors.")
        .prompt(include_str!("bugs_bunny_prompt.md"))
        .permissions(vec!["Read".into(), "Grep".into(), "Glob".into()])
        .hooks(hooks)
        .build()
}

pub fn default_daffy_duck() -> Agent {
    // Daffy Duck can Edit but not Write (no new files — only fix existing code).
    let mut hooks = AgentHooks::new();
    hooks.insert("PreToolUse".into(), vec![
        AgentHookGroup {
            matcher: Some("Write".into()),
            hooks: vec![AgentHook::Command {
                command: r#"echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Thuffering thuccotash! Daffy only fixes existing files, no new ones!"}}';"#.into(),
            }],
        },
    ]);

    Agent::builder(DAFFY_DUCK_ID)
        .name("Daffy Duck")
        .description("Bug fixer. Takes review findings and fixes them.")
        .prompt(include_str!("daffy_duck_prompt.md"))
        .permissions(vec!["Read".into(), "Edit".into(), "Grep".into(), "Glob".into()])
        .hooks(hooks)
        .build()
}

pub fn default_tweety_bird() -> Agent {
    // Tweety Bird runs tests — no file-editing hooks needed.
    // Guard against dangerous bash commands as a safety measure.
    let mut hooks = AgentHooks::new();
    hooks.insert("PreToolUse".into(), vec![
        AgentHookGroup {
            matcher: Some("Bash".into()),
            hooks: vec![AgentHook::Command {
                // Block destructive commands: rm -rf, git push --force, etc.
                command: concat!(
                    r#"INPUT=$(cat); CMD=$(echo "$INPUT" | "#,
                    r#"grep -oP '"command"\s*:\s*"[^"]*"' | head -1 | sed 's/.*"command"\s*:\s*"//;s/"$//'); "#,
                    r#"if echo "$CMD" | grep -qiE '(rm\s+-rf\s+/|git\s+push\s+--force|DROP\s+TABLE|format\s+c:)'; then "#,
                    r#"echo '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"Bad puddy tat! That command is too dangerous!"}}'; "#,
                    r#"fi"#,
                ).into(),
            }],
        },
    ]);

    Agent::builder(TWEETY_BIRD_ID)
        .name("Tweety Bird")
        .description("Test runner. Runs tests, reports results, identifies coverage gaps.")
        .prompt(include_str!("tweety_bird_prompt.md"))
        .permissions(vec!["Read".into(), "Bash".into(), "Glob".into()])
        .hooks(hooks)
        .build()
}
