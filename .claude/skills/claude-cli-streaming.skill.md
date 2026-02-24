---
name: claude-cli-streaming
description: Use when working on Claude CLI integration -- persistent processes, stream-json protocol, SSE bridging, and session management.
---

# Claude CLI Streaming Protocol

## When to Apply

- Modifying `interact_node()` or `interact_flow()` in `flow_routes.rs`
- Working with `LiveClaudeProcess` in `mod.rs`
- Debugging Claude CLI subprocess issues
- Implementing new chat/interact features

## Persistent Process Architecture

Instead of spawning a new `claude` process per message, we keep a single process alive using `--input-format stream-json`:

```
[Cthulu Server] --stdin JSON--> [claude --input-format stream-json] --stdout JSON--> [SSE to browser]
```

The process stays alive between messages. Each user message is written to stdin as a JSON line; Claude responds on stdout with streaming JSON events.

## Required CLI Flags

```
claude --print --verbose --input-format stream-json --output-format stream-json
```

- `--print` -- non-interactive mode (required)
- `--verbose` -- required by `--output-format stream-json`
- `--input-format stream-json` -- accept JSON messages on stdin
- `--output-format stream-json` -- emit JSON events on stdout

## Message Format (stdin -> Claude)

```json
{"type":"user","message":{"role":"user","content":"Your message here"}}
```

**Common mistake**: Sending `{"type":"user","content":"..."}` without the `message.role` wrapper. This produces: `TypeError: undefined is not an object (evaluating 'R.message.role')`.

## Event Format (Claude -> stdout)

Events are newline-delimited JSON. Key event types:

| Event | When | Key Fields |
|-------|------|------------|
| `system` | Process startup or new message | `subtype`, `session_id`, `tools` |
| `assistant` | Model response chunk | `message.content[]` (text, tool_use, tool_result) |
| `result` | Turn complete | `result`, `total_cost_usd`, `num_turns`, `session_id` |

### Content block types in `assistant` events:

```json
{
  "type": "assistant",
  "message": {
    "content": [
      {"type": "text", "text": "Hello!"},
      {"type": "tool_use", "name": "Bash", "input": {"command": "ls"}},
      {"type": "tool_result", "content": "file1.txt\nfile2.txt"}
    ]
  }
}
```

## Session Management

- `--session-id <uuid>` -- use a specific session ID (new sessions)
- `--resume <uuid>` -- resume an existing session
- Second message to a persistent process emits a new `system` init event -- skip or suppress it

## Process Pool (`live_processes`)

Processes keyed by `flow_id::node_id` in `AppState.live_processes`:

```rust
pub live_processes: Arc<tokio::sync::Mutex<HashMap<String, LiveClaudeProcess>>>
```

**Lifecycle**:
1. First message: spawn process, insert into pool
2. Subsequent messages: find in pool, write to stdin
3. Stop: remove from pool, kill process
4. Process death: detect via `child.try_wait()`, remove from pool, next message spawns fresh

## SSE Bridge

The backend reads stdout JSON events and re-emits them as SSE events to the browser:

```
Claude stdout JSON -> parse -> yield Ok(Event::default().event("text").data(...))
```

Key SSE events sent to browser: `system`, `text`, `tool_use`, `tool_result`, `result`, `error`, `done`, `stderr`.
