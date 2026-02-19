# Cthulu

Cron for Claude Code. A config-driven task runner that delegates to Claude Code for automated PR reviews, news monitoring, changelogs, and more.

## How It Works

Cthulu runs pipelines defined in `cthulu.toml`:

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
- [Node.js](https://nodejs.org/) 18+ (for Studio only)

## Quick Start

```bash
# Clone and build
git clone git@github.com:saltyskip/cthulu.git
cd cthulu
cargo build --release

# Set up environment
cp .env.example .env
# Edit .env with your API keys (see Environment Variables below)

# Create your config (or use the example)
cp examples/crypto_news.toml cthulu.toml

# Run
cargo run
```

The server starts on `http://localhost:8081`.

## Environment Variables

Create a `.env` file in the project root:

```bash
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

The env var names in your `.env` must match the `*_env` fields in your `cthulu.toml` sink configs. For example, if your Notion sink has `token_env = "NOTION_TOKEN"`, you need `NOTION_TOKEN=...` in `.env`.

## Configuration

Cthulu is configured via `cthulu.toml`. Here's a minimal example:

```toml
[server]
port = 8081

[[tasks]]
name = "news-brief"
executor = "claude-code"
prompt = "prompts/my_prompt.md"
permissions = []          # empty = all permissions

[tasks.trigger.cron]
schedule = "0 */4 * * *"  # every 4 hours

[[tasks.sources]]
type = "rss"
url = "https://example.com/feed"
limit = 10

[[tasks.sinks]]
type = "notion"
token_env = "NOTION_TOKEN"
database_id = "your-notion-database-id"
```

### Triggers

**Cron** — standard 5-field cron expressions:
```toml
[tasks.trigger.cron]
schedule = "0 */4 * * *"
```

**GitHub PR** — polls for new/updated PRs:
```toml
[tasks.trigger.github]
event = "pull_request"
repos = [
  { slug = "owner/repo", path = "/local/path/to/repo" },
]
poll_interval = 60
skip_drafts = true
review_on_push = true
```

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

### Build & Run Studio

```bash
cd cthulu-studio
npm install
npm run tauri dev
```

This opens the desktop app pointing at `http://localhost:8081`. Make sure the server is running first.

### Build for Distribution

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

```
cthulu/
├── src/
│   ├── config.rs          # TOML config parsing
│   ├── flows/             # Flow model, runner, storage, history
│   ├── github/            # GitHub API client
│   ├── server/            # Axum HTTP server + API routes
│   └── tasks/
│       ├── sources/       # RSS, web scrape, GitHub PRs, market data
│       ├── filters/       # Keyword filter
│       ├── executors/     # Claude Code executor
│       ├── sinks/         # Slack, Notion
│       └── triggers/      # Cron, GitHub PR polling
├── cthulu-studio/         # Tauri + React Flow desktop app
├── prompts/               # Prompt templates
├── examples/              # Example configs
└── cthulu.toml            # Your task configuration
```
