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
// Sub-agent definitions — passed to Claude Code via --agents flag
// ---------------------------------------------------------------------------

/// A sub-agent that can be delegated to within a session.
/// Matches Claude Code's native `--agents` JSON format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentDef {
    pub description: String,
    pub prompt: String,
    pub tools: Vec<String>,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_max_turns", rename = "maxTurns")]
    pub max_turns: u32,
}

fn default_model() -> String { "sonnet".into() }
fn default_max_turns() -> u32 { 10 }

/// Sub-agent map keyed by agent name (e.g. "bugs-bunny").
pub type SubAgents = HashMap<String, SubAgentDef>;

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
    /// Sub-agents available via Claude Code's native --agents flag.
    /// When non-empty, the CLI is spawned with `--agents <json>` so the
    /// parent session can delegate tasks to specialized sub-agents.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub subagents: SubAgents,
    /// If true, this agent is only usable as a sub-agent and is hidden from the UI agent list.
    #[serde(default)]
    pub subagent_only: bool,
    /// When set, this agent belongs to a team. All team members can access it.
    /// When None, it's a personal agent (existing behavior).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
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
            subagents: HashMap::new(),
            subagent_only: false,
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
    subagents: SubAgents,
    subagent_only: bool,
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
            subagents: self.subagents,
            subagent_only: self.subagent_only,
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

    pub fn subagents(mut self, s: SubAgents) -> Self {
        self.subagents = s;
        self
    }

    pub fn subagent_only(mut self, v: bool) -> Self {
        self.subagent_only = v;
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
            subagents: self.subagents,
            subagent_only: self.subagent_only,
            team_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}

/// Build the default sub-agent definitions for the Studio Assistant.
/// These are passed to Claude Code via `--agents` so the parent session
/// can delegate tasks to specialized sub-agents natively.
pub fn default_subagents() -> SubAgents {
    let mut m = SubAgents::new();

    m.insert("code-reviewer".into(), SubAgentDef {
        description: "Structured code reviewer. Finds bugs, security issues, regressions with severity tagging.".into(),
        prompt: include_str!("code_reviewer_prompt.md").into(),
        tools: vec!["Read".into(), "Grep".into(), "Glob".into(), "Bash".into()],
        model: "sonnet".into(),
        max_turns: 20,
    });

    m.insert("bugs-bunny".into(), SubAgentDef {
        description: "Personality-driven code reviewer. Finds bugs, code smells, and logic errors with read-only access.".into(),
        prompt: include_str!("bugs_bunny_prompt.md").into(),
        tools: vec!["Read".into(), "Grep".into(), "Glob".into()],
        model: "sonnet".into(),
        max_turns: 10,
    });

    m.insert("daffy-duck".into(), SubAgentDef {
        description: "Bug fixer. Takes review findings and fixes them with targeted edits.".into(),
        prompt: include_str!("daffy_duck_prompt.md").into(),
        tools: vec!["Read".into(), "Edit".into(), "Grep".into(), "Glob".into()],
        model: "sonnet".into(),
        max_turns: 15,
    });

    m.insert("tweety-bird".into(), SubAgentDef {
        description: "Test runner. Runs tests, reports results, identifies coverage gaps.".into(),
        prompt: include_str!("tweety_bird_prompt.md").into(),
        tools: vec!["Read".into(), "Bash".into(), "Glob".into()],
        model: "sonnet".into(),
        max_turns: 10,
    });

    m
}

/// Create the default Studio Assistant agent with sub-agents.
pub fn default_studio_assistant() -> Agent {
    Agent::builder(STUDIO_ASSISTANT_ID)
        .name("Studio Assistant")
        .description("Built-in assistant for flow editing and Studio help. Delegates to sub-agents: code-reviewer, bugs-bunny (reviewer), daffy-duck (fixer), tweety-bird (tester).")
        .prompt(include_str!("studio_assistant_prompt.md"))
        .permissions(vec![
            "Read".into(),
            "Grep".into(),
            "Glob".into(),
        ])
        .subagents(default_subagents())
        .build()
}
