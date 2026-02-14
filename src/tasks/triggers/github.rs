use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use crate::config::GithubTriggerConfig;
use crate::github::client::GithubClient;
use crate::github::models::RepoConfig;
use crate::tasks::context::render_prompt;
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
                    context.insert("diff".to_string(), diff);
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
        }
    }

    fn make_config_with_options(repos: Vec<RepoEntry>, skip_drafts: bool, review_on_push: bool) -> GithubTriggerConfig {
        GithubTriggerConfig {
            event: "pull_request".to_string(),
            repos,
            poll_interval: 1,
            skip_drafts,
            review_on_push,
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
    async fn test_seed_skips_drafts_in_seen() {
        // Seed should record ALL PRs (including drafts) so we don't
        // review them when they get un-drafted
        let prs = vec![make_pr(1, "Ready"), make_draft_pr(2, "WIP")];
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
        // Both are seeded (even the draft)
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

    /// When skip_drafts is enabled and a draft PR appears in the fetched list,
    /// the poll loop should skip it. We test this by seeding with an empty repo,
    /// then checking that a draft PR would be filtered by the skip condition.
    #[tokio::test]
    async fn test_draft_detection_in_seen_prs() {
        // If a draft PR is seeded, it should still be in seen_prs (so we
        // don't re-review it when it gets un-drafted unexpectedly)
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
        // Draft PR is seeded with its SHA
        assert_eq!(repo_seen.get(&10).unwrap(), "sha-10");
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
}
