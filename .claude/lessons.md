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

## 2026-02-24 - AppState must derive Clone for Axum — use Arc for non-Clone fields

- **Context**: Axum requires `Clone` on the state type passed to `Router::with_state()`.
- **Mistake**: Removed `#[derive(Clone)]` from `AppState` while fixing `LiveClaudeProcess`. All route handlers broke with `the trait Clone is not implemented for AppState`.
- **Fix**: `AppState` must always derive `Clone`. Any non-Clone field needs to be wrapped in `Arc`. Example: `sandbox_provider: Arc<dyn SandboxProvider>`. Since all AppState fields are `Arc<...>`, `PathBuf`, or `broadcast::Sender` (all Clone), it works even when inner types (like `LiveClaudeProcess`) are not Clone.

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
- **Fix**: Iframe `src` points directly to the VM Manager's `web_terminal` URL (e.g., `http://34.100.130.60:PORT`). No proxy. Simpler, lower latency.
- **Implication**: The user's browser must be able to reach the VM Manager host directly. If the VM Manager is behind a firewall or NAT, the dynamic web terminal ports must be accessible. This is a deployment consideration, not a code issue.

## 2026-02-25 - shell_escape must use single-quote-with-replacement

- **Context**: PR review found 6+ shell injection vulnerabilities where user-supplied strings (sandbox names, file paths) were interpolated into shell commands without escaping.
- **Mistake**: Used `format!("'{}'", s)` which breaks if the string contains single quotes.
- **Fix**: `shell_escape()` wraps the string in single quotes and replaces any internal `'` with `'\''` (end quote, escaped quote, start quote). This is the standard POSIX shell escaping pattern. Example: `O'Brien` becomes `'O'\''Brien'`.

## 2026-02-25 - SandboxCapabilities::default_safe() must return Disabled, not AllowAll

- **Context**: `SandboxCapabilities::default_safe()` was returning `AllowAll` for network, filesystem, and exec capabilities. This meant new sandboxes had no restrictions by default — a security hole.
- **Mistake**: `default_safe()` returned `AllowAll` — no restrictions by default.
- **Fix**: Changed to return `Disabled` for all capabilities. Sandboxes now have no capabilities until explicitly granted. Security-first default.

## 2026-02-25 - exec_stream race condition — await stdout/stderr before Exit

- **Context**: In `ProcessExecStream`, the exit monitoring task could detect process exit and send the `Exit` event while stdout/stderr reading tasks still had buffered data. This caused truncated output.
- **Mistake**: Exit monitoring task sent `Exit` event as soon as the process exited, while stdout/stderr tasks still had buffered data.
- **Fix**: The exit task now `await`s the stdout and stderr `JoinHandle`s before sending the `Exit` event. This guarantees all output is drained before the stream signals completion.

## 2026-02-25 - Missing npm dependency breaks Studio build

- **Context**: `@uiw/react-md-editor` was used in Studio but not in `package.json`.
- **Mistake**: Assumed all dependencies were already declared. Build failed on a fresh checkout.
- **Fix**: `npm install @uiw/react-md-editor`. Always run `npx nx build cthulu-studio` to catch missing deps.

## 2026-02-25 - Nested KVM does NOT work on Apple Silicon

- **Context**: Tried to run Firecracker inside Lima VM on macOS (both vz and qemu backends).
- **Mistake**: Spent significant time trying to get `/dev/kvm` working.
- **Fix**: Apple Silicon does not expose ARM virtualization extensions to guest VMs. Neither Lima backend works. Use the VM Manager API on a real Linux server instead. Documented in `NOPE.md`.

## 2026-03-05 - Add features to the correct component, not the nearest one

- **Context**: Adding a search bar for template filtering when user clicks "Add New Flow".
- **Mistake**: Added the search bar to `TopBar.tsx` instead of `TemplateGallery.tsx`. The TopBar is always visible but the template search only makes sense inside the modal that appears when creating a new flow.
- **Fix**: Always trace the user flow first. "Add New Flow" opens `TemplateGallery` modal -- that is where the search bar belongs. Had to clean up the TopBar changes and redo them in `TemplateGallery.tsx`.

## 2026-03-05 - useDeferredValue and empty-state text must use the same value

- **Context**: Using `useDeferredValue(searchQuery)` for performance in `TemplateGallery.tsx` filtering.
- **Mistake**: The `filtered` useMemo used `deferredSearch` but the empty-state message (`No templates matching "X"`) used `searchQuery` (immediate). During fast typing, the message could flash "No templates matching X" while the grid still showed stale results from the previous deferred value.
- **Fix**: Any UI that depends on the filtered results must read from the same deferred value. Use `deferredSearch` for both the `useMemo` filter and the empty-state conditional text.

## 2026-03-05 - Consolidate related useEffects to avoid double-firing

- **Context**: Two separate `useEffect`s in `TemplateGallery.tsx` -- one for auto-focusing the search input on mount, another for Escape key handling.
- **Mistake**: The auto-focus effect had no cleanup for its `setTimeout`. If the component unmounted quickly, the timer would fire on an unmounted ref. Also, having two separate effects for related keyboard/focus logic made the component harder to reason about.
- **Fix**: Merge into a single `useEffect` that handles both auto-focus (with `clearTimeout` in cleanup) and keyboard events (with `removeEventListener` in cleanup). One effect, one cleanup function, no leaks.

## 2026-03-05 - npm workspace hoisting causes "Cannot find package" errors

- **Context**: `@tailwindcss/vite` was hoisted to `node_modules/@tailwindcss/vite/` at the monorepo root, but `vite` only existed in `cthulu-studio/node_modules/vite/`. When `@tailwindcss/vite` tried `import 'vite'`, Node resolved from the root and failed with `ERR_MODULE_NOT_FOUND`.
- **Mistake**: Did not account for npm workspace hoisting behavior. A dependency of a workspace package (`@tailwindcss/vite`) was hoisted to root, but its peer dependency (`vite`) was not.
- **Fix**: Add `vite` to the root `package.json` `devDependencies` so it gets hoisted alongside `@tailwindcss/vite`. General rule: if package A is hoisted and depends on package B, B must also be available at the root level.

## 2026-03-05 - npm audit fix vs major version bumps -- be conservative

- **Context**: Running `npm audit` showed vulnerabilities in `dompurify`, `immutable`, and `serialize-javascript`. Also bumping all packages to latest.
- **Mistake**: `npm-check-updates` wanted to bump Nx from 20 to 22 and Next.js from 15 to 16 -- both major version jumps with potential breaking changes.
- **Fix**: Use `--target minor` for packages with major bumps (Nx, Next.js) and `--reject` to exclude them from the blanket latest upgrade. Bump within the current major version: Nx 20.8.0 -> 20.8.4, Next.js 15.0.0 -> 15.5.12. Bump everything else to absolute latest. Always verify with `npx tsc --noEmit` for both projects after bumping.

---

## Sandbox Module Lessons (consolidated from root LESSONS.md)

## 2026-02-25 - FsJail path canonicalization on macOS

- **Context**: `FsJail.resolve()` used `std::fs::canonicalize()` to check path containment.
- **Mistake**: `canonicalize()` fails on non-existent paths. On macOS, `/var` → `/private/var` symlink breaks `starts_with` checks.
- **Fix**: Don't use `canonicalize()`. Manually normalize path components by resolving `.` and `..` segments. File: `src/sandbox/local_host/fs_jail.rs`.

## 2026-02-25 - FlowRunner construction sites are spread across codebase

- **Context**: Adding a field to `FlowRunner`.
- **Mistake**: Missing one of the 7 construction sites caused compile errors.
- **Fix**: Grep for `FlowRunner {` or `FlowRunner::new` — 4 in `src/server/flow_routes/mod.rs`, 3 in `src/flows/scheduler.rs`.

## 2026-02-25 - reqwest is already a dependency — use it

- **Context**: Needed an HTTP client for FC TCP transport.
- **Mistake**: Considered adding a new dependency.
- **Fix**: `reqwest` is already in `Cargo.toml`. Reuse existing deps before adding new ones.

## 2026-02-25 - Don't build Firecracker inside Lima VM

- **Context**: Tried building FC from source in Lima.
- **Mistake**: Slow and painful.
- **Fix**: Download the release binary directly from GitHub releases.

## 2026-02-25 - VM Manager API is the production path, not raw Firecracker

- **Context**: Choosing between direct FC control vs VM Manager abstraction.
- **Mistake**: `FirecrackerProvider` is ~780 lines of complex transport/provisioning logic.
- **Fix**: `VmManagerProvider` is ~200 lines of HTTP client code. Use VM Manager for production.

## 2026-02-25 - Browser terminal iframe — direct URL, not proxied

- **Context**: Embedding ttyd web terminal in BottomPanel.
- **Mistake**: Considered proxying WebSocket through Cthulu — adds complexity and latency.
- **Fix**: Iframe `src` points directly to VM Manager's `web_terminal` URL. No proxy. User's browser must reach VM Manager host directly.

## 2026-02-25 - VmManagerProvider node_vms is in-memory only

- **Context**: After server restart, clicking vm-sandbox node spun up a new VM, wasting resources.
- **Mistake**: `node_vms` HashMap was not persisted.
- **Fix**: Fall back to `vm_mappings` in `sessions.yaml`. Call `restore_node_vm(vm_id)` to verify VM is still alive, re-seed in-memory map. Only provision new if VM returns 404.

## 2026-02-25 - inject_oauth_token must write full credentials blob

- **Context**: Claude CLI inside VMs showing login prompt after token injection.
- **Mistake**: Only wrote `accessToken` and `tokenType` — Claude CLI treated as incomplete session.
- **Fix**: Write ALL fields: `accessToken`, `refreshToken`, `expiresAt`, `scopes`, `subscriptionType`, `rateLimitTier`.

## 2026-02-25 - .bashrc token injection must replace, not skip

- **Context**: Token refresh in VM `.bashrc`.
- **Mistake**: Had skip-if-present logic (`if ! grep -q`). Stale tokens were never updated.
- **Fix**: Always `sed -i` to delete existing line, then append new value. Idempotent.

## 2026-02-25 - isAuthError false positives on bare "401"

- **Context**: `isAuthError()` in `NodeChat.tsx` killed Claude sessions on false positives.
- **Mistake**: Matched any message containing `"401"` — including PR numbers, port numbers, hashes.
- **Fix**: Match specific patterns only: `"401 Unauthorized"`, `"HTTP 401"`, `"Authentication required"`, `"Invalid API key"`, `"not authenticated"`, `"claude login"`.

## 2026-02-25 - flow_routes.rs was split into a module directory

- **Context**: `src/server/flow_routes.rs` grew too large.
- **Mistake**: Documentation still referenced the old single-file path.
- **Fix**: Updated to `src/server/flow_routes/` with sub-modules (`mod.rs`, `crud.rs`, `sandbox.rs`, `interact.rs`, `node_chat.rs`).
