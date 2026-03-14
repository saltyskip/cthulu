use axum::extract::{Path, State};
use axum::Json;
use base64::Engine;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;

use crate::api::AppState;

const REPO_NAME: &str = "cthulu-workflows";

/// Helper: get the local clone path (~/.cthulu/cthulu-workflows).
fn clone_dir(state: &AppState) -> PathBuf {
    state.data_dir.join(REPO_NAME)
}

/// Helper: require the PAT or return 401.
async fn require_pat(state: &AppState) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    state
        .github_pat
        .read()
        .await
        .clone()
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "GitHub PAT not configured"})),
            )
        })
}

/// Read the repo owner from secrets.json (saved during setup).
fn read_owner(state: &AppState) -> Option<String> {
    let content = std::fs::read_to_string(&state.secrets_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v["workspace_repo"]["owner"].as_str().map(String::from)
}

// ---------------------------------------------------------------------------
// GitHub REST API helpers (replace git CLI usage)
// ---------------------------------------------------------------------------

/// Fetch file/directory contents from GitHub via the Contents API.
/// Returns the parsed JSON response.
async fn github_contents(
    client: &reqwest::Client,
    pat: &str,
    owner: &str,
    path: &str,
) -> Result<serde_json::Value, (StatusCode, Json<serde_json::Value>)> {
    let url = if path.is_empty() {
        format!(
            "https://api.github.com/repos/{}/{}/contents",
            owner, REPO_NAME
        )
    } else {
        format!(
            "https://api.github.com/repos/{}/{}/contents/{}",
            owner, REPO_NAME, path
        )
    };

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, url = %url, "GitHub Contents API request failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("GitHub API error: {e}")})),
            )
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!(status = %status, path = %path, body = %body, "GitHub Contents API returned error");
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("GitHub Contents API error ({}): {}", status, body)})),
        ));
    }

    resp.json::<serde_json::Value>().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Failed to parse GitHub Contents response: {e}")})),
        )
    })
}

/// Create or update a single file on GitHub via the Contents API.
/// Automatically fetches the current SHA if the file already exists (needed for updates).
async fn github_put_file(
    client: &reqwest::Client,
    pat: &str,
    owner: &str,
    path: &str,
    content: &[u8],
    message: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}",
        owner, REPO_NAME, path
    );

    // Check if file already exists to get its SHA (required for updates)
    let existing_sha = {
        let check = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", pat))
            .header("User-Agent", "cthulu-studio")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await;

        match check {
            Ok(resp) if resp.status().is_success() => {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                body["sha"].as_str().map(String::from)
            }
            _ => None,
        }
    };

    // Base64-encode the content
    let encoded = base64::engine::general_purpose::STANDARD.encode(content);

    let mut body = json!({
        "message": message,
        "content": encoded,
    });

    if let Some(sha) = existing_sha {
        body["sha"] = json!(sha);
    }

    let resp = client
        .put(&url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, path = %path, "GitHub PUT file request failed");
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("GitHub API error: {e}")})),
            )
        })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!(status = %status, path = %path, body = %body, "GitHub PUT file returned error");
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("GitHub PUT file error ({}): {}", status, body)})),
        ));
    }

    Ok(())
}

/// Delete a file from GitHub via the Contents API.
async fn github_delete_file(
    client: &reqwest::Client,
    pat: &str,
    owner: &str,
    path: &str,
    message: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}",
        owner, REPO_NAME, path
    );

    // First GET the file to obtain its SHA (required for deletion)
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("GitHub API error: {e}")})),
            )
        })?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Failed to get file for deletion: {}", body)})),
        ));
    }

    let file_info: serde_json::Value = resp.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Failed to parse file info: {e}")})),
        )
    })?;

    let sha = file_info["sha"].as_str().ok_or_else(|| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "GitHub file response missing sha field"})),
        )
    })?;

    // DELETE the file
    let del_resp = client
        .delete(&url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .json(&json!({
            "message": message,
            "sha": sha,
        }))
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("GitHub DELETE error: {e}")})),
            )
        })?;

    let del_status = del_resp.status();
    if !del_status.is_success() {
        let body = del_resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("GitHub DELETE file error ({}): {}", del_status, body)})),
        ));
    }

    Ok(())
}

/// Recursively sync the GitHub repo contents to the local directory.
/// Downloads all files via the Contents API and mirrors the directory structure.
async fn sync_repo_to_local(
    client: &reqwest::Client,
    pat: &str,
    owner: &str,
    local_root: &std::path::Path,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    sync_repo_path(client, pat, owner, local_root, "").await
}

/// Recursive helper for sync_repo_to_local.
async fn sync_repo_path(
    client: &reqwest::Client,
    pat: &str,
    owner: &str,
    local_root: &std::path::Path,
    repo_path: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    let contents = github_contents(client, pat, owner, repo_path).await?;

    let entries = match contents.as_array() {
        Some(arr) => arr.clone(),
        None => {
            // Single file response (shouldn't happen for root, but handle gracefully)
            vec![contents]
        }
    };

    for entry in &entries {
        let name = match entry["name"].as_str() {
            Some(n) => n,
            None => continue,
        };

        // Skip hidden entries (like .git)
        if name.starts_with('.') {
            continue;
        }

        let entry_type = entry["type"].as_str().unwrap_or("");
        let entry_path = entry["path"].as_str().unwrap_or(name);

        match entry_type {
            "dir" => {
                let local_dir = local_root.join(entry_path);
                let _ = std::fs::create_dir_all(&local_dir);
                // Recurse into subdirectory
                Box::pin(sync_repo_path(client, pat, owner, local_root, entry_path)).await?;
            }
            "file" => {
                if let Some(download_url) = entry["download_url"].as_str() {
                    let local_file = local_root.join(entry_path);

                    // Ensure parent directory exists
                    if let Some(parent) = local_file.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }

                    // Download raw file content
                    let file_resp = client
                        .get(download_url)
                        .header("Authorization", format!("Bearer {}", pat))
                        .header("User-Agent", "cthulu-studio")
                        .send()
                        .await
                        .map_err(|e| {
                            (
                                StatusCode::BAD_GATEWAY,
                                Json(json!({"error": format!("Failed to download {}: {}", entry_path, e)})),
                            )
                        })?;

                    if file_resp.status().is_success() {
                        let bytes = file_resp.bytes().await.map_err(|e| {
                            (
                                StatusCode::BAD_GATEWAY,
                                Json(json!({"error": format!("Failed to read bytes for {}: {}", entry_path, e)})),
                            )
                        })?;

                        std::fs::write(&local_file, &bytes).map_err(|e| {
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(json!({"error": format!("Failed to write {}: {}", entry_path, e)})),
                            )
                        })?;
                    } else {
                        tracing::warn!(
                            path = %entry_path,
                            status = %file_resp.status(),
                            "failed to download file from GitHub"
                        );
                    }
                }
            }
            _ => {
                tracing::debug!(entry_type = %entry_type, name = %name, "skipping unknown entry type");
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// POST /api/workflows/setup
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct SetupResponse {
    repo_url: String,
    created: bool,
    username: String,
}

pub async fn setup_repo(
    State(state): State<AppState>,
) -> Result<Json<SetupResponse>, (StatusCode, Json<serde_json::Value>)> {
    let pat = require_pat(&state).await?;

    // 1. Get authenticated user
    let user_resp = state
        .http_client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("GitHub API error: {e}")})),
            )
        })?;

    let user: serde_json::Value = user_resp.json().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": format!("Failed to parse user response: {e}")})),
        )
    })?;

    let username = user["login"]
        .as_str()
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "GitHub user response missing login field"})),
            )
        })?
        .to_string();

    // 2. Check if repo exists
    let repo_check = state
        .http_client
        .get(format!(
            "https://api.github.com/repos/{}/{}",
            username, REPO_NAME
        ))
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("Failed to check repo: {e}")})),
            )
        })?;

    let repo_status = repo_check.status();
    let created = if repo_status == 404 {
        // Repo doesn't exist (or token can't see it) — try to create
        let create_resp = state
            .http_client
            .post("https://api.github.com/user/repos")
            .header("Authorization", format!("Bearer {}", pat))
            .header("User-Agent", "cthulu-studio")
            .header("Accept", "application/vnd.github+json")
            .json(&json!({
                "name": REPO_NAME,
                "private": true,
                "description": "Cthulu Studio workflow definitions",
                "auto_init": true,
            }))
            .send()
            .await
            .map_err(|e| {
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({"error": format!("Failed to create repo: {e}")})),
                )
            })?;

        let create_status = create_resp.status();
        if create_status == 422 {
            // 422 = "name already exists" — the token couldn't see the repo
            // but it does exist. Treat as already-exists.
            tracing::info!(username = %username, "repo {REPO_NAME} already exists (422 on create)");
            false
        } else if !create_status.is_success() {
            let body = create_resp.text().await.unwrap_or_default();
            return Err((
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": format!("Failed to create repo: {body}")})),
            ));
        } else {
            tracing::info!(username = %username, "created {REPO_NAME} repo on GitHub");
            true
        }
    } else {
        false
    };

    let repo_url = format!("https://github.com/{}/{}", username, REPO_NAME);

    // 3. Sync repo contents to local directory via API
    let clone_path = clone_dir(&state);
    let _ = std::fs::create_dir_all(&clone_path);
    sync_repo_to_local(&state.http_client, &pat, &username, &clone_path).await?;
    tracing::info!("synced {REPO_NAME} to {}", clone_path.display());

    // 4. Save repo config to secrets.json
    {
        let secrets_path = &state.secrets_path;
        let mut secrets: serde_json::Value = if secrets_path.exists() {
            let content = std::fs::read_to_string(secrets_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };

        secrets["workspace_repo"] = json!({
            "owner": username,
            "name": REPO_NAME,
        });

        let tmp_path = secrets_path.with_extension("json.tmp");
        let json_str = serde_json::to_string_pretty(&secrets).unwrap_or_default();
        let _ = std::fs::write(&tmp_path, &json_str);
        let _ = std::fs::rename(&tmp_path, secrets_path);
    }

    Ok(Json(SetupResponse {
        repo_url,
        created,
        username,
    }))
}

// ---------------------------------------------------------------------------
// GET /api/workflows/workspaces
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct WorkspacesResponse {
    workspaces: Vec<String>,
}

pub async fn list_workspaces(
    State(state): State<AppState>,
) -> Result<Json<WorkspacesResponse>, (StatusCode, Json<serde_json::Value>)> {
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Ok(Json(WorkspacesResponse {
            workspaces: vec![],
        }));
    }

    let mut workspaces = Vec::new();
    let entries = std::fs::read_dir(&clone_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to read clone directory: {e}")})),
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            // Skip hidden directories (.git, etc.)
            if !name.starts_with('.') {
                workspaces.push(name);
            }
        }
    }

    workspaces.sort();

    Ok(Json(WorkspacesResponse { workspaces }))
}

// ---------------------------------------------------------------------------
// POST /api/workflows/workspaces
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    name: String,
}

#[derive(Serialize)]
pub struct CreateWorkspaceResponse {
    ok: bool,
    name: String,
}

pub async fn create_workspace(
    State(state): State<AppState>,
    Json(body): Json<CreateWorkspaceRequest>,
) -> Result<Json<CreateWorkspaceResponse>, (StatusCode, Json<serde_json::Value>)> {
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).unwrap_or_default();
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Workflows repo not set up. Call POST /workflows/setup first."})),
        ));
    }

    let name = body.name.trim().to_string();
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Invalid workspace name"})),
        ));
    }

    let workspace_dir = clone_path.join(&name);
    if workspace_dir.exists() {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({"error": format!("Workspace '{}' already exists", name)})),
        ));
    }

    std::fs::create_dir_all(&workspace_dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to create workspace directory: {e}")})),
        )
    })?;

    // Add a .gitkeep so the empty directory is tracked
    let gitkeep = workspace_dir.join(".gitkeep");
    let _ = std::fs::write(&gitkeep, "");

    // Push .gitkeep to GitHub via API
    github_put_file(
        &state.http_client,
        &pat,
        &owner,
        &format!("{name}/.gitkeep"),
        b"",
        &format!("Create workspace: {name}"),
    )
    .await?;

    tracing::info!(workspace = %name, "created workspace and pushed");

    Ok(Json(CreateWorkspaceResponse { ok: true, name }))
}

// ---------------------------------------------------------------------------
// GET /api/workflows/workspaces/{workspace}
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct WorkflowSummary {
    name: String,
    workspace: String,
    description: Option<String>,
    node_count: usize,
}

#[derive(Serialize)]
pub struct WorkspaceWorkflowsResponse {
    workspace: String,
    workflows: Vec<WorkflowSummary>,
}

pub async fn list_workspace_workflows(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
) -> Result<Json<WorkspaceWorkflowsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let clone_path = clone_dir(&state);
    let workspace_dir = clone_path.join(&workspace);

    if !workspace_dir.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("Workspace '{}' not found", workspace)})),
        ));
    }

    let mut workflows = Vec::new();
    let entries = std::fs::read_dir(&workspace_dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to read workspace: {e}")})),
        )
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }

            let yaml_path = path.join("workflow.yaml");
            if yaml_path.exists() {
                let (description, node_count) = parse_workflow_meta(&yaml_path);
                workflows.push(WorkflowSummary {
                    name,
                    workspace: workspace.clone(),
                    description,
                    node_count,
                });
            }
        }
    }

    workflows.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(WorkspaceWorkflowsResponse {
        workspace,
        workflows,
    }))
}

/// Parse basic metadata from a workflow.yaml without full deserialization.
fn parse_workflow_meta(path: &PathBuf) -> (Option<String>, usize) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (None, 0),
    };

    let yaml: serde_json::Value = match serde_yaml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return (None, 0),
    };

    let description = yaml["description"].as_str().map(String::from);

    let node_count = yaml["nodes"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    (description, node_count)
}

// ---------------------------------------------------------------------------
// GET /api/workflows/workspaces/{workspace}/{name}
// ---------------------------------------------------------------------------

pub async fn get_workflow(
    State(state): State<AppState>,
    Path((workspace, name)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let yaml_path = clone_dir(&state)
        .join(&workspace)
        .join(&name)
        .join("workflow.yaml");

    if !yaml_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("Workflow '{}/{}' not found", workspace, name)})),
        ));
    }

    let content = std::fs::read_to_string(&yaml_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to read workflow: {e}")})),
        )
    })?;

    let yaml_value: serde_json::Value = serde_yaml::from_str(&content).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to parse workflow YAML: {e}")})),
        )
    })?;

    // Normalize to a consistent Flow-like format with nodes[] and edges[].
    // Handles both "flow format" (has nodes array) and "template format"
    // (has trigger/sources/executors/sinks top-level keys).
    let normalized = normalize_workflow_yaml(&yaml_value, &name);

    Ok(Json(normalized))
}

/// Convert a workflow YAML value to a normalized Flow-like JSON object.
/// Supports two formats:
/// 1. Flow format: already has `nodes` array → normalize missing id/position
/// 2. Template format: has trigger/sources/executors/sinks → convert to nodes/edges
fn normalize_workflow_yaml(v: &serde_json::Value, fallback_name: &str) -> serde_json::Value {
    let flow_name = v["name"].as_str().unwrap_or(fallback_name);
    let description = v["description"].as_str().unwrap_or("");

    // Check if this is already in flow format (has nodes array)
    if let Some(nodes_arr) = v["nodes"].as_array() {
        let nodes: Vec<serde_json::Value> = nodes_arr
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let has_pos = n["position"]["x"].is_number();
                json!({
                    "id": n["id"].as_str().unwrap_or(&format!("node-{i}")),
                    "node_type": n["node_type"].as_str().unwrap_or("executor"),
                    "kind": n["kind"].as_str().unwrap_or("unknown"),
                    "config": if n["config"].is_object() { n["config"].clone() } else { json!({}) },
                    "position": if has_pos { n["position"].clone() } else { json!({"x": 300.0 * i as f64, "y": 100.0}) },
                    "label": n["label"].as_str().unwrap_or(n["kind"].as_str().unwrap_or(&format!("Node {}", i + 1))),
                })
            })
            .collect();

        let edges = if let Some(edges_arr) = v["edges"].as_array() {
            edges_arr
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    json!({
                        "id": e["id"].as_str().unwrap_or(&format!("edge-{i}")),
                        "source": e["source"],
                        "target": e["target"],
                    })
                })
                .collect::<Vec<_>>()
        } else {
            auto_wire_edges(&nodes)
        };

        return json!({
            "name": flow_name,
            "description": description,
            "nodes": nodes,
            "edges": edges,
        });
    }

    // Template format: convert trigger/sources/filters/executors/sinks to nodes
    let mut nodes: Vec<serde_json::Value> = Vec::new();
    let mut idx = 0usize;

    // Trigger (single object)
    if let Some(t) = v.get("trigger") {
        if t.is_object() {
            let kind = t["kind"].as_str().unwrap_or("manual");
            nodes.push(json!({
                "id": format!("node-{idx}"),
                "node_type": "trigger",
                "kind": kind,
                "config": if t["config"].is_object() { t["config"].clone() } else { json!({}) },
                "position": {"x": 300.0 * idx as f64, "y": 100.0},
                "label": t["label"].as_str().unwrap_or(&format!("Trigger: {kind}")),
            }));
            idx += 1;
        }
    }

    // Sources
    if let Some(arr) = v["sources"].as_array() {
        for s in arr {
            let kind = s["kind"].as_str().unwrap_or("unknown");
            nodes.push(json!({
                "id": format!("node-{idx}"),
                "node_type": "source",
                "kind": kind,
                "config": if s["config"].is_object() { s["config"].clone() } else { json!({}) },
                "position": {"x": 300.0 * idx as f64, "y": 100.0},
                "label": s["label"].as_str().unwrap_or(&format!("Source: {kind}")),
            }));
            idx += 1;
        }
    }

    // Filters (mapped to source node type for display)
    if let Some(arr) = v["filters"].as_array() {
        for f in arr {
            let kind = f["kind"].as_str().unwrap_or("keyword");
            nodes.push(json!({
                "id": format!("node-{idx}"),
                "node_type": "source",
                "kind": kind,
                "config": if f["config"].is_object() { f["config"].clone() } else { json!({}) },
                "position": {"x": 300.0 * idx as f64, "y": 100.0},
                "label": f["label"].as_str().unwrap_or(&format!("Filter: {kind}")),
            }));
            idx += 1;
        }
    }

    // Executors
    if let Some(arr) = v["executors"].as_array() {
        for e in arr {
            let kind = e["kind"].as_str().unwrap_or("claude-code");
            nodes.push(json!({
                "id": format!("node-{idx}"),
                "node_type": "executor",
                "kind": kind,
                "config": if e["config"].is_object() { e["config"].clone() } else { json!({}) },
                "position": {"x": 300.0 * idx as f64, "y": 100.0},
                "label": e["label"].as_str().unwrap_or(&format!("Executor: {kind}")),
            }));
            idx += 1;
        }
    }

    // Sinks
    if let Some(arr) = v["sinks"].as_array() {
        for s in arr {
            let kind = s["kind"].as_str().unwrap_or("unknown");
            nodes.push(json!({
                "id": format!("node-{idx}"),
                "node_type": "sink",
                "kind": kind,
                "config": if s["config"].is_object() { s["config"].clone() } else { json!({}) },
                "position": {"x": 300.0 * idx as f64, "y": 100.0},
                "label": s["label"].as_str().unwrap_or(&format!("Sink: {kind}")),
            }));
            idx += 1;
        }
    }

    let edges = auto_wire_edges(&nodes);

    json!({
        "name": flow_name,
        "description": description,
        "nodes": nodes,
        "edges": edges,
    })
}

/// Auto-wire edges by connecting nodes sequentially in pipeline order.
fn auto_wire_edges(nodes: &[serde_json::Value]) -> Vec<serde_json::Value> {
    if nodes.len() < 2 {
        return vec![];
    }

    // Sort by pipeline order: trigger(0) → source(1) → executor(2) → sink(3)
    let mut sorted: Vec<(usize, &serde_json::Value)> = nodes.iter().enumerate().collect();
    sorted.sort_by_key(|(_, n)| {
        match n["node_type"].as_str().unwrap_or("") {
            "trigger" => 0,
            "source" => 1,
            "executor" => 2,
            "sink" => 3,
            _ => 9,
        }
    });

    sorted
        .windows(2)
        .enumerate()
        .map(|(i, pair)| {
            json!({
                "id": format!("edge-{i}"),
                "source": pair[0].1["id"],
                "target": pair[1].1["id"],
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// POST /api/workflows/workspaces/{workspace}/{name}/save  (local only, no git)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveRequest {
    flow: serde_json::Value,
}

#[derive(Serialize)]
pub struct SaveResponse {
    ok: bool,
}

pub async fn save_workflow(
    State(state): State<AppState>,
    Path((workspace, name)): Path<(String, String)>,
    Json(body): Json<SaveRequest>,
) -> Result<Json<SaveResponse>, (StatusCode, Json<serde_json::Value>)> {
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Workflows repo not set up"})),
        ));
    }

    let workflow_dir = clone_path.join(&workspace).join(&name);
    let _ = std::fs::create_dir_all(&workflow_dir);

    // Convert JSON flow to YAML
    let yaml = serde_yaml::to_string(&body.flow).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to serialize to YAML: {e}")})),
        )
    })?;

    let yaml_path = workflow_dir.join("workflow.yaml");

    // Atomic write
    let tmp_path = yaml_path.with_extension("yaml.tmp");
    std::fs::write(&tmp_path, &yaml).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to write workflow: {e}")})),
        )
    })?;
    std::fs::rename(&tmp_path, &yaml_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to save workflow: {e}")})),
        )
    })?;

    tracing::info!(workspace = %workspace, workflow = %name, "saved workflow locally");

    Ok(Json(SaveResponse { ok: true }))
}

// ---------------------------------------------------------------------------
// POST /api/workflows/workspaces/{workspace}/{name}/publish
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PublishRequest {
    flow: serde_json::Value,
}

#[derive(Serialize)]
pub struct PublishResponse {
    ok: bool,
}

pub async fn publish_workflow(
    State(state): State<AppState>,
    Path((workspace, name)): Path<(String, String)>,
    Json(body): Json<PublishRequest>,
) -> Result<Json<PublishResponse>, (StatusCode, Json<serde_json::Value>)> {
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).unwrap_or_default();
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Workflows repo not set up"})),
        ));
    }

    let workflow_dir = clone_path.join(&workspace).join(&name);
    let _ = std::fs::create_dir_all(&workflow_dir);

    // Convert JSON flow to YAML
    let yaml = serde_yaml::to_string(&body.flow).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to serialize to YAML: {e}")})),
        )
    })?;

    let yaml_path = workflow_dir.join("workflow.yaml");

    // Atomic write
    let tmp_path = yaml_path.with_extension("yaml.tmp");
    std::fs::write(&tmp_path, &yaml).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to write workflow: {e}")})),
        )
    })?;
    std::fs::rename(&tmp_path, &yaml_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to save workflow: {e}")})),
        )
    })?;

    // Push workflow.yaml to GitHub via API
    github_put_file(
        &state.http_client,
        &pat,
        &owner,
        &format!("{workspace}/{name}/workflow.yaml"),
        yaml.as_bytes(),
        &format!("Publish {workspace}/{name}"),
    )
    .await?;

    tracing::info!(workspace = %workspace, workflow = %name, "published workflow");

    Ok(Json(PublishResponse { ok: true }))
}

// ---------------------------------------------------------------------------
// DELETE /api/workflows/workspaces/{workspace}/{name}
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct DeleteWorkflowResponse {
    ok: bool,
}

pub async fn delete_workflow(
    State(state): State<AppState>,
    Path((workspace, name)): Path<(String, String)>,
) -> Result<Json<DeleteWorkflowResponse>, (StatusCode, Json<serde_json::Value>)> {
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).unwrap_or_default();
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Workflows repo not set up"})),
        ));
    }

    let workflow_dir = clone_path.join(&workspace).join(&name);

    if !workflow_dir.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("Workflow {workspace}/{name} not found")})),
        ));
    }

    // Remove the workflow directory from disk
    std::fs::remove_dir_all(&workflow_dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("Failed to delete workflow directory: {e}")})),
        )
    })?;

    // Delete workflow.yaml from GitHub via API
    github_delete_file(
        &state.http_client,
        &pat,
        &owner,
        &format!("{workspace}/{name}/workflow.yaml"),
        &format!("Delete {workspace}/{name}"),
    )
    .await?;

    // Also try to delete .gitkeep if it exists in the workflow dir
    let _ = github_delete_file(
        &state.http_client,
        &pat,
        &owner,
        &format!("{workspace}/{name}/.gitkeep"),
        &format!("Clean up {workspace}/{name}"),
    )
    .await;

    tracing::info!(workspace = %workspace, workflow = %name, "deleted workflow");

    Ok(Json(DeleteWorkflowResponse { ok: true }))
}

// ---------------------------------------------------------------------------
// POST /api/workflows/sync
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct SyncResponse {
    ok: bool,
    workspaces: Vec<String>,
}

pub async fn sync_workflows(
    State(state): State<AppState>,
) -> Result<Json<SyncResponse>, (StatusCode, Json<serde_json::Value>)> {
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).unwrap_or_default();
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "Workflows repo not set up. Call POST /workflows/setup first."})),
        ));
    }

    // Sync from GitHub via API
    sync_repo_to_local(&state.http_client, &pat, &owner, &clone_path).await?;

    // Re-list workspaces
    let mut workspaces = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&clone_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') {
                    workspaces.push(name);
                }
            }
        }
    }
    workspaces.sort();

    tracing::info!(workspaces = ?workspaces, "synced workflows repo");

    Ok(Json(SyncResponse {
        ok: true,
        workspaces,
    }))
}
