use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct RepoConfig {
    pub owner: String,
    pub repo: String,
    pub local_path: PathBuf,
}

impl RepoConfig {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    pub head: PrRef,
    pub base: PrRef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrRef {
    pub sha: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
}
