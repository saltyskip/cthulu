# Agent Design Guide for Cthulu

Practical application of the highest-impact concepts from 65 years of AI
planning research (1961-2026) to Cthulu's flow runner and executor architecture.

This is not academic — every recommendation maps to a specific file, struct, or
runtime behavior in the codebase.

---

> **Note on code references**: This document references specific components in the Cthulu codebase. For maintainability, it uses structural location markers (like `// ── 1. SOURCES`) rather than hardcoded line numbers where possible, as line numbers frequently go stale.

## The 5 Pillars

These account for ~80% of agent efficiency gains. Everything else is either
already built into modern LLMs (Chain-of-Thought) or has poor ROI for coding
tasks (Tree-of-Thought, PDDL).

| # | Pillar | Source Concept | Cthulu Implementation |
|---|--------|---------------|----------------------|
| 1 | Layered Context | Context Engineering (Anthropic 2025) | `AGENT.md` + `Skill.md` + `workflow.json` + attachments |
| 2 | Explicit Contracts | STRIPS pre/postconditions (1971) | Prompt template structure, `CLAUDE.md` rules |
| 3 | Hierarchical Decomposition | HTN Planning (1994) + WBS (1962) | Flow DAG: Trigger -> Source -> Filter -> Executor -> Sink |
| 4 | Verified Execution Loop | ReAct (2022) + OODA (1976) | `execute_inner()` 5-stage pipeline |
| 5 | Bounded Retry with Reflection | Reflexion (2023) + CSP (1977) | `NOPE.md`, `LESSONS.md`, error handling per stage |

---

## 1. Layered Context

> *"A well-structured context window does more for agent efficiency than any
> amount of 'think step by step.'"*

### The three layers

Cthulu's runtime generates three context layers for every executor agent,
assembled by `node_chat.rs:build_workflow_context_md()`:

**Instructional layer** (`AGENT.md` — copied to `.skills/AGENT.md`):
- Agent identity and role (pipeline processor, not general assistant)
- Scope boundaries (NOPE list — what NOT to do)
- Output format constraints (Slack Block Kit, Notion blocks)
- Efficiency rules (no preamble, batch tool calls, data density)

**Knowledge layer** (`Skill.md` — generated per-node):
- Pipeline position (upstream sources, downstream sinks, peer executors)
- Node configuration (cron schedule, RSS URLs, Slack channels)
- E01/E02 numbering for multi-executor flows
- User-uploaded training files synced to `.skills/`

**Tool layer** (template variables + `workflow.json`):
- `{{content}}`, `{{diff}}`, `{{market_data}}` (market data source) — guaranteed input data
- `{{repo}}`, `{{pr_number}}`, `{{head_sha}}` — GitHub context
- Available CLI tools (`gh`, `claude`, filesystem access)
- Sink format constraints (Slack 3000-char limit, Notion block types)
- `workflow.json` contains the full flow definition and complete configuration, enabling the executor to understand its broader pipeline context beyond just the immediately adjacent nodes.

### How to apply this

When designing a new flow:

1. **Instructional**: The agent already gets `AGENT.md`. If the task needs
   domain-specific rules, add them to the prompt template file — not inline.
   File-based prompts (`prompt_file`) are preferred over inline prompts
   (`prompt`) because they're versioned, reviewable, and reusable.

2. **Knowledge**: Upload training files through the Studio UI. They land in
   `.skills/` alongside the generated context. For code review flows, this
   means: style guides, architecture decision records, coding standards.

3. **Tool**: Use template variables to inject data rather than asking the agent
   to fetch it. Sources already do the fetching — the agent should transform
   and analyze, not gather.

### The 4 context strategies

| Strategy | When | Example |
|----------|------|---------|
| **Write** | Context doesn't exist yet | Create `AGENT.md`, skill files, prompt templates |
| **Select** | Too much context available | Filters narrow sources; `Skill.md` shows only relevant pipeline nodes |
| **Compress** | Context exists but is too long | `format_items()` truncates summaries to 500 chars; diffs are chunked |
| **Isolate** | Parallel agents need separate context | Each executor node gets its own `.skills/` directory and Claude process |

---

## 2. Contract-Driven Task Design

> *From STRIPS (Fikes & Nilsson, 1971): Every action has explicit preconditions
> and postconditions. Postconditions are cheaper than retries.*

### Contracts per pipeline stage

Each stage in `execute_inner()` has implicit contracts. Making them explicit
prevents silent failures:

**Sources** (near `// ── 1. SOURCES`):
```
Precondition: Network accessible, API tokens configured (env vars set)
Action:       Fetch content items in parallel
Postcondition: Vec<ContentItem> with at least title and URL populated
Failure mode: Log warning, continue with empty items (best-effort)
```

**Filters** (near `// ── 2. FILTERS`):
```
Precondition: Vec<ContentItem> from sources
Action:       Apply keyword matching (any/all, title/summary/both)
Postcondition: Filtered vec, possibly empty
Failure mode: None (deterministic, cannot fail)
```

**Prompt rendering** (near `// ── 3. PROMPT RENDERING`):
```
Precondition: Prompt template exists (file or inline), content items available
Action:       Substitute {{variables}}, append content if no {{content}} placeholder
Postcondition: Fully rendered prompt string, no unresolved {{variables}}
Failure mode: Fail the run (missing prompt is unrecoverable)
```

**Executor** (near `// ── 4. EXECUTOR`):
```
Precondition: Rendered prompt, working_dir exists, executor binary available (Claude CLI or sandbox environment)
Action:       Spawn process (e.g. `ClaudeCodeExecutor` or `SandboxExecutor`), pipe prompt to stdin, collect output
Postcondition: ExecutionResult with text, cost_usd, num_turns
Failure mode: 15-min timeout -> fail run; process error -> fail run
```

**Sinks** (near `// ── 5. SINKS`):
```
Precondition: ExecutionResult.text is non-empty, sink credentials configured
Action:       Deliver to all sinks sequentially
Postcondition: All sinks acknowledge delivery
Failure mode: Log error, continue to next sink (best-effort)
```

### How to apply this in prompt templates

The `examples/prompts/pr_review.md` is an exemplar of contract-driven design:

```markdown
## Process                              <- Ordered steps (HTN decomposition)
1. Read the PR diff                     <- Precondition: diff is available
2. Read changed files for context       <- Action: gather knowledge
3. Identify bugs, security issues...    <- Action: analyze
4. Post review to GitHub using `gh`     <- Postcondition: review exists on GitHub

## Scope Rules                          <- Constraints (CSP)
- ONLY explore changed files
- Do NOT explore build artifacts
- Do NOT search git history
```

Every prompt template should include:
- **Ordered process steps** with clear verbs (read, identify, post)
- **Scope boundaries** (what NOT to do — prevents token waste)
- **Output format** (exact structure the sink expects)
- **Verification action** (post the review, then approve/request-changes)

---

## 3. Hierarchical Decomposition

> *From HTN Planning (Erol, Hendler & Nau, 1994): Every goal decomposes into
> methods, which decompose into primitive actions. The agent cannot skip levels.*

### Mapping HTN to Cthulu's DAG

```
        GOAL (Flow)
       "Review new PRs and post feedback to GitHub"
              |
    METHOD (Node Chain)
    Trigger -> Source -> Filter -> Executor -> Sink
              |
    PRIMITIVE ACTIONS (Individual Node Execution)
    - cron fires every 30 min
    - github-merged-prs fetches open PRs
    - keyword filter selects relevant ones
    - Claude analyzes diff, posts gh review
    - Slack sink notifies the team
```

### Decomposition depth

Match decomposition depth to agent capability:

| Too shallow | Right depth | Too deep |
|------------|-------------|----------|
| "Review PRs" | "Read diff, check for bugs/security/perf, post review with inline comments, then approve or request changes" | "Open file X at line Y, check if variable Z is properly bounds-checked, if not write a comment object with path=X, line=Y, body=..." |

The sweet spot: **decompose until each leaf is something the agent can do in
one tool-call cycle**. For Claude Code, that's roughly "edit this file" or
"run this command" — not pseudocode-level primitives.

### When to use multiple executors

Use multi-executor flows (E01, E02) when:
- Tasks require **different skills** (code review vs summarization)
- Tasks can run **independently** (no data dependency between executors)
- Output goes to **different sinks** (Slack summary + Notion detailed report)

**Note on execution mode**: Multi-executor flows currently execute sequentially, resolving configurations and triggering each executor node one after the other within the single run.

Use a single executor when:
- The task is **sequential** (analyze then act)
- Output is **unified** (one Slack message)
- Context needs to be **shared** (analysis informs the action)

---

## 4. Verified Execution Loop

> *From ReAct (Yao et al., 2022): Interleave Thought -> Action -> Observation.
> From OODA (Boyd, 1976): Never skip the Orient phase.*

### How `execute_inner()` maps to OODA

```
Stage 1 — OBSERVE:  Sources fetch external data (RSS, GitHub, web)
Stage 2 — ORIENT:   Filters contextualize (keyword match, relevance)
                     Prompt rendering adds structure (template + variables)
Stage 3 — DECIDE:   Executor agent reasons about the task
Stage 4 — ACT:      Executor agent produces output
Stage 5 — VERIFY:   Sinks deliver + postconditions checked
    |
    └─── Loop back? No — Cthulu flows are single-pass by design.
         The next trigger fires the next loop iteration.
```

The critical insight: **stages 1-2 (Observe + Orient) are done FOR the agent,
not BY the agent**. Sources and filters run before the executor sees anything.
This is by design — the agent should transform and analyze, not gather.

### Building verification into prompts

Don't rely on the agent to verify itself — build verification into the task:

**Weak** (agent decides when it's done):
```
Review this PR and post feedback.
```

**Strong** (verification is an explicit step):
```
1. Read the diff
2. Analyze for bugs and security issues
3. Post inline comments via `gh api`
4. Submit official review via `gh pr review`
   <- This step proves the review was posted
```

In `pr_review.md`, the "Posting Your Review" section IS the verification — the
act of posting to GitHub confirms the review was completed. The sink (Slack
notification) then confirms delivery to the team.

### The Definition of Done

Every flow should have an implicit DoD. For automated flows it's built into the
pipeline: sources returned data + executor produced output + sinks delivered.
For interactive (chat) sessions, the DoD lives in the prompt template.

For development-time agents working ON Cthulu (not inside it), the DoD is
in `CLAUDE.md`:
- `cargo check` passes
- `cargo test` passes
- No new warnings in modified files

---

## 5. Failure Handling

> *From Reflexion (Shinn et al., 2023): After failure, generate a verbal
> reflection on what went wrong before retrying.*
>
> *From CSP (Mackworth, 1977): When a constraint is violated, backtrack.*

### Three failure types in Cthulu

| Type | Current Behavior | Where |
|------|-----------------|-------|
| **Source failure** (network, auth) | Log warning, continue with empty data | `execute_inner()` stage 1 |
| **Executor failure** (timeout, crash) | Fail the entire run | `execute_inner()` stage 4 |
| **Sink failure** (webhook down, API error) | Log error, continue to next sink | `execute_inner()` stage 5 |

Source and sink failures are best-effort. Executor failure is fail-fast. This
is a reasonable default but should eventually be configurable per-flow.

### Organizational reflexion

Cthulu implements reflexion at the organizational level through two files:

**`LESSONS.md`** — Records what went wrong and why. Each lesson is tagged with a
date and captures the root cause. Development agents read this on session start
(per `AI-WORKFLOW.md`). Example:

```markdown
## 2026-02-21
- Axum 0.8 uses {param} not :param for path parameters
- Never derive Clone on types containing ChildStdin
```

**`NOPE.md`** — Records dead ends. Things that were tried and definitively failed.
This prevents agents from wasting cycles rediscovering known limitations:

```markdown
## 1. Nested KVM on Apple Silicon via Lima
Don't try. Neither vz nor qemu provides working /dev/kvm to the guest.
Instead: Use a real Linux server with bare-metal KVM.
```

### How to apply reflexion in prompt templates

For prompts where the agent might fail and retry (e.g., code generation):

```markdown
## If your approach fails
1. Stop and analyze WHY it failed (don't just retry with slight changes)
2. Check .skills/ for any relevant context you missed
3. Try a fundamentally different approach
4. If 3 approaches fail, report what you tried and why each failed
```

The "3 approaches then report" pattern is bounded retry (CSP backtracking)
combined with reflexion. It prevents infinite loops while capturing useful
failure information.

---

## Where These Concepts Live in the Codebase

| Concept | Implementation | File(s) |
|---------|---------------|---------|
| Context layers | `.skills/` generation | [`src/server/flow_routes/node_chat.rs`](../src/server/flow_routes/node_chat.rs) (around `build_workflow_context_md`) |
| Instructional context | Agent rules | [`AGENT.md`](../AGENT.md) |
| Knowledge context | Pipeline position | [`node_chat.rs`](../src/server/flow_routes/node_chat.rs) (`build_workflow_context_md()`) |
| Tool context | Template variables | [`src/flows/runner.rs`](../src/flows/runner.rs) (around `// ── 3. PROMPT RENDERING`) |
| Pre/postconditions | Stage validation | [`src/flows/runner.rs`](../src/flows/runner.rs) (`execute_inner()`) |
| HTN decomposition | DAG structure | Flow JSON (nodes + edges) |
| OODA/ReAct loop | 5-stage pipeline | [`src/flows/runner.rs`](../src/flows/runner.rs) (`execute_inner()`) |
| Reflexion | Failure recording | `LESSONS.md`, [`NOPE.md`](../NOPE.md) |
| Bounded retry | Executor timeout | [`src/tasks/executors/claude_code.rs`](../src/tasks/executors/claude_code.rs) (15-min timeout) |
| Scope isolation | Per-node context | `.skills/` per executor, `live_processes` pool |
| Definition of Done | Build verification | [`CLAUDE.md`](../CLAUDE.md) rules, prompt template verification steps |

---

## Key Takeaways

1. **Invest in context artifacts** (`AGENT.md`, `Skill.md`, prompt templates,
   training files) over clever prompting techniques. A well-structured context
   window outperforms any amount of "think step by step."

2. **Build verification into the task**, not after it. The agent's last action
   should prove the work was done (post the review, run the tests, deliver to
   the sink).

3. **Decompose to the right depth** — one tool-call cycle per leaf, not
   pseudocode primitives. Over-decomposition wastes context window.
   Under-decomposition causes hallucination.

4. **Record failures** in `LESSONS.md` and `NOPE.md`. Organizational reflexion
   prevents every future session from rediscovering the same dead ends.

5. **Let the pipeline handle gathering** (sources + filters). The executor
   agent should transform and analyze, not fetch. This is OODA's Orient
   phase done systematically.
