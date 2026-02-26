# AI Workflow Guide

How AI agents should work in this monorepo. The root [CLAUDE.md](../CLAUDE.md) covers **what the rules are**; this document covers **how to work**.

---

## Plan Before You Code

For any non-trivial task (more than a few lines, multiple files, or unclear scope):

1. **Explore first** -- read the files you'll change, understand existing patterns
2. **Write a plan** with specific files, changes, and verification steps
3. **Get approval** before implementing
4. **Re-plan on failure** -- if your approach doesn't work, stop and re-plan instead of brute-forcing

**Skip planning for**: typo fixes, single-line changes, tasks with very specific instructions.

---

## Subagent Strategy

Use subagents to parallelize work and protect context:

- **Offload research** -- use Explore agents for codebase investigation
- **One task per subagent** -- keep scopes narrow and focused
- **Run independent agents in parallel** -- don't serialize what can be parallelized
- **Don't duplicate work** -- if you delegate research to a subagent, don't also search yourself

---

## Self-Improvement Loop

After receiving a correction or discovering a mistake:

1. Fix the immediate issue
2. Check if the lesson applies elsewhere in the current task
3. Record the lesson in `.claude/LESSONS.md` if it's likely to recur

**Format for lessons**:

```markdown
## [Date] - Brief title
- **Context**: What you were doing
- **Mistake**: What went wrong
- **Fix**: What the correct approach is
```

At the start of each session, review `.claude/LESSONS.md` for recent entries.

---

## Verification Before Done

Never mark a task as complete without verifying your work:

### Build & Lint Checklist

| Change type | Verification command |
|-------------|---------------------|
| Rust backend (any .rs file) | `cargo check` (fast) or `cargo build` (full) |
| Rust logic | `cargo test` |
| Rust style | `cargo clippy -- -D warnings` |
| Studio component | `npx nx build cthulu-studio` |
| Studio + backend together | `cargo check && npx nx build cthulu-studio` |
| Site page/section | `npx nx build cthulu-site` |
| New API endpoint | `cargo check`, restart server, test with `curl` |
| Flow config / YAML | Start server, load flow in Studio, verify in UI |
| Sandbox module (Rust) | `cargo test sandbox && cargo check` |
| VM Manager integration | `cargo test vm_manager && cargo check` |
| VmTerminal / BottomPanel | `npx nx build cthulu-studio` |
| Full sandbox (both ends) | `cargo check && cargo test && npx nx build cthulu-studio` |
| Template routes | `cargo check`, restart server, `curl /api/templates` |
| Auth routes | `cargo check`, restart server, `curl /api/auth/token-status` |
| Template Gallery UI | `npx nx build cthulu-studio`, open Studio and click + New |

### Staff-Engineer Bar

Before submitting, ask yourself:

- Would a staff engineer approve this without comments?
- Is this the **simplest correct approach**?
- Are there any edge cases I haven't handled?
- Did I leave orphaned code, dead imports, or `// TODO` comments?

---

## Autonomous Bug Fixing

When fixing bugs:

1. **Reproduce first** -- understand the exact failure before changing code
2. **Find root cause** -- don't patch symptoms
3. **Prove the fix works** -- run the relevant build/lint/test command
4. **Check for siblings** -- does the same bug pattern exist elsewhere?
5. **Zero hand-holding** -- the fix should be complete; no TODOs or "the user should also..."

---

## Task Tracking

For multi-step work:

1. **Create tasks upfront** -- break the work into trackable units before starting
2. **Mark progress** -- set tasks to `in_progress` when starting, `completed` when done
3. **Capture discoveries** -- if you find additional work needed, create new tasks
4. **One in-progress at a time** -- complete current task before starting the next

---

## Working with the Template Gallery

The template gallery (`TemplateGallery.tsx`) lets users start from a pre-built workflow instead of a blank canvas.

### Key Architecture Points

- **Templates on disk**: YAML files live in `static/workflows/{category}/slug.yaml`. The folder name becomes the category tag automatically.
- **Backend loading**: `src/templates.rs` scans `static/workflows/` on startup and converts YAMLs to `TemplateMetadata` + `Flow` objects.
- **Template routes**: `src/server/template_routes.rs` exposes `GET /api/templates`, `GET /api/templates/{slug}`, `POST /api/templates/import-yaml`, `POST /api/templates/import-github`.
- **GitHub import**: `POST /api/templates/import-github` uses the GitHub Contents API (no auth, public repos only). It recurses 2 levels deep and fetches all `.yaml`/`.yml` files. Returns a list of `ImportResult` (success/error per file).
- **YAML upload**: `POST /api/templates/import-yaml` accepts raw YAML body, parses it as a flow definition, and creates the flow. Validation errors are returned as `400`.
- **MiniFlowDiagram**: `MiniFlowDiagram.tsx` renders a read-only React Flow preview inside each template card. Uses the same node/edge structure as the full canvas.

### Adding a New Template

1. Create a YAML file in `static/workflows/{category}/your-template.yaml`
2. Use the same structure as existing templates (see `static/workflows/media/daily-news-brief.yaml`)
3. Restart the server — templates are loaded at startup
4. Verify: `curl http://localhost:8081/api/templates` shows your template

---

## Working with the VM Sandbox Module

The sandbox module (`src/sandbox/`) spans both Rust backend and React frontend. When working on it:

### Key Architecture Points

- **VmManagerProvider** is the primary backend. It proxies to an external VM Manager API that manages Firecracker microVMs with web terminal (ttyd) access.
- **VMs are interactive-only** — users connect via browser terminal (iframe in BottomPanel). Automated flow runs still use `ClaudeCodeExecutor`.
- **One VM per flow** — persistent, created on first click, reused across interactions, destroyed explicitly.
- **VM session persistence** — `node_vms` (in-memory HashMap) is seeded from `sessions.yaml` on miss. `get_or_create_vm_with_persisted()` calls `restore_node_vm(vm_id)` to verify the VM is still alive before returning it. Server restarts do NOT lose existing VMs.
- **Cthulu is a relay** — all VM Manager calls go through Cthulu backend. Frontend calls `/api/sandbox/vm/{flowId}`, Cthulu proxies to VM Manager.

### Files You'll Likely Touch

| Area | Key Files |
|------|-----------|
| Backend provider | `src/sandbox/backends/vm_manager.rs`, `src/sandbox/vm_manager/mod.rs` |
| Backend routes | `src/server/flow_routes/sandbox.rs` |
| Frontend terminal | `cthulu-studio/src/components/VmTerminal.tsx` |
| Frontend panel | `cthulu-studio/src/components/BottomPanel.tsx` |
| Frontend API | `cthulu-studio/src/api/client.ts` |
| Types | `src/sandbox/types.rs` |
| Provider init | `src/main.rs` (env var dispatch) |
| AppState | `src/server/mod.rs` (holds `vm_manager: Option<Arc<VmManagerProvider>>`) |

### Common Pitfalls

- **FlowRunner construction sites**: Adding a field to `FlowRunner` requires updating 7 places (4 in `flow_routes/mod.rs`, 3 in `scheduler.rs`). Grep for `FlowRunner {`.
- **AppState must derive Clone**: Any new field must be `Arc`-wrapped or inherently `Clone`.
- **VM Manager-specific methods** (`get_or_create_vm`, `get_flow_vm`, `destroy_flow_vm`) live on `VmManagerProvider` directly, NOT on the `SandboxProvider` trait. This is why `AppState` has a separate `vm_manager` field.
- **BottomTab.nodeKind**: The `BottomTab` type has a `nodeKind` field that determines whether to render `VmTerminal` or `NodeChat`. Always pass this through when opening tabs.
- **CSS variables**: VM terminal styles use `var(--bg)`, `var(--border)`, etc. Never hardcode colors.
- **Shell escape**: Any user input going into shell commands must use `shell_escape()` (single-quote-with-replacement idiom).

### End-to-End Testing

To manually test the VM browser terminal:

1. Set `VM_MANAGER_URL=http://34.100.130.60:8080` in `.env`
2. Start the server: `cargo run -- serve`
3. Start Studio: `npx nx dev cthulu-studio`
4. Create a flow, drag a "VM Sandbox" executor node onto the canvas
5. Click the node → BottomPanel should show VmTerminal with loading spinner
6. VM creates (~2-5s) → iframe loads ttyd web terminal
7. Interact with Claude CLI inside the VM
8. Click "Destroy VM" to clean up

To test VM session persistence after restart:

1. Create a VM (steps above), verify it's running
2. Stop the server (`Ctrl+C`)
3. Restart: `cargo run -- serve`
4. Click the same vm-sandbox node → should reconnect to the same VM (no spinner delay for creation)

---

## Working with OAuth Token Refresh

The token refresh system keeps Claude CLI authenticated inside VMs.

### Key Architecture Points

- **TopBar token status**: `GET /api/auth/token-status` reads the current OAuth token from the macOS Keychain (or env var), checks `expiresAt`, and returns `{ valid: bool, expires_at }`. The TopBar shows amber pulse when expired.
- **Full credentials blob**: `inject_oauth_token` writes ALL fields to `~/.claude/.credentials.json` inside the VM: `accessToken`, `refreshToken`, `expiresAt`, `scopes`, `subscriptionType`, `rateLimitTier`. Missing any field causes Claude CLI to force re-login.
- **`.bashrc` replace, not skip**: Token injection always replaces the existing `CLAUDE_API_KEY` export in `.bashrc` — it does NOT skip if already present. This ensures stale tokens are always overwritten.
- **Refresh hits all VMs**: `POST /api/auth/refresh-token` iterates over all active VMs in `node_vms` and calls `inject_oauth_token` on each. VMs that have been garbage-collected are skipped.

### Common Pitfall

If Claude CLI inside a VM shows the login prompt even after token injection, the credentials file is incomplete. Check that `read_full_credentials()` in `auth_routes.rs` is reading all 6 fields from the Keychain and that `inject_oauth_token` writes them all to `~/.claude/.credentials.json`.

---

## Session Start Checklist

At the beginning of each session:

1. Review `.claude/LESSONS.md` for recent lessons
2. Read `CLAUDE.md` for project rules and architecture
3. For non-trivial tasks, plan before coding
4. Run `cargo check` to ensure the codebase compiles before making changes
5. If working on sandbox: review `.claude/skills/sandbox-module.md` for architecture and file map
