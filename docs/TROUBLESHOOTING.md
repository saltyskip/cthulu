# Troubleshooting Guide

Common errors and their fixes, consolidated from development sessions.

---

## Rust / Axum Errors

### `the trait Clone is not implemented for AppState`

**Error**: Every route handler fails with `Handler<_, _> is not implemented` and `AppState: Clone is not satisfied`.

**Cause**: `AppState` is missing `#[derive(Clone)]`. Axum requires state types to be `Clone`.

**Fix**: Add `#[derive(Clone)]` to `AppState`. This works because all fields are `Arc<...>`, `PathBuf`, or `broadcast::Sender` (all Clone). Inner types like `LiveClaudeProcess` don't need Clone since they're behind `Arc<Mutex<...>>`.

---

### `the trait bound Clone is not satisfied` on ChildStdin/Child/Receiver

**Error**: `#[derive(Clone)]` on a struct containing `tokio::process::ChildStdin`, `tokio::process::Child`, or `tokio::sync::mpsc::UnboundedReceiver`.

**Cause**: These Tokio types do not implement `Clone`.

**Fix**: Remove `#[derive(Clone)]` from the struct. If shared access is needed, wrap the struct in `Arc<Mutex<...>>`.

---

### Orphaned code after `async_stream::stream!` block

**Error**: Variables like `child`, `stdin`, `stderr`, `stdout` are "not found in this scope" after the stream block.

**Cause**: The `async_stream::stream! { ... };` macro creates a closure. Variables defined inside are not accessible outside. Old code was left between `};` and the function return.

**Fix**: Delete all code between the stream block's closing `};` and the function's return statement (`Ok(Sse::new(stream)...)`).

---

### Axum route 404 -- wrong path parameter syntax

**Error**: Routes return 404 even though they appear registered.

**Cause**: Used Express-style `:param` syntax instead of Axum 0.8's `{param}` syntax.

**Fix**: Use `{param}` for all path parameters:

```rust
.route("/flows/{id}/nodes/{node_id}/interact", post(interact_node))
```

---

### Double mutex lock in async stream

**Symptom**: Intermittent failures where a process disappears from the pool between two lock acquisitions.

**Cause**: Acquiring `pool.lock().await`, dropping it, then immediately re-acquiring. Another task can modify the pool between the two locks.

**Fix**: Use a single lock acquisition. Drain all needed data into local variables, then drop the lock:

```rust
let (line, stderr_batch) = {
    let mut pool = live_processes.lock().await;
    if let Some(proc) = pool.get_mut(&key) {
        let mut errs = Vec::new();
        while let Ok(err) = proc.stderr_lines.try_recv() { errs.push(err); }
        (proc.stdout_lines.try_recv().ok(), errs)
    } else { break; }
};
// Use line and stderr_batch outside the lock
```

---

## Claude CLI Errors

### `TypeError: undefined is not an object (evaluating 'R.message.role')`

**Cause**: Wrong JSON format for `--input-format stream-json`. Sent `{"type":"user","content":"..."}`.

**Fix**: Correct format includes `message.role`:

```json
{"type":"user","message":{"role":"user","content":"Your message"}}
```

---

### `--output-format=stream-json requires --verbose`

**Cause**: `--output-format stream-json` cannot be used without `--verbose`.

**Fix**: Always include both flags:

```bash
claude --print --verbose --output-format stream-json --input-format stream-json
```

---

### Process stays in pool after kill

**Symptom**: After stopping a node interact, the next message fails because it finds the dead process in the pool and tries to write to its closed stdin.

**Cause**: `stop_node_interact()` killed the process via PID but didn't remove it from the `live_processes` pool.

**Fix**: Remove from pool first, then kill:

```rust
let mut pool = state.live_processes.lock().await;
if let Some(mut proc) = pool.remove(&key) {
    let _ = proc.child.kill().await;
}
```

---

## Frontend Errors

### `display: flex` breaks React Fragment children

**Symptom**: Layout breaks in PropertyPanel -- child elements don't flex properly.

**Cause**: Parent has `display: flex` but direct children are React Fragments (`<>...</>`). Fragments don't create DOM elements, so flex can't see the actual children.

**Fix**: Either wrap Fragment contents in a `<div>`, or restructure so flex children are real DOM elements.

---

### React Flow edge renderers crash

**Symptom**: Edges disappear or throw errors after updating node data.

**Cause**: Nodes were replaced wholesale with `setNodes(newArray)`, destroying React Flow's internal `measured`, `internals`, and `handleBounds` properties.

**Fix**: Always spread-merge:

```tsx
setNodes((prev) =>
  prev.map((n) => n.id === id ? { ...n, data: { ...n.data, ...updates } } : n)
);
```

---

## Build / Dev Errors

### Rust changes not reflected

**Symptom**: Added new routes or changed logic but the running server doesn't pick them up.

**Cause**: Rust has no hot-reload. The running binary is the old compiled version.

**Fix**: Stop the server and restart with `cargo run -- serve`.

---

### Studio build fails with TypeScript errors

**Fix**: Run `npx nx build cthulu-studio` to see exact errors. Common causes:
- Missing imports after refactoring
- Type mismatches in API response handling
- Removed interfaces still referenced elsewhere

---

### `serde_yaml` deprecation warnings

**Context**: `serde_yaml` 0.9 is deprecated in favor of other YAML libraries.

**Status**: Safe to ignore for now. The crate still works and is widely used. Consider migrating to `serde_yml` or `yaml-rust2` in the future.
