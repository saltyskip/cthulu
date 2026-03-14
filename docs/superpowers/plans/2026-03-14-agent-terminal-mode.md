# Agent Terminal Mode Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `@assistant-ui/react` agent chat UI with an embedded xterm.js terminal running Claude Code CLI in a real PTY via `portable-pty`.

**Architecture:** Rust backend spawns `claude` CLI in a PTY (via `portable-pty`). A background task reads PTY output and emits Tauri events. Frontend renders an xterm.js terminal that writes user input to the PTY and displays output from it.

**Tech Stack:** Rust + portable-pty (backend PTY), React 19 + @xterm/xterm (frontend terminal), Tauri 2 IPC (bridge)

**Spec:** `docs/superpowers/specs/2026-03-14-agent-terminal-mode-design.md`

---

## File Structure

### Files to Create

| File | Responsibility |
|------|----------------|
| `cthulu-studio/src-tauri/src/commands/pty.rs` | PTY Tauri commands: spawn_pty, write_pty, resize_pty, kill_pty |
| `cthulu-studio/src/components/AgentTerminal.tsx` | xterm.js terminal component for agents |

### Files to Modify

| File | Changes |
|------|---------|
| `cthulu-studio/src-tauri/Cargo.toml` | Add `portable-pty` dependency |
| `cthulu-studio/src-tauri/src/commands/mod.rs` | Add `pub mod pty;` |
| `cthulu-studio/src-tauri/src/main.rs` | Register PTY commands in `generate_handler![]`, manage `PtyState` |
| `cthulu-studio/src/components/AgentDetailView.tsx` | Replace `AgentChatView` with `AgentTerminal` |
| `cthulu-studio/src/components/FlowWorkspaceView.tsx` | Replace `StudioAssistantChat` with `AgentTerminal` |
| `cthulu-studio/src/api/client.ts` | Add `spawnPty`, `writePty`, `resizePty`, `killPty` wrappers |
| `cthulu-studio/package.json` | Add `@xterm/xterm`, `@xterm/addon-fit`, `@xterm/addon-web-links` |

---

## Chunk 1: Backend — PTY Commands

### Task 1: Add `portable-pty` Dependency

**Files:**
- Modify: `cthulu-studio/src-tauri/Cargo.toml`

- [ ] **Step 1: Add portable-pty to Cargo.toml**

In `cthulu-studio/src-tauri/Cargo.toml`, add `portable-pty` after the `hyper` line (line 28) and before the `cthulu` path dependency:

```toml
portable-pty = "0.9"
```

Note: Version 0.9 is the current release. The API used in this plan is verified against 0.9.

- [ ] **Step 2: Run cargo check to verify dependency resolves**

Run: `cargo check` from `cthulu/`
Expected: Compiles successfully (warnings OK). The `portable-pty` crate is downloaded and available.

- [ ] **Step 3: Commit**

```bash
git add cthulu-studio/src-tauri/Cargo.toml
git commit -m "feat: add portable-pty dependency for agent terminal PTY support"
```

---

### Task 2: Create `pty.rs` — PTY State and `spawn_pty` Command

**Files:**
- Create: `cthulu-studio/src-tauri/src/commands/pty.rs`
- Modify: `cthulu-studio/src-tauri/src/commands/mod.rs`

- [ ] **Step 1: Add `pub mod pty;` to commands/mod.rs**

In `cthulu-studio/src-tauri/src/commands/mod.rs`, add after line 11 (`pub mod workflows;`):

```rust
pub mod pty;
```

- [ ] **Step 2: Create pty.rs with PtyState struct and spawn_pty command**

Create `cthulu-studio/src-tauri/src/commands/pty.rs` with the following content:

```rust
use cthulu::api::AppState;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

/// A live PTY session for an agent.
/// Holds the master end of the PTY pair, writer handle, child process, and reader task.
pub struct PtySession {
    pub writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub master: Arc<Mutex<Box<dyn portable_pty::MasterPty + Send>>>,
    pub child: Arc<Mutex<Box<dyn portable_pty::Child + Send>>>,
    pub reader_handle: tokio::task::JoinHandle<()>,
}

/// Tauri-managed state for PTY sessions.
/// Separate from AppState because portable-pty types are not Clone.
pub struct PtyState {
    pub sessions: Arc<Mutex<HashMap<String, PtySession>>>,
}

impl PtyState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[derive(Serialize)]
pub struct SpawnPtyResponse {
    pub session_id: String,
}

#[tauri::command]
pub async fn spawn_pty(
    state: tauri::State<'_, AppState>,
    pty_state: tauri::State<'_, PtyState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    app: tauri::AppHandle,
    agent_id: String,
    session_id: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;

    // Check if PTY already exists for this session (idempotent)
    {
        let sessions = pty_state.sessions.lock().await;
        if sessions.contains_key(&session_id) {
            return Ok(json!({ "session_id": session_id }));
        }
    }

    // Look up agent config (async — requires .await)
    let agent = {
        let agent_repo = state.agent_repo.lock().await;
        agent_repo
            .get(&agent_id)
            .await
            .ok_or_else(|| format!("Agent not found: {agent_id}"))?
    };

    // Determine working directory
    let working_dir = agent
        .working_dir
        .clone()
        .unwrap_or_else(|| {
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
        });

    // Check if session has history (for --resume vs new session)
    // Use state.data_dir for consistency with existing code in chat.rs
    let session_log_path = state.data_dir
        .join("session_logs")
        .join(format!("{}.jsonl", &session_id));
    let has_history = session_log_path.exists()
        && std::fs::metadata(&session_log_path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);

    // Write hook settings (permissions, file change tracking)
    // Note: state.hook_socket_path is Option<PathBuf>, NOT behind a Mutex
    super::chat::write_hook_settings(
        &state.hook_socket_path,
        &working_dir,
        &session_id,
        &agent.hooks,
    );

    // Build the claude command
    let mut cmd = CommandBuilder::new("claude");

    // Permissions / allowed tools
    if !agent.permissions.is_empty() {
        cmd.arg("--allowedTools");
        cmd.arg(agent.permissions.join(","));
    }

    // Sub-agents (JSON-encoded, single --agents flag)
    if !agent.subagents.is_empty() {
        if let Ok(agents_json) = serde_json::to_string(&agent.subagents) {
            cmd.arg("--agents");
            cmd.arg(agents_json);
        }
    }

    // Session handling: new vs resume
    if has_history {
        cmd.arg("--resume");
        cmd.arg(&session_id);
    } else {
        // Build system prompt for new sessions
        let mut sys_prompt = format!(
            "You are \"{agent_name}\", an AI assistant. \
             Your working directory is: {working_dir}\n\
             Be efficient: short answers, no preamble, batch tool calls when possible.",
            agent_name = agent.name,
        );
        if let Some(ref extra) = agent.append_system_prompt {
            if !extra.is_empty() {
                sys_prompt.push_str(&format!("\n\n{extra}"));
            }
        }
        cmd.arg("--system-prompt");
        cmd.arg(&sys_prompt);
    }

    // Auto-permissions
    if agent.auto_permissions {
        cmd.arg("--dangerously-skip-permissions");
    }

    // Working directory
    cmd.cwd(&working_dir);

    // Environment for full color support in xterm.js
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("FORCE_COLOR", "3");
    // Remove CLAUDECODE env var to avoid conflicts
    cmd.env("CLAUDECODE", "");

    // Create PTY pair
    let pty_system = native_pty_system();
    let pty_pair = pty_system
        .openpty(PtySize {
            rows: 40,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to create PTY: {e}"))?;

    // Spawn child process in the PTY
    let child = pty_pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn claude in PTY: {e}"))?;

    // Get reader and writer from master
    let mut reader = pty_pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to clone PTY reader: {e}"))?;

    let writer = pty_pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

    // Spawn background reader task
    let app_handle = app.clone();
    let sid = session_id.clone();
    let pty_sessions_ref = pty_state.sessions.clone();

    let reader_handle = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let data = String::from_utf8_lossy(&buf[..n]).to_string();
                    let _ = app_handle.emit(
                        &format!("pty-data-{}", sid),
                        data,
                    );
                }
                Err(e) => {
                    eprintln!("PTY reader error for session {}: {}", sid, e);
                    break;
                }
            }
        }
        // PTY closed — child exited
        let _ = app_handle.emit(
            &format!("pty-exit-{}", sid),
            json!({ "session_id": sid }),
        );
        // Clean up session from map
        let sessions_ref = pty_sessions_ref.clone();
        let sid_owned = sid.clone();
        tokio::spawn(async move {
            let mut sessions = sessions_ref.lock().await;
            sessions.remove(&sid_owned);
        });
    });

    // Store PTY session
    let pty_session = PtySession {
        writer: Arc::new(Mutex::new(writer)),
        master: Arc::new(Mutex::new(pty_pair.master)),
        child: Arc::new(Mutex::new(child)),
        reader_handle,
    };

    {
        let mut sessions = pty_state.sessions.lock().await;
        sessions.insert(session_id.clone(), pty_session);
    }

    Ok(json!({ "session_id": session_id }))
}
```

- [ ] **Step 3: Run cargo check to verify spawn_pty compiles**

Run: `cargo check` from `cthulu/`
Expected: Compiles. May need to adjust `write_hook_settings` visibility (make it `pub(crate)` in chat.rs if not already).

- [ ] **Step 4: Commit**

```bash
git add cthulu-studio/src-tauri/src/commands/pty.rs cthulu-studio/src-tauri/src/commands/mod.rs
git commit -m "feat: add spawn_pty command with PTY session management"
```

---

### Task 3: Add `write_pty`, `resize_pty`, and `kill_pty` Commands

**Files:**
- Modify: `cthulu-studio/src-tauri/src/commands/pty.rs`

- [ ] **Step 1: Add write_pty command to pty.rs**

Append to `cthulu-studio/src-tauri/src/commands/pty.rs`:

```rust
#[tauri::command]
pub async fn write_pty(
    pty_state: tauri::State<'_, PtyState>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let sessions = pty_state.sessions.lock().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("No PTY session found for {}", session_id))?;

    let mut writer = session.writer.lock().await;
    writer
        .write_all(data.as_bytes())
        .map_err(|e| format!("Failed to write to PTY: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("Failed to flush PTY: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn resize_pty(
    pty_state: tauri::State<'_, PtyState>,
    session_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    let sessions = pty_state.sessions.lock().await;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("No PTY session found for {}", session_id))?;

    let master = session.master.lock().await;
    master
        .resize(PtySize {
            rows: rows as u16,
            cols: cols as u16,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to resize PTY: {e}"))?;

    Ok(())
}

#[tauri::command]
pub async fn kill_pty(
    pty_state: tauri::State<'_, PtyState>,
    session_id: String,
) -> Result<(), String> {
    let mut sessions = pty_state.sessions.lock().await;
    if let Some(session) = sessions.remove(&session_id) {
        // Kill the child process
        {
            let mut child = session.child.lock().await;
            let _ = child.kill();
        }
        // Abort the reader task
        session.reader_handle.abort();
        // master and writer are dropped automatically
    }
    Ok(())
}
```

- [ ] **Step 2: Run cargo check**

Run: `cargo check` from `cthulu/`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
git add cthulu-studio/src-tauri/src/commands/pty.rs
git commit -m "feat: add write_pty, resize_pty, kill_pty commands"
```

---

### Task 4: Register PTY Commands and State in main.rs

**Files:**
- Modify: `cthulu-studio/src-tauri/src/main.rs`
- Modify: `cthulu-studio/src-tauri/src/commands/chat.rs` (make `write_hook_settings` pub)

- [ ] **Step 1: Make `write_hook_settings` visible to pty.rs**

In `cthulu-studio/src-tauri/src/commands/chat.rs`, change the function signature from:

```rust
fn write_hook_settings(
```

to:

```rust
pub(crate) fn write_hook_settings(
```

- [ ] **Step 2: Register PtyState as managed state in main.rs**

In `cthulu-studio/src-tauri/src/main.rs`, after the line `app_handle.manage(app_state.clone());` (around line 216), add:

```rust
    app_handle.manage(commands::pty::PtyState::new());
```

- [ ] **Step 3: Register PTY commands in generate_handler![]**

In `cthulu-studio/src-tauri/src/main.rs`, inside the `generate_handler![]` macro, add after the Cloud section (around line 153, before the closing `]`):

```rust
            // PTY
            commands::pty::spawn_pty,
            commands::pty::write_pty,
            commands::pty::resize_pty,
            commands::pty::kill_pty,
```

- [ ] **Step 4: Add PTY cleanup on app exit**

In `cthulu-studio/src-tauri/src/main.rs`, find the app shutdown/exit handler (around line 230-233 in the `init_desktop` function or in the builder chain). Add cleanup code that kills all PTY sessions when the app exits:

In the `tauri::Builder` chain (around line 69), add an `on_window_event` handler if one doesn't exist, or add to the existing exit logic:

```rust
// After .manage(commands::pty::PtyState::new()):
// Store a clone of PtyState sessions Arc for cleanup
let pty_cleanup = pty_state_for_cleanup.clone(); // Will need to create this before .manage()
```

Alternatively, add a simpler approach — in `init_desktop`, after all setup is done, register a shutdown callback. The simplest reliable approach is to add a `Drop` impl on `PtyState`:

Add to `pty.rs`:

```rust
impl Drop for PtyState {
    fn drop(&mut self) {
        // Best-effort kill all PTY child processes on app exit
        if let Ok(mut sessions) = self.sessions.try_lock() {
            for (sid, session) in sessions.drain() {
                if let Ok(mut child) = session.child.try_lock() {
                    let _ = child.kill();
                }
                session.reader_handle.abort();
                eprintln!("Cleaned up PTY session: {}", sid);
            }
        }
    }
}
```

Note: `try_lock()` is used instead of `.await` because `Drop` is not async. This is best-effort cleanup.

- [ ] **Step 5: Run cargo check**

Run: `cargo check` from `cthulu/`
Expected: Compiles successfully with the PTY commands registered and PtyState managed.

- [ ] **Step 6: Commit**

```bash
git add cthulu-studio/src-tauri/src/main.rs cthulu-studio/src-tauri/src/commands/chat.rs cthulu-studio/src-tauri/src/commands/pty.rs
git commit -m "feat: register PTY commands and state in Tauri app with cleanup on exit"
```

---

## Chunk 2: Frontend — xterm.js Terminal Component and Layout Wiring

### Task 5: Install xterm.js NPM Dependencies

**Files:**
- Modify: `cthulu-studio/package.json`

- [ ] **Step 1: Install xterm.js packages**

Run from `cthulu/cthulu-studio/`:

```bash
npm install @xterm/xterm @xterm/addon-fit @xterm/addon-web-links
```

- [ ] **Step 2: Verify installation**

Run: `npm ls @xterm/xterm` from `cthulu/cthulu-studio/`
Expected: Shows `@xterm/xterm` in the dependency tree.

- [ ] **Step 3: Commit**

```bash
git add cthulu-studio/package.json cthulu-studio/package-lock.json
git commit -m "feat: add xterm.js dependencies for agent terminal"
```

---

### Task 6: Add PTY IPC Wrappers to client.ts

**Files:**
- Modify: `cthulu-studio/src/api/client.ts`

- [ ] **Step 1: Add spawnPty, writePty, resizePty, killPty functions**

Add the following functions at the end of `cthulu-studio/src/api/client.ts` (before the final closing brace or at the bottom of the file):

```typescript
// ─── PTY ────────────────────────────────────────────────────────────

export async function spawnPty(
  agentId: string,
  sessionId: string,
): Promise<{ session_id: string }> {
  log("pty", `invoke spawn_pty agentId=${agentId} sessionId=${sessionId}`);
  return invoke<{ session_id: string }>("spawn_pty", {
    agentId,
    sessionId,
  });
}

export async function writePty(
  sessionId: string,
  data: string,
): Promise<void> {
  await invoke("write_pty", { sessionId, data });
}

export async function resizePty(
  sessionId: string,
  cols: number,
  rows: number,
): Promise<void> {
  await invoke("resize_pty", { sessionId, cols: Math.floor(cols), rows: Math.floor(rows) });
}

export async function killPty(sessionId: string): Promise<void> {
  log("pty", `invoke kill_pty sessionId=${sessionId}`);
  await invoke("kill_pty", { sessionId });
}
```

- [ ] **Step 2: Run frontend build to verify**

Run: `npx nx build cthulu-studio` from `cthulu/`
Expected: Builds successfully.

- [ ] **Step 3: Commit**

```bash
git add cthulu-studio/src/api/client.ts
git commit -m "feat: add PTY IPC wrapper functions"
```

---

### Task 7: Create AgentTerminal.tsx Component

**Files:**
- Create: `cthulu-studio/src/components/AgentTerminal.tsx`

- [ ] **Step 1: Create the AgentTerminal component**

Create `cthulu-studio/src/components/AgentTerminal.tsx`:

```tsx
import { useEffect, useRef, useCallback } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { spawnPty, writePty, resizePty } from "../api/client";
import "@xterm/xterm/css/xterm.css";

interface AgentTerminalProps {
  agentId: string;
  sessionId: string;
}

/**
 * Embedded xterm.js terminal that connects to a Claude Code CLI process
 * running in a real PTY on the Rust backend.
 */
export default function AgentTerminal({ agentId, sessionId }: AgentTerminalProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const unlistenDataRef = useRef<UnlistenFn | null>(null);
  const unlistenExitRef = useRef<UnlistenFn | null>(null);
  const resizeObserverRef = useRef<ResizeObserver | null>(null);
  const connectedSessionRef = useRef<string | null>(null);

  // Read CSS variable values for xterm.js theme
  const getTheme = useCallback(() => {
    const style = getComputedStyle(document.documentElement);
    const bg = style.getPropertyValue("--bg").trim();
    const text = style.getPropertyValue("--text").trim();
    const accent = style.getPropertyValue("--accent").trim();
    return {
      background: bg || "#1a1a2e",
      foreground: text || "#e0e0e0",
      cursor: accent || "#7c3aed",
      selectionBackground: (accent || "#7c3aed") + "40",
    };
  }, []);

  // Cleanup function
  const cleanup = useCallback(() => {
    unlistenDataRef.current?.();
    unlistenDataRef.current = null;
    unlistenExitRef.current?.();
    unlistenExitRef.current = null;
    resizeObserverRef.current?.disconnect();
    resizeObserverRef.current = null;
    if (terminalRef.current) {
      terminalRef.current.dispose();
      terminalRef.current = null;
    }
    fitAddonRef.current = null;
    connectedSessionRef.current = null;
  }, []);

  useEffect(() => {
    if (!containerRef.current) return;

    // Create terminal
    const terminal = new Terminal({
      cursorBlink: true,
      cursorStyle: "bar",
      fontSize: 13,
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
      lineHeight: 1.2,
      scrollback: 10000,
      theme: getTheme(),
      allowProposedApi: true,
      convertEol: false,
      disableStdin: false,
    });

    const fitAddon = new FitAddon();
    const webLinksAddon = new WebLinksAddon();

    terminal.loadAddon(fitAddon);
    terminal.loadAddon(webLinksAddon);
    terminal.open(containerRef.current);

    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;

    // Fit to container
    requestAnimationFrame(() => {
      fitAddon.fit();
    });

    // Forward user input to PTY
    terminal.onData((data: string) => {
      writePty(sessionId, data).catch((err) => {
        console.error("write_pty error:", err);
      });
    });

    // Forward resize events to PTY
    terminal.onResize(({ cols, rows }: { cols: number; rows: number }) => {
      resizePty(sessionId, cols, rows).catch((err) => {
        console.error("resize_pty error:", err);
      });
    });

    // Watch container for size changes
    const observer = new ResizeObserver(() => {
      requestAnimationFrame(() => {
        fitAddonRef.current?.fit();
      });
    });
    observer.observe(containerRef.current);
    resizeObserverRef.current = observer;

    // Spawn PTY and subscribe to output
    let cancelled = false;

    (async () => {
      try {
        // Spawn the PTY (idempotent — returns existing if already running)
        await spawnPty(agentId, sessionId);

        if (cancelled) return;
        connectedSessionRef.current = sessionId;

        // Listen for PTY output
        const unlistenData = await listen<string>(
          `pty-data-${sessionId}`,
          (event) => {
            terminal.write(event.payload);
          },
        );
        unlistenDataRef.current = unlistenData;

        // Listen for PTY exit
        const unlistenExit = await listen<{ session_id: string }>(
          `pty-exit-${sessionId}`,
          (_event) => {
            terminal.write(
              "\r\n\x1b[90m[Session ended. Press Enter to restart.]\x1b[0m\r\n",
            );
            // On next Enter, restart the PTY
            const disposable = terminal.onData((data: string) => {
              if (data === "\r" || data === "\n") {
                disposable.dispose();
                terminal.clear();
                terminal.write("Restarting session...\r\n");
                spawnPty(agentId, sessionId)
                  .then(() => {
                    terminal.write("Session restarted.\r\n");
                  })
                  .catch((err) => {
                    terminal.write(
                      `\r\n\x1b[31mFailed to restart: ${err}\x1b[0m\r\n`,
                    );
                  });
              }
            });
          },
        );
        unlistenExitRef.current = unlistenExit;

        // Send initial resize after connection
        const { cols, rows } = terminal;
        await resizePty(sessionId, cols, rows);
      } catch (err) {
        const msg = typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
        terminal.write(
          `\r\n\x1b[31mFailed to start terminal: ${msg}\x1b[0m\r\n`,
        );
        if (msg.includes("not found") || msg.includes("No such file")) {
          terminal.write(
            "\x1b[33mMake sure Claude Code CLI is installed: brew install claude-code\x1b[0m\r\n",
          );
        }
      }
    })();

    return () => {
      cancelled = true;
      cleanup();
    };
  }, [agentId, sessionId, getTheme, cleanup]);

  return (
    <div
      ref={containerRef}
      className="agent-terminal"
      style={{
        width: "100%",
        height: "100%",
        overflow: "hidden",
      }}
    />
  );
}
```

- [ ] **Step 2: Run frontend build to verify**

Run: `npx nx build cthulu-studio` from `cthulu/`
Expected: Builds successfully. The component compiles.

- [ ] **Step 3: Commit**

```bash
git add cthulu-studio/src/components/AgentTerminal.tsx
git commit -m "feat: create AgentTerminal component with xterm.js + PTY integration"
```

---

### Task 8: Wire AgentTerminal into AgentDetailView

**Files:**
- Modify: `cthulu-studio/src/components/AgentDetailView.tsx`

**Important:** `AgentDetailView` currently uses `chat.gitSnapshot`, `chat.debugEvents`, and `chat.clearDebugEvents` from the `useAgentChat` hook to feed the right-pane `ChangesPanel` and `DebugPanel`. When removing `useAgentChat`, we need to handle this:

- **`chat.debugEvents` / `chat.clearDebugEvents`**: These tracked SSE events from the stream-json protocol. In terminal mode, there are no SSE events — Claude Code handles its own display. The `DebugPanel` will show hook events only (which already come via the `hookDebugEvents` prop from `App.tsx`). Chat-specific debug events are no longer available.
- **`chat.gitSnapshot`**: Used by `ChangesPanel` to show git diffs. We'll replace this with a periodic poll using the existing `getGitSnapshot` API, or just pass `null` initially and let `ChangesPanel` fetch its own data.

- [ ] **Step 1: Replace AgentChatView with AgentTerminal and handle right-pane data**

In `cthulu-studio/src/components/AgentDetailView.tsx`:

1. Replace imports:
   ```typescript
   // REMOVE these lines:
   import AgentChatView, { useAgentChat } from "./AgentChatView";
   import type { DebugEvent } from "./chat/useAgentChat";
   
   // ADD:
   import AgentTerminal from "./AgentTerminal";
   ```

2. Remove the `useAgentChat` hook call (line 38):
   ```typescript
   // REMOVE: const chat = useAgentChat(agentId, sessionId);
   ```

3. Replace the `<AgentChatView>` in the left pane (lines 91-97):

   Replace:
   ```tsx
   <AgentChatView
     chat={chat}
     pendingPermissions={pendingPermissions}
     onPermissionResponse={onPermissionResponse}
   />
   ```

   With:
   ```tsx
   <AgentTerminal agentId={agentId} sessionId={sessionId} />
   ```

4. Fix the `ChangesPanel` — replace `chat.gitSnapshot` with `null` (lines 104-109):

   Replace:
   ```tsx
   <ChangesPanel
     agentId={agentId}
     sessionId={sessionId}
     gitSnapshot={chat.gitSnapshot}
     hookChangedFiles={hookChangedFiles}
   />
   ```

   With:
   ```tsx
   <ChangesPanel
     agentId={agentId}
     sessionId={sessionId}
     gitSnapshot={null}
     hookChangedFiles={hookChangedFiles}
   />
   ```

5. Fix the `DebugPanel` — pass empty arrays for chat events (lines 113-118):

   Replace:
   ```tsx
   <DebugPanel
     chatEvents={chat.debugEvents}
     hookEvents={hookDebugEvents}
     onClearChat={chat.clearDebugEvents}
     onClearHook={onClearHookDebug}
   />
   ```

   With:
   ```tsx
   <DebugPanel
     chatEvents={[]}
     hookEvents={hookDebugEvents}
     onClearChat={() => {}}
     onClearHook={onClearHookDebug}
   />
   ```

6. Remove unused props from the interface (`pendingPermissions`, `onPermissionResponse`) if they are no longer needed. However, if these are still passed from `App.tsx`, keep the interface but just don't use them. Clean up unused variables.

7. Remove the `DebugEvent` type import if no longer used.

- [ ] **Step 2: Run frontend build to verify**

Run: `npx nx build cthulu-studio` from `cthulu/`
Expected: Builds successfully. Clean up any unused import/variable warnings.

- [ ] **Step 3: Commit**

```bash
git add cthulu-studio/src/components/AgentDetailView.tsx
git commit -m "feat: replace AgentChatView with AgentTerminal in agent workspace"
```

---

### Task 9: Wire AgentTerminal into FlowWorkspaceView Bottom Panel

**Files:**
- Modify: `cthulu-studio/src/components/FlowWorkspaceView.tsx`

- [ ] **Step 1: Replace StudioAssistantChat with AgentTerminal**

In `cthulu-studio/src/components/FlowWorkspaceView.tsx`:

1. Add the import of `AgentTerminal`:
   ```typescript
   import AgentTerminal from "./AgentTerminal";
   ```

2. Replace the `StudioAssistantChat` usage in the bottom panel (around line 434-438):

   Replace:
   ```tsx
   <StudioAssistantChat
     key={`workspace-chat:${STUDIO_ASSISTANT_ID}::${studioSessionId}`}
     sessionId={studioSessionId}
   />
   ```

   With:
   ```tsx
   <AgentTerminal
     key={`workspace-term:${STUDIO_ASSISTANT_ID}::${studioSessionId}`}
     agentId={STUDIO_ASSISTANT_ID}
     sessionId={studioSessionId}
   />
   ```

3. Remove the `StudioAssistantChat` function definition at the bottom of the file (lines 505-511).

4. Remove the now-unused imports of `useAgentChat` and `AgentChatView`.

- [ ] **Step 2: Run frontend build to verify**

Run: `npx nx build cthulu-studio` from `cthulu/`
Expected: Builds successfully.

- [ ] **Step 3: Commit**

```bash
git add cthulu-studio/src/components/FlowWorkspaceView.tsx
git commit -m "feat: replace StudioAssistantChat with AgentTerminal in flow workspace"
```

---

### Task 10: Add CSS for Agent Terminal

**Files:**
- Modify: `cthulu-studio/src/styles.css`

- [ ] **Step 1: Add agent-terminal styles**

In `cthulu-studio/src/styles.css`, add the following styles (can be added at the end of the file, inside a `@layer base {}` block if using Tailwind v4):

```css
/* Agent Terminal */
.agent-terminal {
  background: var(--bg);
}

.agent-terminal .xterm {
  height: 100%;
  padding: 4px;
}

.agent-terminal .xterm-viewport {
  scrollbar-width: thin;
  scrollbar-color: var(--border) transparent;
}

.agent-terminal .xterm-viewport::-webkit-scrollbar {
  width: 6px;
}

.agent-terminal .xterm-viewport::-webkit-scrollbar-thumb {
  background: var(--border);
  border-radius: 3px;
}
```

- [ ] **Step 2: Run frontend build to verify**

Run: `npx nx build cthulu-studio` from `cthulu/`
Expected: Builds successfully.

- [ ] **Step 3: Commit**

```bash
git add cthulu-studio/src/styles.css
git commit -m "feat: add CSS styles for agent terminal"
```

---

### Task 11: Full Build Verification and Dev Test

**Files:** None (verification only)

- [ ] **Step 1: Run cargo check for Rust backend**

Run: `cargo check` from `cthulu/`
Expected: Compiles with only pre-existing warnings.

- [ ] **Step 2: Run frontend build**

Run: `npx nx build cthulu-studio` from `cthulu/`
Expected: TypeScript compilation + Vite build succeed.

- [ ] **Step 3: Test in dev mode**

Run: `npx nx dev cthulu-studio` from `cthulu/`
Expected:
1. App opens at localhost:1420 / Tauri window
2. Click on an agent in the sidebar → agent workspace shows xterm.js terminal in the left pane
3. Terminal should show Claude Code's interactive REPL
4. Type a message → Claude responds
5. Right-side panels (Files, Changes, Debug) still visible

- [ ] **Step 4: Test the flow workspace terminal tab**

1. Navigate to a flow
2. Click the "Terminal" tab in the bottom panel
3. Should show an xterm.js terminal with Claude Code for the Studio Assistant

- [ ] **Step 5: Commit final state**

```bash
git add -A
git commit -m "feat: agent terminal mode - complete implementation"
```
