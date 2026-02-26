//! Network setup for Firecracker microVMs.
//!
//! Each VM gets a unique TAP device and IP address pair from a pool.
//! Handles:
//! - TAP device creation and teardown
//! - IP address assignment (host side)
//! - Guest IP derivation from MAC address
//! - NAT/masquerade rules for internet access

use std::sync::atomic::{AtomicU16, Ordering};

use crate::sandbox::error::SandboxError;
use crate::sandbox::firecracker::host_transport::HostTransport;
use crate::sandbox::firecracker::vm_api::VmNetworkConfig;

/// Network allocation for a single VM.
#[derive(Debug, Clone)]
pub struct VmNetwork {
    /// TAP device name (e.g., "fc-tap0")
    pub tap_name: String,
    /// Host-side IP for the TAP (e.g., "172.16.0.1")
    pub host_ip: String,
    /// Guest-side IP (e.g., "172.16.0.2")
    pub guest_ip: String,
    /// MAC address for the guest NIC
    pub guest_mac: String,
    /// Subnet mask in short form (e.g., "/30")
    pub mask_short: String,
    /// The VM network config to pass to Firecracker API
    pub vm_config: VmNetworkConfig,
}

/// Manages IP/TAP allocation for Firecracker VMs.
///
/// Uses a simple incrementing counter to assign unique /30 subnets
/// from the 172.16.0.0/16 range. Each VM gets:
/// - Host IP: 172.16.{high}.{low*4 + 1}
/// - Guest IP: 172.16.{high}.{low*4 + 2}
/// - /30 subnet mask
pub struct NetworkAllocator {
    counter: AtomicU16,
    /// Starting port range for the counter
    start: u16,
}

impl NetworkAllocator {
    pub fn new(start: u16) -> Self {
        Self {
            counter: AtomicU16::new(0),
            start,
        }
    }

    /// Allocate a new network for a VM.
    pub fn allocate(&self, vm_id: &str) -> VmNetwork {
        let idx = self.counter.fetch_add(1, Ordering::SeqCst) + self.start;

        // Each /30 subnet uses 4 addresses: network, host, guest, broadcast
        // We pack them into 172.16.{octet3}.{base}
        let octet3 = (idx / 64) as u8; // 64 VMs per /24
        let base = ((idx % 64) * 4) as u8;

        let host_ip = format!("172.16.{}.{}", octet3, base + 1);
        let guest_ip = format!("172.16.{}.{}", octet3, base + 2);

        // MAC derived from IP: 06:00:AC:10:{octet3}:{base+2}
        let guest_mac = format!("06:00:AC:10:{:02X}:{:02X}", octet3, base + 2);

        let tap_name = format!("fc-tap{idx}");

        // Sanitize vm_id for iface_id (alphanumeric + hyphen only)
        let iface_id = format!("net-{}", sanitize_iface_id(vm_id));

        VmNetwork {
            tap_name: tap_name.clone(),
            host_ip: host_ip.clone(),
            guest_ip: guest_ip.clone(),
            guest_mac: guest_mac.clone(),
            mask_short: "/30".into(),
            vm_config: VmNetworkConfig {
                iface_id,
                guest_mac,
                host_dev_name: tap_name,
            },
        }
    }
}

/// Set up the TAP device and host networking for a VM.
pub async fn setup_tap(
    transport: &dyn HostTransport,
    network: &VmNetwork,
) -> Result<(), SandboxError> {
    tracing::info!(
        tap = %network.tap_name,
        host_ip = %network.host_ip,
        guest_ip = %network.guest_ip,
        "setting up TAP device"
    );

    // Delete existing TAP if any
    let _ = transport
        .run_cmd_sudo(&["ip", "link", "del", &network.tap_name])
        .await;

    // Create TAP
    transport
        .run_cmd_sudo(&[
            "ip",
            "tuntap",
            "add",
            "dev",
            &network.tap_name,
            "mode",
            "tap",
        ])
        .await?
        .check()
        .map_err(|e| SandboxError::Provision(format!("TAP creation failed: {e}")))?;

    // Assign IP
    let ip_cidr = format!("{}{}", network.host_ip, network.mask_short);
    transport
        .run_cmd_sudo(&["ip", "addr", "add", &ip_cidr, "dev", &network.tap_name])
        .await?
        .check()
        .map_err(|e| SandboxError::Provision(format!("TAP IP assignment failed: {e}")))?;

    // Bring up
    transport
        .run_cmd_sudo(&["ip", "link", "set", "dev", &network.tap_name, "up"])
        .await?
        .check()
        .map_err(|e| SandboxError::Provision(format!("TAP link up failed: {e}")))?;

    // Enable IP forwarding
    transport
        .run_cmd_sudo(&["sh", "-c", "echo 1 > /proc/sys/net/ipv4/ip_forward"])
        .await?;

    Ok(())
}

/// Set up NAT masquerade for internet access from the VM.
pub async fn setup_nat(
    transport: &dyn HostTransport,
    host_interface: &str,
) -> Result<(), SandboxError> {
    // Idempotent: delete existing rule then add
    let _ = transport
        .run_cmd_sudo(&[
            "iptables",
            "-t",
            "nat",
            "-D",
            "POSTROUTING",
            "-o",
            host_interface,
            "-j",
            "MASQUERADE",
        ])
        .await;

    transport
        .run_cmd_sudo(&[
            "iptables",
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-o",
            host_interface,
            "-j",
            "MASQUERADE",
        ])
        .await?
        .check()
        .map_err(|e| SandboxError::Provision(format!("iptables NAT setup failed: {e}")))?;

    // Accept forwarded packets
    transport
        .run_cmd_sudo(&["iptables", "-P", "FORWARD", "ACCEPT"])
        .await?;

    Ok(())
}

/// Configure guest networking (default route + DNS) via SSH.
pub async fn setup_guest_network(
    guest_agent: &dyn super::guest_agent::GuestAgent,
    host_ip: &str,
) -> Result<(), SandboxError> {
    use crate::sandbox::types::ExecRequest;
    use std::collections::BTreeMap;

    // Add default route via host IP
    let route_req = ExecRequest {
        command: vec![format!("ip route add default via {host_ip} dev eth0")],
        cwd: None,
        env: BTreeMap::new(),
        stdin: None,
        timeout: Some(std::time::Duration::from_secs(10)),
        tty: false,
        detach: false,
    };
    let route_result = guest_agent.exec(&route_req).await?;
    if route_result.exit_code != Some(0) {
        tracing::warn!(
            stderr = %String::from_utf8_lossy(&route_result.stderr),
            "default route setup returned non-zero (may already exist)"
        );
    }

    // Set up DNS
    let dns_req = ExecRequest {
        command: vec!["echo 'nameserver 8.8.8.8' > /etc/resolv.conf".into()],
        cwd: None,
        env: BTreeMap::new(),
        stdin: None,
        timeout: Some(std::time::Duration::from_secs(10)),
        tty: false,
        detach: false,
    };
    guest_agent.exec(&dns_req).await?;

    Ok(())
}

/// Tear down TAP device for a VM.
pub async fn teardown_tap(
    transport: &dyn HostTransport,
    tap_name: &str,
) -> Result<(), SandboxError> {
    tracing::info!(tap = %tap_name, "tearing down TAP device");
    let _ = transport
        .run_cmd_sudo(&["ip", "link", "del", tap_name])
        .await;
    Ok(())
}

/// Sanitize a string for use as a network interface ID.
fn sanitize_iface_id(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .take(15)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocator_unique_ips() {
        let alloc = NetworkAllocator::new(0);
        let net0 = alloc.allocate("vm-0");
        let net1 = alloc.allocate("vm-1");

        assert_ne!(net0.host_ip, net1.host_ip);
        assert_ne!(net0.guest_ip, net1.guest_ip);
        assert_ne!(net0.tap_name, net1.tap_name);
        assert_ne!(net0.guest_mac, net1.guest_mac);
    }

    #[test]
    fn allocator_first_ip() {
        let alloc = NetworkAllocator::new(0);
        let net = alloc.allocate("test");

        assert_eq!(net.host_ip, "172.16.0.1");
        assert_eq!(net.guest_ip, "172.16.0.2");
        assert_eq!(net.tap_name, "fc-tap0");
        assert_eq!(net.guest_mac, "06:00:AC:10:00:02");
    }

    #[test]
    fn allocator_wraps_octets() {
        let alloc = NetworkAllocator::new(64); // Start at 64 = next octet3
        let net = alloc.allocate("test");

        assert_eq!(net.host_ip, "172.16.1.1");
        assert_eq!(net.guest_ip, "172.16.1.2");
    }

    #[test]
    fn sanitize_iface_id_works() {
        assert_eq!(sanitize_iface_id("hello-world!@#"), "hello-world");
        assert_eq!(sanitize_iface_id("a".repeat(20).as_str()), "a".repeat(15));
    }

    #[test]
    fn vm_network_config_matches() {
        let alloc = NetworkAllocator::new(0);
        let net = alloc.allocate("vm-0");

        assert_eq!(net.vm_config.host_dev_name, net.tap_name);
        assert_eq!(net.vm_config.guest_mac, net.guest_mac);
    }
}
