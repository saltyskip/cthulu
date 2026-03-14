//! HTTP client for the VM Manager API running on GCP.
//!
//! The VM Manager provisions Firecracker microVMs and exposes a simple
//! REST API for lifecycle management.  Cthulu uses this client to
//! manage a persistent pool of cloud VMs that run ADK A2A agent servers.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Response types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub active_vms: usize,
    pub max_vms: usize,
    pub host_iface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    pub vm_id: usize,
    pub pid: Option<u64>,
    pub guest_ip: String,
    pub ssh_port: u16,
    pub web_port: u16,
    pub socket: Option<String>,
    pub rootfs: Option<String>,
    pub tier: String,
    pub ssh_command: String,
    pub web_terminal: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListVmsResponse {
    pub vms: HashMap<String, VmInfo>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreateVmRequest {
    pub tier: String,
    pub api_key: String,
}

// ── Client ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct VmManagerClient {
    base_url: String,
    http: reqwest::Client,
}

impl VmManagerClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build reqwest client"),
        }
    }

    /// Check VM Manager health.
    pub async fn health(&self) -> Result<HealthResponse> {
        let resp = self
            .http
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .context("VM Manager health check failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("VM Manager health returned {status}: {body}");
        }

        resp.json().await.context("failed to parse health response")
    }

    /// List all running VMs.
    pub async fn list_vms(&self) -> Result<HashMap<String, VmInfo>> {
        let resp = self
            .http
            .get(format!("{}/vms", self.base_url))
            .send()
            .await
            .context("failed to list VMs")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("list VMs returned {status}: {body}");
        }

        let data: ListVmsResponse = resp.json().await.context("failed to parse VM list")?;
        Ok(data.vms)
    }

    /// Get info for a single VM.
    pub async fn get_vm(&self, vm_id: usize) -> Result<VmInfo> {
        let resp = self
            .http
            .get(format!("{}/vms/{}", self.base_url, vm_id))
            .send()
            .await
            .context("failed to get VM")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("get VM {vm_id} returned {status}: {body}");
        }

        resp.json().await.context("failed to parse VM info")
    }

    /// Create a new VM with the specified tier and Anthropic API key.
    pub async fn create_vm(&self, tier: &str, api_key: &str) -> Result<VmInfo> {
        let body = CreateVmRequest {
            tier: tier.to_string(),
            api_key: api_key.to_string(),
        };

        let resp = self
            .http
            .post(format!("{}/vms", self.base_url))
            .json(&body)
            .send()
            .await
            .context("failed to create VM")?;

        let status = resp.status();
        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("create VM returned {status}: {body_text}");
        }

        resp.json().await.context("failed to parse create VM response")
    }

    /// Delete a VM by ID.
    pub async fn delete_vm(&self, vm_id: usize) -> Result<()> {
        let resp = self
            .http
            .delete(format!("{}/vms/{}", self.base_url, vm_id))
            .send()
            .await
            .context("failed to delete VM")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("delete VM {vm_id} returned {status}: {body}");
        }

        Ok(())
    }

    /// Return the base URL of the VM Manager.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Derive the host part of the VM Manager URL (e.g. "34.100.130.60")
    /// Used to construct SSH commands and A2A URLs for individual VMs.
    pub fn host(&self) -> &str {
        self.base_url
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split(':')
            .next()
            .unwrap_or("localhost")
    }
}
