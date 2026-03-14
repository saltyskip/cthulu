//! Persistent VM pool for cloud agent execution.
//!
//! On startup, the pool fetches the current VM list from the VM Manager,
//! selects up to `pool_size` VMs, and marks them as idle.  When a workflow
//! run needs a cloud executor, it acquires an idle VM from the pool.
//! After the run completes, the VM is released back to idle.

use anyhow::{Context, Result};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::vm_manager_client::{VmInfo, VmManagerClient};

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum VmStatus {
    /// VM is idle and available for assignment.
    Idle,
    /// VM is assigned to a workflow run.
    Assigned { workflow_run_id: String },
    /// VM is being provisioned (ADK agent server starting).
    Provisioning,
    /// VM is in an error state.
    Error(String),
}

#[derive(Debug, Clone)]
pub struct PoolVm {
    pub info: VmInfo,
    pub status: VmStatus,
    /// URL where this VM's A2A agent server is reachable.
    /// Derived from the VM Manager host + vm's web_port.
    pub a2a_url: String,
}

/// Manages a fixed-size pool of cloud VMs.
pub struct VmPool {
    client: VmManagerClient,
    /// Host IP/hostname of the VM Manager (used to construct A2A URLs).
    host: String,
    #[allow(dead_code)]
    pool_size: usize,
    pool: RwLock<Vec<PoolVm>>,
}

impl VmPool {
    /// Initialize the pool by fetching existing VMs from the VM Manager
    /// and selecting up to `pool_size` of them.
    pub async fn init(client: VmManagerClient, pool_size: usize) -> Result<Arc<Self>> {
        let host = client.host().to_string();

        // Verify VM Manager is healthy
        let health = client
            .health()
            .await
            .context("VM Manager is not reachable")?;
        tracing::info!(
            active_vms = health.active_vms,
            max_vms = health.max_vms,
            "connected to VM Manager"
        );

        // Fetch all VMs
        let all_vms = client
            .list_vms()
            .await
            .context("failed to list VMs from VM Manager")?;

        // Sort by vm_id and take the first pool_size VMs
        let mut vm_list: Vec<VmInfo> = all_vms.into_values().collect();
        vm_list.sort_by_key(|v| v.vm_id);
        vm_list.truncate(pool_size);

        if vm_list.is_empty() {
            anyhow::bail!(
                "no VMs available from VM Manager (need at least 1, requested {pool_size})"
            );
        }

        let pool: Vec<PoolVm> = vm_list
            .into_iter()
            .map(|info| {
                let a2a_url = format!("http://{}:{}", host, info.web_port);
                tracing::info!(
                    vm_id = info.vm_id,
                    ssh_port = info.ssh_port,
                    web_port = info.web_port,
                    a2a_url = %a2a_url,
                    "added VM to pool"
                );
                PoolVm {
                    info,
                    status: VmStatus::Idle,
                    a2a_url,
                }
            })
            .collect();

        tracing::info!(pool_size = pool.len(), "VM pool initialized");

        Ok(Arc::new(Self {
            client,
            host,
            pool_size,
            pool: RwLock::new(pool),
        }))
    }

    /// Acquire an idle VM for a workflow run.
    /// Returns `None` if all VMs are busy.
    pub async fn acquire(&self, workflow_run_id: &str) -> Option<PoolVm> {
        let mut pool = self.pool.write().await;
        for vm in pool.iter_mut() {
            if vm.status == VmStatus::Idle {
                vm.status = VmStatus::Assigned {
                    workflow_run_id: workflow_run_id.to_string(),
                };
                tracing::info!(
                    vm_id = vm.info.vm_id,
                    workflow_run_id = %workflow_run_id,
                    "acquired VM from pool"
                );
                return Some(vm.clone());
            }
        }
        tracing::warn!(
            workflow_run_id = %workflow_run_id,
            "no idle VMs available in pool"
        );
        None
    }

    /// Release a VM back to idle after a workflow run completes.
    pub async fn release(&self, vm_id: usize) {
        let mut pool = self.pool.write().await;
        if let Some(vm) = pool.iter_mut().find(|v| v.info.vm_id == vm_id) {
            tracing::info!(vm_id = vm_id, "released VM back to pool");
            vm.status = VmStatus::Idle;
        }
    }

    /// Mark a VM as errored.
    pub async fn mark_error(&self, vm_id: usize, error: String) {
        let mut pool = self.pool.write().await;
        if let Some(vm) = pool.iter_mut().find(|v| v.info.vm_id == vm_id) {
            tracing::warn!(vm_id = vm_id, error = %error, "VM marked as error");
            vm.status = VmStatus::Error(error);
        }
    }

    /// Get a snapshot of the current pool state.
    pub async fn status(&self) -> Vec<PoolVm> {
        self.pool.read().await.clone()
    }

    /// Health check: ping each VM's A2A agent card endpoint.
    pub async fn health_check(&self) -> Vec<(usize, bool)> {
        let pool = self.pool.read().await;
        let mut results = Vec::new();

        for vm in pool.iter() {
            let url = format!("{}/.well-known/agent.json", vm.a2a_url);
            let ok = self
                .client_http()
                .get(&url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            results.push((vm.info.vm_id, ok));
        }

        results
    }

    /// Return the number of idle VMs.
    pub async fn idle_count(&self) -> usize {
        self.pool
            .read()
            .await
            .iter()
            .filter(|v| v.status == VmStatus::Idle)
            .count()
    }

    /// Return the total pool size.
    pub async fn total_count(&self) -> usize {
        self.pool.read().await.len()
    }

    /// Return the VM Manager client (for direct API access).
    pub fn vm_manager(&self) -> &VmManagerClient {
        &self.client
    }

    /// Return a reqwest client for HTTP requests.
    fn client_http(&self) -> &reqwest::Client {
        // Reuse a simple client for health checks
        // (The VmManagerClient's internal client is private, so we create one here)
        static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        CLIENT.get_or_init(|| {
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .expect("failed to build health check client")
        })
    }

    /// Return the host IP of the VM Manager.
    pub fn host(&self) -> &str {
        &self.host
    }
}
