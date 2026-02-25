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

---

## VM Manager / Sandbox Errors

### VM Manager unreachable — `POST /api/sandbox/vm/{flow_id}` returns 500

**Symptom**: Clicking a VM Sandbox node in Studio shows "Failed to create VM" error in VmTerminal.

**Cause**: The Cthulu backend can't reach the VM Manager API at `VM_MANAGER_URL`.

**Fix**:
1. Verify `VM_MANAGER_URL` is set in `.env` (e.g., `VM_MANAGER_URL=http://34.100.130.60:8080`)
2. Test connectivity: `curl http://34.100.130.60:8080/health`
3. Check if the VM Manager service is running on the remote server
4. If behind a firewall, ensure port 8080 is open

---

### Web terminal iframe shows blank/refused to connect

**Symptom**: VmTerminal component shows the iframe but it's blank or displays "refused to connect".

**Cause**: The ttyd web terminal port on the VM Manager host is not accessible from the user's browser. Remember: the iframe `src` points **directly** to the VM Manager's `web_terminal` URL (e.g., `http://34.100.130.60:PORT`), not through Cthulu.

**Fix**:
1. The user's browser must be able to reach the VM Manager host directly
2. Check that the web terminal port is open in the firewall
3. If the VM Manager is behind NAT, the web terminal ports (dynamic, one per VM) must be forwarded
4. Test in browser: navigate directly to the `web_terminal` URL returned by the API

---

### VM already exists for flow — duplicate creation attempts

**Symptom**: Rapidly clicking a VM Sandbox node creates multiple VMs.

**Cause**: The `VmManagerProvider` uses an in-memory `DashMap` to track `flow_id → vm_id` mappings. If two requests arrive before the first completes, both may call `POST /vms`.

**Fix**: This is handled by `get_or_create_vm()` which is idempotent — it checks the map first. If you see duplicates, the frontend may be sending multiple rapid requests. The VmTerminal component has loading state to prevent this, but if the race persists, add a debounce.

---

### `vm_manager` is `None` on AppState — routes return 404

**Symptom**: VM sandbox endpoints (`/api/sandbox/vm/{flow_id}`) return 404 or "VM Manager not configured".

**Cause**: `VM_MANAGER_URL` env var was not set at startup. The `vm_manager` field on `AppState` is `None`.

**Fix**: Set `VM_MANAGER_URL` in `.env` before starting the server. The env var dispatch in `main.rs` checks this first (highest priority provider).

---

### Missing `@uiw/react-md-editor` — Studio build fails

**Symptom**: `npx nx build cthulu-studio` fails with missing module `@uiw/react-md-editor`.

**Cause**: Pre-existing dependency not in `package.json`.

**Fix**: `npm install @uiw/react-md-editor`

---

### `exec_stream` race condition — Exit arrives before stdout drained

**Symptom**: Process output appears truncated. The `Exit` event arrives before all stdout/stderr lines have been yielded.

**Cause**: The exit monitoring task and the stdout/stderr reading tasks race. If the process exits quickly, the exit event can be sent while there's still buffered output.

**Fix**: The exit task now awaits stdout/stderr `JoinHandle`s before sending the `Exit` event. This ensures all output is drained first. See `ProcessExecStream` in `src/sandbox/local_host/process_supervisor.rs`.

---

### Shell injection in sandbox commands

**Symptom**: User-controlled strings (sandbox names, file paths, commands) could be injected into shell commands.

**Cause**: String interpolation without escaping in `format!()` calls passed to shell execution.

**Fix**: All user-supplied strings passed to shell commands use `shell_escape()` which implements the single-quote-with-replacement idiom: wrap in single quotes, replace internal `'` with `'\''`. This was fixed in 6+ locations per PR review feedback.

---

### `SandboxCapabilities::default()` was AllowAll

**Symptom**: New sandboxes had all capabilities enabled by default (network, filesystem, exec).

**Cause**: `default_safe()` was returning `AllowAll` instead of `Disabled`.

**Fix**: `default_safe()` now returns `Disabled` for all capabilities. Capabilities must be explicitly granted. This was a security fix identified in PR review.

---

## Nested KVM / Firecracker Errors

### `/dev/kvm` returns ENODEV inside Lima VM (Apple Silicon)

**Symptom**: Firecracker fails with "Cannot open /dev/kvm" or Python test shows `ENODEV (errno 19)`.

**Cause**: Apple Silicon (M-series) does not expose ARM virtualization extensions to guest VMs. Neither Lima `vz` nor `qemu` backends provide working KVM. The device node may exist but the hardware backing isn't there. See `NOPE.md` for full details.

**Fix**: Don't try to run Firecracker on macOS. Use the VM Manager API (`VM_MANAGER_URL`) which runs on a real Linux server with bare-metal KVM. This is the production path.

---

### Firecracker kernel arch mismatch — instant VM crash

**Symptom**: Firecracker process starts but VM crashes immediately with no useful error.

**Cause**: Downloaded kernel image doesn't match the host architecture (e.g., x86_64 kernel on aarch64 host or vice versa).

**Fix**: Match the architecture when downloading from FC CI S3 bucket:
- `aarch64/vmlinux-6.1` for ARM64
- `x86_64/vmlinux-6.1` for Intel/AMD
