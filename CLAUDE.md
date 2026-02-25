# Cthulu - AI Assistant Guide

This guide is the entry point for AI assistants working in this monorepo. It provides an overview, critical patterns, and links to detailed documentation.

---

## Quick Reference

### Tech Stack

| Technology | Version | Purpose |
|------------|---------|---------|
| Rust | 2024 edition | Backend language |
| Axum | 0.8 | HTTP framework |
| Tokio | 1.38 | Async runtime |
| React | 19 | UI framework (Studio) |
| TypeScript | ~5.x | Type safety (Studio, Site) |
| Vite | 6.x | Build tool (Studio) |
| Tauri | 2.5 | Desktop app wrapper |
| React Flow | 12.6 | Visual flow editor |
| Next.js | 15 | Marketing site |
| Tailwind CSS | 4 | Styling (Site) |
| Nx | 20.8 | Monorepo build system |

### Critical Rules (Must Follow)

1. **React Flow node merging**: Never replace nodes wholesale with `setNodes(newArray)`. Always spread-merge: `{ ...existingNode, data: newData }`. Wholesale replacement destroys React Flow's internal measurements (`measured`, `internals`, `handleBounds`), crashing edge renderers.
2. **No `useEffect` for derived state**: Use `useMemo` for computed values, callback forms of state setters, and event handlers for side effects. Only use `useEffect` when truly needed (e.g., syncing external props). Never depend on state that the effect itself modifies.
3. **Axum path params**: Use `{param}` syntax, not `:param`. Example: `/flows/{id}/nodes/{node_id}/interact`.
4. **Restart server after Rust changes**: There is no hot-reload. Always `cargo run` after modifying Rust code.
5. **`tokio::sync::Mutex`** for async contexts: Use `tokio::sync::Mutex` (not `std::sync::Mutex`) when the lock is held across `.await` points.
6. **Never derive Clone on process handles**: `ChildStdin`, `Child`, `mpsc::UnboundedReceiver` are not Clone. Wrap in `Arc<Mutex<...>>` for shared access. `AppState` must always derive Clone (all its fields are `Arc<...>` or inherently Clone).
7. **SSE streams use `async_stream::stream!`**: All streaming logic lives inside the block. Nothing after the closing `};` can reference variables from inside. Delete orphaned code between `};` and the return statement.
8. **Session keys**: Sessions are keyed by `flow_id::node_id` for node-level, `flow_id` for flow-level.
9. **Atomic YAML persistence**: Always write sessions via temp file + rename (`path.with_extension("yaml.tmp")` then `std::fs::rename`).
10. **Claude CLI stream-json**: Correct message format is `{"type":"user","message":{"role":"user","content":"..."}}`. Must use `--verbose` with `--output-format stream-json`.
11. **Manual Run always works**: The run button must work even when a flow is disabled (manual override policy).
12. **Plan-First Workflow**: For non-trivial tasks, explore first and plan before writing code. See [docs/AI-WORKFLOW.md](docs/AI-WORKFLOW.md).
13. **Verify Before Done**: Always run `cargo check` (Rust) or `npm run build` (Studio) before marking a task complete. See [docs/AI-WORKFLOW.md](docs/AI-WORKFLOW.md).

### Essential Commands

| Task | Command |
|------|---------|
| Start backend + Studio | `npm run dev` |
| Start all projects | `npm run dev:all` |
| Build Rust backend | `npx nx build cthulu` |
| Dev Rust backend | `npx nx dev cthulu` or `cargo run -- serve` |
| Check Rust compiles | `cargo check` |
| Lint Rust | `cargo clippy -- -D warnings` |
| Test Rust | `cargo test` |
| Build Studio | `npx nx build cthulu-studio` |
| Dev Studio | `npx nx dev cthulu-studio` |
| Build Site | `npx nx build cthulu-site` |
| Dev Site | `npx nx dev cthulu-site` |
| Nx dependency graph | `npx nx graph` |

---

## Project Overview

Cthulu is an **AI-powered workflow automation system** that orchestrates Claude Code agents in directed-acyclic-graph (DAG) pipelines. It connects triggers, data sources, filters, AI executors, and output sinks.

### Pipeline Model

```
Trigger (cron / github-pr / manual / webhook)
  -> Sources (rss / web-scrape / github-merged-prs / market-data)
  -> Filters (keyword matching)
  -> Executor (Claude Code / Claude API)
  -> Sinks (slack / notion)
```

### Projects

| Project | Port | Purpose |
|---------|------|---------|
| `cthulu` (root) | 8081 | Rust backend -- flow runner, scheduler, REST API |
| `cthulu-studio` | 5173 | Visual flow editor -- React Flow canvas, agent chat, Tauri desktop |
| `cthulu-site` | 3000 | Marketing website -- Next.js 15, Tailwind 4, Framer Motion |

---

## Architecture

```
cthulu/
├── src/                            # Rust backend
│   ├── main.rs                     # CLI entry point (clap)
│   ├── config.rs                   # Env-based configuration
│   ├── flows/
│   │   ├── runner.rs               # Flow execution engine
│   │   ├── scheduler.rs            # Cron scheduling (croner)
│   │   ├── file_store.rs           # File-based flow persistence
│   │   ├── history.rs              # Run history tracking
│   │   ├── events.rs               # Flow event types
│   │   └── store.rs                # Store trait
│   ├── server/
│   │   ├── mod.rs                  # AppState, LiveClaudeProcess, sessions
│   │   ├── flow_routes.rs          # Flow CRUD, interact, scheduler endpoints
│   │   ├── prompt_routes.rs        # Prompt management endpoints
│   │   ├── routes.rs               # Route registration
│   │   └── middleware.rs           # HTTP middleware
│   ├── tasks/
│   │   ├── pipeline.rs             # Pipeline orchestration
│   │   ├── context.rs              # Execution context
│   │   ├── executors/claude_code.rs  # Claude Code integration
│   │   ├── sources/                # RSS, web-scrape, GitHub PRs, market data
│   │   ├── filters/keyword.rs      # Keyword filter
│   │   └── sinks/                  # Slack (Block Kit), Notion (blocks)
│   ├── github/                     # GitHub API client
│   └── tui/                        # Terminal UI (ratatui)
├── cthulu-studio/                  # Visual flow editor
│   ├── src/
│   │   ├── App.tsx                 # Root component
│   │   ├── components/
│   │   │   ├── Canvas.tsx          # React Flow canvas
│   │   │   ├── BottomPanel.tsx     # VS Code-like tabbed panel
│   │   │   ├── NodeChat.tsx        # Per-node agent chat
│   │   │   ├── PropertyPanel.tsx   # Node property editor
│   │   │   ├── FlowList.tsx        # Flow list sidebar
│   │   │   ├── TopBar.tsx          # Top navigation
│   │   │   └── NodeTypes/          # 5 custom node components
│   │   ├── api/                    # REST client, SSE streaming
│   │   └── types/flow.ts           # TypeScript types
│   ├── src-tauri/                  # Tauri desktop config
│   ├── AGENTS.md                   # Studio-specific AI docs
│   └── CLAUDE.md                   # Symlink -> AGENTS.md
├── cthulu-site/                    # Marketing website
│   ├── app/                        # Next.js app directory
│   ├── components/                 # Landing page sections
│   ├── AGENTS.md                   # Site-specific AI docs
│   └── CLAUDE.md                   # Symlink -> AGENTS.md
├── .claude/
│   ├── skills/                     # Shared skills (Rust, React Flow, Nx, Claude CLI)
│   └── LESSONS.md                  # Lessons learned (self-improvement)
├── docs/
│   ├── AI-WORKFLOW.md              # How agents should work
│   └── TROUBLESHOOTING.md          # Common errors and fixes
├── examples/                       # Example flows and prompt templates
├── scripts/dev.sh                  # Dev startup script
├── AGENT.md                        # Agent rules for executor agents (injected at runtime)
├── CLAUDE.md                       # This file (AI entry point)
├── Cargo.toml                      # Rust dependencies
├── nx.json                         # Nx workspace config
└── package.json                    # npm workspaces (Studio, Site)
```

### Data Flow

```
Flows (JSON on disk, ~/.cthulu/flows/)
  -> Flow Runner (resolves DAG, parallel sources)
    -> Claude Code CLI (persistent process via stream-json)
      -> Sinks (Slack webhook/bot, Notion API)

Sessions (sessions.yaml, local state)
  -> LiveClaudeProcess pool (in-memory, keyed by flow_id::node_id)
    -> SSE bridge to browser
```

---

## Detailed Documentation

| Area | Documentation |
|------|---------------|
| Studio (React Flow) | [cthulu-studio/AGENTS.md](cthulu-studio/AGENTS.md) |
| Site (Next.js) | [cthulu-site/AGENTS.md](cthulu-site/AGENTS.md) |
| AI Workflow | [docs/AI-WORKFLOW.md](docs/AI-WORKFLOW.md) -- plan-first, verification, self-improvement |
| Troubleshooting | [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md) -- common errors and fixes |
| Agent Rules | [AGENT.md](AGENT.md) -- rules for executor agents running inside workflows |
| Skills | [.claude/skills/](.claude/skills/) -- Rust/Axum, React Flow, Nx, Claude CLI |
| Lessons | [.claude/LESSONS.md](.claude/LESSONS.md) -- recorded mistakes and insights |

---

## Common Workflows

### Adding a New Source Type

1. Create file in `src/tasks/sources/` (e.g., `my_source.rs`)
2. Implement the source function that returns `Vec<String>` content
3. Register in `src/tasks/sources/mod.rs`
4. Add to pipeline dispatch in `src/tasks/pipeline.rs`
5. Add node config UI in `cthulu-studio/src/components/PropertyPanel.tsx`
6. Add source type to node validation in `cthulu-studio/src/utils/validateNode.ts`
7. **Verify**: `cargo check` passes, `npx nx build cthulu-studio` passes

### Adding a New Sink Type

1. Create file in `src/tasks/sinks/` (e.g., `my_sink.rs`)
2. Implement the sink function that receives executor output
3. Register in `src/tasks/sinks/mod.rs`
4. Add to pipeline dispatch in `src/tasks/pipeline.rs`
5. Add node config UI in PropertyPanel
6. Add env vars to `.env.example` if needed
7. **Verify**: `cargo check` passes

### Adding an API Endpoint

1. Add handler function in `src/server/flow_routes.rs`
2. Register route in `flow_router()` function
3. Use `{param}` for path parameters
4. Extract `State(state): State<AppState>` as first parameter
5. Add corresponding client function in `cthulu-studio/src/api/client.ts`
6. **Verify**: `cargo check` passes, restart server, test with `curl`

### Adding a Studio Component

1. Create component in `cthulu-studio/src/components/`
2. Use CSS variables for theming (`var(--bg)`, `var(--border)`, `var(--accent)`)
3. For React Flow nodes: always spread-merge, never replace wholesale
4. For state: use `useMemo` for derived values, callback setters for updates
5. Add styles in `cthulu-studio/src/styles.css`
6. **Verify**: `npx nx build cthulu-studio` passes

### Adding a Custom Node Type

1. Create component in `cthulu-studio/src/components/NodeTypes/`
2. Register in the `nodeTypes` map in `Canvas.tsx`
3. Add to `addNodeAtScreen()` with appropriate defaults
4. Add config fields in `PropertyPanel.tsx`
5. Add validation rules in `validateNode.ts`
6. Add backend handling in `src/tasks/pipeline.rs`
7. **Verify**: both `cargo check` and Studio build pass

---

## Agent Infrastructure

### AGENT.md vs CLAUDE.md

- **CLAUDE.md** (this file) -- Rules for developers/agents working **on** the Cthulu codebase
- **AGENT.md** -- Rules for executor agents running **inside** Cthulu workflows (injected into `.skills/AGENT.md` at runtime)

### .skills/ Directory (Runtime)

When a user sends their first message to an executor node, the backend auto-generates `.skills/` in the node's working directory:

- `.skills/AGENT.md` -- Copied from project root `AGENT.md`
- `.skills/Skill.md` -- Workflow context (upstream/downstream nodes, config summaries)
- `.skills/workflow.json` -- Full flow definition

Agents are scoped to `.skills/` and their working directory only (NOPE.md-style boundaries).
