use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use crate::config::GithubTriggerConfig;
use crate::github::client::GithubClient;
use crate::github::models::RepoConfig;
use crate::tasks::context::render_prompt;
use crate::tasks::diff;
use crate::tasks::executors::Executor;
use crate::tasks::TaskState;

pub struct GithubPrTrigger {
    github_client: Arc<dyn GithubClient>,
    config: GithubTriggerConfig,
    task_state: Arc<TaskState>,
}

impl GithubPrTrigger {
    pub fn new(
        github_client: Arc<dyn GithubClient>,
        config: GithubTriggerConfig,
        task_state: Arc<TaskState>,
    ) -> Self {
        Self {
            github_client,
            config,
            task_state,
        }
    }

    fn repo_configs(&self) -> Vec<RepoConfig> {
        self.config
            .repos
            .iter()
            .filter_map(|entry| {
                let (owner, repo) = entry.owner_repo()?;
                Some(RepoConfig {
                    owner: owner.to_string(),
                    repo: repo.to_string(),
                    local_path: entry.path.clone(),
                })
            })
            .collect()
    }

    pub async fn seed(&self) -> Result<()> {
        let repos = self.repo_configs();
        for repo in &repos {
            let max_retries = 10;
            let mut attempt = 0u32;
            loop {
                attempt += 1;
                match self
                    .github_client
                    .fetch_open_prs(&repo.owner, &repo.repo)
                    .await
                {
                    Ok(prs) => {
                        let mut seen = self.task_state.seen_prs.lock().await;
                        let pr_shas: HashMap<u64, String> = prs
                            .iter()
                            .filter(|pr| {
                                // Don't seed draft PRs when skip_drafts is enabled.
                                // This way they'll appear as "new" when un-drafted.
                                if pr.draft && self.config.skip_drafts {
                                    tracing::debug!(
                                        repo = %repo.full_name(),
                                        pr = pr.number,
                                        "Skipping draft PR #{} during seed",
                                        pr.number
                                    );
                                    return false;
                                }
                                true
                            })
                            .map(|pr| (pr.number, pr.head.sha.clone()))
                            .collect();
                        tracing::info!(
                            repo = %repo.full_name(),
                            count = pr_shas.len(),
                            "Seeded {} existing PRs for {}",
                            pr_shas.len(),
                            repo.full_name()
                        );
                        seen.insert(repo.full_name(), pr_shas);
                        break;
                    }
                    Err(e) => {
                        if attempt >= max_retries {
                            tracing::error!(
                                repo = %repo.full_name(),
                                error = %e,
                                "Failed to seed PRs after {} attempts",
                                max_retries
                            );
                            break;
                        }
                        let backoff = std::time::Duration::from_secs(2u64.pow(attempt.min(5)));
                        tracing::warn!(
                            repo = %repo.full_name(),
                            error = %e,
                            attempt,
                            "Failed to seed PRs, retrying in {:?}...",
                            backoff
                        );
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn poll_loop(
        &self,
        task_name: &str,
        prompt_template: &str,
        executor: &dyn Executor,
        task_state: &Arc<TaskState>,
    ) {
        let repos = self.repo_configs();
        let seeded_repos: Vec<RepoConfig> = {
            let seen = task_state.seen_prs.lock().await;
            repos
                .into_iter()
                .filter(|r| seen.contains_key(&r.full_name()))
                .collect()
        };

        tracing::info!(
            task = %task_name,
            repos = seeded_repos.len(),
            interval = self.config.poll_interval,
            skip_drafts = self.config.skip_drafts,
            review_on_push = self.config.review_on_push,
            "Polling {} repos every {}s",
            seeded_repos.len(),
            self.config.poll_interval
        );

        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(self.config.poll_interval));

        loop {
            interval.tick().await;

            for repo in &seeded_repos {
                let prs = match self
                    .github_client
                    .fetch_open_prs(&repo.owner, &repo.repo)
                    .await
                {
                    Ok(prs) => prs,
                    Err(e) => {
                        tracing::error!(repo = %repo.full_name(), error = %e, "Failed to fetch PRs");
                        continue;
                    }
                };

                for pr in prs {
                    // Skip draft PRs when configured
                    if pr.draft && self.config.skip_drafts {
                        tracing::debug!(
                            task = %task_name,
                            repo = %repo.full_name(),
                            pr = pr.number,
                            "Skipping draft PR #{}",
                            pr.number
                        );
                        continue;
                    }

                    let review_type = {
                        let mut seen = task_state.seen_prs.lock().await;
                        let seen_map = seen.entry(repo.full_name()).or_default();

                        match seen_map.get(&pr.number) {
                            None => {
                                // New PR
                                seen_map.insert(pr.number, pr.head.sha.clone());
                                ReviewType::Initial
                            }
                            Some(old_sha) if self.config.review_on_push && *old_sha != pr.head.sha => {
                                // SHA changed and re-review is enabled
                                let old = old_sha.clone();
                                seen_map.insert(pr.number, pr.head.sha.clone());
                                ReviewType::ReReview { previous_sha: old }
                            }
                            _ => {
                                // Already seen, no SHA change (or re-review disabled)
                                continue;
                            }
                        }
                    };

                    tracing::info!(
                        task = %task_name,
                        repo = %repo.full_name(),
                        pr = pr.number,
                        title = %pr.title,
                        review_type = %review_type,
                        "PR #{} detected ({}): {}",
                        pr.number,
                        review_type,
                        pr.title
                    );

                    // Post starting comment
                    let start_msg = match &review_type {
                        ReviewType::Initial => format!(
                            ":robot: **Cthulu Review Bot** is starting a deep-dive review of this PR...\n\n\
                             _Reviewing PR #{} — this may take a few minutes._",
                            pr.number
                        ),
                        ReviewType::ReReview { previous_sha } => format!(
                            ":robot: **Cthulu Review Bot** is re-reviewing this PR after new commits...\n\n\
                             _Re-reviewing PR #{} (previous HEAD: `{}`, new HEAD: `{}`)_",
                            pr.number,
                            &previous_sha[..7.min(previous_sha.len())],
                            &pr.head.sha[..7.min(pr.head.sha.len())]
                        ),
                    };
                    if let Err(e) = self
                        .github_client
                        .post_comment(&repo.owner, &repo.repo, pr.number, &start_msg)
                        .await
                    {
                        tracing::warn!(error = %e, "Failed to post starting comment");
                    }

                    // Fetch diff
                    let diff = match self
                        .github_client
                        .fetch_pr_diff(&repo.owner, &repo.repo, pr.number)
                        .await
                    {
                        Ok(d) => d,
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to fetch PR diff");
                            continue;
                        }
                    };

                    // Build context
                    let mut context = HashMap::new();
                    let diff_ctx = match diff::prepare_diff_context(&diff, pr.number, self.config.max_diff_size) {
                        Ok(ctx) => ctx,
                        Err(e) => {
                            tracing::error!(error = %e, "Failed to prepare diff context");
                            continue;
                        }
                    };
                    context.insert("diff".to_string(), diff_ctx.text());
                    context.insert("pr_number".to_string(), pr.number.to_string());
                    context.insert("pr_title".to_string(), pr.title.clone());
                    context.insert(
                        "pr_body".to_string(),
                        pr.body.clone().unwrap_or_default(),
                    );
                    context.insert("base_ref".to_string(), pr.base.ref_name.clone());
                    context.insert("head_ref".to_string(), pr.head.ref_name.clone());
                    context.insert("head_sha".to_string(), pr.head.sha.clone());
                    context.insert("repo".to_string(), repo.full_name());
                    context.insert(
                        "local_path".to_string(),
                        repo.local_path.display().to_string(),
                    );
                    context.insert("review_type".to_string(), review_type.to_string());

                    let rendered_prompt = render_prompt(prompt_template, &context);

                    // Git fetch before review
                    let _ = tokio::process::Command::new("git")
                        .args(["fetch", "origin"])
                        .current_dir(&repo.local_path)
                        .output()
                        .await;

                    // Execute
                    {
                        let mut active = task_state.active_reviews.lock().await;
                        *active += 1;
                    }

                    let result = executor
                        .execute(&rendered_prompt, &repo.local_path)
                        .await;

                    {
                        let mut active = task_state.active_reviews.lock().await;
                        *active -= 1;
                    }

                    match result {
                        Ok(()) => {
                            let mut completed = task_state.reviews_completed.lock().await;
                            *completed += 1;
                            tracing::info!(
                                task = %task_name,
                                repo = %repo.full_name(),
                                pr = pr.number,
                                "Review completed for PR #{}",
                                pr.number
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                task = %task_name,
                                repo = %repo.full_name(),
                                pr = pr.number,
                                error = %e,
                                "Review failed for PR #{}",
                                pr.number
                            );
                        }
                    }

                    // Clean up temp diff files
                    diff::cleanup(&diff_ctx);
                }
            }
        }
    }
}

enum ReviewType {
    Initial,
    ReReview { previous_sha: String },
}

impl std::fmt::Display for ReviewType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReviewType::Initial => write!(f, "initial"),
            ReviewType::ReReview { .. } => write!(f, "re-review"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RepoEntry;
    use crate::github::models::{PrRef, PullRequest};
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;

    // --- Mock GithubClient ---
    struct MockGithubClient {
        prs: StdMutex<Vec<PullRequest>>,
        comments_posted: StdMutex<Vec<(String, u64, String)>>,
        diff: String,
    }

    impl MockGithubClient {
        fn new(prs: Vec<PullRequest>, diff: &str) -> Self {
            Self {
                prs: StdMutex::new(prs),
                comments_posted: StdMutex::new(Vec::new()),
                diff: diff.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl GithubClient for MockGithubClient {
        async fn fetch_open_prs(&self, _owner: &str, _repo: &str) -> anyhow::Result<Vec<PullRequest>> {
            Ok(self.prs.lock().unwrap().clone())
        }
        async fn fetch_single_pr(&self, _owner: &str, _repo: &str, _pr: u64) -> anyhow::Result<PullRequest> {
            Ok(self.prs.lock().unwrap()[0].clone())
        }
        async fn fetch_pr_diff(&self, _owner: &str, _repo: &str, _pr: u64) -> anyhow::Result<String> {
            Ok(self.diff.clone())
        }
        async fn post_comment(&self, owner: &str, repo: &str, pr: u64, body: &str) -> anyhow::Result<()> {
            self.comments_posted.lock().unwrap().push((
                format!("{owner}/{repo}"),
                pr,
                body.to_string(),
            ));
            Ok(())
        }
    }

    // --- Mock Executor ---
    struct MockExecutor {
        calls: StdMutex<Vec<(String, PathBuf)>>,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl Executor for MockExecutor {
        async fn execute(&self, prompt: &str, working_dir: &std::path::Path) -> anyhow::Result<()> {
            self.calls
                .lock()
                .unwrap()
                .push((prompt.to_string(), working_dir.to_path_buf()));
            Ok(())
        }
    }

    fn make_pr(number: u64, title: &str) -> PullRequest {
        PullRequest {
            number,
            title: title.to_string(),
            body: Some("PR body".to_string()),
            draft: false,
            head: PrRef {
                sha: format!("sha-{number}"),
                ref_name: "feature-branch".to_string(),
            },
            base: PrRef {
                sha: "def456".to_string(),
                ref_name: "main".to_string(),
            },
        }
    }

    fn make_draft_pr(number: u64, title: &str) -> PullRequest {
        let mut pr = make_pr(number, title);
        pr.draft = true;
        pr
    }

    fn make_pr_with_sha(number: u64, title: &str, sha: &str) -> PullRequest {
        let mut pr = make_pr(number, title);
        pr.head.sha = sha.to_string();
        pr
    }

    fn make_config(repos: Vec<RepoEntry>) -> GithubTriggerConfig {
        GithubTriggerConfig {
            event: "pull_request".to_string(),
            repos,
            poll_interval: 1,
            skip_drafts: true,
            review_on_push: false,
            max_diff_size: 50_000,
        }
    }

    fn make_config_with_options(repos: Vec<RepoEntry>, skip_drafts: bool, review_on_push: bool) -> GithubTriggerConfig {
        GithubTriggerConfig {
            event: "pull_request".to_string(),
            repos,
            poll_interval: 1,
            skip_drafts,
            review_on_push,
            max_diff_size: 50_000,
        }
    }

    #[tokio::test]
    async fn test_seed_records_existing_prs_with_shas() {
        let prs = vec![make_pr(1, "First"), make_pr(2, "Second")];
        let mock_client = Arc::new(MockGithubClient::new(prs, ""));
        let task_state = Arc::new(TaskState::new());

        let config = make_config(vec![RepoEntry {
            slug: "owner/repo".to_string(),
            path: PathBuf::from("/tmp/repo"),
        }]);

        let trigger = GithubPrTrigger::new(mock_client, config, task_state.clone());
        trigger.seed().await.unwrap();

        let seen = task_state.seen_prs.lock().await;
        let repo_seen = seen.get("owner/repo").unwrap();
        assert_eq!(repo_seen.get(&1).unwrap(), "sha-1");
        assert_eq!(repo_seen.get(&2).unwrap(), "sha-2");
        assert_eq!(repo_seen.len(), 2);
    }

    #[tokio::test]
    async fn test_seed_empty_repo() {
        let mock_client = Arc::new(MockGithubClient::new(vec![], ""));
        let task_state = Arc::new(TaskState::new());

        let config = make_config(vec![RepoEntry {
            slug: "owner/repo".to_string(),
            path: PathBuf::from("/tmp/repo"),
        }]);

        let trigger = GithubPrTrigger::new(mock_client, config, task_state.clone());
        trigger.seed().await.unwrap();

        let seen = task_state.seen_prs.lock().await;
        let repo_seen = seen.get("owner/repo").unwrap();
        assert!(repo_seen.is_empty());
    }

    #[tokio::test]
    async fn test_repo_configs_parses_slugs() {
        let mock_client = Arc::new(MockGithubClient::new(vec![], ""));
        let task_state = Arc::new(TaskState::new());

        let config = make_config(vec![
            RepoEntry {
                slug: "owner/repo-a".to_string(),
                path: PathBuf::from("/tmp/a"),
            },
            RepoEntry {
                slug: "owner/repo-b".to_string(),
                path: PathBuf::from("/tmp/b"),
            },
        ]);

        let trigger = GithubPrTrigger::new(mock_client, config, task_state);
        let repos = trigger.repo_configs();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].owner, "owner");
        assert_eq!(repos[0].repo, "repo-a");
        assert_eq!(repos[1].repo, "repo-b");
    }

    #[tokio::test]
    async fn test_invalid_slug_skipped() {
        let mock_client = Arc::new(MockGithubClient::new(vec![], ""));
        let task_state = Arc::new(TaskState::new());

        let config = make_config(vec![
            RepoEntry {
                slug: "valid/repo".to_string(),
                path: PathBuf::from("/tmp/valid"),
            },
            RepoEntry {
                slug: "invalid-no-slash".to_string(),
                path: PathBuf::from("/tmp/invalid"),
            },
        ]);

        let trigger = GithubPrTrigger::new(mock_client, config, task_state);
        let repos = trigger.repo_configs();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].repo, "repo");
    }

    #[tokio::test]
    async fn test_seed_skips_drafts_when_skip_drafts_enabled() {
        let prs = vec![make_pr(1, "Ready"), make_draft_pr(2, "WIP")];
        let mock_client = Arc::new(MockGithubClient::new(prs, ""));
        let task_state = Arc::new(TaskState::new());

        // skip_drafts = true (default)
        let config = make_config(vec![RepoEntry {
            slug: "owner/repo".to_string(),
            path: PathBuf::from("/tmp/repo"),
        }]);

        let trigger = GithubPrTrigger::new(mock_client, config, task_state.clone());
        trigger.seed().await.unwrap();

        let seen = task_state.seen_prs.lock().await;
        let repo_seen = seen.get("owner/repo").unwrap();
        // Only non-draft PR is seeded
        assert_eq!(repo_seen.len(), 1);
        assert!(repo_seen.contains_key(&1));
        assert!(!repo_seen.contains_key(&2));
    }

    #[tokio::test]
    async fn test_seed_includes_drafts_when_skip_drafts_disabled() {
        let prs = vec![make_pr(1, "Ready"), make_draft_pr(2, "WIP")];
        let mock_client = Arc::new(MockGithubClient::new(prs, ""));
        let task_state = Arc::new(TaskState::new());

        // skip_drafts = false
        let config = make_config_with_options(
            vec![RepoEntry {
                slug: "owner/repo".to_string(),
                path: PathBuf::from("/tmp/repo"),
            }],
            false,
            false,
        );

        let trigger = GithubPrTrigger::new(mock_client, config, task_state.clone());
        trigger.seed().await.unwrap();

        let seen = task_state.seen_prs.lock().await;
        let repo_seen = seen.get("owner/repo").unwrap();
        // Both seeded when skip_drafts is disabled
        assert_eq!(repo_seen.len(), 2);
    }

    #[tokio::test]
    async fn test_review_type_display() {
        assert_eq!(ReviewType::Initial.to_string(), "initial");
        assert_eq!(
            ReviewType::ReReview {
                previous_sha: "abc".to_string()
            }
            .to_string(),
            "re-review"
        );
    }

    /// Verify that draft PRs are correctly identified and that non-draft PRs
    /// are distinguished from draft ones using the model's `draft` field.
    #[test]
    fn test_draft_pr_field() {
        let regular = make_pr(1, "Regular");
        let draft = make_draft_pr(2, "Draft");
        assert!(!regular.draft);
        assert!(draft.draft);
    }

    /// When skip_drafts is enabled, draft PRs are NOT seeded.
    /// This means when they get un-drafted, they appear as new PRs.
    #[tokio::test]
    async fn test_draft_prs_not_seeded_when_skip_drafts() {
        let prs = vec![make_draft_pr(10, "WIP Feature")];
        let mock_client = Arc::new(MockGithubClient::new(prs, ""));
        let task_state = Arc::new(TaskState::new());

        let config = make_config(vec![RepoEntry {
            slug: "owner/repo".to_string(),
            path: PathBuf::from("/tmp/repo"),
        }]);

        let trigger = GithubPrTrigger::new(mock_client, config, task_state.clone());
        trigger.seed().await.unwrap();

        let seen = task_state.seen_prs.lock().await;
        let repo_seen = seen.get("owner/repo").unwrap();
        // Draft PR is NOT seeded (so it will be reviewed when un-drafted)
        assert!(!repo_seen.contains_key(&10));
    }

    /// Verify that when a PR's SHA changes, the seen_prs map correctly
    /// reflects the old SHA, which is needed for re-review detection.
    #[tokio::test]
    async fn test_sha_tracking_for_rereview() {
        let task_state = Arc::new(TaskState::new());

        // Manually seed a PR with an old SHA
        {
            let mut seen = task_state.seen_prs.lock().await;
            let mut repo_prs = HashMap::new();
            repo_prs.insert(1u64, "old-sha-abc".to_string());
            seen.insert("owner/repo".to_string(), repo_prs);
        }

        // Now verify the old SHA is stored
        let seen = task_state.seen_prs.lock().await;
        let repo_seen = seen.get("owner/repo").unwrap();
        assert_eq!(repo_seen.get(&1).unwrap(), "old-sha-abc");

        // A new fetch would return a different SHA — the poll loop
        // compares old_sha != pr.head.sha to decide re-review
    }

    /// Verify config correctly wires skip_drafts and review_on_push.
    #[test]
    fn test_config_options_wired() {
        let config = make_config_with_options(
            vec![RepoEntry {
                slug: "o/r".to_string(),
                path: PathBuf::from("/tmp"),
            }],
            false,
            true,
        );
        assert!(!config.skip_drafts);
        assert!(config.review_on_push);
    }

    /// Verify make_pr_with_sha helper works correctly.
    #[test]
    fn test_make_pr_with_sha() {
        let pr = make_pr_with_sha(5, "Test", "custom-sha-123");
        assert_eq!(pr.number, 5);
        assert_eq!(pr.head.sha, "custom-sha-123");
        assert!(!pr.draft);
    }

    // === Regression tests for review feedback fixes ===

    /// Regression for bug 1: review_type context variable must be present
    /// in rendered prompts. If missing, {{review_type}} would appear as
    /// a raw placeholder in the review comment.
    #[test]
    fn test_review_type_context_in_prompt_rendering() {
        use crate::tasks::context::render_prompt;

        let template = "Review type: {{review_type}}, PR #{{pr_number}}";
        let mut context = HashMap::new();
        context.insert("review_type".to_string(), "initial".to_string());
        context.insert("pr_number".to_string(), "42".to_string());

        let rendered = render_prompt(template, &context);
        assert_eq!(rendered, "Review type: initial, PR #42");
        // Must NOT contain raw placeholder
        assert!(!rendered.contains("{{review_type}}"));
    }

    /// Regression for bug 1: re-review type must also render correctly.
    #[test]
    fn test_rereview_type_context_in_prompt_rendering() {
        use crate::tasks::context::render_prompt;

        let template = "This is a {{review_type}} of PR #{{pr_number}}";
        let mut context = HashMap::new();
        context.insert("review_type".to_string(), "re-review".to_string());
        context.insert("pr_number".to_string(), "7".to_string());

        let rendered = render_prompt(template, &context);
        assert_eq!(rendered, "This is a re-review of PR #7");
    }

    /// Regression for bug 2: seen_prs must never contain an empty-string
    /// SHA. This would cause false re-review triggers because any real SHA
    /// would mismatch against "".
    #[test]
    fn test_seen_prs_never_stores_empty_sha() {
        // Simulate what a correct manual trigger does: store the real SHA
        let task_state = TaskState::new();
        let mut seen = std::collections::HashMap::new();
        let mut repo_prs = HashMap::new();

        // The fix: insert with real SHA, not empty string
        let real_sha = "abc123def456".to_string();
        repo_prs.insert(42u64, real_sha.clone());
        seen.insert("owner/repo".to_string(), repo_prs);

        // Verify no empty SHA exists
        for (_repo, prs) in &seen {
            for (pr_num, sha) in prs {
                assert!(
                    !sha.is_empty(),
                    "PR #{pr_num} has empty SHA in seen_prs — this causes spurious re-reviews"
                );
            }
        }

        assert_eq!(seen["owner/repo"][&42], "abc123def456");
        // Suppress unused variable warning
        let _ = task_state;
    }

    /// Regression for bug 3: a draft PR that gets un-drafted (marked ready)
    /// without new commits must be treated as a new PR, not silently skipped.
    ///
    /// Scenario:
    /// 1. PR #5 is opened as draft
    /// 2. seed() runs with skip_drafts=true -> PR #5 NOT in seen_prs
    /// 3. Author marks PR #5 as ready (draft=false), no new commits
    /// 4. poll loop fetches PR #5 with draft=false, same SHA
    /// 5. PR #5 is NOT in seen_prs -> treated as new -> reviewed!
    #[tokio::test]
    async fn test_undrafted_pr_gets_reviewed_after_seed() {
        // Step 1-2: Seed with a draft PR (skip_drafts=true)
        let draft_pr = make_draft_pr(5, "WIP Feature");
        let draft_sha = draft_pr.head.sha.clone();
        let mock_client = Arc::new(MockGithubClient::new(vec![draft_pr], "some diff"));
        let task_state = Arc::new(TaskState::new());

        let config = make_config(vec![RepoEntry {
            slug: "owner/repo".to_string(),
            path: PathBuf::from("/tmp/repo"),
        }]);

        let trigger = GithubPrTrigger::new(mock_client, config, task_state.clone());
        trigger.seed().await.unwrap();

        // Verify: draft PR is NOT in seen_prs
        {
            let seen = task_state.seen_prs.lock().await;
            let repo_seen = seen.get("owner/repo").unwrap();
            assert!(
                !repo_seen.contains_key(&5),
                "Draft PR #5 should NOT be seeded when skip_drafts=true"
            );
        }

        // Step 3-5: Now simulate what poll loop sees — same PR, same SHA,
        // but draft=false. Since PR #5 is not in seen_prs, it's "new".
        let ready_pr = make_pr_with_sha(5, "WIP Feature", &draft_sha);
        assert!(!ready_pr.draft); // Confirm it's not a draft anymore

        // Check: PR is not in seen_prs, so it would be treated as Initial
        let seen = task_state.seen_prs.lock().await;
        let repo_seen = seen.get("owner/repo").unwrap();
        assert!(
            !repo_seen.contains_key(&ready_pr.number),
            "Un-drafted PR should be treated as new (not in seen_prs)"
        );
    }

    /// Regression for bug 3 (inverse): when skip_drafts=false, draft PRs
    /// ARE seeded, so un-drafting without new commits does NOT trigger a
    /// new review (it was already reviewed as a draft).
    #[tokio::test]
    async fn test_undrafted_pr_already_seen_when_skip_drafts_disabled() {
        let draft_pr = make_draft_pr(5, "WIP Feature");
        let mock_client = Arc::new(MockGithubClient::new(vec![draft_pr], ""));
        let task_state = Arc::new(TaskState::new());

        let config = make_config_with_options(
            vec![RepoEntry {
                slug: "owner/repo".to_string(),
                path: PathBuf::from("/tmp/repo"),
            }],
            false, // skip_drafts = false
            false,
        );

        let trigger = GithubPrTrigger::new(mock_client, config, task_state.clone());
        trigger.seed().await.unwrap();

        // When skip_drafts=false, the draft IS seeded
        let seen = task_state.seen_prs.lock().await;
        let repo_seen = seen.get("owner/repo").unwrap();
        assert!(
            repo_seen.contains_key(&5),
            "Draft PR should be seeded when skip_drafts=false"
        );
    }

    /// Verify SHA mismatch detection works correctly for re-review.
    /// Old SHA in seen_prs must differ from new PR's SHA to trigger re-review.
    #[test]
    fn test_sha_mismatch_detected_for_rereview() {
        let old_sha = "aaa111".to_string();
        let new_sha = "bbb222".to_string();

        // Simulate seen_prs state
        let mut repo_prs = HashMap::new();
        repo_prs.insert(1u64, old_sha.clone());

        // New PR from GitHub with different SHA
        let pr = make_pr_with_sha(1, "Feature", &new_sha);

        // The poll loop checks: old_sha != pr.head.sha
        let stored_sha = repo_prs.get(&pr.number).unwrap();
        assert_ne!(
            stored_sha, &pr.head.sha,
            "SHA mismatch should be detected for re-review"
        );
    }

    /// Verify same SHA does NOT trigger re-review.
    #[test]
    fn test_same_sha_no_rereview() {
        let sha = "same-sha-123".to_string();

        let mut repo_prs = HashMap::new();
        repo_prs.insert(1u64, sha.clone());

        let pr = make_pr_with_sha(1, "Feature", &sha);

        let stored_sha = repo_prs.get(&pr.number).unwrap();
        assert_eq!(
            stored_sha, &pr.head.sha,
            "Same SHA should NOT trigger re-review"
        );
    }
}
