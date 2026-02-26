//! Firecracker REST API client.
//!
//! Supports two transport modes:
//! - **Unix socket**: `curl --unix-socket` via HostTransport (standard FC setup)
//! - **TCP**: `reqwest` to a base URL like `http://localhost:8080` (Lima + socat setup)
//!
//! The TCP path is useful when FC's Unix socket is exposed via socat/port-forward
//! from a Lima VM to the macOS host.

use std::path::{Path, PathBuf};

use crate::sandbox::error::SandboxError;
use crate::sandbox::firecracker::host_transport::HostTransport;

/// How to reach the Firecracker API.
pub enum ApiTransport {
    /// Unix socket on the FC host — uses curl via HostTransport.
    UnixSocket {
        socket_path: PathBuf,
        transport: Box<dyn HostTransport>,
    },
    /// TCP endpoint — FC API is accessible at this base URL (e.g., http://localhost:8080).
    Tcp {
        base_url: String,
        client: reqwest::Client,
    },
}

/// Typed client for the Firecracker API.
pub struct FirecrackerVmApi {
    transport: ApiTransport,
}

/// Configuration for booting a Firecracker microVM.
#[derive(Debug, Clone)]
pub struct VmBootConfig {
    pub kernel_image_path: String,
    pub boot_args: String,
    pub rootfs_path: String,
    pub rootfs_read_only: bool,
    pub vcpu_count: u8,
    pub mem_size_mib: u32,
    pub track_dirty_pages: bool,
}

impl Default for VmBootConfig {
    fn default() -> Self {
        Self {
            kernel_image_path: String::new(),
            boot_args: "console=ttyS0 reboot=k panic=1 i8042.noaux i8042.nomux i8042.nopnp i8042.dumbkbd".into(),
            rootfs_path: String::new(),
            rootfs_read_only: false,
            vcpu_count: 1,
            mem_size_mib: 256,
            track_dirty_pages: false,
        }
    }
}

/// Network interface configuration for the VM.
#[derive(Debug, Clone)]
pub struct VmNetworkConfig {
    pub iface_id: String,
    pub guest_mac: String,
    pub host_dev_name: String,
}

/// Snapshot creation parameters.
#[derive(Debug, Clone)]
pub struct SnapshotCreateParams {
    pub snapshot_path: String,
    pub mem_file_path: String,
    pub snapshot_type: SnapshotType,
}

#[derive(Debug, Clone, Copy)]
pub enum SnapshotType {
    Full,
    Diff,
}

/// Snapshot load parameters.
#[derive(Debug, Clone)]
pub struct SnapshotLoadParams {
    pub snapshot_path: String,
    pub mem_file_path: String,
    pub track_dirty_pages: bool,
    pub resume_vm: bool,
}

impl FirecrackerVmApi {
    /// Create an API client using a Unix socket transport.
    pub fn new_unix(socket_path: PathBuf, transport: Box<dyn HostTransport>) -> Self {
        Self {
            transport: ApiTransport::UnixSocket {
                socket_path,
                transport,
            },
        }
    }

    /// Create an API client using a TCP endpoint (e.g., http://localhost:8080).
    pub fn new_tcp(base_url: String) -> Self {
        Self {
            transport: ApiTransport::Tcp {
                base_url,
                client: reqwest::Client::new(),
            },
        }
    }

    /// Get a display string for the transport (for logging).
    fn transport_display(&self) -> String {
        match &self.transport {
            ApiTransport::UnixSocket { socket_path, .. } => {
                format!("unix:{}", socket_path.display())
            }
            ApiTransport::Tcp { base_url, .. } => base_url.clone(),
        }
    }

    /// Send a PUT request to the Firecracker API.
    async fn api_put(&self, endpoint: &str, body: &str) -> Result<String, SandboxError> {
        match &self.transport {
            ApiTransport::Tcp { base_url, client } => {
                let url = format!("{base_url}{endpoint}");
                let resp = client
                    .put(&url)
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .send()
                    .await
                    .map_err(|e| {
                        SandboxError::Backend(format!("FC API PUT {endpoint} failed: {e}"))
                    })?;

                let status = resp.status().as_u16();
                let resp_body = resp.text().await.unwrap_or_default();

                if status >= 200 && status < 300 {
                    Ok(resp_body)
                } else {
                    Err(SandboxError::Backend(format!(
                        "FC API PUT {endpoint} returned HTTP {status}: {resp_body}"
                    )))
                }
            }
            ApiTransport::UnixSocket {
                socket_path,
                transport,
            } => {
                let socket = socket_path.to_string_lossy();
                let url = format!("http://localhost{endpoint}");

                let result = transport
                    .run_cmd_sudo(&[
                        "curl",
                        "--unix-socket",
                        &socket,
                        "-s",
                        "-w",
                        "\n%{http_code}",
                        "-X",
                        "PUT",
                        "-H",
                        "Content-Type: application/json",
                        "-d",
                        body,
                        &url,
                    ])
                    .await?;

                let output = result.stdout_string();
                Self::parse_curl_response(endpoint, &output, &result.stderr)
            }
        }
    }

    /// Send a PATCH request to the Firecracker API.
    async fn api_patch(&self, endpoint: &str, body: &str) -> Result<String, SandboxError> {
        match &self.transport {
            ApiTransport::Tcp { base_url, client } => {
                let url = format!("{base_url}{endpoint}");
                let resp = client
                    .patch(&url)
                    .header("Accept", "application/json")
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .send()
                    .await
                    .map_err(|e| {
                        SandboxError::Backend(format!("FC API PATCH {endpoint} failed: {e}"))
                    })?;

                let status = resp.status().as_u16();
                let resp_body = resp.text().await.unwrap_or_default();

                if status >= 200 && status < 300 {
                    Ok(resp_body)
                } else {
                    Err(SandboxError::Backend(format!(
                        "FC API PATCH {endpoint} returned HTTP {status}: {resp_body}"
                    )))
                }
            }
            ApiTransport::UnixSocket {
                socket_path,
                transport,
            } => {
                let socket = socket_path.to_string_lossy();
                let url = format!("http://localhost{endpoint}");

                let result = transport
                    .run_cmd_sudo(&[
                        "curl",
                        "--unix-socket",
                        &socket,
                        "-s",
                        "-w",
                        "\n%{http_code}",
                        "-X",
                        "PATCH",
                        "-H",
                        "Accept: application/json",
                        "-H",
                        "Content-Type: application/json",
                        "-d",
                        body,
                        &url,
                    ])
                    .await?;

                let output = result.stdout_string();
                Self::parse_curl_response(endpoint, &output, &result.stderr)
            }
        }
    }

    /// Send a GET request to the Firecracker API.
    pub async fn api_get(&self, endpoint: &str) -> Result<String, SandboxError> {
        match &self.transport {
            ApiTransport::Tcp { base_url, client } => {
                let url = format!("{base_url}{endpoint}");
                let resp = client
                    .get(&url)
                    .send()
                    .await
                    .map_err(|e| {
                        SandboxError::Backend(format!("FC API GET {endpoint} failed: {e}"))
                    })?;

                let status = resp.status().as_u16();
                let resp_body = resp.text().await.unwrap_or_default();

                if status >= 200 && status < 300 {
                    Ok(resp_body)
                } else {
                    Err(SandboxError::Backend(format!(
                        "FC API GET {endpoint} returned HTTP {status}: {resp_body}"
                    )))
                }
            }
            ApiTransport::UnixSocket {
                socket_path,
                transport,
            } => {
                let socket = socket_path.to_string_lossy();
                let url = format!("http://localhost{endpoint}");

                let result = transport
                    .run_cmd_sudo(&[
                        "curl",
                        "--unix-socket",
                        &socket,
                        "-s",
                        "-w",
                        "\n%{http_code}",
                        &url,
                    ])
                    .await?;

                let output = result.stdout_string();
                Self::parse_curl_response(endpoint, &output, &result.stderr)
            }
        }
    }

    /// Parse curl response: last line is HTTP status code, rest is body.
    fn parse_curl_response(
        endpoint: &str,
        output: &str,
        stderr: &[u8],
    ) -> Result<String, SandboxError> {
        let lines: Vec<&str> = output.lines().collect();
        if lines.is_empty() {
            let stderr_str = String::from_utf8_lossy(stderr);
            return Err(SandboxError::Backend(format!(
                "FC API {endpoint}: no response (stderr: {stderr_str})"
            )));
        }

        let status_str = lines.last().unwrap();
        let body = if lines.len() > 1 {
            lines[..lines.len() - 1].join("\n")
        } else {
            String::new()
        };

        let status: u16 = status_str.parse().unwrap_or(0);
        if status == 0 {
            return Err(SandboxError::Backend(format!(
                "FC API {endpoint}: could not parse HTTP status from: {output}"
            )));
        }

        if status >= 200 && status < 300 {
            Ok(body)
        } else {
            Err(SandboxError::Backend(format!(
                "FC API {endpoint} returned HTTP {status}: {body}"
            )))
        }
    }

    // ── High-level API ────────────────────────────────────────────

    /// Query the instance info (GET /).
    pub async fn get_instance_info(&self) -> Result<serde_json::Value, SandboxError> {
        let body = self.api_get("/").await?;
        serde_json::from_str(&body)
            .map_err(|e| SandboxError::Backend(format!("failed to parse instance info: {e}")))
    }

    /// Configure the boot source (kernel + boot args).
    pub async fn set_boot_source(
        &self,
        kernel_image_path: &str,
        boot_args: &str,
    ) -> Result<(), SandboxError> {
        let body = serde_json::json!({
            "kernel_image_path": kernel_image_path,
            "boot_args": boot_args,
        });
        self.api_put("/boot-source", &body.to_string()).await?;
        Ok(())
    }

    /// Configure the root filesystem drive.
    pub async fn set_rootfs(
        &self,
        drive_id: &str,
        path_on_host: &str,
        read_only: bool,
    ) -> Result<(), SandboxError> {
        let body = serde_json::json!({
            "drive_id": drive_id,
            "path_on_host": path_on_host,
            "is_root_device": true,
            "is_read_only": read_only,
        });
        self.api_put(&format!("/drives/{drive_id}"), &body.to_string())
            .await?;
        Ok(())
    }

    /// Configure machine parameters (vcpu, memory).
    pub async fn set_machine_config(
        &self,
        vcpu_count: u8,
        mem_size_mib: u32,
        track_dirty_pages: bool,
    ) -> Result<(), SandboxError> {
        let body = serde_json::json!({
            "vcpu_count": vcpu_count,
            "mem_size_mib": mem_size_mib,
            "smt": false,
            "track_dirty_pages": track_dirty_pages,
        });
        self.api_put("/machine-config", &body.to_string()).await?;
        Ok(())
    }

    /// Configure a network interface.
    pub async fn set_network_interface(
        &self,
        config: &VmNetworkConfig,
    ) -> Result<(), SandboxError> {
        let body = serde_json::json!({
            "iface_id": config.iface_id,
            "guest_mac": config.guest_mac,
            "host_dev_name": config.host_dev_name,
        });
        self.api_put(
            &format!("/network-interfaces/{}", config.iface_id),
            &body.to_string(),
        )
        .await?;
        Ok(())
    }

    /// Configure logging.
    pub async fn set_logger(&self, log_path: &str) -> Result<(), SandboxError> {
        let body = serde_json::json!({
            "log_path": log_path,
            "level": "Info",
            "show_level": true,
            "show_log_origin": true,
        });
        self.api_put("/logger", &body.to_string()).await?;
        Ok(())
    }

    /// Start the microVM (InstanceStart action).
    pub async fn start_instance(&self) -> Result<(), SandboxError> {
        let body = serde_json::json!({
            "action_type": "InstanceStart",
        });
        self.api_put("/actions", &body.to_string()).await?;
        Ok(())
    }

    /// Pause the microVM.
    pub async fn pause(&self) -> Result<(), SandboxError> {
        let body = serde_json::json!({
            "state": "Paused",
        });
        self.api_patch("/vm", &body.to_string()).await?;
        Ok(())
    }

    /// Resume the microVM.
    pub async fn resume(&self) -> Result<(), SandboxError> {
        let body = serde_json::json!({
            "state": "Resumed",
        });
        self.api_patch("/vm", &body.to_string()).await?;
        Ok(())
    }

    /// Create a snapshot (full or diff). VM must be paused first.
    pub async fn create_snapshot(
        &self,
        params: &SnapshotCreateParams,
    ) -> Result<(), SandboxError> {
        let snapshot_type = match params.snapshot_type {
            SnapshotType::Full => "Full",
            SnapshotType::Diff => "Diff",
        };
        let body = serde_json::json!({
            "snapshot_type": snapshot_type,
            "snapshot_path": params.snapshot_path,
            "mem_file_path": params.mem_file_path,
        });
        self.api_put("/snapshot/create", &body.to_string()).await?;
        Ok(())
    }

    /// Load a snapshot. Must be called before any other configuration.
    pub async fn load_snapshot(&self, params: &SnapshotLoadParams) -> Result<(), SandboxError> {
        let body = serde_json::json!({
            "snapshot_path": params.snapshot_path,
            "mem_backend": {
                "backend_path": params.mem_file_path,
                "backend_type": "File",
            },
            "track_dirty_pages": params.track_dirty_pages,
            "resume_vm": params.resume_vm,
        });
        self.api_put("/snapshot/load", &body.to_string()).await?;
        Ok(())
    }

    /// Fully configure and boot a VM from a boot config.
    pub async fn configure_and_boot(&self, config: &VmBootConfig) -> Result<(), SandboxError> {
        tracing::info!(
            transport = %self.transport_display(),
            vcpu = config.vcpu_count,
            mem_mib = config.mem_size_mib,
            "configuring firecracker VM"
        );

        self.set_machine_config(
            config.vcpu_count,
            config.mem_size_mib,
            config.track_dirty_pages,
        )
        .await?;

        self.set_boot_source(&config.kernel_image_path, &config.boot_args)
            .await?;

        self.set_rootfs("rootfs", &config.rootfs_path, config.rootfs_read_only)
            .await?;

        // Small delay to let FC process the config before starting
        tokio::time::sleep(std::time::Duration::from_millis(15)).await;

        self.start_instance().await?;

        tracing::info!(
            transport = %self.transport_display(),
            "firecracker VM started"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_boot_config_defaults() {
        let config = VmBootConfig::default();
        assert_eq!(config.vcpu_count, 1);
        assert_eq!(config.mem_size_mib, 256);
        assert!(!config.rootfs_read_only);
        assert!(config.boot_args.contains("console=ttyS0"));
    }

    #[test]
    fn parse_curl_response_success() {
        let result = FirecrackerVmApi::parse_curl_response("/test", "some body\n200", &[]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "some body");
    }

    #[test]
    fn parse_curl_response_error() {
        let result = FirecrackerVmApi::parse_curl_response("/test", "bad request\n400", &[]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("400"));
    }

    #[test]
    fn parse_curl_response_empty() {
        let result = FirecrackerVmApi::parse_curl_response("/test", "", b"connection refused");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn tcp_api_get_instance_info() {
        // This test will only pass if FC is running at localhost:8080
        // Skip gracefully if not available
        let api = FirecrackerVmApi::new_tcp("http://localhost:8080".into());
        match api.get_instance_info().await {
            Ok(info) => {
                assert!(info.get("vmm_version").is_some());
                let state = info["state"].as_str().unwrap_or("unknown");
                eprintln!("FC instance state: {state}");
            }
            Err(_) => {
                eprintln!("FC not available at localhost:8080, skipping live test");
            }
        }
    }
}
