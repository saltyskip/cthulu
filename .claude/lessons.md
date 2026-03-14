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

---

## Tauri IPC Migration Lessons (2026-03-13)

## 2026-03-13 - Tauri command names must match frontend invoke() names exactly

- **Context**: Converting 60 HTTP endpoints to Tauri `#[tauri::command]` functions.
- **Mistake**: Named Rust functions differently from what the frontend `invoke()` calls expect. Had 7 mismatches: `save_prompt` vs `create_prompt`, `get_token_status` vs `token_status`, `import_from_github` vs `import_github`, `setup_workflows` vs `setup_workflows_repo`, `list_workflows` vs `list_workspace_workflows`, `get_claude_status` vs `claude_status`, `respond_to_permission` vs `permission_response`.
- **Fix**: The Rust function name IS the command name. Always verify every `invoke("command_name")` in TypeScript has a matching `fn command_name()` registered in `generate_handler![]`. These fail silently at runtime — no compile-time check.

## 2026-03-13 - Tauri #[tauri::command] parameter naming: auto-camelCase for top-level only

- **Context**: Frontend sends `{ agentId, sessionId }` (camelCase) to Tauri commands with `agent_id: String, session_id: String` (snake_case) params.
- **Mistake**: Assumed serde `rename_all = "camelCase"` was needed on request structs. It's not — Tauri auto-converts snake_case parameter names to camelCase at the top level only.
- **Fix**: Top-level command parameters are auto-converted (snake→camel). But struct fields INSIDE those parameters use whatever serde expects — by default snake_case. If Rust struct has `request_id: String`, frontend must send `"request_id"` (snake_case) not `"requestId"`.

## 2026-03-13 - Tauri struct parameters must be passed wrapped, not flat

- **Context**: Rust command `fn create_agent(state, request: CreateAgentRequest)` expects `{ request: { name, ... } }`.
- **Mistake**: Frontend sent `{ data: { name, ... } }` — wrong wrapper key. Had 8 parameter shape mismatches where frontend sent flat args or used wrong key names.
- **Fix**: If a Rust command parameter is `request: MyStruct`, the frontend MUST send `{ request: { ...fields } }`. The JSON key must match the Rust parameter name (after Tauri's camelCase conversion). Always check the Rust function signature when writing `invoke()` calls.

## 2026-03-13 - AppState race condition in Tauri desktop apps

- **Context**: `setup()` closure spawns a background thread for async AppState init. The webview starts rendering immediately.
- **Mistake**: `app_handle.manage(app_state)` is called asynchronously after `setup()` returns. Commands invoked before it's ready cause panics or silent failures.
- **Fix**: Register a `tokio::sync::watch::Receiver<bool>` (ReadySignal) synchronously in `setup()`. Background thread sends `true` after `manage()`. All commands call `wait_ready()` before accessing state. 30-second timeout prevents hanging.

## 2026-03-13 - Dead invoke() calls fail silently in Tauri

- **Context**: Frontend called `invoke("stream_session_log")` but no backend command with that name existed.
- **Mistake**: The `.catch()` handler swallowed the error. The feature appeared to work because the real streaming happened via a separate `listen()` call.
- **Fix**: Audit every `invoke()` call against the `generate_handler![]` list. Dead invokes waste IPC round-trips and mask real errors. Remove them or connect them to actual commands.

## 2026-03-13 - Tauri event emit double-serialization

- **Context**: Emitting chat events from Rust background task to frontend via `app.emit(channel, &payload)`.
- **Mistake**: If `payload` is a `String` (already JSON), Tauri serializes it with serde, producing a JSON string containing JSON. Frontend gets `"\"{ ... }\"" ` and needs double-parse.
- **Fix**: Emit `serde_json::Value` directly instead of `String`, or emit the raw string and have the frontend handle the double-parse. Test the exact payload format by logging `event.payload` in the frontend listener.

## 2026-03-13 - Debug Tauri desktop apps by launching from terminal

- **Context**: Installed DMG app showed blank screen, 479% CPU. No visible errors.
- **Mistake**: Double-clicked the app icon — stdout/stderr go nowhere visible.
- **Fix**: Launch directly: `/Applications/Cthulu\ Studio.app/Contents/MacOS/cthulu-studio 2>&1`. All `println!`, `eprintln!`, and `tracing` output appears in the terminal. Check macOS unified log with `/usr/bin/log show --predicate 'process == "cthulu-studio"'` for WebKit-level issues.

## 2026-03-13 - Nightly Rust std::fmt::Arguments is not Send

- **Context**: Backend init uses tracing spans across `.await` points inside `init_desktop()`.
- **Mistake**: Tried to `tokio::spawn(init_desktop(...))` from Tauri's setup closure — Tauri requires spawned futures to be `Send`.
- **Fix**: Run the backend on a dedicated `std::thread` with its own `tokio::runtime`. The thread's `rt.block_on()` doesn't require `Send`. This avoids the nightly-specific issue where `std::fmt::Arguments` (used by tracing macros) is not `Send`.

## 2026-03-13 - No HTTP server in Tauri desktop mode

- **Context**: Migrating from `fetch()` + `EventSource` to `invoke()` + `listen()`.
- **Mistake**: Initially considered a hybrid approach keeping the HTTP server running alongside Tauri.
- **Fix**: Pure Tauri IPC — no ports, no CORS, no HTTP server. The `server_port` and `cors_origins` fields on AppState are only used in CLI mode (`cargo run -- serve`). Desktop mode uses `init_app_state()` directly without `start_server()`.

## 2026-03-13 - Frontend must listen() BEFORE invoke() for streaming

- **Context**: `interactStream.ts` sets up Tauri event listeners for chat streaming.
- **Mistake**: If `invoke("agent_chat")` is called before `listen("chat-event-{sid}")`, events emitted immediately after spawn are lost.
- **Fix**: Always `await listen(channel, callback)` first, THEN call `invoke()`. The listen promise resolves when the listener is registered. Events emitted between listen and invoke are captured.

## 2026-03-13 - Claude CLI hooks: command type for desktop, http type for server

- **Context**: Claude CLI hooks in `.claude/settings.local.json` need to communicate with the app.
- **Mistake**: Initially used `"type": "http"` hooks pointing to `localhost:{port}` — but desktop mode has no HTTP server.
- **Fix**: Desktop mode uses `"type": "command"` hooks with a shell script (`/tmp/cthulu-hook.sh`) that communicates via Unix domain socket (`/tmp/cthulu-{pid}.sock`). The `hook_socket_path` field on AppState is `Some(path)` in desktop mode, `None` in server mode.

## 2026-03-13 - async_stream cannot be used in Tauri commands

- **Context**: HTTP chat handler uses `async_stream::stream!` to yield SSE events.
- **Mistake**: Tried to reuse the same pattern in a `#[tauri::command]` function.
- **Fix**: Tauri commands return `Result<T, String>`, not streams. Replace `async_stream` with a spawned `tokio::spawn` background task that reads process output and calls `app.emit()` for each event. The command returns `{ session_id }` immediately.

## 2026-03-13 - Onboarding screen must wait for backend ready

- **Context**: Setup screen calls `checkSetupStatus()` which reads AppState fields (`github_pat`, `oauth_token`).
- **Mistake**: If the frontend renders before AppState is registered, the setup check fails and shows the setup screen even when credentials exist.
- **Fix**: The `check_setup_status` Tauri command includes `wait_ready()` guard. The frontend shows a blank loading state until the check resolves.

## 2026-03-13 - Missing hook socket means backend init failed

- **Context**: Testing the installed DMG — app was running but not functional.
- **Mistake**: Didn't check if the backend initialization completed.
- **Fix**: Check for `/tmp/cthulu-{pid}.sock` after launch. If it doesn't exist, the backend thread either failed or is still initializing. Also check stdout for the `"backend initialized"` message.

## 2026-03-14 - Tailwind v4 `@layer` and unlayered CSS reset `* { padding: 0 }` kills all Tailwind padding

- **Context**: `styles.css` had `* { margin: 0; padding: 0; box-sizing: border-box; }` after `@import "tailwindcss"`. Tailwind v4 padding utilities like `.p-2`, `.pl-4` had no visible effect on Radix Select dropdown items.
- **Root cause**: Tailwind v4 puts utilities inside `@layer utilities`. The `* { padding: 0 }` was **unlayered CSS**. In CSS, unlayered styles ALWAYS beat layered styles regardless of specificity. So `* { padding: 0 }` (specificity 0,0,0, unlayered) beat `.p-2` (specificity 0,1,0, in `@layer utilities`).
- **Fix**: Wrap the global reset in `@layer base { * { margin: 0; padding: 0; box-sizing: border-box; } }`. The `base` layer is lower priority than `utilities` in Tailwind's layer order, so utility classes now properly override the reset.
- **Rule**: In Tailwind v4, NEVER put CSS resets as unlayered styles. Always use `@layer base {}` for resets. Test in dev mode first (`npm run dev`) before building DMG.

## 2026-03-14 - Always clean caches before DMG rebuild after CSS changes

- **Context**: Made padding changes to `select.tsx`, rebuilt DMG, installed, but dropdown still showed old tight spacing.
- **Root cause**: Nx cache, Vite dist cache, and Tauri bundle cache can all serve stale assets. The binary embeds frontend assets at build time — if any cache layer serves old CSS, the DMG gets old styles baked in.
- **Fix**: Before rebuilding DMG after CSS/styling changes, always clean: `rm -rf dist .nx/cache src-tauri/target/release/bundle`. Then `npx tauri build --bundles dmg`.
- **Rule**: Always test CSS changes in `npm run dev` first. Only build DMG once visually confirmed in dev mode. When building DMG, clean caches if prior DMG had stale styles.

## 2026-03-14 - Always build DMG, reinstall, and launch after changes

- **Context**: User needs to visually validate and test every change in the actual desktop app, not just dev mode.
- **Workflow**: After ANY code change, always do the full cycle: clean caches → build DMG → detach old volume → mount new DMG → copy to /Applications → launch app. The user will test and provide feedback.
- **Commands**:
  1. `rm -rf dist .nx/cache src-tauri/target/release/bundle` (from `cthulu-studio/`)
  2. `npx tauri build --bundles dmg` (from `cthulu-studio/`)
  3. `hdiutil detach "/Volumes/Cthulu Studio" 2>/dev/null; open "...dmg" && sleep 4 && cp -R "/Volumes/Cthulu Studio/Cthulu Studio.app" /Applications/ && open "/Applications/Cthulu Studio.app"`
- **Rule**: Never mark a UI task as complete without building DMG, installing, and launching for the user to validate. The user tests in the actual desktop app, not dev mode.

---

## Architecture & Design Discoveries (2026-03-14)

## Flows vs Workflows — Two Completely Different Systems

- **Flows** (Agents tab): Local disk storage at `~/.cthulu/flows/`, UUID-based identity, have `enabled` boolean + cron scheduler, Run/Run(Manual) button, auto-saved via debounced `updateFlow()`. Enable/disable toggle is in `Sidebar.tsx` using a `Switch` component with `onToggleEnabled` callback. Run is via `api.triggerFlow(id)` → Tauri IPC `invoke("trigger_flow", { id })`.
- **Workflows** (Workflows tab): GitHub-backed, composite identity `workspace/name`, Save/Publish is manual. Activate/deactivate + Run are currently **UI-only** (no backend integration yet — logs to console). `editingWorkflow` state (`{ workspace, name } | null`) in `NavigationContext` determines whether TopBar shows flow controls vs workflow controls.
- **Key difference**: Flows are the execution engine. Workflows are the GitHub-synced definitions. They operate on entirely different state systems. Don't confuse them.

## WorkflowContext — Shared State Between Sidebar and WorkflowsView

- `WorkflowContext.tsx` provides shared state consumed by both `Sidebar.tsx` and `WorkflowsView.tsx`.
- **`enabledWorkflows: Set<string>`**: Keyed by `${workspace}::${name}`. Toggling in either Sidebar or WorkflowsView grid cards updates both views.
- **`workflowSearch: string`**: Typing in either the sidebar search input or the grid toolbar search filters both views simultaneously.
- **`setWfActiveWorkspace` wrapper**: Auto-clears `workflowSearch` when the workspace changes (wraps `setWfActiveWorkspaceRaw` in a `useCallback` that compares prev/next and calls `setWorkflowSearch("")` when different).
- **Performance**: Both components use `useDeferredValue(workflowSearch)` + `useMemo` for filtering. The deferred value prevents jank during fast typing.

## WorkspacePicker Combobox — Custom Component, No Radix Popover

- Replaced Radix `Select` in TopBar for the workspace dropdown with a custom `WorkspacePicker` component.
- Built with plain `div` + absolute positioning + `ref` + click-outside pattern. **No Radix Popover dependency** — Radix Popover is not installed in the project.
- Features: search filtering, Enter to select single match, Escape to clear search or close dropdown, checkmark icon on active workspace, click outside to dismiss.
- CSS classes: `.ws-picker-*` (~150 lines in `styles.css`).
- **Pattern**: When Radix doesn't provide the UX needed (searchable combobox), build a custom one with refs and event handlers rather than adding a new dependency.

## Sidebar Workflow Item Controls

- Each workflow item in the sidebar has: `Switch` toggle (activate/deactivate), `Play` button (run), `Trash2` button (delete).
- CSS classes: `.sidebar-wf-actions`, `.sidebar-run-btn`, `.sidebar-wf-enabled`, `.sidebar-wf-active-badge`.
- The meta line below the workflow name shows node count and active/inactive badge. Fixed with `.sidebar-wf-meta { display: flex; justify-content: space-between }`.

## Tauri Error Handling — Throws Strings Not Error Objects

- Tauri `invoke()` rejects with plain strings, not `Error` objects.
- **Always use**: `typeof e === "string" ? e : (e instanceof Error ? e.message : String(e))` in catch blocks.
- This was discovered during `CreateWorkflowDialog` error handling fix.

## CSS Variables for Theming — Never Hardcode Colors

- Always use: `var(--bg)`, `var(--border)`, `var(--accent)`, `var(--text)`, `var(--text-secondary)`, `var(--bg-secondary)`, `var(--success)`.
- Never hardcode hex/rgb values. The theme system in `ThemeContext.tsx` + `themes.ts` sets these variables on the root element.
- Custom components added this session follow the pattern (e.g., `.ws-picker-dropdown`, `.wf-search-input`, `.sidebar-wf-search-input`).

## useDeferredValue + Empty State Must Use Same Value

- When using `useDeferredValue(searchQuery)` for performance, the `useMemo` filter AND the empty-state message MUST both read from the deferred value.
- If the empty state reads from `searchQuery` (immediate) while the grid reads from `deferredSearch`, the message can flash "No results for X" while stale results are still visible.
- Already documented as lesson #18 in the context of TemplateGallery, but applies equally to WorkflowsView and Sidebar workflow search.

---

## Session Progress & Current State (as of 2026-03-14)

### What Has Been Built

1. **Tauri IPC migration** (previous sessions): Converted ~60 HTTP endpoints to pure Tauri `invoke()` + `listen()` commands. No HTTP server in desktop mode.
2. **UI padding & spacing fixes**: Select viewport, dialog gaps, theme selector trigger height (`h-7` → `h-8`), Tailwind v4 `@layer base` fix for global reset.
3. **Workflow card controls**: Switch toggle + Run button on each workflow card in `WorkflowsView.tsx`.
4. **Sidebar workflow controls**: Switch toggle + Play button + Delete button on each sidebar workflow item in `Sidebar.tsx`.
5. **Shared enabled state**: Lifted `enabledWorkflows` from local `WorkflowsView` state to `WorkflowContext`, shared between Sidebar and WorkflowsView.
6. **Workflow search bar (grid)**: Search input in WorkflowsView toolbar with clear button and empty state.
7. **Workflow search bar (sidebar)**: Compact search input in Sidebar's Workflows collapsible section, shared state with grid search.
8. **Workspace combobox**: Custom `WorkspacePicker` combobox in TopBar replacing Radix Select, with search filtering and keyboard support.
9. **Auto-clear search on workspace change**: `setWfActiveWorkspace` wrapper in WorkflowContext.
10. **"5 nodesActive" spacing fix**: Made sidebar workflow meta a flex row with space-between.

### Key Modified Files (This Session)

| File | What Changed |
|------|-------------|
| `cthulu-studio/src/contexts/WorkflowContext.tsx` | Added `enabledWorkflows`, `toggleWorkflowEnabled`, `isWorkflowEnabled`, `workflowSearch`, `setWorkflowSearch`; workspace-change auto-clears search |
| `cthulu-studio/src/components/WorkflowsView.tsx` | Shared enabled state + search via `useWorkflowContext`; search input in toolbar; `useDeferredValue` filtering; empty search state |
| `cthulu-studio/src/components/Sidebar.tsx` | Switch + Play + Delete on workflow items; compact search input; shared `workflowSearch`; meta line flex fix |
| `cthulu-studio/src/components/TopBar.tsx` | Custom `WorkspacePicker` combobox replacing Radix Select; lucide-react icons |
| `cthulu-studio/src/components/App.tsx` | Wired `toggleWorkflowEnabled`, `isWorkflowEnabled`, `onRunWorkflow` props to Sidebar |
| `cthulu-studio/src/styles.css` | ~300 lines of new CSS: `.ws-picker-*`, `.wf-search-*`, `.sidebar-wf-search-*`, `.sidebar-wf-meta`, `.sidebar-wf-actions`, `.sidebar-run-btn`, `.workflow-card-controls`, `.workflow-card-active`, `.workflow-active-badge`, `.workflow-card-empty` |

### Known Gaps / Not Yet Implemented

- **Backend integration for workflow activate/deactivate**: Currently UI-only (`enabledWorkflows` is in-memory React state). No Tauri command or persistence. Toggling logs to console but doesn't persist across app restarts.
- **Backend integration for workflow Run**: Currently UI-only (logs to console). No `invoke("run_workflow")` command exists yet.
- **Backend integration for workflow Delete**: Currently UI-only. No `invoke("delete_workflow")` command exists yet.
- **Workspace combobox polish**: May need styling adjustments based on user testing feedback.
- **Search UX polish**: May need adjustments based on user testing feedback (e.g., search icon positioning, clear button behavior).

### Dev Mode vs DMG Build

- **Dev mode**: `npx nx dev cthulu-studio` from `cthulu/` directory. Vite at `localhost:1420`, Tauri window opens. Good for fast iteration.
- **DMG build**: Required for user validation. Full cycle: clean caches → `npx tauri build --bundles dmg` → mount → copy to `/Applications/` → launch.
- **Output**: `cthulu-studio/src-tauri/target/release/bundle/dmg/Cthulu Studio_0.1.0_aarch64.dmg`
- **Debug tip**: Launch from terminal `/Applications/Cthulu\ Studio.app/Contents/MacOS/cthulu-studio 2>&1` to see all stdout/stderr.

## 2026-03-14 - agent_chat and stop_agent_chat IPC parameter mismatch broke agent chat

- **Context**: Agent chat didn't respond — user typed messages but got no response from Claude CLI.
- **Root cause**: The `agent_chat` Tauri command accepted `request: AgentChatRequest` (a struct parameter), but the frontend sent all fields flat: `{ agentId, prompt, sessionId, flowId, nodeId, images }`. Tauri expected `{ agentId, request: { prompt, session_id, ... } }`. Same issue with `stop_agent_chat` which expected `request: Option<StopChatRequest>`.
- **Fix**: Flattened both command signatures to use individual top-level parameters instead of struct wrappers. Tauri auto-converts `snake_case` params to `camelCase` for top-level params, so `session_id` → `sessionId` matches the frontend automatically. Removed the now-unused `AgentChatRequest` and `StopChatRequest` structs.
- **Rule**: When writing Tauri `#[tauri::command]` functions, prefer flat top-level parameters over struct wrappers. Flat params get automatic snake→camel conversion. Struct params require the frontend to know the exact wrapper key name AND use the struct's serde field naming convention (usually snake_case), which is confusing and error-prone. This is a recurring pattern — see lessons from 2026-03-13 about Tauri struct parameters.
- **Files**: `cthulu-studio/src-tauri/src/commands/chat.rs` — `agent_chat` (line 265) and `stop_agent_chat` (line 831).
