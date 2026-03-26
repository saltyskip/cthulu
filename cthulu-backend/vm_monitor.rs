//! Background task that periodically health-checks user VMs.
//!
//! Every 60 seconds, iterates over all users with a `vm_id` set and pings the
//! VM Manager API. Logs a warning for any VM that is unreachable or down.

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::api::local_auth::UserStore;
use crate::vm_manager::VmManagerClient;

/// Spawn the VM health monitor as a background tokio task.
pub fn spawn_vm_monitor(
    http_client: Arc<reqwest::Client>,
    user_store: Arc<RwLock<UserStore>>,
) {
    tokio::spawn(async move {
        let vm_client = VmManagerClient::new((*http_client).clone());
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));

        loop {
            interval.tick().await;

            // Collect (email, vm_id) pairs under a short-lived read lock
            let users_with_vms: Vec<(String, u32)> = {
                let store = user_store.read().await;
                store.users.values()
                    .filter_map(|u| u.vm_id.map(|id| (u.email.clone(), id)))
                    .collect()
            };

            if users_with_vms.is_empty() {
                continue;
            }

            for (email, vm_id) in &users_with_vms {
                match vm_client.get_vm(*vm_id).await {
                    Ok(Some(vm)) => {
                        // VM exists and responding — it's alive
                        if vm.pid.is_none() {
                            warn!(
                                user = %email,
                                vm_id = vm_id,
                                "VM has no PID — may not be running"
                            );
                        }
                    }
                    Ok(None) => {
                        warn!(
                            user = %email,
                            vm_id = vm_id,
                            "VM not found (404) — may have been deleted externally"
                        );
                    }
                    Err(e) => {
                        warn!(
                            user = %email,
                            vm_id = vm_id,
                            error = %e,
                            "VM health check failed — unreachable"
                        );
                    }
                }
            }

            info!(
                checked = users_with_vms.len(),
                "VM health check complete"
            );
        }
    });
}
