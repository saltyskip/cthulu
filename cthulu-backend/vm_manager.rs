//! VM Manager client — creates and manages per-user Firecracker VMs.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct VmManagerClient {
    http: reqwest::Client,
    base_url: String,
}

/// VM info as returned by the VM Manager API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInfo {
    pub vm_id: u32,
    #[serde(default)]
    pub pid: Option<u64>,
    #[serde(default)]
    pub guest_ip: Option<String>,
    pub ssh_port: u16,
    pub web_port: u16,
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub ssh_command: Option<String>,
    #[serde(default)]
    pub web_terminal: Option<String>,
}

/// Response from GET /vms
#[derive(Debug, Deserialize)]
struct ListVmsResponse {
    vms: HashMap<String, VmInfo>,
    count: u32,
}

/// Request body for POST /vms
#[derive(Debug, Serialize)]
struct CreateVmRequest {
    tier: String,
    api_key: String,
}

impl VmManagerClient {
    pub fn new(http: reqwest::Client) -> Self {
        let base_url = std::env::var("VM_MANAGER_URL")
            .unwrap_or_else(|_| "http://34.100.130.60:8080".to_string());
        Self { http, base_url }
    }

    /// Create a new VM. Returns VM info.
    pub async fn create_vm(&self, oauth_token: &str, tier: &str) -> Result<VmInfo> {
        let resp = self.http
            .post(format!("{}/vms", self.base_url))
            .json(&CreateVmRequest {
                tier: tier.to_string(),
                api_key: oauth_token.to_string(),
            })
            .send()
            .await
            .context("VM Manager create request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("VM creation failed ({status}): {body}");
        }

        resp.json::<VmInfo>().await.context("failed to parse VM create response")
    }

    /// Get VM info by ID.
    pub async fn get_vm(&self, vm_id: u32) -> Result<Option<VmInfo>> {
        let resp = self.http
            .get(format!("{}/vms/{}", self.base_url, vm_id))
            .send()
            .await
            .context("VM Manager get request failed")?;

        if resp.status().as_u16() == 404 {
            return Ok(None);
        }
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("VM get failed: {body}");
        }

        Ok(Some(resp.json::<VmInfo>().await?))
    }

    /// List all VMs. Returns the count.
    pub async fn list_vms(&self) -> Result<Vec<VmInfo>> {
        let resp = self.http
            .get(format!("{}/vms", self.base_url))
            .send()
            .await
            .context("VM Manager list request failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("VM list failed: {body}");
        }

        let data: ListVmsResponse = resp.json().await.context("failed to parse VM list")?;
        Ok(data.vms.into_values().collect())
    }

    /// Delete a VM.
    pub async fn delete_vm(&self, vm_id: u32) -> Result<()> {
        let resp = self.http
            .delete(format!("{}/vms/{}", self.base_url, vm_id))
            .send()
            .await
            .context("VM Manager delete request failed")?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("VM delete failed: {body}");
        }
        Ok(())
    }

    /// Get the VM Manager host (for SSH).
    pub fn ssh_host(&self) -> String {
        self.base_url
            .trim_start_matches("http://")
            .trim_start_matches("https://")
            .split(':')
            .next()
            .unwrap_or("34.100.130.60")
            .to_string()
    }

    /// Stream a command on a VM via SSH. Returns a child process handle.
    pub async fn ssh_stream(
        &self,
        ssh_port: u16,
        command: &str,
    ) -> Result<tokio::process::Child> {
        let host = self.ssh_host();

        let child = tokio::process::Command::new("ssh")
            .args([
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-o", "ConnectTimeout=10",
                "-p", &ssh_port.to_string(),
                &format!("root@{host}"),
                command,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .context("SSH stream spawn failed")?;

        Ok(child)
    }
}
