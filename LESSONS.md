# Lessons Learned — Sandbox Module

Hard-won knowledge from building the sandbox/Firecracker integration.

## 1. FsJail path canonicalization on macOS

**Problem**: `std::fs::canonicalize()` fails on non-existent paths (returns `Err`). On macOS, tempdir paths go through `/var` which is a symlink to `/private/var`. So `canonicalize("/var/folders/...")` returns `/private/var/folders/...` but `starts_with("/var/folders/...")` is false. The FsJail `resolve()` method used canonicalize to check path containment, and it broke in two ways:
1. Paths that don't exist yet can't be canonicalized
2. Symlink resolution changes the prefix, breaking `starts_with` checks

**Fix**: Don't use `canonicalize()`. Instead, manually normalize path components by resolving `.` and `..` segments. This handles both non-existent paths and symlink prefix mismatches.

**File**: `src/sandbox/local_host/fs_jail.rs`

## 2. Nested KVM does NOT work on Apple Silicon

**Problem**: Firecracker requires `/dev/kvm`. We tried two Lima VM backends on macOS Apple Silicon (M-series):

- **vz (Apple Virtualization.framework)**: `/dev/kvm` device node exists (kernel has `CONFIG_KVM=y` built-in), but opening it returns `ENODEV (errno 19)`. The CPU (`implementer: 0x61`, Apple Silicon) doesn't expose ARM virtualization extensions to the guest.
- **qemu**: Even with `nestedVirtualization: true` in Lima config, `/dev/kvm` doesn't appear at all. QEMU on Apple Silicon doesn't support nested virtualization for aarch64 guests.

**Result**: Neither Lima backend gives you working KVM on Apple Silicon. This is a fundamental hardware/hypervisor limitation, not a configuration issue.

**Solution**: Use a real Linux server (bare metal or cloud with nested virt) for Firecracker. The `RemoteSsh` transport was built for this — Cthulu on macOS talks to the FC API over TCP, host commands go over SSH.

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

## 3. Lima vz vs qemu — vz is the better choice for everything except KVM

The `default` Lima instance (vz) is faster, more stable, and has better macOS integration than the `firecracker` instance (qemu). The only reason to use qemu is `nestedVirtualization`, and on Apple Silicon it doesn't actually work. Stick with vz for Lima instances.

## 4. Firecracker kernel image must match guest architecture

When downloading FC kernel/rootfs images from the CI S3 bucket, make sure to get the right architecture. The S3 paths include architecture:
- `aarch64/vmlinux-6.1` for ARM64
- `x86_64/vmlinux-6.1` for Intel/AMD

Mismatched arch = instant VM crash with no useful error message.

## 5. socat for exposing Unix sockets over TCP

Firecracker only speaks Unix domain socket. To reach it from another machine (or from macOS host into a Lima VM), use socat:

```bash
socat TCP-LISTEN:8080,fork,reuseaddr UNIX-CONNECT:/tmp/firecracker.sock
```

The `fork` flag is essential — without it, socat exits after the first connection. `reuseaddr` prevents "address already in use" errors on restart.

## 6. FlowRunner construction sites are spread across the codebase

Adding a field to `FlowRunner` requires updating 7 construction sites:
- 4 in `src/server/flow_routes.rs`
- 3 in `src/flows/scheduler.rs`

Missing any one causes a compile error, but it's easy to miss one during refactoring. Grep for `FlowRunner {` or `FlowRunner::new` to find them all.

## 7. AppState must derive Clone — use Arc for non-Clone fields

`AppState` must derive Clone (Axum requirement). Any non-Clone field needs to be wrapped in `Arc`. `sandbox_provider: Arc<dyn SandboxProvider>` follows this pattern.

## 8. Rust edition 2024 implications

Cargo.toml specifies `edition = "2024"`. This affects:
- `use` declarations (edition 2024 changes some import rules)
- `async_trait` is still needed since native async traits aren't fully stabilized for dyn dispatch

## 9. reqwest is already a dependency — use it

No need to add HTTP client dependencies. Cthulu already has `reqwest` in `Cargo.toml`. The FC TCP transport uses it for `PUT`/`GET`/`PATCH` against the FC REST API.

## 10. Don't try to build Firecracker inside a Lima VM for development

Building FC from source inside Lima works but is slow and painful. For the remote server approach, just download the release binary:

```bash
# On the remote Linux server
ARCH=$(uname -m)  # aarch64 or x86_64
curl -Lo firecracker https://github.com/firecracker-microvm/firecracker/releases/download/v1.12.0/firecracker-v1.12.0-${ARCH}
chmod +x firecracker
sudo mv firecracker /usr/local/bin/
```

## 11. VM Manager API is the production path — not raw Firecracker

**Problem**: Direct Firecracker control from Cthulu (the `FirecrackerProvider`) requires SSH access to a Linux server, socat for Unix socket proxying, TAP network setup, rootfs management, and a lot of moving parts. It also requires `/dev/kvm` which doesn't work on macOS.

**Solution**: The VM Manager is a separate service running on the Linux server that abstracts all of this. Cthulu just makes HTTP calls (`POST /vms`, `GET /vms/{id}`, `DELETE /vms/{id}`). The VM Manager handles Firecracker lifecycle, networking, web terminal (ttyd) setup, and Claude CLI installation inside the VM.

**Result**: `VmManagerProvider` is ~200 lines of straightforward HTTP client code vs. `FirecrackerProvider` which is ~780 lines of complex transport/provisioning logic.

## 12. Browser terminal iframe — direct URL, not proxied

**Problem**: The web terminal (ttyd) runs on a dynamic port on the VM Manager host. The iframe in BottomPanel needs to load this URL. Initially considered proxying the terminal through Cthulu.

**Solution**: The iframe `src` points directly to the VM Manager's `web_terminal` URL (e.g., `http://34.100.130.60:PORT`). No proxying through Cthulu. This is simpler and avoids WebSocket proxy complexity.

**Implication**: The user's browser must be able to reach the VM Manager host directly. If the VM Manager is behind a firewall or NAT, the dynamic web terminal ports must be accessible. This is a deployment consideration, not a code issue.

## 13. AppState needs both generic trait and specific provider

**Problem**: The VM sandbox endpoints need `VmManagerProvider`-specific methods (`get_or_create_vm`, `get_flow_vm`, `destroy_flow_vm`) that don't belong on the generic `SandboxProvider` trait.

**Solution**: `AppState` stores both:
- `sandbox_provider: Arc<dyn SandboxProvider>` — for the flow runner (generic dispatch)
- `vm_manager: Option<Arc<VmManagerProvider>>` — for the VM sandbox endpoints (specific methods)

Both are set from the same provider instance in `main.rs`. The `Option` is `None` when `VM_MANAGER_URL` isn't set.

## 14. BottomTab needs nodeKind to decide which component to render

**Problem**: When a user clicks a node, BottomPanel opens a tab. But `vm-sandbox` nodes need `VmTerminal` (iframe) while `claude-code` nodes need `NodeChat` (chat interface). The tab didn't know which node type it was for.

**Solution**: Extended the `BottomTab` type with a `nodeKind: string` field. BottomPanel checks this field to conditionally render the right component. Passed through from `App.tsx` where the node click is handled.

## 15. shell_escape: single-quote-with-replacement idiom

**Problem**: PR review found 6+ shell injection vulnerabilities where user-supplied strings (sandbox names, file paths) were interpolated into shell commands without escaping.

**Fix**: `shell_escape()` wraps the string in single quotes and replaces any internal `'` with `'\''` (end quote, escaped quote, start quote). This is the standard POSIX shell escaping pattern. Example: `O'Brien` becomes `'O'\''Brien'`.

## 16. default_safe() must return Disabled, not AllowAll

**Problem**: `SandboxCapabilities::default_safe()` was returning `AllowAll` for network, filesystem, and exec capabilities. This meant new sandboxes had no restrictions by default.

**Fix**: Changed to return `Disabled` for all capabilities. Sandboxes now have no capabilities until explicitly granted. Security-first default.

## 17. exec_stream race condition — await stdout/stderr before Exit

**Problem**: In `ProcessExecStream`, the exit monitoring task could detect process exit and send the `Exit` event while stdout/stderr reading tasks still had buffered data. This caused truncated output.

**Fix**: The exit task now `await`s the stdout and stderr `JoinHandle`s before sending the `Exit` event. This guarantees all output is drained before the stream signals completion.
