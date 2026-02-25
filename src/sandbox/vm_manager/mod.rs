//! HTTP client for the Firecracker VM Manager API.
//!
//! The VM Manager runs on a remote Linux server and handles all Firecracker
//! lifecycle: process management, rootfs provisioning, networking, web terminal.
//!
//! API: POST /vms, GET /vms, GET /vms/{id}, DELETE /vms/{id}, GET /health

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::sandbox::error::SandboxError;

// ── Request / Response types ────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct VmCreateRequest {
    pub tier: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VmResponse {
    pub vm_id: u32,
    pub tier: String,
    pub guest_ip: String,
    pub ssh_port: u16,
    pub web_port: u16,
    pub ssh_command: String,
    pub web_terminal: String,
    pub pid: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VmListResponse {
    pub vms: HashMap<String, VmResponse>,
    pub count: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub active_vms: u32,
    pub max_vms: u32,
    #[serde(default)]
    pub host_iface: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DeleteResponse {
    #[allow(dead_code)]
    pub status: String,
    #[allow(dead_code)]
    pub vm_id: u32,
}

// ── Client ──────────────────────────────────────────────────────────

/// HTTP client for the VM Manager REST API.
#[derive(Debug, Clone)]
pub struct VmManagerClient {
    base_url: String,
    client: reqwest::Client,
}

impl VmManagerClient {
    pub fn new(base_url: String) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    /// Check VM Manager health.
    pub async fn health(&self) -> Result<HealthResponse, SandboxError> {
        let url = format!("{}/health", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SandboxError::Backend(format!("VM Manager health check failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SandboxError::Backend(format!(
                "VM Manager health check returned {status}: {body}"
            )));
        }

        resp.json::<HealthResponse>()
            .await
            .map_err(|e| SandboxError::Serde(format!("failed to parse health response: {e}")))
    }

    /// Create a new VM.
    pub async fn create_vm(&self, req: &VmCreateRequest) -> Result<VmResponse, SandboxError> {
        let url = format!("{}/vms", self.base_url);

        tracing::info!(
            tier = %req.tier,
            "creating VM via VM Manager"
        );

        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .await
            .map_err(|e| SandboxError::Provision(format!("VM Manager create failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SandboxError::Provision(format!(
                "VM Manager create returned {status}: {body}"
            )));
        }

        let vm = resp
            .json::<VmResponse>()
            .await
            .map_err(|e| SandboxError::Serde(format!("failed to parse VM response: {e}")))?;

        tracing::info!(
            vm_id = vm.vm_id,
            tier = %vm.tier,
            web_terminal = %vm.web_terminal,
            ssh_port = vm.ssh_port,
            "VM created"
        );

        Ok(vm)
    }

    /// Get a specific VM by ID.
    pub async fn get_vm(&self, vm_id: u32) -> Result<VmResponse, SandboxError> {
        let url = format!("{}/vms/{}", self.base_url, vm_id);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SandboxError::Backend(format!("VM Manager get_vm failed: {e}")))?;

        if resp.status().as_u16() == 404 {
            return Err(SandboxError::NotFound(format!("VM {vm_id} not found")));
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SandboxError::Backend(format!(
                "VM Manager get_vm returned {status}: {body}"
            )));
        }

        resp.json::<VmResponse>()
            .await
            .map_err(|e| SandboxError::Serde(format!("failed to parse VM response: {e}")))
    }

    /// List all VMs.
    pub async fn list_vms(&self) -> Result<VmListResponse, SandboxError> {
        let url = format!("{}/vms", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| SandboxError::Backend(format!("VM Manager list failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SandboxError::Backend(format!(
                "VM Manager list returned {status}: {body}"
            )));
        }

        resp.json::<VmListResponse>()
            .await
            .map_err(|e| SandboxError::Serde(format!("failed to parse VM list response: {e}")))
    }

    /// Delete a VM.
    pub async fn delete_vm(&self, vm_id: u32) -> Result<(), SandboxError> {
        let url = format!("{}/vms/{}", self.base_url, vm_id);

        tracing::info!(vm_id = vm_id, "deleting VM via VM Manager");

        let resp = self
            .client
            .delete(&url)
            .send()
            .await
            .map_err(|e| SandboxError::Backend(format!("VM Manager delete failed: {e}")))?;

        if resp.status().as_u16() == 404 {
            // Already gone — not an error
            tracing::warn!(vm_id = vm_id, "VM already deleted");
            return Ok(());
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(SandboxError::Backend(format!(
                "VM Manager delete returned {status}: {body}"
            )));
        }

        tracing::info!(vm_id = vm_id, "VM deleted");
        Ok(())
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_trims_trailing_slash() {
        let c = VmManagerClient::new("http://example.com:8080/".into());
        assert_eq!(c.base_url, "http://example.com:8080");
    }

    #[test]
    fn vm_create_request_serializes() {
        let req = VmCreateRequest {
            tier: "nano".into(),
            api_key: "sk-test".into(),
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["tier"], "nano");
        assert_eq!(json["api_key"], "sk-test");
    }

    #[test]
    fn vm_response_deserializes() {
        let json = r#"{
            "vm_id": 0,
            "tier": "nano",
            "guest_ip": "172.16.0.2",
            "ssh_port": 2222,
            "web_port": 7700,
            "ssh_command": "ssh -p 2222 root@1.2.3.4",
            "web_terminal": "http://1.2.3.4:7700",
            "pid": 12345
        }"#;
        let vm: VmResponse = serde_json::from_str(json).unwrap();
        assert_eq!(vm.vm_id, 0);
        assert_eq!(vm.tier, "nano");
        assert_eq!(vm.ssh_port, 2222);
        assert_eq!(vm.web_terminal, "http://1.2.3.4:7700");
    }

    #[test]
    fn vm_list_response_deserializes() {
        let json = r#"{
            "vms": {
                "0": {
                    "vm_id": 0, "tier": "nano", "guest_ip": "172.16.0.2",
                    "ssh_port": 2222, "web_port": 7700,
                    "ssh_command": "ssh -p 2222 root@1.2.3.4",
                    "web_terminal": "http://1.2.3.4:7700", "pid": 100
                }
            },
            "count": 1
        }"#;
        let list: VmListResponse = serde_json::from_str(json).unwrap();
        assert_eq!(list.count, 1);
        assert!(list.vms.contains_key("0"));
    }

    #[test]
    fn health_response_deserializes() {
        let json = r#"{"status":"ok","active_vms":1,"max_vms":20,"host_iface":"ens4"}"#;
        let h: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(h.status, "ok");
        assert_eq!(h.active_vms, 1);
        assert_eq!(h.max_vms, 20);
    }

    #[test]
    fn health_response_without_host_iface() {
        let json = r#"{"status":"ok","active_vms":0,"max_vms":20}"#;
        let h: HealthResponse = serde_json::from_str(json).unwrap();
        assert!(h.host_iface.is_none());
    }
}
