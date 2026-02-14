use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;

use super::models::PullRequest;

const USER_AGENT: &str = "cthulu-bot";
const GITHUB_API: &str = "https://api.github.com";

#[async_trait]
pub trait GithubClient: Send + Sync {
    async fn fetch_open_prs(&self, owner: &str, repo: &str) -> Result<Vec<PullRequest>>;
    async fn fetch_single_pr(&self, owner: &str, repo: &str, pr_number: u64) -> Result<PullRequest>;
    async fn fetch_pr_diff(&self, owner: &str, repo: &str, pr_number: u64) -> Result<String>;
    async fn post_comment(&self, owner: &str, repo: &str, pr_number: u64, body: &str) -> Result<()>;
}

pub struct HttpGithubClient {
    client: Client,
    token: String,
}

impl HttpGithubClient {
    pub fn new(client: Client, token: String) -> Self {
        Self { client, token }
    }
}

#[async_trait]
impl GithubClient for HttpGithubClient {
    async fn fetch_open_prs(&self, owner: &str, repo: &str) -> Result<Vec<PullRequest>> {
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/pulls");
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("state", "open"),
                ("sort", "created"),
                ("direction", "desc"),
            ])
            .bearer_auth(&self.token)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .context("failed to fetch open PRs")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error {status} fetching PRs for {owner}/{repo}: {body}");
        }

        let prs: Vec<PullRequest> = resp.json().await.context("failed to parse PR list")?;
        Ok(prs)
    }

    async fn fetch_single_pr(&self, owner: &str, repo: &str, pr_number: u64) -> Result<PullRequest> {
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/pulls/{pr_number}");
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .context("failed to fetch PR")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error {status} fetching PR #{pr_number}: {body}");
        }

        resp.json().await.context("failed to parse PR")
    }

    async fn fetch_pr_diff(&self, owner: &str, repo: &str, pr_number: u64) -> Result<String> {
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/pulls/{pr_number}");
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/vnd.github.v3.diff")
            .send()
            .await
            .context("failed to fetch PR diff")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error {status} fetching diff for PR #{pr_number}: {body}");
        }

        resp.text().await.context("failed to read diff body")
    }

    async fn post_comment(&self, owner: &str, repo: &str, pr_number: u64, body: &str) -> Result<()> {
        let url = format!("{GITHUB_API}/repos/{owner}/{repo}/issues/{pr_number}/comments");
        let payload = serde_json::json!({ "body": body });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/vnd.github+json")
            .json(&payload)
            .send()
            .await
            .context("failed to post comment")?;

        let status = resp.status();
        if !status.is_success() {
            let resp_body = resp.text().await.unwrap_or_default();
            anyhow::bail!("GitHub API error {status} posting comment on PR #{pr_number}: {resp_body}");
        }

        Ok(())
    }
}
