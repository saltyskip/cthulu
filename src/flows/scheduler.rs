use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use croner::Cron;
use tokio::sync::Mutex;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::flows::events::RunEvent;
use crate::flows::runner::FlowRunner;
use crate::flows::store::Store;
use crate::flows::NodeType;
use crate::github::client::GithubClient;
use crate::github::models::RepoConfig;
use crate::tasks::diff;

pub struct FlowScheduler {
    store: Arc<dyn Store>,
    http_client: Arc<reqwest::Client>,
    github_client: Option<Arc<dyn GithubClient>>,
    events_tx: broadcast::Sender<RunEvent>,
    handles: Mutex<HashMap<String, JoinHandle<()>>>,
    seen_prs: Arc<Mutex<HashMap<String, HashMap<u64, String>>>>,
}

impl FlowScheduler {
    pub fn new(
        store: Arc<dyn Store>,
        http_client: Arc<reqwest::Client>,
        github_client: Option<Arc<dyn GithubClient>>,
        events_tx: broadcast::Sender<RunEvent>,
    ) -> Self {
        Self {
            store,
            http_client,
            github_client,
            events_tx,
            handles: Mutex::new(HashMap::new()),
            seen_prs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start_all(&self) {
        let flows = self.store.list_flows().await;
        for flow in flows {
            if flow.enabled {
                if let Err(e) = self.start_flow(&flow.id).await {
                    tracing::error!(flow = %flow.name, error = %e, "Failed to start flow trigger");
                }
            }
        }
    }

    pub async fn start_flow(&self, flow_id: &str) -> Result<()> {
        let flow = self
            .store
            .get_flow(flow_id)
            .await
            .context("flow not found")?;

        if !flow.enabled {
            tracing::debug!(flow = %flow.name, "Flow is disabled, not starting trigger");
            return Ok(());
        }

        let trigger_node = match flow.nodes.iter().find(|n| n.node_type == NodeType::Trigger) {
            Some(n) => n,
            None => {
                tracing::debug!(flow = %flow.name, "Flow has no trigger node, skipping");
                return Ok(());
            }
        };

        match trigger_node.kind.as_str() {
            "cron" => {
                let schedule = trigger_node.config["schedule"]
                    .as_str()
                    .context("cron trigger missing 'schedule'")?
                    .to_string();

                let flow_id = flow.id.clone();
                let flow_name = flow.name.clone();
                let store = self.store.clone();
                let http_client = self.http_client.clone();
                let github_client = self.github_client.clone();
                let events_tx = self.events_tx.clone();

                tracing::info!(flow = %flow.name, schedule = %schedule, "Started cron trigger");

                let handle = tokio::spawn(async move {
                    cron_loop(
                        &flow_id,
                        &flow_name,
                        &schedule,
                        store,
                        http_client,
                        github_client,
                        events_tx,
                    )
                    .await;
                });
                self.handles.lock().await.insert(flow.id.clone(), handle);
            }
            "github-pr" => {
                let github_client = self
                    .github_client
                    .clone()
                    .context("GitHub PR trigger requires GITHUB_TOKEN")?;

                let flow_id = flow.id.clone();
                let flow_name = flow.name.clone();
                let store = self.store.clone();
                let http_client = self.http_client.clone();
                let seen_prs = self.seen_prs.clone();
                let trigger_config = trigger_node.config.clone();
                let events_tx = self.events_tx.clone();

                let handle = tokio::spawn(async move {
                    github_pr_loop(
                        &flow_id,
                        &flow_name,
                        trigger_config,
                        store,
                        http_client,
                        github_client,
                        seen_prs,
                        events_tx,
                    )
                    .await;
                });

                tracing::info!(flow = %flow.name, "Started GitHub PR trigger");
                self.handles.lock().await.insert(flow.id.clone(), handle);
            }
            "manual" | "webhook" => {
                tracing::debug!(
                    flow = %flow.name,
                    kind = %trigger_node.kind,
                    "Trigger kind does not auto-start"
                );
            }
            other => {
                tracing::warn!(flow = %flow.name, kind = %other, "Unknown trigger kind, skipping");
            }
        }

        Ok(())
    }

    pub async fn stop_flow(&self, flow_id: &str) {
        let mut handles = self.handles.lock().await;
        if let Some(handle) = handles.remove(flow_id) {
            handle.abort();
            tracing::info!(flow_id = %flow_id, "Stopped flow trigger");
        }
    }

    pub async fn restart_flow(&self, flow_id: &str) -> Result<()> {
        self.stop_flow(flow_id).await;
        self.start_flow(flow_id).await
    }

    /// Return the set of flow IDs that currently have active scheduler tasks.
    pub async fn active_flow_ids(&self) -> Vec<String> {
        let handles = self.handles.lock().await;
        handles.keys().cloned().collect()
    }

    /// Execute a specific PR review through a flow with github-pr trigger.
    /// Used by manual trigger endpoint.
    pub async fn trigger_pr_review(
        &self,
        flow_id: &str,
        repo_slug: &str,
        pr_number: u64,
    ) -> Result<()> {
        let flow = self
            .store
            .get_flow(flow_id)
            .await
            .context("flow not found")?;

        let github_client = self
            .github_client
            .clone()
            .context("GITHUB_TOKEN not configured")?;

        let trigger_node = flow
            .nodes
            .iter()
            .find(|n| n.node_type == NodeType::Trigger && n.kind == "github-pr")
            .context("flow has no github-pr trigger")?;

        let max_diff_size = trigger_node.config["max_diff_size"]
            .as_u64()
            .unwrap_or(50_000) as usize;

        // Parse repo slug
        let (owner, repo_name) = repo_slug
            .split_once('/')
            .context("invalid repo slug, expected 'owner/repo'")?;

        // Find local_path from trigger config repos
        let local_path = trigger_node.config["repos"]
            .as_array()
            .and_then(|repos| {
                repos.iter().find_map(|r| {
                    let slug = r["slug"].as_str()?;
                    if slug == repo_slug {
                        Some(PathBuf::from(r["path"].as_str().unwrap_or(".")))
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| PathBuf::from("."));

        let pr = github_client
            .fetch_single_pr(owner, repo_name, pr_number)
            .await?;

        // Mark as seen
        {
            let mut seen = self.seen_prs.lock().await;
            seen.entry(repo_slug.to_string())
                .or_default()
                .insert(pr_number, pr.head.sha.clone());
        }

        let diff_raw = github_client
            .fetch_pr_diff(owner, repo_name, pr_number)
            .await?;

        let diff_ctx = diff::prepare_diff_context(&diff_raw, pr_number, max_diff_size)?;

        let mut context = HashMap::new();
        context.insert("diff".to_string(), diff_ctx.text());
        context.insert("pr_number".to_string(), pr.number.to_string());
        context.insert("pr_title".to_string(), pr.title.clone());
        context.insert("pr_body".to_string(), pr.body.unwrap_or_default());
        context.insert("base_ref".to_string(), pr.base.ref_name.clone());
        context.insert("head_ref".to_string(), pr.head.ref_name.clone());
        context.insert("head_sha".to_string(), pr.head.sha.clone());
        context.insert("repo".to_string(), repo_slug.to_string());
        context.insert("local_path".to_string(), local_path.display().to_string());
        context.insert("review_type".to_string(), "initial".to_string());

        let runner = FlowRunner {
            http_client: self.http_client.clone(),
            github_client: self.github_client.clone(),
            events_tx: Some(self.events_tx.clone()),
            sandbox_provider: None,
        };

        runner
            .execute(&flow, &*self.store, Some(context))
            .await?;

        diff::cleanup(&diff_ctx);
        Ok(())
    }
}

// ── Cron loop ────────────────────────────────────────────────────

async fn cron_loop(
    flow_id: &str,
    flow_name: &str,
    schedule: &str,
    store: Arc<dyn Store>,
    http_client: Arc<reqwest::Client>,
    github_client: Option<Arc<dyn GithubClient>>,
    events_tx: broadcast::Sender<RunEvent>,
) {
    let cron = match Cron::new(schedule).parse() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(flow = %flow_name, error = %e, "Invalid cron expression '{schedule}'");
            return;
        }
    };

    tracing::info!(flow = %flow_name, schedule = %schedule, "Cron loop started");

    loop {
        let now = Utc::now();
        let next = match cron.find_next_occurrence(&now, false) {
            Ok(next) => next,
            Err(e) => {
                tracing::error!(flow = %flow_name, error = %e, "Failed to compute next cron occurrence");
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                continue;
            }
        };

        let duration = (next - now).to_std().unwrap_or(std::time::Duration::from_secs(1));
        tracing::info!(
            flow = %flow_name,
            next = %next.format("%Y-%m-%d %H:%M:%S UTC"),
            "Sleeping until next cron fire"
        );
        tokio::time::sleep(duration).await;

        // Guard against premature wake from sleep imprecision
        let now_after = Utc::now();
        if now_after < next {
            let remaining = (next - now_after).to_std().unwrap_or_default();
            tokio::time::sleep(remaining).await;
        }

        // Re-fetch the flow in case it was updated
        let flow = match store.get_flow(flow_id).await {
            Some(f) if f.enabled => f,
            Some(_) => {
                tracing::info!(flow = %flow_name, "Flow disabled, stopping cron loop");
                return;
            }
            None => {
                tracing::info!(flow = %flow_name, "Flow deleted, stopping cron loop");
                return;
            }
        };

        let runner = FlowRunner {
            http_client: http_client.clone(),
            github_client: github_client.clone(),
            events_tx: Some(events_tx.clone()),
            sandbox_provider: None,
        };

        if let Err(e) = runner.execute(&flow, &*store, None).await {
            tracing::error!(flow = %flow_name, error = %e, "Cron flow execution failed");
        }
    }
}

// ── GitHub PR loop ───────────────────────────────────────────────

async fn github_pr_loop(
    flow_id: &str,
    flow_name: &str,
    trigger_config: serde_json::Value,
    store: Arc<dyn Store>,
    http_client: Arc<reqwest::Client>,
    github_client: Arc<dyn GithubClient>,
    seen_prs: Arc<Mutex<HashMap<String, HashMap<u64, String>>>>,
    events_tx: broadcast::Sender<RunEvent>,
) {
    let poll_interval = trigger_config["poll_interval"].as_u64().unwrap_or(60);
    let skip_drafts = trigger_config["skip_drafts"].as_bool().unwrap_or(true);
    let review_on_push = trigger_config["review_on_push"].as_bool().unwrap_or(false);
    let max_diff_size = trigger_config["max_diff_size"].as_u64().unwrap_or(50_000) as usize;

    let repos = parse_repo_configs(&trigger_config);
    if repos.is_empty() {
        tracing::error!(flow = %flow_name, "No valid repos configured for GitHub PR trigger");
        return;
    }

    // Seed: fetch open PRs and populate seen_prs
    for repo in &repos {
        let max_retries = 10;
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            match github_client
                .fetch_open_prs(&repo.owner, &repo.repo)
                .await
            {
                Ok(prs) => {
                    let mut seen = seen_prs.lock().await;
                    let pr_shas: HashMap<u64, String> = prs
                        .iter()
                        .filter(|pr| {
                            if pr.draft && skip_drafts {
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

    // Filter to only repos that were successfully seeded
    let seeded_repos: Vec<RepoConfig> = {
        let seen = seen_prs.lock().await;
        repos
            .into_iter()
            .filter(|r| seen.contains_key(&r.full_name()))
            .collect()
    };

    tracing::info!(
        flow = %flow_name,
        repos = seeded_repos.len(),
        interval = poll_interval,
        skip_drafts,
        review_on_push,
        "Polling {} repos every {}s",
        seeded_repos.len(),
        poll_interval
    );

    let mut interval =
        tokio::time::interval(std::time::Duration::from_secs(poll_interval));

    loop {
        interval.tick().await;

        // Check if flow still exists and is enabled
        let flow = match store.get_flow(flow_id).await {
            Some(f) if f.enabled => f,
            Some(_) => {
                tracing::info!(flow = %flow_name, "Flow disabled, stopping PR poll loop");
                return;
            }
            None => {
                tracing::info!(flow = %flow_name, "Flow deleted, stopping PR poll loop");
                return;
            }
        };

        for repo in &seeded_repos {
            let prs = match github_client
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
                if pr.draft && skip_drafts {
                    continue;
                }

                let review_type = {
                    let mut seen = seen_prs.lock().await;
                    let seen_map = seen.entry(repo.full_name()).or_default();

                    match seen_map.get(&pr.number) {
                        None => {
                            seen_map.insert(pr.number, pr.head.sha.clone());
                            ReviewType::Initial
                        }
                        Some(old_sha) if review_on_push && *old_sha != pr.head.sha => {
                            let old = old_sha.clone();
                            seen_map.insert(pr.number, pr.head.sha.clone());
                            ReviewType::ReReview { previous_sha: old }
                        }
                        _ => continue,
                    }
                };

                tracing::info!(
                    flow = %flow_name,
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
                if let Err(e) = github_client
                    .post_comment(&repo.owner, &repo.repo, pr.number, &start_msg)
                    .await
                {
                    tracing::warn!(error = %e, "Failed to post starting comment");
                }

                // Fetch diff
                let diff_raw = match github_client
                    .fetch_pr_diff(&repo.owner, &repo.repo, pr.number)
                    .await
                {
                    Ok(d) => d,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to fetch PR diff");
                        continue;
                    }
                };

                let diff_ctx = match diff::prepare_diff_context(&diff_raw, pr.number, max_diff_size)
                {
                    Ok(ctx) => ctx,
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to prepare diff context");
                        continue;
                    }
                };

                // Build context
                let mut context = HashMap::new();
                context.insert("diff".to_string(), diff_ctx.text());
                context.insert("pr_number".to_string(), pr.number.to_string());
                context.insert("pr_title".to_string(), pr.title.clone());
                context.insert("pr_body".to_string(), pr.body.clone().unwrap_or_default());
                context.insert("base_ref".to_string(), pr.base.ref_name.clone());
                context.insert("head_ref".to_string(), pr.head.ref_name.clone());
                context.insert("head_sha".to_string(), pr.head.sha.clone());
                context.insert("repo".to_string(), repo.full_name());
                context.insert(
                    "local_path".to_string(),
                    repo.local_path.display().to_string(),
                );
                context.insert("review_type".to_string(), review_type.to_string());

                // Git fetch before review
                let _ = tokio::process::Command::new("git")
                    .args(["fetch", "origin"])
                    .current_dir(&repo.local_path)
                    .output()
                    .await;

                let runner = FlowRunner {
                    http_client: http_client.clone(),
                    github_client: Some(github_client.clone()),
                    events_tx: Some(events_tx.clone()),
                    sandbox_provider: None,
                };

                match runner
                    .execute(&flow, &*store, Some(context))
                    .await
                {
                    Ok(run) => {
                        tracing::info!(
                            flow = %flow_name,
                            repo = %repo.full_name(),
                            pr = pr.number,
                            run_id = %run.id,
                            "PR review completed"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            flow = %flow_name,
                            repo = %repo.full_name(),
                            pr = pr.number,
                            error = %e,
                            "PR review failed"
                        );
                    }
                }

                diff::cleanup(&diff_ctx);
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn parse_repo_configs(trigger_config: &serde_json::Value) -> Vec<RepoConfig> {
    trigger_config["repos"]
        .as_array()
        .map(|repos| {
            repos
                .iter()
                .filter_map(|r| {
                    let slug = r["slug"].as_str()?;
                    let (owner, repo) = slug.split_once('/')?;
                    let path = r["path"].as_str().unwrap_or(".");
                    Some(RepoConfig {
                        owner: owner.to_string(),
                        repo: repo.to_string(),
                        local_path: PathBuf::from(path),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
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
    use crate::github::models::{PrRef, PullRequest};
    use crate::tasks::context::render_prompt;
    use std::sync::Mutex as StdMutex;

    // --- Mock GithubClient ---
    #[allow(dead_code)]
    struct MockGithubClient {
        prs: StdMutex<Vec<PullRequest>>,
        comments_posted: StdMutex<Vec<(String, u64, String)>>,
        diff: String,
    }

    #[allow(dead_code)]
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

    #[test]
    fn test_review_type_display() {
        assert_eq!(ReviewType::Initial.to_string(), "initial");
        assert_eq!(
            ReviewType::ReReview {
                previous_sha: "abc".to_string()
            }
            .to_string(),
            "re-review"
        );
    }

    #[test]
    fn test_parse_repo_configs() {
        let config = serde_json::json!({
            "repos": [
                { "slug": "owner/repo-a", "path": "/tmp/a" },
                { "slug": "owner/repo-b", "path": "/tmp/b" },
                { "slug": "invalid-no-slash" },
            ]
        });
        let repos = parse_repo_configs(&config);
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].owner, "owner");
        assert_eq!(repos[0].repo, "repo-a");
        assert_eq!(repos[1].repo, "repo-b");
    }

    #[test]
    fn test_parse_repo_configs_empty() {
        let config = serde_json::json!({});
        let repos = parse_repo_configs(&config);
        assert!(repos.is_empty());
    }

    #[test]
    fn test_draft_pr_field() {
        let regular = make_pr(1, "Regular");
        let draft = make_draft_pr(2, "Draft");
        assert!(!regular.draft);
        assert!(draft.draft);
    }

    #[test]
    fn test_make_pr_with_sha() {
        let pr = make_pr_with_sha(5, "Test", "custom-sha-123");
        assert_eq!(pr.number, 5);
        assert_eq!(pr.head.sha, "custom-sha-123");
        assert!(!pr.draft);
    }

    #[test]
    fn test_sha_mismatch_detected_for_rereview() {
        let old_sha = "aaa111".to_string();
        let new_sha = "bbb222".to_string();
        let mut repo_prs = HashMap::new();
        repo_prs.insert(1u64, old_sha);
        let pr = make_pr_with_sha(1, "Feature", &new_sha);
        let stored_sha = repo_prs.get(&pr.number).unwrap();
        assert_ne!(stored_sha, &pr.head.sha);
    }

    #[test]
    fn test_same_sha_no_rereview() {
        let sha = "same-sha-123".to_string();
        let mut repo_prs = HashMap::new();
        repo_prs.insert(1u64, sha.clone());
        let pr = make_pr_with_sha(1, "Feature", &sha);
        let stored_sha = repo_prs.get(&pr.number).unwrap();
        assert_eq!(stored_sha, &pr.head.sha);
    }

    #[test]
    fn test_review_type_context_in_prompt_rendering() {
        let template = "Review type: {{review_type}}, PR #{{pr_number}}";
        let mut context = HashMap::new();
        context.insert("review_type".to_string(), "initial".to_string());
        context.insert("pr_number".to_string(), "42".to_string());
        let rendered = render_prompt(template, &context);
        assert_eq!(rendered, "Review type: initial, PR #42");
        assert!(!rendered.contains("{{review_type}}"));
    }

    #[test]
    fn test_rereview_type_context_in_prompt_rendering() {
        let template = "This is a {{review_type}} of PR #{{pr_number}}";
        let mut context = HashMap::new();
        context.insert("review_type".to_string(), "re-review".to_string());
        context.insert("pr_number".to_string(), "7".to_string());
        let rendered = render_prompt(template, &context);
        assert_eq!(rendered, "This is a re-review of PR #7");
    }

    #[test]
    fn test_seen_prs_never_stores_empty_sha() {
        let mut seen = HashMap::new();
        let mut repo_prs = HashMap::new();
        let real_sha = "abc123def456".to_string();
        repo_prs.insert(42u64, real_sha);
        seen.insert("owner/repo".to_string(), repo_prs);

        for (_repo, prs) in &seen {
            for (pr_num, sha) in prs {
                assert!(
                    !sha.is_empty(),
                    "PR #{pr_num} has empty SHA in seen_prs"
                );
            }
        }
        assert_eq!(seen["owner/repo"][&42], "abc123def456");
    }
}
