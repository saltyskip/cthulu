use std::path::{Path, PathBuf};

use crate::sandbox::error::SandboxError;
use crate::sandbox::types::{DirEntry, GetFileRequest, GetFileResponse, PutFileRequest};

/// Filesystem jail: workspace creation, path containment, file ops.
///
/// All file operations are validated to stay within the workspace root.
/// This is NOT a chroot — it's best-effort path validation for the
/// `DangerousHost` backend.
pub struct FsJail {
    root: PathBuf,
}

impl FsJail {
    /// Create a new jail rooted at `root`. Creates the directory if needed.
    pub fn create(root: PathBuf) -> Result<Self, SandboxError> {
        std::fs::create_dir_all(&root).map_err(|e| {
            SandboxError::Provision(format!(
                "failed to create workspace dir {}: {e}",
                root.display()
            ))
        })?;
        Ok(Self { root })
    }

    /// Attach to an existing workspace directory.
    pub fn attach(root: PathBuf) -> Result<Self, SandboxError> {
        if !root.exists() {
            return Err(SandboxError::NotFound(format!(
                "workspace dir does not exist: {}",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Resolve a guest path to an absolute host path, ensuring it stays
    /// inside the workspace root. Returns error on path traversal attempts.
    pub fn resolve(&self, guest_path: &str) -> Result<PathBuf, SandboxError> {
        let guest = Path::new(guest_path);

        // Strip leading "/" — guest paths are relative to workspace root
        let relative = if guest.is_absolute() {
            guest.strip_prefix("/").unwrap_or(guest)
        } else {
            guest
        };

        // Normalize the relative path by processing each component.
        // This catches ".." traversal without needing the file to exist.
        let mut normalized = PathBuf::new();
        for component in relative.components() {
            match component {
                std::path::Component::Normal(seg) => normalized.push(seg),
                std::path::Component::CurDir => {} // "." — skip
                std::path::Component::ParentDir => {
                    // ".." — pop one level; if we'd escape root, reject
                    if !normalized.pop() {
                        return Err(SandboxError::Exec(format!(
                            "path escapes workspace: {guest_path}"
                        )));
                    }
                }
                // RootDir ("/") already stripped above; Prefix is Windows-only
                _ => {}
            }
        }

        // Build the final path from the canonical root so symlink resolution
        // is consistent (e.g. /var vs /private/var on macOS).
        let root_canonical = self
            .root
            .canonicalize()
            .unwrap_or_else(|_| self.root.clone());

        Ok(root_canonical.join(normalized))
    }

    pub fn put_file(&self, req: &PutFileRequest) -> Result<(), SandboxError> {
        let path = self.resolve(&req.path)?;
        if req.create_parents {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&path, &req.bytes)?;
        #[cfg(unix)]
        if let Some(mode) = req.mode {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))?;
        }
        Ok(())
    }

    pub fn get_file(&self, req: &GetFileRequest) -> Result<GetFileResponse, SandboxError> {
        let path = self.resolve(&req.path)?;
        let bytes = std::fs::read(&path)?;
        let (bytes, truncated) = match req.max_bytes {
            Some(max) if bytes.len() > max => (bytes[..max].to_vec(), true),
            _ => (bytes, false),
        };
        Ok(GetFileResponse { bytes, truncated })
    }

    pub fn read_dir(&self, guest_path: &str) -> Result<Vec<DirEntry>, SandboxError> {
        let path = self.resolve(guest_path)?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            entries.push(DirEntry {
                path: entry.file_name().to_string_lossy().to_string(),
                is_dir: meta.is_dir(),
                size_bytes: if meta.is_file() {
                    Some(meta.len())
                } else {
                    None
                },
            });
        }
        Ok(entries)
    }

    pub fn remove_path(&self, guest_path: &str, recursive: bool) -> Result<(), SandboxError> {
        let path = self.resolve(guest_path)?;
        if path.is_dir() {
            if recursive {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_dir(&path)?;
            }
        } else {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// Remove the entire workspace directory.
    pub fn destroy(&self) -> Result<(), SandboxError> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_put_get_file() {
        let tmp = tempfile::tempdir().unwrap();
        let jail = FsJail::create(tmp.path().join("workspace")).unwrap();

        jail.put_file(&PutFileRequest {
            path: "/hello.txt".into(),
            bytes: b"hello world".to_vec(),
            create_parents: false,
            mode: None,
        })
        .unwrap();

        let resp = jail
            .get_file(&GetFileRequest {
                path: "/hello.txt".into(),
                max_bytes: None,
            })
            .unwrap();
        assert_eq!(resp.bytes, b"hello world");
        assert!(!resp.truncated);
    }

    #[test]
    fn get_file_truncation() {
        let tmp = tempfile::tempdir().unwrap();
        let jail = FsJail::create(tmp.path().join("ws")).unwrap();

        jail.put_file(&PutFileRequest {
            path: "/big.txt".into(),
            bytes: vec![b'x'; 1000],
            create_parents: false,
            mode: None,
        })
        .unwrap();

        let resp = jail
            .get_file(&GetFileRequest {
                path: "/big.txt".into(),
                max_bytes: Some(100),
            })
            .unwrap();
        assert_eq!(resp.bytes.len(), 100);
        assert!(resp.truncated);
    }

    #[test]
    fn put_file_create_parents() {
        let tmp = tempfile::tempdir().unwrap();
        let jail = FsJail::create(tmp.path().join("ws")).unwrap();

        jail.put_file(&PutFileRequest {
            path: "/deep/nested/dir/file.txt".into(),
            bytes: b"nested".to_vec(),
            create_parents: true,
            mode: None,
        })
        .unwrap();

        let resp = jail
            .get_file(&GetFileRequest {
                path: "/deep/nested/dir/file.txt".into(),
                max_bytes: None,
            })
            .unwrap();
        assert_eq!(resp.bytes, b"nested");
    }

    #[test]
    fn path_traversal_blocked() {
        let tmp = tempfile::tempdir().unwrap();
        let jail = FsJail::create(tmp.path().join("ws")).unwrap();

        // Write a canary file outside the jail
        std::fs::write(tmp.path().join("secret.txt"), "secret").unwrap();

        let result = jail.get_file(&GetFileRequest {
            path: "/../secret.txt".into(),
            max_bytes: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn read_dir_works() {
        let tmp = tempfile::tempdir().unwrap();
        let jail = FsJail::create(tmp.path().join("ws")).unwrap();

        jail.put_file(&PutFileRequest {
            path: "/a.txt".into(),
            bytes: b"a".to_vec(),
            create_parents: false,
            mode: None,
        })
        .unwrap();
        jail.put_file(&PutFileRequest {
            path: "/b.txt".into(),
            bytes: b"bb".to_vec(),
            create_parents: false,
            mode: None,
        })
        .unwrap();
        std::fs::create_dir(jail.root().join("subdir")).unwrap();

        let entries = jail.read_dir("/").unwrap();
        assert_eq!(entries.len(), 3);

        let dir_entry = entries.iter().find(|e| e.path == "subdir").unwrap();
        assert!(dir_entry.is_dir);
    }

    #[test]
    fn remove_path_file() {
        let tmp = tempfile::tempdir().unwrap();
        let jail = FsJail::create(tmp.path().join("ws")).unwrap();

        jail.put_file(&PutFileRequest {
            path: "/rm_me.txt".into(),
            bytes: b"gone".to_vec(),
            create_parents: false,
            mode: None,
        })
        .unwrap();

        jail.remove_path("/rm_me.txt", false).unwrap();
        assert!(jail
            .get_file(&GetFileRequest {
                path: "/rm_me.txt".into(),
                max_bytes: None
            })
            .is_err());
    }

    #[test]
    fn destroy_removes_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let ws_path = tmp.path().join("ws");
        let jail = FsJail::create(ws_path.clone()).unwrap();

        jail.put_file(&PutFileRequest {
            path: "/file.txt".into(),
            bytes: b"data".to_vec(),
            create_parents: false,
            mode: None,
        })
        .unwrap();

        jail.destroy().unwrap();
        assert!(!ws_path.exists());
    }

    #[test]
    fn attach_fails_if_not_exists() {
        let result = FsJail::attach(PathBuf::from("/nonexistent/path/xyz"));
        assert!(result.is_err());
    }
}
