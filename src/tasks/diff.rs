use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{Context, Result};

pub struct FileDiff {
    pub path: String,
    pub content: String,
    pub additions: usize,
    pub deletions: usize,
}

pub enum DiffContext {
    Inline(String),
    Chunked { manifest: String, dir: PathBuf },
}

impl DiffContext {
    pub fn text(&self) -> String {
        match self {
            DiffContext::Inline(d) => d.clone(),
            DiffContext::Chunked { manifest, .. } => manifest.clone(),
        }
    }
}

pub fn split_diff_by_file(diff: &str) -> Vec<FileDiff> {
    let mut files = Vec::new();
    let mut current_path = String::new();
    let mut current_content = String::new();

    for line in diff.lines() {
        if line.starts_with("diff --git a/") {
            // Flush previous chunk
            if !current_path.is_empty() {
                let (additions, deletions) = count_changes(&current_content);
                files.push(FileDiff {
                    path: current_path,
                    content: current_content,
                    additions,
                    deletions,
                });
            }

            // Extract path from "diff --git a/path b/path"
            current_path = line
                .strip_prefix("diff --git a/")
                .and_then(|rest| rest.split_once(" b/"))
                .map(|(path, _)| path.to_string())
                .unwrap_or_default();
            current_content = String::from(line);
            current_content.push('\n');
        } else if !current_path.is_empty() {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    // Flush last chunk
    if !current_path.is_empty() {
        let (additions, deletions) = count_changes(&current_content);
        files.push(FileDiff {
            path: current_path,
            content: current_content,
            additions,
            deletions,
        });
    }

    files
}

fn count_changes(diff_content: &str) -> (usize, usize) {
    let mut additions = 0;
    let mut deletions = 0;
    for line in diff_content.lines() {
        if line.starts_with('+') && !line.starts_with("+++") {
            additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            deletions += 1;
        }
    }
    (additions, deletions)
}

fn sanitize_path(path: &str) -> String {
    path.replace('/', "__")
}

pub fn prepare_diff_context(
    diff: &str,
    pr_number: u64,
    max_inline_size: usize,
) -> Result<DiffContext> {
    if diff.len() <= max_inline_size {
        return Ok(DiffContext::Inline(diff.to_string()));
    }

    let file_diffs = split_diff_by_file(diff);
    let run_id = uuid::Uuid::new_v4();
    let dir = PathBuf::from(format!("/tmp/cthulu-review/{pr_number}-{run_id}"));

    // Clean any previous run for this PR
    if dir.exists() {
        std::fs::remove_dir_all(&dir).ok();
    }
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create diff dir: {}", dir.display()))?;

    let total_additions: usize = file_diffs.iter().map(|f| f.additions).sum();
    let total_deletions: usize = file_diffs.iter().map(|f| f.deletions).sum();
    let total_lines = total_additions + total_deletions;

    let mut manifest = String::new();
    writeln!(
        manifest,
        "This PR diff is too large to include inline ({} chars, {} files). \
         The diff has been split into per-file chunks.",
        diff.len(),
        file_diffs.len()
    )
    .unwrap();
    writeln!(manifest, "Read each file's diff from the paths below to review it.").unwrap();
    writeln!(manifest).unwrap();
    writeln!(
        manifest,
        "## Changed Files ({} files, {} lines changed: +{} -{})",
        file_diffs.len(),
        total_lines,
        total_additions,
        total_deletions
    )
    .unwrap();
    writeln!(manifest).unwrap();
    writeln!(manifest, "| File | Changes | Diff Path |").unwrap();
    writeln!(manifest, "|------|---------|-----------|").unwrap();

    for file_diff in &file_diffs {
        let filename = format!("{}.diff", sanitize_path(&file_diff.path));
        let file_path = dir.join(&filename);
        std::fs::write(&file_path, &file_diff.content)
            .with_context(|| format!("failed to write diff chunk: {}", file_path.display()))?;

        writeln!(
            manifest,
            "| `{}` | +{} -{} | `{}` |",
            file_diff.path,
            file_diff.additions,
            file_diff.deletions,
            file_path.display()
        )
        .unwrap();
    }

    writeln!(manifest).unwrap();
    writeln!(
        manifest,
        "Review each file's diff by reading the path above, then review the full file in the repo for context."
    )
    .unwrap();

    Ok(DiffContext::Chunked { manifest, dir })
}

pub fn cleanup(ctx: &DiffContext) {
    if let DiffContext::Chunked { dir, .. } = ctx {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_empty_diff() {
        let result = split_diff_by_file("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_split_single_file() {
        let diff = "\
diff --git a/src/main.rs b/src/main.rs
index abc123..def456 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!(\"hello\");
 }
";
        let result = split_diff_by_file(diff);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, "src/main.rs");
        assert_eq!(result[0].additions, 1);
        assert_eq!(result[0].deletions, 0);
        assert!(result[0].content.contains("println"));
    }

    #[test]
    fn test_split_multiple_files() {
        let diff = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1 +1,2 @@
 line1
+line2
diff --git a/src/b.rs b/src/b.rs
--- a/src/b.rs
+++ b/src/b.rs
@@ -1,2 +1 @@
 keep
-removed
";
        let result = split_diff_by_file(diff);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].path, "src/a.rs");
        assert_eq!(result[0].additions, 1);
        assert_eq!(result[0].deletions, 0);
        assert_eq!(result[1].path, "src/b.rs");
        assert_eq!(result[1].additions, 0);
        assert_eq!(result[1].deletions, 1);
    }

    #[test]
    fn test_inline_under_threshold() {
        let diff = "diff --git a/x b/x\n+small\n";
        let result = prepare_diff_context(diff, 1, 1000).unwrap();
        assert!(matches!(result, DiffContext::Inline(d) if d == diff));
    }

    #[test]
    fn test_chunked_over_threshold() {
        let mut diff = String::new();
        // Create a diff with 3 files, each large enough
        for i in 0..3 {
            writeln!(diff, "diff --git a/file{i}.rs b/file{i}.rs").unwrap();
            writeln!(diff, "--- a/file{i}.rs").unwrap();
            writeln!(diff, "+++ b/file{i}.rs").unwrap();
            for j in 0..100 {
                writeln!(diff, "+added line {j} in file {i}").unwrap();
            }
        }

        let result = prepare_diff_context(&diff, 99999, 100).unwrap();
        match &result {
            DiffContext::Chunked { manifest, dir } => {
                assert!(manifest.contains("file0.rs"));
                assert!(manifest.contains("file1.rs"));
                assert!(manifest.contains("file2.rs"));
                assert!(manifest.contains("3 files"));
                assert!(dir.exists());
                assert!(dir.join("file0.rs.diff").exists());
                assert!(dir.join("file1.rs.diff").exists());
                assert!(dir.join("file2.rs.diff").exists());
            }
            DiffContext::Inline(_) => panic!("expected Chunked"),
        }

        // Cleanup
        let chunked_dir = match &result {
            DiffContext::Chunked { dir, .. } => dir.clone(),
            _ => unreachable!(),
        };
        cleanup(&result);
        assert!(!chunked_dir.exists());
    }

    #[test]
    fn test_file_path_sanitization() {
        let diff = "\
diff --git a/src/tasks/triggers/github.rs b/src/tasks/triggers/github.rs
--- a/src/tasks/triggers/github.rs
+++ b/src/tasks/triggers/github.rs
@@ -1 +1,2 @@
 line
+added
";
        let result = prepare_diff_context(diff, 88888, 10).unwrap();
        match &result {
            DiffContext::Chunked { dir, .. } => {
                assert!(dir.join("src__tasks__triggers__github.rs.diff").exists());
            }
            DiffContext::Inline(_) => panic!("expected Chunked"),
        }
        cleanup(&result);
    }

    #[test]
    fn test_cleanup_removes_temp_dir() {
        let diff = "diff --git a/x.rs b/x.rs\n+line\n";
        let result = prepare_diff_context(diff, 77777, 5).unwrap();
        let dir = match &result {
            DiffContext::Chunked { dir, .. } => dir.clone(),
            _ => panic!("expected Chunked"),
        };
        assert!(dir.exists());
        cleanup(&result);
        assert!(!dir.exists());
    }

    #[test]
    fn test_cleanup_noop_for_inline() {
        let ctx = DiffContext::Inline("small".to_string());
        cleanup(&ctx); // should not panic
    }

    #[test]
    fn test_change_counts_in_manifest() {
        let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@
 keep
+added1
+added2
-removed1
";
        let result = prepare_diff_context(diff, 66666, 10).unwrap();
        match &result {
            DiffContext::Chunked { manifest, .. } => {
                assert!(manifest.contains("+2 -1"));
            }
            DiffContext::Inline(_) => panic!("expected Chunked"),
        }
        cleanup(&result);
    }
}
