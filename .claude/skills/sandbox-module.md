# Skill: Sandbox Module

## Overview

The sandbox module (`src/sandbox/`) provides isolated execution environments for AI agent executor nodes in the Cthulu workflow automation platform. It sits between the flow runner and the actual command execution, allowing agent tasks to run in sandboxed environments with varying levels of isolation.

The **primary production backend** is the **VmManagerProvider**, which talks to an external VM Manager API running on a remote Linux server. This service manages Firecracker microVMs with web terminal (ttyd) access. Users interact with VMs through an embedded browser terminal (iframe) in the Studio BottomPanel.

## Architecture

### Core Traits

```
SandboxProvider (factory)     → provisions sandboxes
  └─ SandboxHandle (instance) → exec, file ops, checkpoints, lifecycle
       └─ ExecStream          → streaming exec for long-running commands
```

- **`SandboxProvider`** (`src/sandbox/provider.rs`): Factory trait. One instance per backend lives on `AppState`. Methods: `provision(spec)`, `attach(id)`, `list()`.
- **`SandboxHandle`** (`src/sandbox/handle.rs`): Per-sandbox operations. Methods: `exec`, `exec_stream`, `put_file`, `get_file`, `read_dir`, `remove_path`, `expose_port`, `checkpoint`, `restore`, `stop`, `resume`, `destroy`.
- **`ExecStream`** (`src/sandbox/handle.rs`): Async stream of `ExecEvent`s from a running command. Methods: `next_event`, `write_stdin`, `close_stdin`.

### Backend Implementations

| Backend | File | Status | Isolation |
|---------|------|--------|-----------|
| `VmManagerProvider` | `src/sandbox/backends/vm_manager.rs` | **Complete (Primary)** | Real Firecracker microVM via VM Manager API, web terminal (ttyd iframe) |
| `DangerousHostProvider` | `src/sandbox/backends/dangerous.rs` | Complete | Best-effort host (workspace jail, env filtering, process supervisor) |
| `FirecrackerProvider` | `src/sandbox/backends/firecracker.rs` | Complete (code) | Direct Firecracker control (advanced, needs raw FC access) |
| `FlySpriteProvider` | `src/sandbox/backends/sprite.rs` | Skipped | Cloud sandbox via Fly Sprite API (not being built) |

### Dispatch Flow

```
FlowRunner.execute_node()
  └─ match executor_node.kind.as_str()
       "sandbox" → SandboxExecutor (src/tasks/executors/sandbox.rs)
       _         → ClaudeCodeExecutor (src/tasks/executors/claude_code.rs)
```

`SandboxExecutor` bridges the existing `Executor` trait to `SandboxProvider`/`SandboxHandle`. Each `execute()` call provisions a sandbox, runs `claude --print --verbose --output-format stream-json` inside it, parses the stream-json output, and returns `ExecutionResult`.

**Important**: VMs created via VmManagerProvider are **interactive-only** — the user connects via the embedded web terminal (ttyd iframe). Automated flow runs still use `ClaudeCodeExecutor` locally. The VM is a persistent interactive workspace, not an automated executor.

### AppState Integration

`AppState` holds two separate fields:
- `sandbox_provider: Arc<dyn SandboxProvider>` — generic sandbox trait for flow runner dispatch
- `vm_manager: Option<Arc<VmManagerProvider>>` — specific to VM Manager endpoints (needed for `get_or_create_vm`, `get_flow_vm`, `destroy_flow_vm` which aren't on the generic trait)

Both are passed into `FlowRunner` at construction (7 sites: 4 in `flow_routes/mod.rs`, 3 in `scheduler.rs`).

## VM Browser Terminal — End-to-End Flow

This is the primary user-facing feature of the sandbox module.

### How It Works

```
1. User drags a "VM Sandbox" executor node onto the Studio canvas
2. User clicks the node → BottomPanel opens
3. BottomPanel detects nodeKind === "vm-sandbox"
4. VmTerminal component renders (instead of NodeChat)
5. VmTerminal calls POST /api/sandbox/vm/{flowId}
   → Cthulu backend proxies to VM Manager API: POST /vms { tier, api_key }
   → VM Manager creates Firecracker microVM with ttyd web terminal
   → Returns: { vm_id, web_terminal: "http://HOST:PORT", ssh_command, ... }
6. VmTerminal embeds web_terminal URL as iframe src
7. User interacts with Claude CLI inside the VM via the browser terminal
8. VM persists across interactions (one VM per flow, reused on subsequent clicks)
9. User clicks "Destroy VM" → DELETE /api/sandbox/vm/{flowId}
   → Cthulu proxies to VM Manager: DELETE /vms/{vm_id}
```

### VM Lifecycle (Per Flow)

- **First click**: Creates VM via VM Manager API, stores mapping `flow_id → vm_id` in `VmManagerProvider.flow_vms` (in-memory `DashMap`)
- **Subsequent clicks**: Returns existing VM info (idempotent)
- **Destroy**: User clicks "Destroy VM" button, or flow is deleted
- **No auto-cleanup**: VMs persist until explicitly destroyed (by design — they're interactive workspaces)

### What the User Sees

The BottomPanel shows:
- **Loading spinner** while VM is being created (Firecracker boot takes ~2-5 seconds)
- **Status bar** with VM ID, tier, SSH command
- **Full-screen iframe** of the ttyd web terminal (a real terminal emulator in the browser)
- **Destroy VM** button (red, with confirmation)

Inside the terminal, the VM has:
- Claude CLI pre-installed
- `ANTHROPIC_API_KEY` injected into `.bashrc`
- Full Linux environment (Ubuntu-based rootfs)
- SSH access (passwordless `ssh -p PORT root@HOST`)

### Frontend Components

| Component | File | Purpose |
|-----------|------|---------|
| `VmTerminal` | `cthulu-studio/src/components/VmTerminal.tsx` | Iframe embed + VM lifecycle controls (create, status, destroy) |
| `BottomPanel` | `cthulu-studio/src/components/BottomPanel.tsx` | Conditional render: VmTerminal for `vm-sandbox`, NodeChat for `claude-code` |
| `PropertyPanel` | `cthulu-studio/src/components/PropertyPanel.tsx` | VM config fields: tier dropdown (nano/micro), API key input |
| API client | `cthulu-studio/src/api/client.ts` | `getFlowVm()`, `createFlowVm()`, `deleteFlowVm()` |
| Validation | `cthulu-studio/src/utils/validateNode.ts` | `vm-sandbox` case (no required fields) |
| App | `cthulu-studio/src/App.tsx` | Passes `nodeKind` through BottomTab type |
| Styles | `cthulu-studio/src/styles.css` | `.vm-terminal-*` classes (container, infobar, iframe, spinner, status) |

### BottomTab Extension

The `BottomTab` type was extended with a `nodeKind: string` field so BottomPanel can decide which component to render:
- `nodeKind === "vm-sandbox"` → `<VmTerminal>`
- `nodeKind === "claude-code"` (or any other) → `<NodeChat>`

### CSS Theming

All VM terminal styles use CSS variables for consistent theming:
- `var(--bg)`, `var(--bg-secondary)` — backgrounds
- `var(--border)` — borders
- `var(--accent)` — buttons, highlights
- `var(--text)`, `var(--text-secondary)` — text colors

Never hardcode colors in VM terminal components.

## File Map

### Core types and traits
- `src/sandbox/mod.rs` — Module root, re-exports, `build_provider()` factory
- `src/sandbox/error.rs` — `SandboxError` enum (9 variants)
- `src/sandbox/types.rs` — All data types (~730 lines): specs, configs, results, capabilities
- `src/sandbox/provider.rs` — `SandboxProvider` trait
- `src/sandbox/handle.rs` — `SandboxHandle` + `ExecStream` traits

### VM Manager backend (Primary — production path)
- `src/sandbox/vm_manager/mod.rs` — `VmManagerClient` HTTP client, request/response types
- `src/sandbox/backends/vm_manager.rs` — `VmManagerProvider` (flow_vms DashMap, get_or_create_vm) + `VmManagerHandle`
- `src/server/flow_routes/sandbox.rs` — Proxy endpoints: `get_flow_vm`, `create_flow_vm`, `delete_flow_vm`
- `cthulu-studio/src/components/VmTerminal.tsx` — Embedded web terminal component
- `cthulu-studio/src/api/client.ts` — `getFlowVm()`, `createFlowVm()`, `deleteFlowVm()`

### DangerousHost backend (Phase 1)
- `src/sandbox/backends/dangerous.rs` — `DangerousHostProvider` + `DangerousHandle`
- `src/sandbox/local_host/fs_jail.rs` — `FsJail` (path containment)
- `src/sandbox/local_host/process_supervisor.rs` — `ProcessSupervisor` + `ProcessExecStream`

### Firecracker backend (Phase 2 — advanced/raw FC)
- `src/sandbox/backends/firecracker.rs` — `FirecrackerProvider` + `FirecrackerHandle` (~780 lines)
- `src/sandbox/firecracker/mod.rs` — Submodule declarations
- `src/sandbox/firecracker/host_transport.rs` — `HostTransport` trait + 3 impls: `LocalLinuxTransport`, `LimaSshTransport`, `RemoteSshTransport`
- `src/sandbox/firecracker/vm_api.rs` — `FirecrackerVmApi` (dual transport: Unix socket via curl, TCP via reqwest)
- `src/sandbox/firecracker/guest_agent.rs` — `GuestAgent` trait + `SshGuestAgent` (SSH into guest VM)
- `src/sandbox/firecracker/net.rs` — `NetworkAllocator`, TAP setup/teardown, NAT, guest network config
- `src/sandbox/firecracker/snapshot.rs` — `SnapshotStore` (on-disk snapshot management)

### Modified existing files
- `src/main.rs` — Provider initialization with env var dispatch (VM_MANAGER_URL highest priority)
- `src/server/mod.rs` — `AppState` with `sandbox_provider` + `vm_manager: Option<Arc<VmManagerProvider>>`
- `src/server/flow_routes/mod.rs` — Routes including `/sandbox/vm/{flow_id}`
- `src/server/flow_routes/sandbox.rs` — `sandbox_info`, `sandbox_list`, `get_flow_vm`, `create_flow_vm`, `delete_flow_vm`
- `src/server/flow_routes/crud.rs` — `vm-sandbox` node type in `get_node_types()`
- `src/flows/runner.rs` — `sandbox_provider` field, executor dispatch
- `src/flows/scheduler.rs` — FlowRunner constructions with sandbox_provider
- `src/tasks/executors/sandbox.rs` — `SandboxExecutor`

## Provider Selection (main.rs)

Priority order via env vars:

1. **`VM_MANAGER_URL`** → `VmManagerProvider` (remote VM Manager API — **recommended for production**)
   - Also reads: `VM_MANAGER_SSH_HOST`, `VM_MANAGER_TIER`, `VM_MANAGER_API_KEY`
2. **`FIRECRACKER_SSH_HOST`** → `FirecrackerProvider` with `RemoteSsh` transport
   - Also reads: `FIRECRACKER_API_URL`, `FIRECRACKER_SSH_PORT`, `FIRECRACKER_SSH_KEY`, `FC_REMOTE_STATE_DIR`, `FC_REMOTE_BIN`
3. **`FIRECRACKER_API_URL`** → `FirecrackerProvider` with `LimaTcp` transport
   - Also reads: `LIMA_INSTANCE`
4. **None set** → `DangerousHostProvider` (default, no VM)

## VM Manager Integration

The VM Manager is a separate service running on a Linux server that manages Firecracker microVMs with web terminals (ttyd). Cthulu acts as a relay — all VM Manager calls go through the Cthulu backend (proxy pattern). The iframe `src` points directly to the VM Manager's `web_terminal` URL.

### VM Manager API (external service at VM_MANAGER_URL)
- `POST /vms` `{ tier: "nano"|"micro", api_key: "sk-ant-..." }` → create VM
  - Returns: `{ vm_id, tier, guest_ip, ssh_port, web_port, ssh_command, web_terminal, pid }`
- `GET /vms` → list all VMs
- `GET /vms/{id}` → get VM info
- `DELETE /vms/{id}` → destroy VM
- `GET /health` → health check

### VM Manager VM features
- Claude CLI pre-installed in every VM
- `ANTHROPIC_API_KEY` injected into `.bashrc` (from `api_key` in create request)
- SSH passwordless: `ssh -p PORT root@HOST`
- Web terminal via ttyd: `http://HOST:WEB_PORT`
- Max 20 VMs per host
- Tiers: `nano` (lightweight), `micro` (more resources)

### Cthulu proxy endpoints
- `GET /api/sandbox/vm/{flow_id}` → get VM info for a flow
- `POST /api/sandbox/vm/{flow_id}` → create/get VM for a flow (idempotent per flow)
- `DELETE /api/sandbox/vm/{flow_id}` → destroy VM for a flow

### Frontend: vm-sandbox executor node
- Node kind: `"vm-sandbox"` (registered in node-types endpoint via `get_node_types()`)
- Config: `tier` (nano/micro), `api_key` (optional Anthropic key)
- Click on node → BottomPanel renders `<VmTerminal>` component (iframe of web terminal)
- VM is persistent per flow (created on first click, reused across subsequent clicks)
- VMs are interactive-only — flow runs still use `ClaudeCodeExecutor` locally

### Env vars
```bash
VM_MANAGER_URL=http://34.100.130.60:8080   # VM Manager API base URL
VM_MANAGER_SSH_HOST=34.100.130.60           # optional, extracted from URL
VM_MANAGER_TIER=nano                         # default tier (nano or micro)
VM_MANAGER_API_KEY=sk-ant-xxx               # Anthropic key to inject into VMs
```

## Firecracker Transport Architecture

```
macOS (Cthulu) ──TCP──→ Remote Linux Server
                        ├─ FC API (socat: TCP:8080 → /tmp/firecracker.sock)
                        ├─ Host commands via SSH (RemoteSshTransport)
                        ├─ /dev/kvm (real hardware virtualization)
                        └─ microVM (guest)
                           ├─ Guest network (172.16.x.x)
                           └─ SshGuestAgent (exec commands inside VM)
```

The `FirecrackerVmApi` has two transports:
- `ApiTransport::UnixSocket` — uses `curl --unix-socket` via `HostTransport` (standard Linux setup)
- `ApiTransport::Tcp` — uses `reqwest` to a base URL like `http://server:8080` (remote setup)

The `HostTransport` trait abstracts running commands on the FC host:
- `LocalLinuxTransport` — direct execution (Linux with /dev/kvm)
- `LimaSshTransport` — `limactl shell` into Lima VM (macOS dev)
- `RemoteSshTransport` — SSH into remote Linux server (production path)

## Firecracker Provision Flow

1. Create VM state directory
2. Copy rootfs base image for this VM (CoW where possible)
3. Allocate network (unique TAP device + IP from 172.16.0.0/16)
4. Set up TAP device on host
5. Generate SSH keypair
6. Start Firecracker process (or connect to existing for TCP modes)
7. Configure via FC REST API: boot source, rootfs, machine config, network
8. Boot the VM
9. Wait for SSH to become available inside guest
10. Set up guest networking (default route + DNS)
11. Return `FirecrackerHandle`

## Testing

247 tests total. All unit tests; no integration tests requiring a live VM Manager or Firecracker instance yet.

```bash
cargo test                           # all tests (247)
cargo test sandbox                   # sandbox module tests only
cargo test firecracker               # firecracker-specific tests
cargo test vm_manager                # vm manager-specific tests
```

Frontend: `npx nx build cthulu-studio` — builds clean with all VM terminal components.

## API Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/sandbox/info` | GET | Returns current provider info (kind, capabilities) |
| `/api/sandbox/list` | GET | Lists active/known sandboxes |
| `/api/sandbox/vm/{flow_id}` | GET | Get VM info for a flow (if exists) |
| `/api/sandbox/vm/{flow_id}` | POST | Create or get VM for a flow (idempotent) |
| `/api/sandbox/vm/{flow_id}` | DELETE | Destroy VM for a flow |
| `/api/node-types` | GET | Includes `vm-sandbox` in available node types |

## Key Design Decisions

- **VmManagerProvider is the primary backend** — handles real VM provisioning via external API.
- **Cthulu is a relay** — all VM Manager calls go through Cthulu backend (proxy pattern). Frontend never talks to VM Manager directly.
- **One VM per flow (persistent)** — VMs are reused across interactions. Destroyed explicitly by user or when flow is deleted.
- **Interactive-only VMs** — User connects via web terminal iframe. Flow runs still use `ClaudeCodeExecutor` locally.
- **No new Cargo dependencies** — uses existing `reqwest`, `serde`, `async-trait`.
- **Backward compatible**: Flows without `vm-sandbox` executor kind use existing `ClaudeCodeExecutor`.
- **`AppState` derives Clone** via `Arc` wrapping — `sandbox_provider: Arc<dyn SandboxProvider>`, `vm_manager: Option<Arc<VmManagerProvider>>`.
- **Rust edition 2024** — uses edition 2024 features.
- **Phase 3 (FlySpriteProvider) is skipped** — not being built.
- **CSS variables for theming** — all VM terminal styles use `var(--bg)`, `var(--border)`, `var(--accent)`, etc.
- **`shell_escape` security** — single-quote-with-replacement idiom prevents shell injection.
- **`default_safe()` returns Disabled** — not AllowAll. Security-first default.

## Remote Server Setup (VM Manager)

The VM Manager service runs on a Linux server and manages Firecracker microVMs. Setup:

1. Linux with `/dev/kvm` (bare metal or VM with nested virt)
2. VM Manager binary running on port 8080
3. Firecracker binary, kernel image, rootfs image pre-configured
4. ttyd installed for web terminal access
5. Cthulu connects via `VM_MANAGER_URL=http://server-ip:8080`

No SSH from Cthulu to VMs is needed — all interaction is through the web terminal.

## Remote Server Setup (Direct Firecracker — Advanced)

For direct Firecracker control (not recommended for most use cases):

1. Linux with `/dev/kvm` (bare metal or VM with nested virt, aarch64 or x86_64)
2. Firecracker binary (build from source or download release)
3. Kernel image + rootfs image (from FC CI S3 bucket)
4. `socat` to expose FC API: `socat TCP-LISTEN:8080,fork,reuseaddr UNIX-CONNECT:/tmp/firecracker.sock`
5. SSH access from Cthulu host
6. Network: TAP device support, IP forwarding enabled

Env vars on Cthulu host:
```bash
FIRECRACKER_SSH_HOST=user@server-ip
FIRECRACKER_SSH_PORT=22          # optional, default 22
FIRECRACKER_SSH_KEY=~/.ssh/id_ed25519  # optional, uses ssh-agent if omitted
FIRECRACKER_API_URL=http://server-ip:8080
FC_REMOTE_BIN=/usr/local/bin/firecracker
FC_REMOTE_STATE_DIR=/var/lib/firecracker
FC_KERNEL_IMAGE=/var/lib/firecracker/vmlinux
FC_ROOTFS_IMAGE=/var/lib/firecracker/rootfs.ext4
FC_VCPU=1
FC_MEMORY_MB=256
```

## Security Notes

- **Shell injection**: All user-supplied strings passed to shell commands use `shell_escape()` (single-quote-with-replacement: `'` → `'\''`). Fixed in 6+ locations per PR review.
- **Default safety**: `SandboxCapabilities::default_safe()` returns `Disabled` (not `AllowAll`). Capabilities must be explicitly granted.
- **No auth on VM Manager yet**: The VM Manager API currently has no authentication layer. This is a known TODO — the `VmManagerConfig` struct has a placeholder for auth tokens.
- **FsJail**: `DangerousHostProvider` uses `FsJail` for path containment. Known gap: symlink escape (canonicalize + prefix recheck not yet implemented).
- **Env filtering**: `DangerousHostProvider` strips sensitive env vars (`*KEY*`, `*SECRET*`, `*TOKEN*`, `*PASSWORD*`) from child processes.
