# Cthulu

AI-powered workflow automation that runs Claude Code agents in visual DAG pipelines. Connect triggers, data sources, filters, executors, and output sinks — build once, run on schedule.

## How It Works

```
Trigger → Sources → Filters → Executor (Claude Code / VM Sandbox) → Sinks
```

- **Triggers**: Cron schedules, GitHub PR webhooks, manual runs
- **Sources**: RSS feeds, web scrapers, GitHub merged PRs, market data, Google Sheets
- **Filters**: Keyword matching (AND/OR, by field)
- **Executors**: Claude Code (automated pipelines) or VM Sandbox (interactive terminal)
- **Sinks**: Slack (webhook or Bot API), Notion

---

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) — installed and authenticated (`claude` must be on your PATH)
- [Node.js](https://nodejs.org/) 18+

---

## Quick Start

```bash
# Clone and install
git clone <repo-url>
cd cthulu
npm install          # installs Nx, plugins, and workspace dependencies

# Set up environment
cp .env.example .env
# Edit .env — at minimum set PORT, GITHUB_TOKEN if needed, Slack/Notion tokens for sinks

# Start backend + Studio
npm run dev
```

Backend runs on `http://localhost:8081`. Studio runs on `http://localhost:1420`.

### Other Commands

```bash
npm run dev:all        # Backend + Studio + Site
npm run dev:studio     # Studio only
npm run dev:site       # Marketing site only (Next.js, port 3000)
npm run build          # Build all projects
npm run test           # Run all tests
npm run lint           # Lint all projects

# Nx directly:
npx nx dev cthulu              # Rust backend only
npx nx build cthulu            # cargo build --release
npx nx test cthulu             # cargo test
npx nx dev cthulu-studio       # Studio only
npx nx build cthulu-studio     # tsc + vite build
```

### Without Nx

```bash
cargo build --release && ./target/release/cthulu serve
cd cthulu-studio && npm run dev
```

---

## Environment Variables

```bash
# Server
PORT=8081
ENVIRONMENT=local

# VM Manager (required for VM Sandbox executor nodes)
VM_MANAGER_URL=http://<host>:8080

# GitHub (required for PR review trigger and merged PRs source)
GITHUB_TOKEN=ghp_...

# Slack (pick one per sink)
SLACK_WEBHOOK_URL=https://hooks.slack.com/services/...
SLACK_BOT_TOKEN=xoxb-...

# Notion (required for Notion sinks)
NOTION_TOKEN=ntn_...

# Google Sheets (required for google-sheets source)
GOOGLE_SHEETS_SERVICE_ACCOUNT_KEY=<base64-encoded JSON or path>

# Logging
RUST_LOG=cthulu=info   # debug for verbose output
```

---

## Flows

Flows are directed graphs of nodes. Each flow is a JSON file stored in `~/.cthulu/flows/`.

### Node Types

| Type | Kinds | Description |
|------|-------|-------------|
| **Trigger** | `cron`, `github-pr`, `webhook`, `manual` | What starts the flow |
| **Source** | `rss`, `web-scrape`, `web-scraper`, `github-merged-prs`, `market-data`, `google-sheets` | Where data comes from |
| **Filter** | `keyword` | Filters items before execution |
| **Executor** | `claude-code`, `vm-sandbox` | AI that processes the data |
| **Sink** | `slack`, `notion` | Where results are delivered |

### Sources

| Type | Key Fields |
|------|-----------|
| `rss` | `url`, `limit`, `keywords` (optional) |
| `web-scrape` | `url`, `keywords` (optional) — extracts full page text |
| `web-scraper` | `url`, `items_selector`, `title_selector`, `url_selector` — CSS selector-based |
| `github-merged-prs` | `repos` (list of `"owner/repo"`), `since_days` |
| `market-data` | (no config) — BTC/ETH prices, Fear & Greed, S&P 500 |
| `google-sheets` | `spreadsheet_id`, `range`, `service_account_key_env`, `limit` |

### Executors

| Kind | What It Does |
|------|-------------|
| `claude-code` | Automated: flow runner pipes rendered prompt to Claude CLI, collects output, delivers to sinks |
| `vm-sandbox` | Interactive: provisions a Firecracker microVM with Claude CLI pre-installed; user gets a browser terminal (ttyd iframe in BottomPanel) |

### Sinks

| Type | Key Fields |
|------|-----------|
| `slack` | `webhook_url_env` or `bot_token_env` + `channel` |
| `notion` | `token_env`, `database_id` |

### Prompt Templates

Prompts can be inline strings or file paths (`.md` or `.txt`). Templates support `{{variable}}` substitution:

| Variable | Content |
|----------|---------|
| `{{content}}` | Formatted source items |
| `{{item_count}}` | Number of items fetched |
| `{{timestamp}}` | Current UTC timestamp |
| `{{market_data}}` | Crypto/market snapshot |
| `{{diff}}` | PR diff (for code review flows) |
| `{{pr_number}}`, `{{pr_title}}`, `{{repo}}` | GitHub PR context |

See `prompts/` for examples.

---

## Cthulu Studio

Studio is the visual flow editor. Drag-and-drop nodes, edit configs in the property panel, trigger runs, and watch results live.

### Template Gallery

Click **+ New** in the flow list to open the template gallery — a Vercel-style card grid with 10 pre-built workflows across four categories:

| Category | Templates |
|----------|-----------|
| **Media** | Daily news brief, PR review bot, changelog generator |
| **Social** | Trending topics monitor, Reddit digest |
| **Research** | Competitor monitor, product launch tracker |
| **Finance** | Crypto market brief, earnings digest, macro weekly |

From the gallery you can also:
- **Upload a YAML file** — drag or click to import a `.yaml`/`.yml` workflow definition
- **Import from GitHub** — paste any public GitHub repo URL to bulk-import all workflow YAMLs from it (uses GitHub Contents API, recurses 2 levels deep)

### OAuth Token Status

The TopBar always shows a token status button:
- **Green** — token is valid
- **Amber pulse** — token has expired

Click the button to refresh the token. This calls `POST /api/auth/refresh-token`, which re-injects the full credentials into all active VMs so Claude CLI inside them never hits a login prompt.

### VM Session Persistence

VM sessions survive server restarts. When you click a `vm-sandbox` node after a restart, the backend looks up the existing VM ID from `sessions.yaml`, calls the VM Manager to verify it's still alive, and reconnects — no new VM is spun up. You get the same persistent workspace you left.

### Build for Distribution (Tauri desktop app)

```bash
cd cthulu-studio
npm run tauri build
```

Output: `cthulu-studio/src-tauri/target/release/bundle/` (`.dmg` / `.msi` / `.deb`).

---

## API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/flows` | GET | List all flows |
| `/api/flows` | POST | Create a flow |
| `/api/flows/{id}` | GET | Get flow details |
| `/api/flows/{id}` | PUT | Update a flow |
| `/api/flows/{id}` | DELETE | Delete a flow |
| `/api/flows/{id}/trigger` | POST | Manually trigger a flow |
| `/api/flows/{id}/runs` | GET | Get run history |
| `/api/node-types` | GET | List available node types |
| `/api/status` | GET | Server status + task states |
| `/api/templates` | GET | List all workflow templates |
| `/api/templates/{slug}` | GET | Get a template by slug |
| `/api/templates/import-yaml` | POST | Import a workflow from uploaded YAML |
| `/api/templates/import-github` | POST | Bulk-import workflow YAMLs from a GitHub repo |
| `/api/auth/token-status` | GET | Check current OAuth token validity |
| `/api/auth/refresh-token` | POST | Re-inject OAuth token into all active VMs |
| `/api/sandbox/vm/{flow_id}` | POST | Provision or retrieve a VM for a flow |
| `/api/sandbox/vm/{flow_id}` | DELETE | Destroy a flow's VM |

---

## Logging

```bash
RUST_LOG=cthulu=info cargo run    # pipeline summaries (default)
RUST_LOG=cthulu=debug cargo run   # per-source details, Claude tool calls, item titles
```

Example run output:
```
flow_run{flow=news-brief run=ba4fa70b}
  INFO ▶ Started nodes=4 edges=3
  INFO Pipeline: 2 source(s) → 0 filter(s) → claude-code → 1 sink(s)
  INFO ✓ Sources fetched items=12 elapsed=1.6s
  INFO ✓ Prompt rendered chars=5596 items=12
  INFO ⟶ Executing executor=claude-code permissions=ALL
  INFO ✓ Executor finished turns=3 cost=$0.0420 output_chars=1842 elapsed=45.2s
  INFO ✓ Delivered sink=Notion elapsed=0.2s
  INFO ✓ Completed elapsed=47.0s
```

---

## Project Structure

Nx 20.8 monorepo with three projects:

```
cthulu/
├── package.json           # Root: Nx workspace, scripts, workspaces
├── nx.json                # Nx config
├── project.json           # Rust backend project
├── Cargo.toml             # Rust dependencies
├── src/                   # Rust backend source
│   ├── config.rs          # Env-based configuration
│   ├── flows/             # Flow model, runner, storage, scheduler, history
│   ├── github/            # GitHub API client
│   ├── server/            # Axum HTTP server + API routes
│   │   ├── mod.rs         # AppState, LiveClaudeProcess, sessions, OAuth token
│   │   ├── flow_routes/   # Flow CRUD, interact, sandbox, scheduler endpoints
│   │   ├── auth_routes.rs # Token status + refresh-token endpoints
│   │   ├── template_routes.rs  # Template list/get/import-yaml/import-github
│   │   └── prompt_routes.rs    # Prompt management
│   ├── sandbox/           # VM sandbox backends (VM Manager, Firecracker)
│   ├── tasks/
│   │   ├── sources/       # RSS, web-scrape, GitHub PRs, market data, Google Sheets
│   │   ├── filters/       # Keyword filter
│   │   ├── executors/     # Claude Code executor
│   │   └── sinks/         # Slack, Notion
│   ├── templates.rs       # Template loading + YAML→Flow conversion
│   └── tui/               # Terminal UI (ratatui)
├── static/
│   └── workflows/         # 10 built-in workflow YAML templates
│       ├── finance/
│       ├── media/
│       ├── research/
│       └── social/
├── cthulu-studio/         # Tauri + React Flow desktop app
│   └── src/
│       ├── components/
│       │   ├── TemplateGallery.tsx   # + New modal with template cards + import
│       │   ├── MiniFlowDiagram.tsx   # Read-only mini React Flow preview
│       │   ├── TopBar.tsx            # Token status button
│       │   ├── NodeChat.tsx          # Per-node agent chat
│       │   └── VmTerminal.tsx        # VM browser terminal (ttyd iframe)
│       └── api/client.ts
├── cthulu-site/           # Marketing site (Next.js 15)
├── prompts/               # Prompt templates
├── .skills/               # Executor agent context (tracked in git, injected at runtime)
│   ├── AGENT.md           # Agent rules (copy of root AGENT.md)
│   ├── Skill.md           # Blank template — customized per flow at runtime
│   └── workflow.json      # Blank template — replaced with live flow JSON at runtime
└── examples/              # Sample flow JSON + TOML for reference
```

---

## AGENT.md / .skills/ for Executor Agents

When a user opens a `claude-code` executor node for the first time, the backend auto-generates `.skills/` in the node's working directory:

- `.skills/AGENT.md` — Agent rules (from root `AGENT.md`)
- `.skills/Skill.md` — Pipeline position, upstream/downstream nodes, config summary
- `.skills/workflow.json` — Full live flow definition

These files scope the agent to its pipeline role. See `AGENT.md` for the full executor agent ruleset.
