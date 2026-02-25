# NOPE — Things That Don't Work / Dead Ends

A record of approaches that were tried and definitively failed. Save yourself the time.

## 1. Nested KVM on Apple Silicon via Lima (ANY backend)

**Don't try**: Running Firecracker inside a Lima VM on macOS Apple Silicon.

Neither `vz` (Apple Virtualization.framework) nor `qemu` with `nestedVirtualization: true` provides a working `/dev/kvm` to the guest. The device node may exist but returns `ENODEV` when opened. QEMU on Apple Silicon doesn't emulate ARM virtualization extensions for aarch64 guests. This is a fundamental limitation — no amount of kernel config or Lima flags will fix it.

**Instead**: Use a real Linux server with bare-metal KVM (or a cloud VM with nested virt enabled, e.g., GCP N2/N2D, AWS metal instances, Hetzner dedicated).

## 2. mknod /dev/kvm as a workaround

**Don't try**: `sudo mknod /dev/kvm c 10 232` inside a Lima VM.

This creates the device file but the kernel driver behind it (major 10, minor 232) is non-functional because the CPU doesn't have the virtualization extensions. Opening the device returns `ENODEV (errno 19)`. The kernel's KVM module is compiled in (`CONFIG_KVM=y`) but the hardware backing isn't there.

## 3. Lima firecracker instance (qemu backend)

**Don't use**: The `firecracker` Lima instance we created with `vmType: qemu`.

It was created specifically to try nested virtualization. It doesn't work (see #1). The `default` Lima instance (vz) is faster and more stable for everything else. The `firecracker` instance can be deleted:

```bash
limactl stop firecracker
limactl delete firecracker
```

## 4. Phase 3 — FlySpriteProvider

**Skipped**: Not being built. The stub exists at `src/sandbox/backends/sprite.rs` and `src/sandbox/sprite/` but returns `Unsupported` for everything. Don't invest time here.

## 5. Firecracker without KVM

**Not possible**: Firecracker hard-requires `/dev/kvm`. There is no userspace emulation mode, no TCG fallback, no `--no-kvm` flag. If you don't have KVM, you don't have Firecracker. Alternatives for non-KVM environments:
- `DangerousHostProvider` (process-level isolation, no VM)
- Docker/container backend (not yet built but would be a reasonable middle ground)
- gVisor (would need a new backend)

## 6. Firecracker snapshot restore without re-provisioning

**Not yet working**: `FirecrackerHandle::restore()` returns `Unsupported`. Restoring a Firecracker snapshot requires stopping the current FC process and starting a fresh one that loads the snapshot. This requires re-provisioning logic that hasn't been automated yet. Checkpoints (creating snapshots) work, but restoring them is manual.

## 7. Streaming exec over SSH to Firecracker guest

**Not yet working**: `FirecrackerHandle::exec_stream()` returns `Unsupported`. Streaming exec requires maintaining a persistent SSH channel with multiplexed stdout/stderr/stdin, which is complex over the SSH command-line tool. Would need an SSH library (like `russh`) or a custom agent binary inside the guest VM.

## 8. Port exposure for Firecracker VMs

**Not yet working**: `expose_port()` / `unexpose_port()` return `Unsupported`. Would need iptables DNAT rules or SSH tunnel forwarding. Not yet implemented.

## 9. Proxying web terminal (ttyd) WebSocket through Cthulu

**Don't try**: Routing the ttyd web terminal connection through the Cthulu backend as a WebSocket proxy.

ttyd uses WebSockets for real-time terminal I/O. Proxying this through Cthulu would require WebSocket upgrade handling in Axum, bidirectional frame forwarding, and proper keepalive management. It adds latency and complexity for no real benefit.

**Instead**: The iframe `src` points directly to the VM Manager's `web_terminal` URL (e.g., `http://host:PORT`). The user's browser connects directly to the ttyd server running on the VM Manager host. This is simpler and lower latency. Trade-off: the user's browser must be able to reach the VM Manager host and its dynamic ports.

## 10. Running Firecracker directly from macOS (any method)

**Don't try**: Any scheme to run Firecracker on macOS — whether natively, inside Docker Desktop, inside Lima, or inside UTM/QEMU.

Firecracker hard-requires Linux `/dev/kvm`. macOS doesn't have KVM. Docker Desktop on Mac uses LinuxKit which doesn't expose nested KVM. Lima VMs on Apple Silicon don't provide working KVM (see #1). UTM/QEMU on Apple Silicon can't nest virtualization.

**Instead**: Use the VM Manager API (`VM_MANAGER_URL=http://server:8080`) which runs on a real Linux server. Cthulu on macOS talks to it over HTTP. The user interacts via browser terminal (ttyd iframe). No KVM needed on the Cthulu host.

## 11. SSH exec from Cthulu to VM Manager VMs

**Don't try**: SSHing from Cthulu into VMs created by the VM Manager to execute commands programmatically.

While the VMs do have SSH enabled (passwordless `ssh -p PORT root@HOST`), executing commands from Cthulu over SSH adds complexity: key management, connection pooling, error handling, output parsing. The VMs are designed for **interactive** use through the web terminal, not for programmatic exec from the Cthulu backend.

**Instead**: VMs are interactive-only. Users interact via the browser terminal (ttyd iframe). Automated flow runs use `ClaudeCodeExecutor` locally. If you need programmatic sandbox exec, use `DangerousHostProvider` or direct `FirecrackerProvider` with `SshGuestAgent`.

## 12. Downcasting `Arc<dyn SandboxProvider>` to specific provider type

**Don't try**: Using `Arc::downcast()` or `Any`-based downcasting to get `VmManagerProvider` from `Arc<dyn SandboxProvider>`.

This is fragile, verbose, and breaks when the underlying type changes. It also requires adding `Any` bounds to the trait.

**Instead**: Store the specific provider separately on `AppState`. We use `vm_manager: Option<Arc<VmManagerProvider>>` alongside the generic `sandbox_provider: Arc<dyn SandboxProvider>`. Both are set from the same instance in `main.rs`.
