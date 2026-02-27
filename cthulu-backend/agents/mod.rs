pub mod file_repository;
pub mod repository;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Well-known ID for the built-in Studio Assistant agent.
pub const STUDIO_ASSISTANT_ID: &str = "studio-assistant";

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
