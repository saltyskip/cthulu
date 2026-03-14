//! Cloud VM pool and A2A agent infrastructure.
//!
//! This module manages a pool of persistent Firecracker microVMs running
//! on a GCP host via the VM Manager API.  Each VM runs a Google ADK
//! TypeScript A2A agent server that accepts tasks over the A2A protocol.
//!
//! # Architecture
//!
//! ```text
//! Cthulu Backend ──► VM Pool Manager ──► VM Manager API (GCP :8080)
//!       │                                       │
//!       │  A2A (JSON-RPC)                       │ provisions VMs
//!       ▼                                       ▼
//!   A2A Client ──────────────────────► ADK Agent Server (VM :web_port)
//! ```

pub mod a2a_client;
pub mod vm_manager_client;
pub mod vm_pool;

pub use a2a_client::A2aClient;
pub use vm_manager_client::VmManagerClient;
pub use vm_pool::VmPool;
