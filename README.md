# Cthulu

An AI-powered flow runner that delegates to Claude Code for automated PR reviews, news monitoring, changelogs, and more.

## How It Works

Cthulu runs visual pipelines (flows) built in Studio or via the REST API:

```
Trigger → Sources → Filters → Executor (Claude Code) → Sinks
```

- **Triggers**: Cron schedules, GitHub PR webhooks, manual
- **Sources**: RSS feeds, web scrapers, GitHub merged PRs, market data
- **Filters**: Keyword matching (AND/OR, by field)
- **Executor**: Claude Code with scoped permissions
- **Sinks**: Slack (webhook or Bot API), Notion

## Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Claude Code CLI](https://docs.anthropic.com/en/docs/claude-code) installed and logged in (`claude` must be on your PATH)
- [Node.js](https://nodejs.org/) 18+ (for Studio and site)

## Quick Start

```bash
# Clone and install
git clone <repo-url>
cd cthulu
npm install          # installs Nx, plugins, and workspace dependencies

# Set up environment
cp .env.example .env
# Update .env with your API keys (explore .env for details)

# Start backend + Studio in parallel (one command)
npm run dev
```

This starts the Rust backend on `http://localhost:8081` and Studio's Vite dev server on `http://localhost:1420` simultaneously.

### Other Commands

```bash
npm run dev:all        # Start backend + Studio + Site
npm run dev:studio     # Start Studio only
npm run dev:site       # Start Site only (Next.js on port 3000)
npm run build          # Build all projects
npm run test           # Run all tests (Rust + JS)
npm run lint           # Lint all projects
npm run graph          # Visualize project dependency graph

# Or use Nx directly:
npx nx dev cthulu              # Start only the Rust backend
npx nx build cthulu            # cargo build --release
npx nx test cthulu             # cargo test
npx nx dev cthulu-studio       # Start only Studio
npx nx build cthulu-studio     # tsc + vite build
npx nx dev cthulu-site         # Start only the Site
```

### Without Nx (standalone)

You can still run projects individually without Nx:

```bash
# Backend only
cargo build --release
./target/release/cthulu serve

# Studio only
cd cthulu-studio && npm run dev
```

## Environment Variables

Create a `.env` file in the project root:

```bash
# Server
PORT=8081
ENVIRONMENT=local

# GitHub token (required for PR review trigger and merged PRs source)
GITHUB_TOKEN=ghp_...

# Slack (pick one per sink)
SLACK_WEBHOOK_URL=https://hooks.slack.com/services/...
SLACK_BOT_TOKEN=xoxb-...

# Notion (required for Notion sinks)
NOTION_TOKEN=ntn_...

# Optional
SENTRY_DSN=https://...@sentry.io/...
RUST_LOG=cthulu=info   # debug for verbose output
```

## Flows

Flows are the core unit of work. Each flow is a directed graph of nodes:

### Node Types

| Type | Kinds | Description |
|------|-------|-------------|
| **Trigger** | `cron`, `github-pr`, `webhook`, `manual` | What starts the flow |
| **Source** | `rss`, `web-scrape`, `web-scraper`, `github-merged-prs` | Where data comes from |
| **Filter** | `keyword` | Filters items before execution |
| **Executor** | `claude-code`, `claude-api` | AI that processes the data |
| **Sink** | `slack`, `notion` | Where results are delivered |

### Sources

| Type | Key Fields |
|------|-----------|
| `rss` | `url`, `limit`, `keywords` (optional) |
| `web-scrape` | `url`, `keywords` (optional) — extracts full page text |
| `web-scraper` | `url`, `items_selector`, `title_selector`, `url_selector`, etc. — CSS selector-based |
| `github-merged-prs` | `repos` (list of `"owner/repo"`), `since_days` |

### Sinks

| Type | Key Fields |
|------|-----------|
| `slack` | `webhook_url_env` or `bot_token_env` + `channel` |
| `notion` | `token_env`, `database_id` |

### Prompt Templates

Prompts can be inline strings or file paths (`.md` or `.txt`). Templates support `{{variable}}` substitution:

- `{{content}}` — formatted source items
- `{{item_count}}` — number of items fetched
- `{{timestamp}}` — current UTC timestamp
- `{{market_data}}` — crypto/market snapshot (fetched automatically if present)

See `prompts/` for examples.

## Cthulu Studio

Studio is a visual flow editor for creating and monitoring pipelines. It connects to the running server via REST API.

### Run Studio (with Nx)

```bash
npm run dev         # starts backend + studio together
# or individually:
npm run dev:studio  # studio only (backend must be running separately)
```

### Build for Distribution (Tauri desktop app)

```bash
cd cthulu-studio
npm run tauri build
```

The built app will be in `cthulu-studio/src-tauri/target/release/bundle/` (`.dmg` on macOS, `.msi` on Windows, `.deb`/`.AppImage` on Linux).

### What Studio Does

- Drag-and-drop nodes from the sidebar to build pipelines
- Edit node configs in the property panel
- Manually trigger flow runs
- View run history per flow
- Auto-saves changes to the server

## API

The server exposes a REST API on the configured port:

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

## Logging

Cthulu uses hierarchical structured logging. Control verbosity with `RUST_LOG`:

```bash
RUST_LOG=cthulu=info cargo run   # pipeline summaries (default)
RUST_LOG=cthulu=debug cargo run  # per-source details, Claude tool calls, item titles
```

Flow runs output nested under their span:
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

## Project Structure

This is an Nx monorepo with three projects:

```
cthulu/
├── package.json           # Root: Nx workspace, scripts, workspaces
├── nx.json                # Nx config: plugins, caching, target defaults
├── project.json           # Rust backend project (Nx targets for cargo)
├── Cargo.toml             # Rust dependencies
├── src/                   # Rust backend source
│   ├── config.rs          # Env-based configuration
│   ├── flows/             # Flow model, runner, storage, scheduler, history
│   ├── github/            # GitHub API client
│   ├── server/            # Axum HTTP server + API routes
│   ├── tui/               # Terminal UI (ratatui)
│   └── tasks/
│       ├── sources/       # RSS, web scrape, GitHub PRs, market data
│       ├── filters/       # Keyword filter
│       ├── executors/     # Claude Code executor
│       └── sinks/         # Slack, Notion
├── cthulu-studio/         # Tauri + React Flow desktop app (Nx: @nx/vite)
├── cthulu-site/           # Marketing site, Next.js (Nx: @nx/next)
├── prompts/               # Prompt templates
└── examples/              # Sample flow JSON + TOML for reference
```

### Nx Project Graph

Run `npm run graph` to visualize how the projects depend on each other. Studio has an implicit dependency on the backend (it needs the API running).
