use std::path::PathBuf;
use std::sync::Arc;

use crate::flows::repository::FlowRepository;
use crate::flows::Flow;
use crate::templates;

pub struct TemplateRepository {
    flow_repo: Arc<dyn FlowRepository>,
    static_dir: PathBuf,
}

impl TemplateRepository {
    pub fn new(flow_repo: Arc<dyn FlowRepository>, static_dir: PathBuf) -> Self {
        Self { flow_repo, static_dir }
    }

    pub fn list_templates(&self) -> Vec<templates::TemplateMetadata> {
        templates::load_templates(&self.static_dir)
    }

    pub fn get_template_yaml(&self, category: &str, slug: &str) -> Result<String, std::io::Error> {
        let file_path = self
            .static_dir
            .join("workflows")
            .join(category)
            .join(format!("{slug}.yaml"));
        std::fs::read_to_string(file_path)
    }

    pub async fn save_imported_flow(&self, flow: Flow) -> anyhow::Result<()> {
        self.flow_repo.save_flow(flow).await
    }
}

/// Fetch all `.yaml` / `.yml` files from a GitHub repo path using the Contents API.
/// Recurses into subdirectories up to 2 levels deep.
/// Returns `Vec<(filename, yaml_content)>`.
pub async fn fetch_github_yaml_files(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    path: &str,
    branch: &str,
) -> Result<Vec<(String, String)>, String> {
    let api_url = if path.is_empty() {
        format!("https://api.github.com/repos/{owner}/{repo}/contents?ref={branch}")
    } else {
        format!("https://api.github.com/repos/{owner}/{repo}/contents/{path}?ref={branch}")
    };

    let resp = client
        .get(&api_url)
        .header("User-Agent", "cthulu-studio/1.0")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub API returned {status}: {body}"));
    }

    let entries: Vec<serde_json::Value> = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse GitHub API response: {e}"))?;

    let mut yaml_files: Vec<(String, String)> = Vec::new();

    for entry in &entries {
        let entry_type = entry["type"].as_str().unwrap_or("");
        let entry_name = entry["name"].as_str().unwrap_or("");
        let entry_path = entry["path"].as_str().unwrap_or("");
        let download_url = entry["download_url"].as_str().unwrap_or("");

        if entry_type == "file"
            && (entry_name.ends_with(".yaml") || entry_name.ends_with(".yml"))
        {
            match client
                .get(download_url)
                .header("User-Agent", "cthulu-studio/1.0")
                .send()
                .await
            {
                Ok(file_resp) if file_resp.status().is_success() => {
                    match file_resp.text().await {
                        Ok(content) => yaml_files.push((entry_name.to_string(), content)),
                        Err(e) => tracing::warn!(file = %entry_name, error = %e, "failed to read file content"),
                    }
                }
                Ok(r) => tracing::warn!(file = %entry_name, status = %r.status(), "non-200 fetching file"),
                Err(e) => tracing::warn!(file = %entry_name, error = %e, "failed to fetch file"),
            }
        } else if entry_type == "dir" {
            // Recurse one level into subdirectories
            match Box::pin(fetch_github_yaml_files(client, owner, repo, entry_path, branch)).await {
                Ok(sub_files) => yaml_files.extend(sub_files),
                Err(e) => tracing::warn!(dir = %entry_path, error = %e, "failed to recurse into directory"),
            }
        }
    }

    Ok(yaml_files)
}
