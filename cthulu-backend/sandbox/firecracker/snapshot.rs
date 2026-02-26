//! Snapshot file management for Firecracker VMs.
//!
//! Manages the on-disk layout of snapshot files. Each VM's snapshots are
//! stored under `{state_dir}/{vm_id}/snapshots/{snapshot_id}/`:
//! - `vm_state` — Firecracker VM state file
//! - `mem_file` — Guest memory file
//!
//! The rootfs disk is managed separately (it's per-VM, not per-snapshot).

use std::path::{Path, PathBuf};

use crate::sandbox::error::SandboxError;
use crate::sandbox::types::{CheckpointId, CheckpointRef};

/// On-disk paths for a single snapshot.
#[derive(Debug, Clone)]
pub struct SnapshotPaths {
    /// Directory containing this snapshot's files
    pub dir: PathBuf,
    /// Path to the VM state file
    pub vm_state: PathBuf,
    /// Path to the guest memory file
    pub mem_file: PathBuf,
}

/// Manages snapshot storage for a single VM.
pub struct SnapshotStore {
    /// Base directory: {state_dir}/{vm_id}/snapshots/
    snapshots_dir: PathBuf,
}

impl SnapshotStore {
    /// Create a new snapshot store. Creates the directory if needed.
    pub fn new(vm_state_dir: &Path) -> Result<Self, SandboxError> {
        let snapshots_dir = vm_state_dir.join("snapshots");
        std::fs::create_dir_all(&snapshots_dir).map_err(|e| {
            SandboxError::Provision(format!(
                "failed to create snapshots dir {}: {e}",
                snapshots_dir.display()
            ))
        })?;
        Ok(Self { snapshots_dir })
    }

    /// Get paths for a named snapshot. Does not check existence.
    pub fn paths_for(&self, snapshot_id: &str) -> SnapshotPaths {
        let dir = self.snapshots_dir.join(snapshot_id);
        SnapshotPaths {
            vm_state: dir.join("vm_state"),
            mem_file: dir.join("mem_file"),
            dir,
        }
    }

    /// Prepare a directory for a new snapshot. Returns the paths.
    pub fn prepare(&self, snapshot_id: &str) -> Result<SnapshotPaths, SandboxError> {
        let paths = self.paths_for(snapshot_id);
        std::fs::create_dir_all(&paths.dir).map_err(|e| {
            SandboxError::Exec(format!(
                "failed to create snapshot dir {}: {e}",
                paths.dir.display()
            ))
        })?;
        Ok(paths)
    }

    /// List all snapshots for this VM.
    pub fn list(&self) -> Result<Vec<CheckpointRef>, SandboxError> {
        let mut refs = Vec::new();

        if !self.snapshots_dir.exists() {
            return Ok(refs);
        }

        for entry in std::fs::read_dir(&self.snapshots_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();
            let paths = self.paths_for(&name);

            // Only list complete snapshots (both files exist)
            if paths.vm_state.exists() && paths.mem_file.exists() {
                let created_at = entry
                    .metadata()?
                    .created()
                    .ok()
                    .and_then(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_millis() as i64)
                    })
                    .unwrap_or(0);

                refs.push(CheckpointRef {
                    id: name.clone(),
                    name: Some(name),
                    created_at_unix_ms: created_at,
                });
            }
        }

        // Sort by creation time, newest first
        refs.sort_by(|a, b| b.created_at_unix_ms.cmp(&a.created_at_unix_ms));
        Ok(refs)
    }

    /// Delete a specific snapshot.
    pub fn delete(&self, snapshot_id: &str) -> Result<(), SandboxError> {
        let paths = self.paths_for(snapshot_id);
        if paths.dir.exists() {
            std::fs::remove_dir_all(&paths.dir)?;
        }
        Ok(())
    }

    /// Delete all snapshots.
    pub fn delete_all(&self) -> Result<(), SandboxError> {
        if self.snapshots_dir.exists() {
            std::fs::remove_dir_all(&self.snapshots_dir)?;
            std::fs::create_dir_all(&self.snapshots_dir)?;
        }
        Ok(())
    }

    /// Check if a snapshot exists (both files present).
    pub fn exists(&self, snapshot_id: &str) -> bool {
        let paths = self.paths_for(snapshot_id);
        paths.vm_state.exists() && paths.mem_file.exists()
    }
}

/// Generate a unique snapshot ID.
pub fn generate_snapshot_id(name: Option<&str>) -> CheckpointId {
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    match name {
        Some(n) => format!("{n}-{ts}"),
        None => format!("snap-{ts}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_store_lifecycle() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(tmp.path()).unwrap();

        // No snapshots initially
        assert!(store.list().unwrap().is_empty());

        // Prepare a snapshot dir
        let paths = store.prepare("snap-001").unwrap();
        assert!(paths.dir.exists());

        // Not listed yet (files don't exist)
        assert!(store.list().unwrap().is_empty());
        assert!(!store.exists("snap-001"));

        // Write fake snapshot files
        std::fs::write(&paths.vm_state, b"state").unwrap();
        std::fs::write(&paths.mem_file, b"memory").unwrap();

        // Now it's listed
        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "snap-001");
        assert!(store.exists("snap-001"));

        // Delete it
        store.delete("snap-001").unwrap();
        assert!(!store.exists("snap-001"));
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn snapshot_paths_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(tmp.path()).unwrap();
        let paths = store.paths_for("my-snap");

        assert!(paths.dir.ends_with("snapshots/my-snap"));
        assert!(paths.vm_state.ends_with("snapshots/my-snap/vm_state"));
        assert!(paths.mem_file.ends_with("snapshots/my-snap/mem_file"));
    }

    #[test]
    fn generate_snapshot_id_formats() {
        let id_named = generate_snapshot_id(Some("checkpoint"));
        assert!(id_named.starts_with("checkpoint-"));

        let id_anon = generate_snapshot_id(None);
        assert!(id_anon.starts_with("snap-"));
    }

    #[test]
    fn snapshot_store_delete_all() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(tmp.path()).unwrap();

        // Create two snapshots
        for name in &["snap-a", "snap-b"] {
            let paths = store.prepare(name).unwrap();
            std::fs::write(&paths.vm_state, b"s").unwrap();
            std::fs::write(&paths.mem_file, b"m").unwrap();
        }
        assert_eq!(store.list().unwrap().len(), 2);

        store.delete_all().unwrap();
        assert!(store.list().unwrap().is_empty());
    }
}
