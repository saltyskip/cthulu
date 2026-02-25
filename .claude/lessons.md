# Lessons Learned

Record corrections, mistakes, and insights here so future sessions can avoid repeating them.

<!-- Format:
## [Date] - Brief title
- **Context**: What you were doing
- **Mistake**: What went wrong
- **Fix**: What the correct approach is
-->

## 2026-02-21 - Axum 0.8 path parameter syntax

- **Context**: Adding new routes in `flow_routes.rs`
- **Mistake**: Used `:param` syntax (e.g., `/flows/:id`) which is Express-style. Axum 0.8 uses `{param}`.
- **Fix**: Always use `{param}` for path parameters: `/flows/{id}/nodes/{node_id}/interact`.

## 2026-02-21 - Server must be restarted after Rust changes

- **Context**: Added new routes to `flow_routes.rs` but the running binary didn't pick them up.
- **Mistake**: Expected hot-reload like frontend dev servers.
- **Fix**: Always restart the server (`cargo run`) after any Rust code change. There is no hot-reload for Rust binaries.

## 2026-02-21 - Never derive Clone on process handles

- **Context**: `LiveClaudeProcess` struct holds `ChildStdin`, `UnboundedReceiver<String>`, and `Child`.
- **Mistake**: Added `#[derive(Clone)]` to the struct. These types do not implement `Clone`.
- **Fix**: Do not derive `Clone` on structs containing Tokio process handles or mpsc receivers. Wrap in `Arc<Mutex<...>>` if shared access is needed. `AppState` can still derive `Clone` because it holds `Arc<Mutex<HashMap<String, LiveClaudeProcess>>>`.

## 2026-02-21 - Claude CLI stream-json message format

- **Context**: Implementing persistent Claude CLI processes with `--input-format stream-json`.
- **Mistake**: Sent `{"type":"user","content":"..."}`. Got error: `TypeError: undefined is not an object (evaluating 'R.message.role')`.
- **Fix**: Correct format is `{"type":"user","message":{"role":"user","content":"..."}}`. The `message` wrapper with `role` field is required.

## 2026-02-21 - stream-json output requires --verbose

- **Context**: Running Claude CLI with `--output-format stream-json`.
- **Mistake**: Omitted `--verbose` flag. Got error: `--output-format=stream-json requires --verbose`.
- **Fix**: Always pair `--output-format stream-json` with `--verbose`.

## 2026-02-21 - Double mutex lock anti-pattern in async streams

- **Context**: Reading stdout/stderr from a persistent process inside an `async_stream::stream!` block.
- **Mistake**: Acquired `live_processes.lock().await`, dropped it, then immediately re-acquired. Between drop and re-acquire another task could remove the process.
- **Fix**: Use a single lock acquisition. Drain both stderr and stdout into local variables, then drop the lock before yielding SSE events.

## 2026-02-21 - Orphaned code after async_stream block

- **Context**: Refactoring `interact_node()` from one-shot to persistent process model.
- **Mistake**: Left ~150 lines of old one-shot streaming logic after the `};` that closes the `async_stream::stream!` block. Variables like `child`, `stdin`, `stderr` were out of scope.
- **Fix**: When rewriting code inside `async_stream::stream! { ... };`, delete all old code between the closing `};` and the function's return statement. The stream block is a closure -- nothing outside it can reference variables defined inside.

## 2026-02-21 - stop handler must clean up process pool

- **Context**: `stop_node_interact()` killed the process via PID but didn't remove it from `live_processes`.
- **Mistake**: Dead process stayed in the pool. Next message found it, skipped spawning, tried to write to dead stdin, failed.
- **Fix**: Always remove from `live_processes` pool AND kill the process in `stop_node_interact()`. Use `pool.remove(&key)` to get ownership, then `proc.child.kill().await`.

## 2026-02-21 - display: flex breaks React Fragment children

- **Context**: PropertyPanel uses Fragments (`<>...</>`) to group form fields.
- **Mistake**: Added `display: flex` to `.property-panel`. Fragments don't create DOM elements, so flex layout couldn't see the children properly.
- **Fix**: Avoid `display: flex` on containers whose direct children are React Fragments. Use a wrapper `<div>` inside the Fragment, or restructure so flex children are real DOM elements.

## 2026-02-24 - AppState must derive Clone for Axum

- **Context**: Axum requires `Clone` on the state type passed to `Router::with_state()`.
- **Mistake**: Removed `#[derive(Clone)]` from `AppState` while fixing `LiveClaudeProcess`. All route handlers broke with `the trait Clone is not implemented for AppState`.
- **Fix**: `AppState` must always derive `Clone`. Since all its fields are `Arc<...>`, `PathBuf`, or `broadcast::Sender` (all Clone), it works even when inner types (like `LiveClaudeProcess`) are not Clone.

## 2026-02-25 - AppState needs both generic trait and specific provider for sandbox

- **Context**: Adding VM Manager sandbox endpoints that need `VmManagerProvider`-specific methods (`get_or_create_vm`, `get_flow_vm`, `destroy_flow_vm`).
- **Mistake**: Tried to downcast `Arc<dyn SandboxProvider>` to `VmManagerProvider`, which is fragile and error-prone.
- **Fix**: Store both on `AppState`: `sandbox_provider: Arc<dyn SandboxProvider>` (generic) and `vm_manager: Option<Arc<VmManagerProvider>>` (specific). Both point to the same instance. The `Option` is `None` when `VM_MANAGER_URL` isn't set.

## 2026-02-25 - BottomTab needs nodeKind to dispatch component rendering

- **Context**: BottomPanel needs to render `VmTerminal` for `vm-sandbox` nodes and `NodeChat` for `claude-code` nodes.
- **Mistake**: Initially tried to detect node kind inside BottomPanel by looking up the node — but the panel doesn't have direct access to the flow's node data.
- **Fix**: Extended `BottomTab` type with `nodeKind: string` field. Pass it through from `App.tsx` where the node click is handled. BottomPanel checks `tab.nodeKind` to decide which component to render.

## 2026-02-25 - VM browser terminal iframe points directly to VM Manager

- **Context**: Web terminal (ttyd) runs on a dynamic port on the VM Manager host. Needed to embed it in BottomPanel.
- **Mistake**: Considered proxying the WebSocket through Cthulu backend — this adds complexity and latency.
- **Fix**: Iframe `src` points directly to the VM Manager's `web_terminal` URL (e.g., `http://host:PORT`). No proxy. Simpler, lower latency. Trade-off: user's browser must be able to reach the VM Manager host directly.

## 2026-02-25 - shell_escape must use single-quote-with-replacement

- **Context**: PR review found shell injection in 6+ locations where user strings were interpolated into shell commands.
- **Mistake**: Used `format!("'{}'", s)` which breaks if the string contains single quotes.
- **Fix**: Proper shell escape: wrap in single quotes, replace internal `'` with `'\''`. Example: `O'Brien` → `'O'\''Brien'`. This is the standard POSIX pattern.

## 2026-02-25 - SandboxCapabilities::default_safe() must return Disabled

- **Context**: Creating default sandbox capabilities.
- **Mistake**: `default_safe()` returned `AllowAll` — no restrictions by default. Security hole.
- **Fix**: Return `Disabled` for all capabilities (network, filesystem, exec). Must be explicitly granted.

## 2026-02-25 - exec_stream Exit event must wait for stdout/stderr drain

- **Context**: `ProcessExecStream` in the sandbox process supervisor.
- **Mistake**: Exit monitoring task sent `Exit` event as soon as the process exited, while stdout/stderr tasks still had buffered data.
- **Fix**: Exit task now `await`s stdout/stderr `JoinHandle`s before sending `Exit`. Guarantees all output is yielded before stream completes.

## 2026-02-25 - Missing npm dependency breaks Studio build

- **Context**: `@uiw/react-md-editor` was used in Studio but not in `package.json`.
- **Mistake**: Assumed all dependencies were already declared. Build failed on a fresh checkout.
- **Fix**: `npm install @uiw/react-md-editor`. Always run `npx nx build cthulu-studio` to catch missing deps.

## 2026-02-25 - Nested KVM on Apple Silicon is a dead end

- **Context**: Tried to run Firecracker inside Lima VM on macOS (both vz and qemu backends).
- **Mistake**: Spent significant time trying to get `/dev/kvm` working.
- **Fix**: Apple Silicon does not expose ARM virtualization extensions to guest VMs. Neither Lima backend works. Use the VM Manager API on a real Linux server instead. Documented in `NOPE.md`.
