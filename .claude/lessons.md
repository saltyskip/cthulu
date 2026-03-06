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

- **Context**: Firecracker requires `/dev/kvm`. We tried two Lima VM backends on macOS Apple Silicon (M-series).
- **Mistake**: Spent significant time trying to get `/dev/kvm` working in both backends.
- **vz (Apple Virtualization.framework)**: `/dev/kvm` device node exists (kernel has `CONFIG_KVM=y` built-in), but opening it returns `ENODEV (errno 19)`. The CPU (`implementer: 0x61`, Apple Silicon) doesn't expose ARM virtualization extensions to the guest.
- **qemu**: Even with `nestedVirtualization: true` in Lima config, `/dev/kvm` doesn't appear at all. QEMU on Apple Silicon doesn't support nested virtualization for aarch64 guests.
- **Result**: Neither Lima backend gives you working KVM on Apple Silicon. This is a fundamental hardware/hypervisor limitation, not a configuration issue.
- **Fix**: Use a real Linux server (bare metal or cloud with nested virt) for Firecracker. The `RemoteSsh` transport was built for this — Cthulu on macOS talks to the FC API over TCP, host commands go over SSH. Documented in `NOPE.md`.

**Evidence**:
```
# Inside Lima (vz) — device exists but doesn't work
$ ls -la /dev/kvm
crw-rw-rw- 1 root root 10, 232 ... /dev/kvm
$ python3 -c "import os; os.open('/dev/kvm', os.O_RDWR)"
OSError: [Errno 19] No such device: '/dev/kvm'

# Inside Lima (qemu + nestedVirtualization:true) — device doesn't exist
$ ls /dev/kvm
ls: cannot access '/dev/kvm': No such file or directory
```

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

## Sandbox & Infrastructure Lessons

Hard-won knowledge from building the sandbox/Firecracker integration. Originally tracked in root `LESSONS.md`, consolidated here as the single source of truth.

## 2026-02-25 - FsJail path canonicalization on macOS

- **Context**: `FsJail.resolve()` used `std::fs::canonicalize()` to check path containment.
- **Mistake**: `canonicalize()` fails on non-existent paths (returns `Err`). On macOS, tempdir paths go through `/var` which is a symlink to `/private/var`. So `canonicalize("/var/folders/...")` returns `/private/var/folders/...` but `starts_with("/var/folders/...")` is false. The FsJail `resolve()` method broke in two ways: (1) paths that don't exist yet can't be canonicalized, (2) symlink resolution changes the prefix, breaking `starts_with` checks.
- **Fix**: Don't use `canonicalize()`. Instead, manually normalize path components by resolving `.` and `..` segments. This handles both non-existent paths and symlink prefix mismatches.
- **File**: `src/sandbox/local_host/fs_jail.rs`

## 2026-02-25 - Lima vz vs qemu — vz is the better choice for everything except KVM

- **Context**: Choosing between Lima VM backends on macOS.
- **Mistake**: Tried qemu first for `nestedVirtualization` support.
- **Fix**: The `default` Lima instance (vz) is faster, more stable, and has better macOS integration than qemu. The only reason to use qemu is `nestedVirtualization`, and on Apple Silicon it doesn't actually work (see "Nested KVM" lesson). Stick with vz for Lima instances.

## 2026-02-25 - Firecracker kernel image must match guest architecture

- **Context**: Downloading FC kernel/rootfs images from the CI S3 bucket.
- **Mistake**: Downloaded wrong architecture image. Mismatched arch = instant VM crash with no useful error message.
- **Fix**: The S3 paths include architecture: `aarch64/vmlinux-6.1` for ARM64, `x86_64/vmlinux-6.1` for Intel/AMD. Always verify the arch matches the host.

## 2026-02-25 - socat for exposing Unix sockets over TCP

- **Context**: Firecracker only speaks Unix domain socket. Need to reach it from another machine (or from macOS host into a Lima VM).
- **Mistake**: Tried various proxy approaches.
- **Fix**: Use socat: `socat TCP-LISTEN:8080,fork,reuseaddr UNIX-CONNECT:/tmp/firecracker.sock`. The `fork` flag is essential — without it, socat exits after the first connection. `reuseaddr` prevents "address already in use" errors on restart.

## 2026-02-25 - FlowRunner construction sites are spread across the codebase

- **Context**: Adding a field to `FlowRunner` requires updating 7 construction sites.
- **Mistake**: Missing one causes a compile error, but it's easy to miss one during refactoring.
- **Fix**: Grep for `FlowRunner {` or `FlowRunner::new` — 4 in `src/server/flow_routes/mod.rs`, 3 in `src/flows/scheduler.rs`. Check all 7 before compiling.

## 2026-02-25 - Rust edition 2024 implications

- **Context**: `Cargo.toml` specifies `edition = "2024"`.
- **Mistake**: Assumed standard edition 2021 behavior.
- **Fix**: Edition 2024 affects `use` declarations (some import rules changed). `async_trait` is still needed since native async traits aren't fully stabilized for dyn dispatch. Check edition-specific behavior when debugging unexpected compiler errors.

## 2026-02-25 - reqwest is already a dependency — use it

- **Context**: Needed an HTTP client for FC TCP transport.
- **Mistake**: Considered adding a new HTTP client dependency.
- **Fix**: `reqwest` is already in `Cargo.toml`. Reuse existing deps before adding new ones. The FC TCP transport uses it for `PUT`/`GET`/`PATCH` against the FC REST API.

## 2026-02-25 - Don't try to build Firecracker inside a Lima VM for development

- **Context**: Tried building FC from source inside Lima.
- **Mistake**: Building FC from source inside Lima works but is slow and painful.
- **Fix**: Download the release binary directly from GitHub releases:
```bash
# On the remote Linux server
ARCH=$(uname -m)  # aarch64 or x86_64
curl -Lo firecracker https://github.com/firecracker-microvm/firecracker/releases/download/v1.12.0/firecracker-v1.12.0-${ARCH}
chmod +x firecracker
sudo mv firecracker /usr/local/bin/
```

## 2026-02-25 - VM Manager API is the production path — not raw Firecracker

- **Context**: Choosing between direct Firecracker control (`FirecrackerProvider`) vs VM Manager abstraction (`VmManagerProvider`).
- **Mistake**: Direct Firecracker control requires SSH access to a Linux server, socat for Unix socket proxying, TAP network setup, rootfs management, and `/dev/kvm` which doesn't work on macOS.
- **Fix**: The VM Manager is a separate service running on the Linux server that abstracts all of this. Cthulu just makes HTTP calls (`POST /vms`, `GET /vms/{id}`, `DELETE /vms/{id}`). The VM Manager handles Firecracker lifecycle, networking, web terminal (ttyd) setup, and Claude CLI installation inside the VM. `VmManagerProvider` is ~200 lines of straightforward HTTP client code vs. `FirecrackerProvider` which is ~780 lines of complex transport/provisioning logic.

## 2026-02-25 - VmManagerProvider node_vms is in-memory only — use sessions.yaml as fallback

- **Context**: `VmManagerProvider.node_vms` is a `HashMap` in memory keyed by `flow_id::node_id`. After a server restart it's empty. Clicking a vm-sandbox node would spin up a brand-new VM even though the user's previous VM was still alive on the VM Manager host, wasting resources and losing the persistent workspace.
- **Mistake**: `node_vms` HashMap was not persisted. Server restart lost all VM associations.
- **Fix**: `get_node_vm()` now falls back to `vm_mappings` (persisted in `sessions.yaml`) when the in-memory map misses. It then calls `restore_node_vm(vm_id)` to verify the VM is still alive on the VM Manager, re-seeds the in-memory map, and returns the existing VM. Only if the VM is gone (404 from VM Manager) does it provision a new one.
- **File**: `src/sandbox/backends/vm_manager.rs` (`get_node_vm`, `restore_node_vm`, `get_or_create_vm_with_persisted`)

## 2026-02-25 - inject_oauth_token must write the complete credentials blob

- **Context**: `inject_oauth_token` was writing `~/.claude/.credentials.json` with only `accessToken` and `tokenType`. Claude CLI treated this as an incomplete/invalid session and displayed the login prompt every time a new VM connected, blocking automated use.
- **Mistake**: Only wrote `accessToken` and `tokenType` — Claude CLI forced re-login.
- **Fix**: `inject_oauth_token` now writes the complete credentials blob: `accessToken`, `refreshToken`, `expiresAt`, `scopes`, `subscriptionType`, `rateLimitTier`. The server reads all of these from the Keychain via `read_full_credentials()` and passes them through `POST /api/auth/refresh-token` → `inject_oauth_token`. Claude CLI now recognizes the session as fully authenticated.
- **File**: `src/sandbox/backends/vm_manager.rs` (`inject_oauth_token`), `src/server/auth_routes.rs` (`read_full_credentials`)

## 2026-02-25 - .bashrc CLAUDE_API_KEY sed was skip-if-present — stale token never updated

- **Context**: `inject_oauth_token` had logic to skip writing `CLAUDE_API_KEY` to `.bashrc` if the line already existed (`if ! grep -q CLAUDE_API_KEY ~/.bashrc`). On token refresh, the old (expired) value was never replaced, so the VM's shell environment kept the stale token.
- **Mistake**: Had skip-if-present logic. Stale tokens were never updated.
- **Fix**: Changed to always replace: use `sed -i` to delete any existing `CLAUDE_API_KEY` export line, then append the new value. This is idempotent and always correct.
- **File**: `src/sandbox/backends/vm_manager.rs` (`inject_oauth_token`)

## 2026-02-25 - isAuthError false positives on bare "401" substring

- **Context**: `isAuthError()` in `NodeChat.tsx` matched any message containing the string `"401"` — including PR numbers, issue IDs, port numbers, and hash strings. This caused false positives that killed the Claude session mid-task for unrelated reasons.
- **Mistake**: Bare numeric match on `"401"` was too broad.
- **Fix**: Tightened the check to only match explicit auth error patterns: HTTP 401 status messages (`"401 Unauthorized"`, `"HTTP 401"`), Claude auth error strings (`"Authentication required"`, `"Invalid API key"`, `"not authenticated"`), and the Claude CLI login prompt (`"claude login"`). Bare numeric matches were removed.
- **File**: `cthulu-studio/src/components/NodeChat.tsx` (`isAuthError`)

## 2026-02-25 - flow_routes.rs was split into a module directory

- **Context**: `src/server/flow_routes.rs` grew too large and was refactored into `src/server/flow_routes/` with sub-modules (`mod.rs`, `crud.rs`, `sandbox.rs`, `interact.rs`, `node_chat.rs`). Some documentation and skill files still referenced the old single-file path.
- **Mistake**: Documentation still referenced the old single-file path.
- **Impact**: Any grep for `flow_routes.rs` to find route registration or handler code returns no results. Use `src/server/flow_routes/` instead.
- **Files affected**: `docs/AGENT_DESIGN.md` code references, `CLAUDE.md` architecture map.
