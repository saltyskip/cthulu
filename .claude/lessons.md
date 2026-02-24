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
