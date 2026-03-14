# Agent Terminal Mode Design Spec

**Date:** 2026-03-14
**Status:** Approved
**Goal:** Replace the `@assistant-ui/react` chat UI for agents with an embedded xterm.js terminal running Claude Code CLI in a real PTY.

## Problem

The current agent chat UI uses `@assistant-ui/react` with `useExternalStoreRuntime`. This has a critical bug: `ComposerPrimitive.Send` does not trigger the `onNew` callback, so users cannot send messages through the normal UI flow. The backend (Tauri IPC to Claude CLI via `stream-json`) works correctly, but the frontend library is broken.

Rather than continue debugging the `@assistant-ui/react` integration, we are replacing the agent UI entirely with an embedded terminal that runs Claude Code's interactive REPL directly. This gives users the exact same experience as running `claude` in iTerm or VS Code's integrated terminal.

## Architecture

```
Frontend (React + xterm.js)          Backend (Rust + portable-pty)
┌─────────────────────┐              ┌──────────────────────────┐
│ AgentTerminal.tsx    │              │ commands/pty.rs          │
│ ┌─────────────────┐ │              │                          │
│ │ xterm.js        │ │  invoke()    │ spawn_pty(agentId, sid)  │
│ │ Terminal        │─┼─────────────>│   → CommandBuilder::new  │
│ │                 │ │              │     ("claude")           │
│ │ onData(bytes)───┼─┼─write_pty──>│   → PairOfPtys::new()   │
│ │                 │ │              │   → child.spawn()        │
│ │ onResize()──────┼─┼─resize_pty─>│   → reader task          │
│ │                 │ │              │                          │
│ │ write(data) <───┼─┼─pty-data-*──│   → app.emit()          │
│ └─────────────────┘ │              │                          │
└─────────────────────┘              └──────────────────────────┘
```

### Data Flow

1. User selects an agent session in the sidebar.
2. `AgentDetailView` renders `AgentTerminal` in the left pane.
3. `AgentTerminal` mounts, calls `invoke("spawn_pty", { agentId, sessionId })`.
4. Rust resolves the agent config (system prompt, allowed tools, sub-agents), builds a `claude` command with CLI flags, spawns it in a PTY via `portable-pty`.
5. A background tokio task reads PTY output and emits `pty-data-{sessionId}` Tauri events.
6. xterm.js receives events via `listen("pty-data-{sessionId}")` and calls `terminal.write(data)`.
7. User types in xterm.js → `terminal.onData(data)` → `invoke("write_pty", { sessionId, data })` → written to PTY master stdin.
8. Terminal resize → `terminal.onResize({ cols, rows })` → `invoke("resize_pty", { sessionId, cols, rows })` → PTY resize (sends SIGWINCH to child).

## Backend: New Rust Module

### File: `src-tauri/src/commands/pty.rs`

### Dependencies

Add to `src-tauri/Cargo.toml`:

```toml
portable-pty = "0.8"
```

### State

PTY sessions are desktop-only (Tauri) and must NOT be added to the shared `AppState` in `cthulu-backend/api/mod.rs`. Instead, create a separate Tauri-managed state struct:

```rust
use portable_pty::{MasterPty, Child, CommandBuilder, PtySize, native_pty_system};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn std::io::Write + Send>,
    child: Box<dyn Child + Send>,
    reader_handle: tokio::task::JoinHandle<()>,
}

/// Tauri-managed state for PTY sessions. Separate from AppState because
/// portable-pty types are not Clone and PTY is desktop-only.
pub struct PtyState {
    pub sessions: Arc<Mutex<HashMap<String, PtySession>>>,
}

// Registered in main.rs as Tauri managed state:
//   .manage(PtyState { sessions: Arc::new(Mutex::new(HashMap::new())) })
// Accessed in commands via:
//   pty_state: tauri::State<'_, PtyState>
```

### Commands

#### `spawn_pty`

**Params:** `agent_id: String, session_id: String`
**Returns:** `Result<SpawnPtyResponse, String>` where `SpawnPtyResponse = { session_id: String }`

Logic:
1. Look up agent config via `agent_repo.get(&agent_id)`.
2. Check if a PTY session already exists for this `session_id`. If so, return the existing session ID (idempotent).
3. Build `CommandBuilder`:
   - Binary: `claude` (resolved from PATH or `/opt/homebrew/bin/claude`)
   - `--system-prompt <agent.system_prompt>` (if set)
   - `--allowedTools <tools>` (if agent has `permissions` vec)
   - `--agents <json>` (single flag, JSON-encoded array of sub-agent configs)
   - `--resume <session_id>` if the session has history
   - `--dangerously-skip-permissions` if agent has `auto_permissions: true`
   - CWD: agent's `working_directory` or project root
   - Env: `TERM=xterm-256color`, `COLORTERM=truecolor`, `FORCE_COLOR=3`
4. Write hook settings via `write_hook_settings()` (same as existing `agent_chat` in `chat.rs`) — configures `.claude/settings.local.json` with hook groups for permissions, file change tracking, and stop detection. Without this, the right-side panels (DebugPanel, ChangesPanel) will not receive data.
5. Create PTY pair: `native_pty_system().openpty(PtySize { rows: 40, cols: 120, .. })`.
6. Spawn child: `slave.spawn_command(cmd)`.
7. Get reader from master: `master.try_clone_reader()`.
8. Spawn background reader task (see below).
9. Store `PtySession` in `PtyState.sessions`.
10. Return `{ session_id }`.

#### `write_pty`

**Params:** `session_id: String, data: String`
**Returns:** `Result<(), String>`

Logic:
1. Look up PTY session by `session_id`.
2. Write `data.as_bytes()` to the PTY writer.
3. Flush.

#### `resize_pty`

**Params:** `session_id: String, cols: u32, rows: u32`
**Returns:** `Result<(), String>`

Logic:
1. Look up PTY session by `session_id`.
2. Call `master.resize(PtySize { rows: rows as u16, cols: cols as u16, .. })`.

#### `kill_pty`

**Params:** `session_id: String`
**Returns:** `Result<(), String>`

Logic:
1. Remove PTY session from map.
2. Call `child.kill()`.
3. Abort the reader task handle.
4. Drop the master/writer (closes the PTY).

### Background Reader Task

```rust
tokio::task::spawn_blocking(move || {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break,  // EOF
            Ok(n) => {
                // Emit PTY output as UTF-8 lossy (invalid bytes replaced with U+FFFD)
                let data = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = app_handle.emit(
                    &format!("pty-data-{}", session_id),
                    data,
                );
            }
            Err(_) => break,
        }
    }
    // PTY closed — child exited
    let _ = app_handle.emit(
        &format!("pty-exit-{}", session_id),
        serde_json::json!({ "session_id": session_id }),
    );
});
```

### Registration

Add to `main.rs` `generate_handler![]`:
- `commands::pty::spawn_pty`
- `commands::pty::write_pty`
- `commands::pty::resize_pty`
- `commands::pty::kill_pty`

Register `PtyState` as Tauri managed state in `main.rs`:
```rust
.manage(PtyState { sessions: Arc::new(Mutex::new(HashMap::new())) })
```

Register PTY cleanup in the existing shutdown path (`init_desktop()` line ~230-233) to kill all PTY processes on app exit.

## Frontend: New Component

### NPM Dependencies

```bash
npm install @xterm/xterm @xterm/addon-fit @xterm/addon-web-links
```

### File: `src/components/AgentTerminal.tsx`

```typescript
import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import "@xterm/xterm/css/xterm.css";

interface AgentTerminalProps {
  agentId: string;
  sessionId: string;
}
```

#### Behavior

1. **Mount:** Create `Terminal` instance with theme colors read from CSS variables (computed style). Attach `FitAddon` and `WebLinksAddon`. Open terminal in a container div.

2. **Connect:** Call `invoke("spawn_pty", { agentId, sessionId })`. Subscribe to `listen("pty-data-{sessionId}")`. On data events, call `terminal.write(data)`.

3. **Input:** `terminal.onData(data)` → `invoke("write_pty", { sessionId, data })`.

4. **Resize:** Use `ResizeObserver` on the container div. On resize, call `fitAddon.fit()`. Then `terminal.onResize(({ cols, rows }) => invoke("resize_pty", { sessionId, cols, rows }))`.

5. **Exit:** Listen for `pty-exit-{sessionId}`. On exit, write `"\r\n[Session ended. Press Enter to restart.]\r\n"` to the terminal. On next Enter keypress, call `spawn_pty` again to restart.

6. **Unmount:** Unlisten from events. Dispose the terminal. Do NOT kill the PTY (session persists for switching back).

7. **Reconnect:** If unmounted and remounted (user switches tabs and back), re-subscribe to `pty-data-{sessionId}`. The PTY is still running. Any output emitted while disconnected is lost (acceptable — user can scroll up in Claude's Ink UI, and `--resume` restores context).

#### Theme Integration

xterm.js does not support CSS variables in its `theme` option. Read computed values at mount:

```typescript
const style = getComputedStyle(document.documentElement);
const theme = {
  background: style.getPropertyValue("--bg").trim(),
  foreground: style.getPropertyValue("--text").trim(),
  cursor: style.getPropertyValue("--accent").trim(),
  selectionBackground: style.getPropertyValue("--accent").trim() + "40",
};
```

#### xterm.js Configuration

```typescript
const terminal = new Terminal({
  cursorBlink: true,
  cursorStyle: "bar",
  fontSize: 13,
  fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
  lineHeight: 1.2,
  scrollback: 10000,
  theme,
  allowProposedApi: true,
  convertEol: false,      // PTY handles line endings
  disableStdin: false,     // User needs to type
});
```

## Layout Changes

### AgentDetailView.tsx

**Current (left pane):**
```tsx
<AgentChatView chat={chat} pendingPermissions={...} onPermissionResponse={...} />
```

**New (left pane):**
```tsx
<AgentTerminal agentId={agentId} sessionId={sessionId} />
```

The right pane (FileViewer, ChangesPanel, DebugPanel tabs), the resizable divider, and the session header all remain unchanged.

### FlowWorkspaceView.tsx

**Current (bottom panel "Terminal" tab):**
```tsx
<StudioAssistantChat sessionId={studioSessionId} />
```

**New:**
```tsx
<AgentTerminal agentId={STUDIO_ASSISTANT_ID} sessionId={studioSessionId} />
```

The `StudioAssistantChat` wrapper component is deleted.

## Session Lifecycle

| Action | Behavior |
|--------|----------|
| Open agent session | `spawn_pty` creates PTY with `claude` + agent CLI flags. If session has history, `--resume {sessionId}` restores context. |
| Switch to another session | Frontend unsubscribes from `pty-data-*`. PTY continues running in background. On switch back, re-subscribe. |
| Create new session | `new_agent_session` (existing command) then `spawn_pty` with new session ID. |
| Delete session | `kill_pty` then `delete_agent_session` (existing command). |
| Claude exits | PTY child exits. Reader detects EOF. Emits `pty-exit-{sessionId}`. Terminal shows "Session ended." prompt. |
| App closes | All PTY processes killed via Tauri `on_exit` hook or Drop impl. |

## Files Removed After Migration

These files become dead code once terminal mode is fully working:

| File | Reason |
|------|--------|
| `src/components/chat/AgentChatThread.tsx` | Replaced by `AgentTerminal.tsx` |
| `src/components/AgentChatView.tsx` | Thin wrapper, no longer needed |
| `src/components/chat/useAgentChat.ts` | Chat state management replaced by PTY |
| `src/components/chat/useAgentChat.test.ts` | Tests for removed hook |
| `src/components/chat/chatParser.ts` | JSONL replay replaced by `claude --resume` |
| `src/components/chat/chatParser.test.ts` | Tests for removed parser |

| `src/components/chat/StickyTodoPanel.tsx` | Todo display handled by Claude Code |
| `src/components/chat/chatUtils.ts` | Utility functions no longer needed |
| `src/components/chat/chatUtils.test.ts` | Tests for removed utils |
| `src/components/chat/FilePreviewPanel.tsx` | Imports `@assistant-ui/react`, no longer needed |
| `src/components/chat/FilePreviewContext.ts` | Types used only by FilePreviewPanel |
| `src/components/chat/useShikiTokens.ts` | Used only by FilePreviewPanel |

| `src/api/interactStream.ts` | Stream-json IPC bridge replaced by PTY |
| `src-tauri/src/commands/chat.rs` (partially) | `agent_chat`, `stop_agent_chat`, `reconnect_agent_chat` commands removed. Other chat commands for flow execution may remain. |

**Note on `@assistant-ui/react` dependency:** `FlowRunChatView.tsx` also imports `@assistant-ui/react` but serves flow execution runs (not agent chat). If flow run chat stays, the `@assistant-ui/react` npm dependency must be kept. If we want to fully remove it, `FlowRunChatView.tsx` would also need to be migrated — but that is out of scope for this spec. For now, `@assistant-ui/react` stays as an npm dependency for flow runs only.

## Files Unchanged

| File | Reason |
|------|--------|
| `src-tauri/src/commands/agents.rs` | Agent/session CRUD stays the same |
| `src/api/client.ts` | Agent/session API wrappers stay (add `spawnPty`, `writePty`, `resizePty`, `killPty`) |
| `src/components/AgentDetailView.tsx` | Modified but not removed — swaps chat for terminal |
| `src/components/FlowWorkspaceView.tsx` | Modified — swaps StudioAssistantChat for AgentTerminal |
| `src/components/Sidebar.tsx` | No changes — agent tree and session management stay |
| `src/components/FileViewer.tsx` | Stays in the right pane |
| `src/components/ChangesPanel.tsx` | Stays in the right pane |
| `src/components/DebugPanel.tsx` | Stays in the right pane |
| `src-tauri/src/events.rs` | Hook events still work |
| `src/components/ChatPrimitives.tsx` | Still used by `FlowRunChatView.tsx` for flow run chat |
| `src/components/ToolRenderers.tsx` | Still used by `ChatPrimitives.tsx` / `FlowRunChatView.tsx` |
| `src/components/assistant-ui/tool-group.tsx` | Still used by `ChatPrimitives.tsx` |
| `src/components/assistant-ui/shiki-highlighter.tsx` | Still used by `ChatPrimitives.tsx` |
| `src/components/FlowRunChatView.tsx` | Flow run chat stays (uses `@assistant-ui/react`) |

## Files Created

| File | Purpose |
|------|---------|
| `src-tauri/src/commands/pty.rs` | PTY spawn/write/resize/kill commands |
| `src/components/AgentTerminal.tsx` | xterm.js terminal component |

## Edge Cases

1. **Binary not found:** If `claude` is not in PATH, `spawn_pty` returns an error. The frontend should display the error in the terminal area with instructions to install Claude Code.

2. **PTY already exists for session:** `spawn_pty` is idempotent — if a PTY is already running for the session, it returns the existing session ID without spawning a new one.

3. **Output while disconnected:** If the user switches away and back, any PTY output emitted while the frontend was unsubscribed is lost. This is acceptable — Claude Code's Ink UI manages its own screen buffer, and `--resume` restores conversation context.

4. **Theme changes:** If the user toggles dark/light mode, xterm.js theme won't auto-update (it's set at mount). A theme change listener could call `terminal.options.theme = newTheme`, but this is a nice-to-have, not a requirement.

5. **Multiple terminals for same session:** Only one xterm.js instance should be connected to a PTY at a time. The component should check if another instance is already connected and disconnect it first.

## Success Criteria

1. User clicks an agent session in the sidebar → an embedded terminal opens showing Claude Code's interactive REPL.
2. User types a message → Claude responds with streaming text, tool use displays, etc. — identical to running `claude` in iTerm.
3. Session resume works — closing and reopening a session restores context via `--resume`.
4. Terminal resizes correctly when the pane is dragged.
5. The right-side panels (FileViewer, ChangesPanel, DebugPanel) continue to work alongside the terminal.
6. The bottom-panel "Terminal" tab in FlowWorkspaceView also uses the embedded terminal.
7. Colors, cursor, and text rendering match the system theme.
