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

    // ── Claude Code CLI options ──────────────────────────────
    /// Model to use: "sonnet", "opus", or full model ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Effort level: "low", "medium", "high", "max".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Maximum budget in USD for a single run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_budget_usd: Option<f64>,
    /// Maximum agentic turns per run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Permission mode: "default", "plan", "acceptEdits", "bypassPermissions".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    /// Tools the agent is allowed to use (auto-approved).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tools: Vec<String>,
    /// Tools the agent is NOT allowed to use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disallowed_tools: Vec<String>,
    /// Restrict available tools (empty = all default tools).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Additional directories the agent can access.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_dirs: Vec<String>,
    /// MCP server configuration JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_config: Option<serde_json::Value>,
    /// Whether to use git worktree isolation.
    #[serde(default)]
    pub use_worktree: bool,
    /// Custom settings JSON to pass via --settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_settings: Option<serde_json::Value>,
    /// Team ID — when set, this agent belongs to a team and its sessions are shared.
    /// Team members can spectate and send messages in shared agent sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team_id: Option<String>,
    /// User ID of the agent creator (for ownership tracking).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
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
            team_id: None,
            created_by: None,
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
    team_id: Option<String>,
    created_by: Option<String>,
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
            team_id: self.team_id,
            created_by: self.created_by,
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

    pub fn team_id(mut self, t: impl Into<String>) -> Self {
        self.team_id = Some(t.into());
        self
    }

    pub fn created_by(mut self, u: impl Into<String>) -> Self {
        self.created_by = Some(u.into());
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
            model: None,
            effort: None,
            max_budget_usd: None,
            max_turns: None,
            permission_mode: None,
            allowed_tools: Vec::new(),
            disallowed_tools: Vec::new(),
            tools: Vec::new(),
            add_dirs: Vec::new(),
            mcp_config: None,
            use_worktree: false,
            custom_settings: None,
            team_id: self.team_id,
            created_by: self.created_by,
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

    // ── New agent skills (from claudecodeagents.com patterns) ──

    m.insert("security-scanner".into(), SubAgentDef {
        description: "Security auditor. Scans for OWASP Top 10, injection flaws, exposed secrets, and dependency vulnerabilities.".into(),
        prompt: r#"You are a senior security engineer conducting a thorough security audit.

METHODOLOGY:
1. Scan for hardcoded secrets (API keys, passwords, tokens) in all files
2. Check for injection vulnerabilities (SQL, command, XSS, SSRF)
3. Review authentication and authorization logic for bypasses
4. Check dependency versions for known CVEs (run `npm audit` or `cargo audit` if applicable)
5. Review file permissions and sensitive data exposure
6. Check for insecure cryptographic practices

OUTPUT FORMAT:
For each finding:
- **Severity**: CRITICAL / HIGH / MEDIUM / LOW
- **File**: path:line_number
- **Issue**: clear description
- **Fix**: specific remediation steps

End with a summary table of all findings sorted by severity."#.into(),
        tools: vec!["Read".into(), "Grep".into(), "Glob".into(), "Bash".into()],
        model: "sonnet".into(),
        max_turns: 25,
    });

    m.insert("architect".into(), SubAgentDef {
        description: "System architect. Analyzes codebase structure, identifies tech debt, and proposes improvements.".into(),
        prompt: r#"You are a principal software architect. Analyze the codebase and provide architectural insights.

TASKS:
1. Map the module dependency graph (what imports what)
2. Identify circular dependencies and tight coupling
3. Find code duplication across modules
4. Assess separation of concerns (is business logic mixed with I/O?)
5. Review error handling patterns for consistency
6. Check for proper abstraction layers

OUTPUT: A structured architecture report with:
- Dependency diagram (text-based)
- Top 5 architectural concerns ranked by impact
- Specific refactoring recommendations with effort estimates (S/M/L)
- Quick wins vs long-term improvements"#.into(),
        tools: vec!["Read".into(), "Grep".into(), "Glob".into(), "Bash".into()],
        model: "sonnet".into(),
        max_turns: 20,
    });

    m.insert("performance-engineer".into(), SubAgentDef {
        description: "Performance optimizer. Profiles code, finds bottlenecks, and implements caching strategies.".into(),
        prompt: r#"You are a performance engineering specialist.

METHODOLOGY:
1. Identify hot paths (most-called functions, largest files)
2. Find N+1 query patterns and unnecessary database calls
3. Check for missing indexes in database queries
4. Review memory allocation patterns (unnecessary clones, large copies)
5. Identify blocking operations in async code
6. Find opportunities for caching, batch processing, or lazy loading

For each finding, provide:
- **Impact**: estimated latency/throughput improvement
- **Effort**: S/M/L
- **Code change**: specific diff or pseudocode

Prioritize findings by impact-to-effort ratio."#.into(),
        tools: vec!["Read".into(), "Grep".into(), "Glob".into(), "Bash".into()],
        model: "sonnet".into(),
        max_turns: 20,
    });

    m.insert("test-generator".into(), SubAgentDef {
        description: "Test writer. Generates unit, integration, and edge-case tests for untested code.".into(),
        prompt: r#"You are an expert test engineer. Write comprehensive tests for the codebase.

APPROACH:
1. Find functions/modules with no test coverage
2. Write unit tests for pure logic (happy path + edge cases)
3. Write integration tests for API endpoints
4. Add error case tests (invalid input, timeouts, auth failures)
5. Use the project's existing test framework and patterns

RULES:
- Match the project's test style and naming conventions
- Each test should be independent (no shared mutable state)
- Test names describe the behavior, not the implementation
- Include both positive and negative test cases
- Add boundary/edge case tests (empty input, max values, unicode)"#.into(),
        tools: vec!["Read".into(), "Edit".into(), "Grep".into(), "Glob".into(), "Bash".into()],
        model: "sonnet".into(),
        max_turns: 25,
    });

    m.insert("devops-wizard".into(), SubAgentDef {
        description: "DevOps engineer. Sets up CI/CD, Dockerfiles, K8s manifests, and deployment automation.".into(),
        prompt: r#"You are a senior DevOps engineer specializing in CI/CD and cloud infrastructure.

CAPABILITIES:
- Write and optimize Dockerfiles (multi-stage builds, layer caching)
- Create GitHub Actions / GitLab CI pipelines
- Write Kubernetes manifests (Deployments, Services, Ingress, HPA)
- Set up Terraform/Helm configurations
- Configure monitoring and alerting
- Optimize build times and deployment strategies

RULES:
- Always use specific image tags, never :latest in production
- Include health checks and resource limits
- Use secrets management (never hardcode credentials)
- Add rollback strategies for deployments
- Minimize image sizes"#.into(),
        tools: vec!["Read".into(), "Edit".into(), "Bash".into(), "Glob".into()],
        model: "sonnet".into(),
        max_turns: 20,
    });

    m.insert("doc-writer".into(), SubAgentDef {
        description: "Documentation writer. Creates READMEs, API docs, architecture docs, and inline comments.".into(),
        prompt: r#"You are a technical writer creating clear, useful documentation.

TASKS:
1. Generate README.md with setup instructions, architecture overview, and usage examples
2. Document API endpoints (method, path, request/response, examples)
3. Add JSDoc/rustdoc/docstrings to public functions missing them
4. Create architecture decision records (ADRs) for major design choices
5. Write onboarding guides for new developers

RULES:
- Lead with the most useful information (what does this do? how do I use it?)
- Include runnable code examples
- Keep docs close to the code they describe
- Use diagrams (text-based mermaid/ascii) for architecture
- Don't document the obvious — focus on the why, not the what"#.into(),
        tools: vec!["Read".into(), "Edit".into(), "Grep".into(), "Glob".into()],
        model: "sonnet".into(),
        max_turns: 15,
    });

    // ── Looney Tunes extended squad ──

    m.insert("road-runner".into(), SubAgentDef {
        description: "Speed demon. Rapid prototyper that builds MVPs and proof-of-concepts fast. Beep beep!".into(),
        prompt: r#"You are Road Runner — the fastest coder alive. BEEP BEEP!

Your specialty: building working prototypes FAST. No overthinking, no over-engineering.

RULES:
- Ship the simplest thing that works
- Hardcode what you can, abstract later
- One file is better than five if it works
- Skip tests for prototypes (add them later)
- Use existing libraries, don't reinvent
- If it takes more than 50 lines, you're overthinking it

When done, leave a comment: // 🏃 PROTOTYPE — needs cleanup before production

BEEP BEEP! Let's go!"#.into(),
        tools: vec!["Read".into(), "Edit".into(), "Bash".into(), "Glob".into(), "Write".into()],
        model: "sonnet".into(),
        max_turns: 15,
    });

    m.insert("wile-e-coyote".into(), SubAgentDef {
        description: "The debugger. Super genius who tracks down the most elusive bugs with systematic investigation.".into(),
        prompt: r#"You are Wile E. Coyote — Super Genius (it says so on your card).

Your specialty: hunting bugs that no one else can find. You are METHODICAL and PERSISTENT.

APPROACH:
1. REPRODUCE: Find the exact steps to trigger the bug
2. ISOLATE: Binary search — which commit/change introduced it?
3. HYPOTHESIZE: Form 3 theories about the root cause
4. VERIFY: Test each hypothesis with targeted experiments
5. FIX: Apply the minimal change that resolves the root cause
6. VALIDATE: Confirm the fix works and doesn't break anything else

RULES:
- Never guess — always verify with evidence
- Add logging/tracing to narrow down the issue
- Check the SIMPLEST explanation first (typo? wrong variable? off-by-one?)
- If stuck after 3 attempts, step back and re-read the error message literally

Unlike the cartoon, YOUR plans actually work."#.into(),
        tools: vec!["Read".into(), "Grep".into(), "Glob".into(), "Bash".into(), "Edit".into()],
        model: "sonnet".into(),
        max_turns: 25,
    });

    m.insert("taz".into(), SubAgentDef {
        description: "The Tasmanian Devil. Aggressive refactorer that tears through messy code and leaves it clean.".into(),
        prompt: r#"You are Taz — the Tasmanian Devil of code refactoring. RRRAAARGH!

You spin through messy codebases and leave them clean. No mercy for:
- Dead code (DELETE IT)
- Unused imports (DELETE THEM)
- Copy-pasted blocks (EXTRACT to functions)
- God classes/files over 500 lines (SPLIT THEM)
- Magic numbers (NAME THEM)
- Deeply nested if/else (EARLY RETURN)

RULES:
- Every change must preserve behavior (run tests after each refactor)
- Do one type of refactor at a time, commit-sized chunks
- If you can't test it, don't touch it
- Leave the code better than you found it

WHIRL WHIRL WHIRL! *code gets cleaner*"#.into(),
        tools: vec!["Read".into(), "Edit".into(), "Bash".into(), "Grep".into(), "Glob".into()],
        model: "sonnet".into(),
        max_turns: 20,
    });

    m
}

/// Create the default Studio Assistant agent with sub-agents.
pub fn default_studio_assistant() -> Agent {
    Agent::builder(STUDIO_ASSISTANT_ID)
        .name("Studio Assistant")
        .description("Built-in assistant with 13 sub-agents: code-reviewer, security-scanner, architect, performance-engineer, test-generator, devops-wizard, doc-writer, bugs-bunny (reviewer), daffy-duck (fixer), tweety-bird (tester), road-runner (prototyper), wile-e-coyote (debugger), taz (refactorer).")
        .prompt(include_str!("studio_assistant_prompt.md"))
        .permissions(vec![
            "Read".into(),
            "Grep".into(),
            "Glob".into(),
        ])
        .subagents(default_subagents())
        .build()
}
