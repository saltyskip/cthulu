//! Git integration: repo discovery, worktree groups, and snapshots.
//!
//! A `WorktreeGroup` manages session-isolated git worktrees so concurrent
//! Claude sessions don't pollute each other's `git status`.
//!
//! Three modes:
//! - **Single repo**: working_dir IS a git repo → one worktree, Claude runs in it.
//! - **Sibling repos**: working_dir contains child repos → shadow root with
//!   per-repo worktrees + symlinks for non-repo content.
//! - **No git**: no repos found → returns error, caller skips git features.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(test)]
mod tests;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A discovered git repository relative to a working directory.
#[derive(Debug, Clone)]
pub struct RepoInfo {
    /// Absolute path to the repo root.
    pub root: PathBuf,
    /// Relative path from the working_dir (e.g., "backend").
    pub relative: String,
}

/// A group of worktrees created for a single session.
#[derive(Debug, Clone)]
pub struct WorktreeGroup {
    pub session_id: String,
    /// The directory Claude actually runs in.
    /// For single-repo: the worktree path.
    /// For sibling-repos: the shadow root.
    pub shadow_root: PathBuf,
    /// One entry per discovered repo.
    pub repos: Vec<WorktreeEntry>,
    /// The original working directory.
    pub source_dir: PathBuf,
    /// Whether this is a single-repo setup (no shadow root, no symlinks).
    pub single_repo: bool,
}

/// A single worktree within a WorktreeGroup.
#[derive(Debug, Clone)]
pub struct WorktreeEntry {
    /// The original repo root (e.g., /project/backend).
    pub repo_root: PathBuf,
    /// The worktree path inside the shadow root.
    pub worktree_path: PathBuf,
    /// The branch name (cthulu/{session_id_short}).
    pub branch: String,
}

/// Serializable metadata stored on InteractSession.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeGroupMeta {
    pub shadow_root: String,
    pub source_dir: String,
    pub single_repo: bool,
    pub repos: Vec<WorktreeEntryMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeEntryMeta {
    pub repo_root: String,
    pub worktree_path: String,
    pub branch: String,
}

/// Snapshot of a single repo's git state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSnapshot {
    /// Relative path to repo root (e.g., "backend" or ".").
    pub root: String,
    pub branch: String,
    pub commit: String,
    pub is_dirty: bool,
    pub files: Vec<GitFileStatus>,
}

/// Status of a single file in a git repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitFileStatus {
    pub path: String,
    /// M, A, D, ?, R
    pub status: String,
    pub additions: u32,
    pub deletions: u32,
}

/// Snapshot of all repos in a WorktreeGroup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiRepoSnapshot {
    pub repos: Vec<RepoSnapshot>,
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// Check if a directory is a git repository root.
pub fn is_git_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Discover git repos as immediate children of `dir` (maxdepth 1 for children,
/// but also checks one level deeper for nested repos like project/packages/foo).
/// Skips hidden directories (starting with '.').
pub fn discover_repos(dir: &Path) -> Vec<RepoInfo> {
    let mut repos = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return repos,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Skip hidden directories
        if name_str.starts_with('.') {
            continue;
        }

        if is_git_repo(&path) {
            repos.push(RepoInfo {
                root: path,
                relative: name_str.to_string(),
            });
        } else {
            // Check one level deeper (maxdepth 2)
            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                for sub_entry in sub_entries.flatten() {
                    let sub_path = sub_entry.path();
                    if !sub_path.is_dir() {
                        continue;
                    }
                    let sub_name = sub_entry.file_name();
                    let sub_name_str = sub_name.to_string_lossy();
                    if sub_name_str.starts_with('.') {
                        continue;
                    }
                    if is_git_repo(&sub_path) {
                        let relative = format!("{}/{}", name_str, sub_name_str);
                        repos.push(RepoInfo {
                            root: sub_path,
                            relative,
                        });
                    }
                }
            }
        }
    }

    repos.sort_by(|a, b| a.relative.cmp(&b.relative));
    repos
}

// ---------------------------------------------------------------------------
// Worktree group lifecycle
// ---------------------------------------------------------------------------

/// Short session ID for branch names and directory names (first 12 chars).
fn short_id(session_id: &str) -> &str {
    &session_id[..session_id.len().min(12)]
}

/// Branch name for a session's worktree.
fn worktree_branch(session_id: &str) -> String {
    format!("cthulu/{}", short_id(session_id))
}

/// Ensure `.claude/` is in a repo's .gitignore (idempotent).
fn ensure_gitignore_claude(repo_root: &Path) -> Result<()> {
    let gitignore = repo_root.join(".gitignore");
    let content = std::fs::read_to_string(&gitignore).unwrap_or_default();

    // Check if .claude/ is already ignored
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == ".claude/" || trimmed == ".claude" {
            return Ok(());
        }
    }

    // Append .claude/ to .gitignore
    let mut new_content = content;
    if !new_content.is_empty() && !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str(".claude/\n");
    std::fs::write(&gitignore, new_content)
        .with_context(|| format!("failed to update .gitignore at {}", gitignore.display()))?;
    Ok(())
}

/// Create a WorktreeGroup for a session.
///
/// - If `working_dir` is itself a git repo: creates a single worktree.
/// - If `working_dir` contains child repos: creates a shadow root with
///   worktrees + symlinks.
/// - If no repos found: returns an error.
pub fn create_worktree_group(working_dir: &Path, session_id: &str) -> Result<WorktreeGroup> {
    let branch = worktree_branch(session_id);

    // Case 1: working_dir itself is a git repo
    if is_git_repo(working_dir) {
        ensure_gitignore_claude(working_dir)?;

        let worktree_path = working_dir
            .join(".claude")
            .join("worktrees")
            .join(short_id(session_id));
        std::fs::create_dir_all(worktree_path.parent().unwrap())?;

        let output = Command::new("git")
            .args([
                "-C",
                &working_dir.to_string_lossy(),
                "worktree",
                "add",
                &worktree_path.to_string_lossy(),
                "-b",
                &branch,
                "--no-track",
            ])
            .output()
            .context("failed to run git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git worktree add failed: {}", stderr.trim());
        }

        return Ok(WorktreeGroup {
            session_id: session_id.to_string(),
            shadow_root: worktree_path.clone(),
            repos: vec![WorktreeEntry {
                repo_root: working_dir.to_path_buf(),
                worktree_path,
                branch,
            }],
            source_dir: working_dir.to_path_buf(),
            single_repo: true,
        });
    }

    // Case 2: check for child repos
    let child_repos = discover_repos(working_dir);
    if child_repos.is_empty() {
        bail!("no git repositories found in {}", working_dir.display());
    }

    // Create shadow root
    let shadow_root = working_dir
        .join(".claude")
        .join("sessions")
        .join(short_id(session_id));
    std::fs::create_dir_all(&shadow_root)?;

    let mut entries = Vec::new();

    // Create worktrees for each repo
    for repo in &child_repos {
        ensure_gitignore_claude(&repo.root)?;

        let wt_path = shadow_root.join(&repo.relative);
        // Ensure parent dirs exist for nested repos (e.g., packages/foo)
        if let Some(parent) = wt_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = Command::new("git")
            .args([
                "-C",
                &repo.root.to_string_lossy(),
                "worktree",
                "add",
                &wt_path.to_string_lossy(),
                "-b",
                &branch,
                "--no-track",
            ])
            .output()
            .with_context(|| format!("failed to run git worktree add for {}", repo.relative))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Clean up already-created worktrees on failure
            for entry in &entries {
                let _ = remove_single_worktree(entry);
            }
            let _ = std::fs::remove_dir_all(&shadow_root);
            bail!(
                "git worktree add failed for {}: {}",
                repo.relative,
                stderr.trim()
            );
        }

        entries.push(WorktreeEntry {
            repo_root: repo.root.clone(),
            worktree_path: wt_path,
            branch: branch.clone(),
        });
    }

    // Symlink non-repo content into shadow root
    let repo_names: std::collections::HashSet<&str> = child_repos
        .iter()
        .map(|r| {
            // For nested repos like "packages/foo", we only need the top-level dir
            r.relative.split('/').next().unwrap_or(&r.relative)
        })
        .collect();

    if let Ok(dir_entries) = std::fs::read_dir(working_dir) {
        for entry in dir_entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Skip .claude, hidden dirs, and repo dirs
            if name_str.starts_with('.') || repo_names.contains(name_str.as_ref()) {
                continue;
            }
            let target = entry.path();
            let link = shadow_root.join(&*name_str);
            if !link.exists() {
                #[cfg(unix)]
                {
                    let _ = std::os::unix::fs::symlink(&target, &link);
                }
                #[cfg(windows)]
                {
                    if target.is_dir() {
                        let _ = std::os::windows::fs::symlink_dir(&target, &link);
                    } else {
                        let _ = std::os::windows::fs::symlink_file(&target, &link);
                    }
                }
            }
        }
    }

    Ok(WorktreeGroup {
        session_id: session_id.to_string(),
        shadow_root,
        repos: entries,
        source_dir: working_dir.to_path_buf(),
        single_repo: false,
    })
}

/// Remove a single worktree entry (worktree dir + branch).
fn remove_single_worktree(entry: &WorktreeEntry) -> Result<()> {
    // Remove the worktree
    let output = Command::new("git")
        .args([
            "-C",
            &entry.repo_root.to_string_lossy(),
            "worktree",
            "remove",
            &entry.worktree_path.to_string_lossy(),
            "--force",
        ])
        .output();

    if let Ok(out) = &output
        && !out.status.success() {
            // If worktree dir is already gone, try prune instead
            let _ = Command::new("git")
                .args([
                    "-C",
                    &entry.repo_root.to_string_lossy(),
                    "worktree",
                    "prune",
                ])
                .output();
        }

    // Delete the branch
    let _ = Command::new("git")
        .args([
            "-C",
            &entry.repo_root.to_string_lossy(),
            "branch",
            "-D",
            &entry.branch,
        ])
        .output();

    Ok(())
}

/// Remove all worktrees in a WorktreeGroup and clean up the shadow root.
pub fn remove_worktree_group(group: &WorktreeGroup) -> Result<()> {
    for entry in &group.repos {
        // Best-effort removal — don't fail if already cleaned up
        let _ = remove_single_worktree(entry);
    }

    // Remove shadow root (for sibling-repo mode)
    if !group.single_repo && group.shadow_root.exists() {
        std::fs::remove_dir_all(&group.shadow_root)
            .with_context(|| {
                format!(
                    "failed to remove shadow root {}",
                    group.shadow_root.display()
                )
            })?;
    }

    // For single-repo mode, the worktree dir should already be gone
    // but clean up if it still exists
    if group.single_repo && group.shadow_root.exists() {
        let _ = std::fs::remove_dir_all(&group.shadow_root);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------

/// Snapshot a single git repo directory.
/// Returns None if the directory is not a git repo.
pub fn snapshot_repo(dir: &Path) -> Option<RepoSnapshot> {
    // Check it's a git repo (either has .git or is a worktree)
    if !dir.join(".git").exists() {
        return None;
    }

    // Get current branch
    let branch_output = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    let branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();

    // Get current commit hash
    let commit_output = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    let commit = String::from_utf8_lossy(&commit_output.stdout).trim().to_string();

    // Get status (porcelain v1)
    let status_output = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "status", "--porcelain"])
        .output()
        .ok()?;
    let status_text = String::from_utf8_lossy(&status_output.stdout);

    let mut files = Vec::new();
    let mut is_dirty = false;

    for line in status_text.lines() {
        if line.len() < 3 {
            continue;
        }
        is_dirty = true;
        let xy = &line[..2];
        let path = line[3..].trim().to_string();

        // Determine status character from XY codes
        let status = if xy.contains('?') {
            "?"
        } else if xy.contains('A') {
            "A"
        } else if xy.contains('D') {
            "D"
        } else if xy.contains('R') {
            "R"
        } else if xy.contains('M') {
            "M"
        } else {
            "M" // default to modified
        };

        files.push(GitFileStatus {
            path,
            status: status.to_string(),
            additions: 0,
            deletions: 0,
        });
    }

    // Get diff --numstat for addition/deletion counts (tracked files only)
    let numstat_output = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "diff", "--numstat"])
        .output()
        .ok();

    // Also get staged numstat
    let staged_numstat = Command::new("git")
        .args(["-C", &dir.to_string_lossy(), "diff", "--numstat", "--staged"])
        .output()
        .ok();

    // Merge numstat results into file statuses
    let mut numstat_map: std::collections::HashMap<String, (u32, u32)> =
        std::collections::HashMap::new();

    for output in [numstat_output, staged_numstat].into_iter().flatten() {
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                let adds: u32 = parts[0].parse().unwrap_or(0);
                let dels: u32 = parts[1].parse().unwrap_or(0);
                let path = parts[2].to_string();
                let entry = numstat_map.entry(path).or_insert((0, 0));
                entry.0 = entry.0.max(adds); // take max to avoid double-counting
                entry.1 = entry.1.max(dels);
            }
        }
    }

    for file in &mut files {
        if let Some((adds, dels)) = numstat_map.get(&file.path) {
            file.additions = *adds;
            file.deletions = *dels;
        }
    }

    Some(RepoSnapshot {
        root: ".".to_string(), // caller should set this
        branch,
        commit,
        is_dirty,
        files,
    })
}

/// Snapshot all repos in a WorktreeGroup.
pub fn snapshot_group(group: &WorktreeGroup) -> MultiRepoSnapshot {
    let mut repos = Vec::new();

    for entry in &group.repos {
        if let Some(mut snapshot) = snapshot_repo(&entry.worktree_path) {
            // Set root relative to source_dir
            if group.single_repo {
                snapshot.root = ".".to_string();
            } else if let Ok(rel) = entry.worktree_path.strip_prefix(&group.shadow_root) {
                snapshot.root = rel.to_string_lossy().to_string();
            }
            repos.push(snapshot);
        }
    }

    MultiRepoSnapshot { repos }
}

/// Create a snapshot from serialized metadata (for use from session data).
pub fn snapshot_from_meta(meta: &WorktreeGroupMeta) -> MultiRepoSnapshot {
    let group = WorktreeGroup {
        session_id: String::new(),
        shadow_root: PathBuf::from(&meta.shadow_root),
        repos: meta
            .repos
            .iter()
            .map(|r| WorktreeEntry {
                repo_root: PathBuf::from(&r.repo_root),
                worktree_path: PathBuf::from(&r.worktree_path),
                branch: r.branch.clone(),
            })
            .collect(),
        source_dir: PathBuf::from(&meta.source_dir),
        single_repo: meta.single_repo,
    };
    snapshot_group(&group)
}

// ---------------------------------------------------------------------------
// Single-file diff
// ---------------------------------------------------------------------------

/// Get the unified diff for a single file in a worktree directory.
///
/// Tries `git diff HEAD -- <path>`, then `git diff --staged HEAD -- <path>`,
/// and falls back to reading an untracked file as an all-added diff.
pub fn diff_file(worktree_path: &Path, file_path: &str) -> Option<String> {
    let dir = worktree_path.to_string_lossy();

    // Try unstaged diff against HEAD
    let output = Command::new("git")
        .args(["-C", &dir, "diff", "HEAD", "--", file_path])
        .output()
        .ok()?;
    let diff = String::from_utf8_lossy(&output.stdout).to_string();
    if !diff.trim().is_empty() {
        return Some(diff);
    }

    // Try staged diff against HEAD
    let output = Command::new("git")
        .args(["-C", &dir, "diff", "--staged", "HEAD", "--", file_path])
        .output()
        .ok()?;
    let diff = String::from_utf8_lossy(&output.stdout).to_string();
    if !diff.trim().is_empty() {
        return Some(diff);
    }

    // Untracked file: read and format as all-added
    let full_path = worktree_path.join(file_path);
    if full_path.is_file() {
        let content = std::fs::read_to_string(&full_path).ok()?;
        let lines: Vec<&str> = content.lines().collect();
        let mut diff = format!(
            "diff --git a/{f} b/{f}\nnew file mode 100644\n--- /dev/null\n+++ b/{f}\n@@ -0,0 +1,{n} @@\n",
            f = file_path,
            n = lines.len()
        );
        for line in &lines {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
        return Some(diff);
    }

    None
}

// ---------------------------------------------------------------------------
// Conversion: WorktreeGroup ↔ WorktreeGroupMeta
// ---------------------------------------------------------------------------

impl From<&WorktreeGroup> for WorktreeGroupMeta {
    fn from(group: &WorktreeGroup) -> Self {
        WorktreeGroupMeta {
            shadow_root: group.shadow_root.to_string_lossy().to_string(),
            source_dir: group.source_dir.to_string_lossy().to_string(),
            single_repo: group.single_repo,
            repos: group
                .repos
                .iter()
                .map(|r| WorktreeEntryMeta {
                    repo_root: r.repo_root.to_string_lossy().to_string(),
                    worktree_path: r.worktree_path.to_string_lossy().to_string(),
                    branch: r.branch.clone(),
                })
                .collect(),
        }
    }
}

impl WorktreeGroupMeta {
    /// Reconstruct a WorktreeGroup from metadata (for cleanup on session delete).
    pub fn to_worktree_group(&self) -> WorktreeGroup {
        WorktreeGroup {
            session_id: String::new(),
            shadow_root: PathBuf::from(&self.shadow_root),
            repos: self
                .repos
                .iter()
                .map(|r| WorktreeEntry {
                    repo_root: PathBuf::from(&r.repo_root),
                    worktree_path: PathBuf::from(&r.worktree_path),
                    branch: r.branch.clone(),
                })
                .collect(),
            source_dir: PathBuf::from(&self.source_dir),
            single_repo: self.single_repo,
        }
    }
}
