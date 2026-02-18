# Cthulu - Slack Interactive Relay Setup

Cthulu supports an interactive Slack relay that lets you @mention the bot (or DM it) and have multi-turn conversations powered by Claude. Conversations are threaded and persistent per Slack thread.

## Prerequisites

- A Slack workspace where you have admin/app-creation permissions
- `claude` CLI installed and available on `$PATH`
- Cthulu built and ready to run

## Quick Start

### 1. Generate a Slack App Manifest

```bash
cargo run -- setup
# or with a custom bot name:
cargo run -- setup --name Kthanid
```

This prints a JSON manifest and step-by-step instructions. Follow them, or use the manual steps below.

### 2. Create the Slack App

1. Go to [https://api.slack.com/apps](https://api.slack.com/apps)
2. Click **Create New App** > **From a manifest**
3. Select your workspace > **JSON** tab > Paste the manifest > **Create**

### 3. Generate App-Level Token (Socket Mode)

1. In the app settings, go to **Basic Information**
2. Scroll to **App-Level Tokens** > **Generate Token and Scopes**
3. Name: `socket-mode`
4. Add scope: `connections:write`
5. Click **Generate**
6. Copy the token (starts with `xapp-`)

### 4. Install the App & Get Bot Token

1. Go to **Install App** in the sidebar
2. Click **Install to Workspace** > **Authorize**
3. Copy the **Bot User OAuth Token** (starts with `xoxb-`)

### 5. Configure Cthulu

Add tokens to your `.env` file:

```env
SLACK_BOT_TOKEN=xoxb-your-token-here
SLACK_APP_TOKEN=xapp-your-token-here
```

Add the `[slack]` section to `cthulu.toml`:

```toml
[slack]
bot_token_env = "SLACK_BOT_TOKEN"
app_token_env = "SLACK_APP_TOKEN"
```

### 6. Invite the Bot & Test

1. In Slack, go to a channel and type `/invite @YourBotName`
2. Start Cthulu: `cargo run`
3. In Slack: `@YourBotName hello!`
4. Or DM the bot directly

## Configuration Reference

### `cthulu.toml`

```toml
[server]
port = 8081                        # HTTP server port
sentry_dsn_env = "SENTRY_DSN"     # Optional: Sentry DSN env var
environment = "local"              # Environment label

[github]
token_env = "GITHUB_TOKEN"        # GitHub token env var (for PR reviews)

[slack]                            # Optional: omit to disable interactive relay
bot_token_env = "SLACK_BOT_TOKEN"  # Env var containing xoxb- bot token
app_token_env = "SLACK_APP_TOKEN"  # Env var containing xapp- app-level token

[[tasks]]
name = "pr-review"
executor = "claude-code"
prompt = "prompts/pr_review.md"
permissions = ["Bash", "Read", "Grep", "Glob"]

[tasks.trigger.github]
event = "pull_request"
repos = [
  { slug = "owner/repo", path = "/path/to/local/clone" },
]
poll_interval = 60                 # Seconds between GitHub polls
skip_drafts = true                 # Skip draft PRs (default: true)
review_on_push = true              # Re-review on new commits

[[tasks]]
name = "scheduled-task"
executor = "claude-code"
prompt = "prompts/my_task.md"
permissions = []

[tasks.trigger.cron]
schedule = "0 9 * * MON"          # Standard 5-field cron expression

[[tasks.sources]]
type = "rss"
url = "https://example.com/feed.xml"
limit = 10                         # Max items to fetch (default: 10)

[[tasks.sources]]
type = "github-merged-prs"
repos = ["owner/repo-a", "owner/repo-b"]
since_days = 7                     # Look back N days (default: 7)

[[tasks.sinks]]
type = "slack"
bot_token_env = "SLACK_BOT_TOKEN"
channel = "#my-channel"

[[tasks.sinks]]
type = "notion"
token_env = "NOTION_TOKEN"
database_id = "your-database-id"
```

### `.env`

```env
ENVIRONMENT=local
SENTRY_DSN=
GITHUB_TOKEN=ghp_...
SLACK_BOT_TOKEN=xoxb-...
SLACK_APP_TOKEN=xapp-...
```

### Slack Scopes Required

| Scope | Purpose |
|-------|---------|
| `app_mentions:read` | Receive @mention events in channels |
| `channels:history` | Read messages in channels the bot joins |
| `chat:write` | Post replies in threads |
| `im:history` | Read DM messages sent to the bot |
| `im:read` | View DM channel info |
| `im:write` | Open DM conversations |

### Socket Mode Events

| Event | When |
|-------|------|
| `app_mention` | Someone @mentions the bot in a channel |
| `message.im` | Someone sends a DM to the bot |

## Interactive Commands

When chatting with the bot in a Slack thread, these hashtag commands are available:

| Command | Description |
|---------|-------------|
| `#status` | Show current session info (ID, message count, cost, busy state) |
| `#stop` | Kill the running Claude process for this thread |
| `#new` | Kill process + reset session (next message starts fresh) |

## Architecture

```
Slack @mention/DM
    |
    v
WebSocket (Socket Mode) --- slack_socket.rs
    |  ack envelope immediately
    v
handle_slack_event() --- relay.rs
    |  dedup (ring buffer, 500 IDs)
    |  filter bots (bot_id, subtype, self-user-id)
    |  detect DM / @mention / existing session
    v
handle_message()
    |  strip bot mention prefix
    |  parse hashtag commands (#status, #stop, #new)
    v
relay_to_claude()
    |  create/resume ThreadSession (thread_ts -> UUID)
    |  first msg:  claude --session-id <UUID>
    |  subsequent: claude --resume <UUID>
    |  parse stream-json stdout
    v
send_chunked_reply() -> Slack thread
    |  4000 char limit per message
    |  split on newline > space > hard cut
    |  max 20 chunks, 300ms delay between
```

### Key Design Points

- **Opt-in**: The relay only starts if `[slack]` is present in `cthulu.toml`. Without it, Cthulu runs exactly as before.
- **Backward compatible**: All existing features (PR reviews, cron tasks, sinks) are completely untouched.
- **Session persistence**: Each Slack thread maps to a Claude session via UUID. Subsequent messages in the same thread resume the conversation.
- **Reconnection**: Socket Mode auto-reconnects with exponential backoff (1s -> 2s -> 4s -> 8s max).
- **Bot loop prevention**: Three-layer bot filtering (bot_id field, subtype field, self-user-id via auth.test).
- **Process management**: `#stop` sends SIGTERM then SIGKILL to the Claude process group for clean termination.

## Docker

```bash
# Build
docker build -t cthulu .

# Run with docker-compose
docker-compose up -d

# View logs
docker logs -f cthulu
```

The `docker-compose.yml` mounts `prompts/` and `cthulu.toml` as read-only volumes and reads `.env` for secrets.

## CLI

```bash
cthulu              # Start the server (default)
cthulu serve        # Start the server (explicit)
cthulu setup        # Generate Slack app manifest + instructions
cthulu setup --name Bot  # With custom bot name
```
