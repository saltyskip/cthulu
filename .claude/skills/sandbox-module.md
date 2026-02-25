# Skill: Sandbox Module

## Overview

The sandbox module (`src/sandbox/`) provides isolated execution environments for AI agent executor nodes in the Cthulu workflow automation platform. It sits between the flow runner and the actual command execution, allowing agent tasks to run in sandboxed environments with varying levels of isolation.

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
| `DangerousHostProvider` | `src/sandbox/backends/dangerous.rs` | Complete | Best-effort host (workspace jail, env filtering, process supervisor) |
| `VmManagerProvider` | `src/sandbox/backends/vm_manager.rs` | **Complete** | Real VM via VM Manager API (Firecracker microVMs, web terminal) |
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

### AppState Integration

`AppState` holds `sandbox_provider: Arc<dyn SandboxProvider>`. This is passed into `FlowRunner` at construction (7 sites: 4 in `flow_routes.rs`, 3 in `scheduler.rs`).

## File Map

### Core types and traits
- `src/sandbox/mod.rs` — Module root, re-exports, `build_provider()` factory
- `src/sandbox/error.rs` — `SandboxError` enum (9 variants)
- `src/sandbox/types.rs` — All data types (~700 lines): specs, configs, results, capabilities
- `src/sandbox/provider.rs` — `SandboxProvider` trait
- `src/sandbox/handle.rs` — `SandboxHandle` + `ExecStream` traits

### DangerousHost backend (Phase 1)
- `src/sandbox/backends/dangerous.rs` — `DangerousHostProvider` + `DangerousHandle`
- `src/sandbox/local_host/fs_jail.rs` — `FsJail` (path containment)
- `src/sandbox/local_host/process_supervisor.rs` — `ProcessSupervisor` + `ProcessExecStream`

### Firecracker backend (Phase 2)
- `src/sandbox/backends/firecracker.rs` — `FirecrackerProvider` + `FirecrackerHandle` (~780 lines)
- `src/sandbox/firecracker/mod.rs` — Submodule declarations
- `src/sandbox/firecracker/host_transport.rs` — `HostTransport` trait + 3 impls: `LocalLinuxTransport`, `LimaSshTransport`, `RemoteSshTransport`
- `src/sandbox/firecracker/vm_api.rs` — `FirecrackerVmApi` (dual transport: Unix socket via curl, TCP via reqwest)
- `src/sandbox/firecracker/guest_agent.rs` — `GuestAgent` trait + `SshGuestAgent` (SSH into guest VM)
- `src/sandbox/firecracker/net.rs` — `NetworkAllocator`, TAP setup/teardown, NAT, guest network config
- `src/sandbox/firecracker/snapshot.rs` — `SnapshotStore` (on-disk snapshot management)

### Modified existing files
- `src/main.rs` — Provider initialization with env var dispatch
- `src/server/mod.rs` — `sandbox_provider` field on `AppState`
- `src/server/flow_routes.rs` — `/api/sandbox/info` + `/api/sandbox/list` routes; FlowRunner constructions
- `src/flows/runner.rs` — `sandbox_provider` field, executor dispatch
- `src/flows/scheduler.rs` — FlowRunner constructions
- `src/tasks/executors/sandbox.rs` — `SandboxExecutor`
- `src/tasks/executors/mod.rs` — `pub mod sandbox;`

## Provider Selection (main.rs)

Priority order via env vars:

1. **`VM_MANAGER_URL`** → `VmManagerProvider` (remote VM Manager API — **recommended**)
   - Also reads: `VM_MANAGER_SSH_HOST`, `VM_MANAGER_TIER`, `VM_MANAGER_API_KEY`
2. **`FIRECRACKER_SSH_HOST`** → `FirecrackerProvider` with `RemoteSsh` transport
   - Also reads: `FIRECRACKER_API_URL`, `FIRECRACKER_SSH_PORT`, `FIRECRACKER_SSH_KEY`, `FC_REMOTE_STATE_DIR`, `FC_REMOTE_BIN`
3. **`FIRECRACKER_API_URL`** → `FirecrackerProvider` with `LimaTcp` transport
   - Also reads: `LIMA_INSTANCE`
4. **None set** → `DangerousHostProvider` (default, no VM)

## VM Manager Integration

The VM Manager is a separate service running on a Linux server that manages Firecracker microVMs. Cthulu acts as a relay:

```
User clicks "VM Sandbox" executor node in Studio
  → Frontend: POST /api/sandbox/vm/{flowId}
  → Cthulu backend: POST http://VM_MANAGER_URL/vms { tier, api_key }
  → VM Manager: creates Firecracker microVM, returns web_terminal URL
  → Frontend: embeds web terminal URL in iframe in BottomPanel
  → User interacts with Claude inside the VM via browser terminal
```

### VM Manager API (external service)
- `POST /vms` { tier, api_key } → create VM
- `GET /vms` → list all VMs
- `GET /vms/{id}` → get VM info
- `DELETE /vms/{id}` → destroy VM
- `GET /health` → health check

### Cthulu proxy endpoints
- `GET /api/sandbox/vm/{flow_id}` → get VM info for a flow
- `POST /api/sandbox/vm/{flow_id}` → create/get VM for a flow (idempotent per flow)
- `DELETE /api/sandbox/vm/{flow_id}` → destroy VM for a flow

### Frontend: vm-sandbox executor node
- Node kind: `"vm-sandbox"` (registered in node-types endpoint)
- Config: `tier` (nano/micro), `api_key` (optional Anthropic key)
- Click on node → BottomPanel renders `<VmTerminal>` component (iframe of web terminal)
- VM is persistent per flow (created on first click, reused across subsequent clicks)
- VMs are interactive-only — flow runs still use `ClaudeCodeExecutor` locally

### Files
- `src/sandbox/vm_manager/mod.rs` — HTTP client for VM Manager API
- `src/sandbox/backends/vm_manager.rs` — `VmManagerProvider` + `VmManagerHandle`
- `src/server/flow_routes/sandbox.rs` — proxy endpoints
- `cthulu-studio/src/components/VmTerminal.tsx` — embedded web terminal component
- `cthulu-studio/src/api/client.ts` — `getFlowVm()`, `createFlowVm()`, `deleteFlowVm()`

### Env vars
```bash
VM_MANAGER_URL=http://34.100.130.60:8080
VM_MANAGER_SSH_HOST=34.100.130.60       # optional, extracted from URL
VM_MANAGER_TIER=nano                     # default tier
VM_MANAGER_API_KEY=sk-ant-xxx           # Anthropic key to inject into VMs
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

226 tests total, 75 sandbox-specific. All unit tests, no integration tests requiring FC yet.

```bash
cargo test                           # all tests
cargo test sandbox                   # sandbox module tests only
cargo test firecracker               # firecracker-specific tests
```

## API Endpoints

- `GET /api/sandbox/info` — Returns current provider info (kind, capabilities)
- `GET /api/sandbox/list` — Lists active/known sandboxes

## Key Design Decisions

- **No new Cargo dependencies for Phase 1.** Phase 2 uses existing `reqwest`.
- **Backward compatible**: Flows without `sandbox` executor kind use existing `ClaudeCodeExecutor`.
- **`AppState` derives Clone** via `Arc` wrapping — `sandbox_provider: Arc<dyn SandboxProvider>`.
- **Rust edition 2024** — uses edition 2024 features.
- **Phase 3 (FlySpriteProvider) is skipped** — not being built.

## Remote Server Setup

To run Firecracker on a remote Linux server, the server needs:

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
