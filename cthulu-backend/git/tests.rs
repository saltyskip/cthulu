//! Tests for the git module — all use real temporary git repos via `tempfile`.

use super::*;
use std::process::Command;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a git repo in the given directory with an initial commit.
fn init_repo(dir: &Path) -> &Path {
    Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .expect("git init failed");
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(dir)
        .output()
        .unwrap();
    // Create initial commit so HEAD exists
    std::fs::write(dir.join("README.md"), "# Test\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(dir)
        .output()
        .unwrap();
    dir
}

/// Create a child directory that is a git repo.
fn init_child_repo(parent: &Path, name: &str) -> PathBuf {
    let child = parent.join(name);
    std::fs::create_dir_all(&child).unwrap();
    init_repo(&child);
    child
}

// ---------------------------------------------------------------------------
// Discovery tests
// ---------------------------------------------------------------------------

#[test]
fn test_is_git_repo_true() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    assert!(is_git_repo(tmp.path()));
}

#[test]
fn test_is_git_repo_false() {
    let tmp = TempDir::new().unwrap();
    assert!(!is_git_repo(tmp.path()));
}

#[test]
fn test_is_git_repo_nested() {
    // A dir that contains a child repo is NOT itself a repo
    let tmp = TempDir::new().unwrap();
    init_child_repo(tmp.path(), "child");
    assert!(!is_git_repo(tmp.path()));
}

#[test]
fn test_discover_repos_empty() {
    let tmp = TempDir::new().unwrap();
    let repos = discover_repos(tmp.path());
    assert!(repos.is_empty());
}

#[test]
fn test_discover_repos_single() {
    let tmp = TempDir::new().unwrap();
    init_child_repo(tmp.path(), "backend");
    let repos = discover_repos(tmp.path());
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].relative, "backend");
}

#[test]
fn test_discover_repos_multiple() {
    let tmp = TempDir::new().unwrap();
    init_child_repo(tmp.path(), "backend");
    init_child_repo(tmp.path(), "frontend");
    std::fs::create_dir_all(tmp.path().join("shared")).unwrap();
    let repos = discover_repos(tmp.path());
    assert_eq!(repos.len(), 2);
    let names: Vec<&str> = repos.iter().map(|r| r.relative.as_str()).collect();
    assert!(names.contains(&"backend"));
    assert!(names.contains(&"frontend"));
}

#[test]
fn test_discover_repos_root_is_repo() {
    // If the root dir itself is a repo, discover_repos should return empty
    // (it only looks at children)
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    let repos = discover_repos(tmp.path());
    assert!(repos.is_empty());
}

#[test]
fn test_discover_repos_ignores_dotdirs() {
    let tmp = TempDir::new().unwrap();
    init_child_repo(tmp.path(), ".hidden");
    init_child_repo(tmp.path(), "visible");
    let repos = discover_repos(tmp.path());
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].relative, "visible");
}

#[test]
fn test_discover_repos_maxdepth() {
    let tmp = TempDir::new().unwrap();
    // depth 1: parent/level1/level2/level3 — repo at level3 should NOT be found
    let deep = tmp.path().join("level1").join("level2").join("level3");
    std::fs::create_dir_all(&deep).unwrap();
    init_repo(&deep);
    let repos = discover_repos(tmp.path());
    assert!(repos.is_empty(), "repo at depth 3 should not be discovered");

    // depth 2: parent/level1/repo — SHOULD be found
    let nested = tmp.path().join("packages").join("core");
    std::fs::create_dir_all(&nested).unwrap();
    init_repo(&nested);
    let repos = discover_repos(tmp.path());
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].relative, "packages/core");
}

// ---------------------------------------------------------------------------
// Snapshot tests
// ---------------------------------------------------------------------------

#[test]
fn test_snapshot_repo_clean() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    let snap = snapshot_repo(tmp.path()).expect("should snapshot");
    assert!(!snap.is_dirty);
    assert!(snap.files.is_empty());
    assert!(!snap.branch.is_empty());
    assert!(!snap.commit.is_empty());
}

#[test]
fn test_snapshot_repo_dirty() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    // Modify a tracked file
    std::fs::write(tmp.path().join("README.md"), "# Modified\n").unwrap();
    let snap = snapshot_repo(tmp.path()).expect("should snapshot");
    assert!(snap.is_dirty);
    assert!(!snap.files.is_empty());
    let readme = snap.files.iter().find(|f| f.path == "README.md");
    assert!(readme.is_some(), "README.md should appear in status");
    assert_eq!(readme.unwrap().status, "M");
}

#[test]
fn test_snapshot_repo_untracked() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    std::fs::write(tmp.path().join("new_file.txt"), "hello\n").unwrap();
    let snap = snapshot_repo(tmp.path()).expect("should snapshot");
    assert!(snap.is_dirty);
    let untracked = snap.files.iter().find(|f| f.path == "new_file.txt");
    assert!(untracked.is_some());
    assert_eq!(untracked.unwrap().status, "?");
}

#[test]
fn test_snapshot_repo_additions() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    // Append lines to tracked file
    std::fs::write(tmp.path().join("README.md"), "# Test\nline2\nline3\n").unwrap();
    let snap = snapshot_repo(tmp.path()).expect("should snapshot");
    let readme = snap.files.iter().find(|f| f.path == "README.md").unwrap();
    assert!(readme.additions > 0, "should have additions");
}

#[test]
fn test_snapshot_repo_deletions() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    // Write multi-line file, commit, then remove lines
    std::fs::write(tmp.path().join("README.md"), "line1\nline2\nline3\nline4\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "multiline"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    // Now remove lines
    std::fs::write(tmp.path().join("README.md"), "line1\n").unwrap();
    let snap = snapshot_repo(tmp.path()).expect("should snapshot");
    let readme = snap.files.iter().find(|f| f.path == "README.md").unwrap();
    assert!(readme.deletions > 0, "should have deletions");
}

#[test]
fn test_snapshot_repo_staged() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    std::fs::write(tmp.path().join("staged.txt"), "staged content\n").unwrap();
    Command::new("git")
        .args(["add", "staged.txt"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let snap = snapshot_repo(tmp.path()).expect("should snapshot");
    assert!(snap.is_dirty);
    let staged = snap.files.iter().find(|f| f.path == "staged.txt");
    assert!(staged.is_some(), "staged file should appear");
    assert_eq!(staged.unwrap().status, "A");
}

#[test]
fn test_snapshot_repo_branch() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());
    Command::new("git")
        .args(["checkout", "-b", "feature/test"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let snap = snapshot_repo(tmp.path()).expect("should snapshot");
    assert_eq!(snap.branch, "feature/test");
}

#[test]
fn test_snapshot_repo_not_git() {
    let tmp = TempDir::new().unwrap();
    let snap = snapshot_repo(tmp.path());
    assert!(snap.is_none());
}

#[test]
fn test_snapshot_group_multi() {
    let tmp = TempDir::new().unwrap();
    let backend = init_child_repo(tmp.path(), "backend");
    let frontend = init_child_repo(tmp.path(), "frontend");

    let group = WorktreeGroup {
        session_id: "test123".to_string(),
        shadow_root: tmp.path().to_path_buf(),
        repos: vec![
            WorktreeEntry {
                repo_root: backend.clone(),
                worktree_path: backend,
                branch: "main".to_string(),
            },
            WorktreeEntry {
                repo_root: frontend.clone(),
                worktree_path: frontend,
                branch: "main".to_string(),
            },
        ],
        source_dir: tmp.path().to_path_buf(),
        single_repo: false,
    };

    let multi = snapshot_group(&group);
    assert_eq!(multi.repos.len(), 2);
}

// ---------------------------------------------------------------------------
// WorktreeGroup lifecycle tests
// ---------------------------------------------------------------------------

#[test]
fn test_create_worktree_single_repo() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let group = create_worktree_group(tmp.path(), "sess-0001-abcd").unwrap();
    assert!(group.single_repo);
    assert_eq!(group.repos.len(), 1);
    assert!(group.shadow_root.exists(), "worktree dir should exist");
    assert!(
        is_git_repo(&group.shadow_root),
        "worktree should be a git repo"
    );

    // Check branch
    let branch_output = Command::new("git")
        .args(["-C", &group.shadow_root.to_string_lossy(), "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();
    let branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();
    assert_eq!(branch, "cthulu/sess-0001-ab");

    // Cleanup
    remove_worktree_group(&group).unwrap();
    assert!(!group.shadow_root.exists(), "worktree should be cleaned up");
}

#[test]
fn test_create_worktree_sibling_repos() {
    let tmp = TempDir::new().unwrap();
    init_child_repo(tmp.path(), "backend");
    init_child_repo(tmp.path(), "frontend");
    // Add a plain dir (should be symlinked)
    let shared = tmp.path().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(shared.join("utils.ts"), "export {};\n").unwrap();

    let group = create_worktree_group(tmp.path(), "sess-0002-efgh").unwrap();
    assert!(!group.single_repo);
    assert_eq!(group.repos.len(), 2);
    assert!(group.shadow_root.exists(), "shadow root should exist");

    // Each worktree should exist
    for entry in &group.repos {
        assert!(entry.worktree_path.exists(), "worktree {} should exist", entry.worktree_path.display());
        assert!(is_git_repo(&entry.worktree_path));
    }

    // Shared dir should be symlinked
    let shared_link = group.shadow_root.join("shared");
    assert!(shared_link.exists(), "shared should be symlinked");
    #[cfg(unix)]
    {
        assert!(
            std::fs::symlink_metadata(&shared_link).unwrap().file_type().is_symlink(),
            "shared should be a symlink"
        );
        let target = std::fs::read_link(&shared_link).unwrap();
        assert_eq!(target, shared);
    }

    // Cleanup
    remove_worktree_group(&group).unwrap();
    assert!(!group.shadow_root.exists(), "shadow root should be cleaned up");
}

#[test]
fn test_create_worktree_no_repos() {
    let tmp = TempDir::new().unwrap();
    std::fs::create_dir_all(tmp.path().join("just_a_dir")).unwrap();

    let result = create_worktree_group(tmp.path(), "sess-0003");
    assert!(result.is_err(), "should fail when no repos found");
}

#[test]
fn test_create_worktree_idempotent_gitignore() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    // Create two worktree groups — .gitignore should only have .claude/ once
    let g1 = create_worktree_group(tmp.path(), "sess-aaa1").unwrap();
    remove_worktree_group(&g1).unwrap();
    let g2 = create_worktree_group(tmp.path(), "sess-bbb2").unwrap();

    let gitignore = std::fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
    let count = gitignore.lines().filter(|l| l.trim() == ".claude/").count();
    assert_eq!(count, 1, ".claude/ should appear exactly once in .gitignore");

    remove_worktree_group(&g2).unwrap();
}

#[test]
fn test_remove_worktree_single() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let group = create_worktree_group(tmp.path(), "sess-del1").unwrap();
    let wt_path = group.shadow_root.clone();
    assert!(wt_path.exists());

    remove_worktree_group(&group).unwrap();
    assert!(!wt_path.exists(), "worktree dir should be removed");

    // Branch should be deleted
    let branch_output = Command::new("git")
        .args(["-C", &tmp.path().to_string_lossy(), "branch", "--list", "cthulu/sess-del"])
        .output()
        .unwrap();
    let branches = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();
    assert!(branches.is_empty(), "branch should be deleted");
}

#[test]
fn test_remove_worktree_sibling() {
    let tmp = TempDir::new().unwrap();
    init_child_repo(tmp.path(), "repo1");
    init_child_repo(tmp.path(), "repo2");

    let group = create_worktree_group(tmp.path(), "sess-del2").unwrap();
    let shadow = group.shadow_root.clone();
    assert!(shadow.exists());

    remove_worktree_group(&group).unwrap();
    assert!(!shadow.exists(), "shadow root should be removed");
}

#[test]
fn test_remove_worktree_missing_dir() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let group = create_worktree_group(tmp.path(), "sess-miss").unwrap();
    // Manually delete the worktree dir
    let _ = std::fs::remove_dir_all(&group.shadow_root);
    // Should not panic
    let result = remove_worktree_group(&group);
    assert!(result.is_ok(), "should handle missing dir gracefully");
}

#[test]
fn test_worktree_isolation() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let g1 = create_worktree_group(tmp.path(), "sess-iso1").unwrap();
    let g2 = create_worktree_group(tmp.path(), "sess-iso2").unwrap();

    // Modify a file in g1's worktree
    std::fs::write(g1.shadow_root.join("README.md"), "modified in session 1\n").unwrap();

    // g2's snapshot should be clean
    let snap2 = snapshot_group(&g2);
    assert_eq!(snap2.repos.len(), 1);
    assert!(!snap2.repos[0].is_dirty, "session 2 should be clean");

    // g1's snapshot should be dirty
    let snap1 = snapshot_group(&g1);
    assert_eq!(snap1.repos.len(), 1);
    assert!(snap1.repos[0].is_dirty, "session 1 should be dirty");

    remove_worktree_group(&g1).unwrap();
    remove_worktree_group(&g2).unwrap();
}

#[test]
fn test_worktree_file_paths_preserved() {
    let tmp = TempDir::new().unwrap();
    let backend = tmp.path().join("backend");
    std::fs::create_dir_all(backend.join("src")).unwrap();
    init_repo(&backend);
    std::fs::write(backend.join("src/main.rs"), "fn main() {}\n").unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&backend)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add main.rs"])
        .current_dir(&backend)
        .output()
        .unwrap();

    let group = create_worktree_group(tmp.path(), "sess-path").unwrap();
    assert!(!group.single_repo);

    // The file should exist at the same relative path in the shadow root
    let mirrored = group.shadow_root.join("backend/src/main.rs");
    assert!(mirrored.exists(), "file should exist at {}", mirrored.display());
    let content = std::fs::read_to_string(&mirrored).unwrap();
    assert_eq!(content, "fn main() {}\n");

    remove_worktree_group(&group).unwrap();
}

// ---------------------------------------------------------------------------
// Meta conversion tests
// ---------------------------------------------------------------------------

#[test]
fn test_meta_roundtrip() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let group = create_worktree_group(tmp.path(), "sess-meta").unwrap();
    let meta: WorktreeGroupMeta = (&group).into();

    // Serialize to YAML and back
    let yaml = serde_yaml::to_string(&meta).unwrap();
    let deserialized: WorktreeGroupMeta = serde_yaml::from_str(&yaml).unwrap();

    assert_eq!(deserialized.shadow_root, meta.shadow_root);
    assert_eq!(deserialized.source_dir, meta.source_dir);
    assert_eq!(deserialized.single_repo, meta.single_repo);
    assert_eq!(deserialized.repos.len(), meta.repos.len());

    // Reconstruct and verify
    let reconstructed = deserialized.to_worktree_group();
    assert_eq!(reconstructed.shadow_root, group.shadow_root);
    assert_eq!(reconstructed.source_dir, group.source_dir);

    remove_worktree_group(&group).unwrap();
}

#[test]
fn test_snapshot_from_meta() {
    let tmp = TempDir::new().unwrap();
    init_repo(tmp.path());

    let group = create_worktree_group(tmp.path(), "sess-smta").unwrap();
    // Make it dirty
    std::fs::write(group.shadow_root.join("README.md"), "changed\n").unwrap();

    let meta: WorktreeGroupMeta = (&group).into();
    let snap = snapshot_from_meta(&meta);
    assert_eq!(snap.repos.len(), 1);
    assert!(snap.repos[0].is_dirty);

    remove_worktree_group(&group).unwrap();
}
