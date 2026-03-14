use serde::Deserialize;
use serde_json::{json, Value};

use cthulu::api::AppState;

const REPO_NAME: &str = "cthulu-workflows";

/// Helper: get the local clone path (~/.cthulu/cthulu-workflows).
fn clone_dir(state: &AppState) -> std::path::PathBuf {
    state.data_dir.join(REPO_NAME)
}

/// Helper: require the PAT or return an error.
async fn require_pat(state: &AppState) -> Result<String, String> {
    state
        .github_pat
        .read()
        .await
        .clone()
        .ok_or_else(|| "GitHub PAT not configured".to_string())
}

/// Read the repo owner from secrets.json.
fn read_owner(state: &AppState) -> Option<String> {
    let content = std::fs::read_to_string(&state.secrets_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v["workspace_repo"]["owner"].as_str().map(String::from)
}

// ---------------------------------------------------------------------------
// Setup repo
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn setup_workflows_repo(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
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
        .map_err(|e| format!("GitHub API error: {e}"))?;

    let user: serde_json::Value = user_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse user response: {e}"))?;

    let username = user["login"]
        .as_str()
        .ok_or_else(|| "GitHub user response missing login field".to_string())?
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
        .map_err(|e| format!("Failed to check repo: {e}"))?;

    let repo_status = repo_check.status();
    let created = if repo_status == 404 {
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
            .map_err(|e| format!("Failed to create repo: {e}"))?;

        let create_status = create_resp.status();
        if create_status == 422 {
            false
        } else if !create_status.is_success() {
            let body = create_resp.text().await.unwrap_or_default();
            return Err(format!("Failed to create repo: {body}"));
        } else {
            true
        }
    } else {
        false
    };

    let repo_url = format!("https://github.com/{}/{}", username, REPO_NAME);

    // 3. Save repo config to secrets.json
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

    // 4. Create local clone directory if it doesn't exist
    let clone_path = clone_dir(&state);
    if !clone_path.exists() {
        std::fs::create_dir_all(&clone_path)
            .map_err(|e| format!("Failed to create local workflows directory: {e}"))?;
    }

    Ok(json!({
        "repo_url": repo_url,
        "created": created,
        "username": username,
    }))
}

// ---------------------------------------------------------------------------
// List workspaces
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_workspaces(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Ok(json!({ "workspaces": [] }));
    }

    let mut workspaces = Vec::new();
    let entries =
        std::fs::read_dir(&clone_path).map_err(|e| format!("Failed to read clone directory: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                workspaces.push(name);
            }
        }
    }

    workspaces.sort();
    Ok(json!({ "workspaces": workspaces }))
}

// ---------------------------------------------------------------------------
// Create workspace
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct CreateWorkspaceRequest {
    name: String,
}

#[tauri::command]
pub async fn create_workspace(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    app_handle: tauri::AppHandle,
    request: CreateWorkspaceRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = require_pat(&state).await?;
    let clone_path = clone_dir(&state);

    // Auto-create local workflows directory if missing
    if !clone_path.exists() {
        std::fs::create_dir_all(&clone_path)
            .map_err(|e| format!("Failed to create local workflows directory: {e}"))?;
    }

    let name = request.name.trim().to_string();
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err("Invalid workspace name".to_string());
    }

    let workspace_dir = clone_path.join(&name);
    if workspace_dir.exists() {
        return Err(format!("Workspace '{}' already exists", name));
    }

    // Local creation — instant
    std::fs::create_dir_all(&workspace_dir)
        .map_err(|e| format!("Failed to create workspace directory: {e}"))?;
    let gitkeep = workspace_dir.join(".gitkeep");
    let _ = std::fs::write(&gitkeep, "");

    // Clone what we need for the background task
    let http_client = state.http_client.clone();
    let secrets_path = state.secrets_path.clone();
    let bg_name = name.clone();
    let bg_pat = pat.clone();
    let bg_app = app_handle.clone();

    // Background GitHub sync
    tokio::spawn(async move {
        use tauri::Emitter;

        let _ = bg_app.emit("sync-status", &json!({
            "status": "syncing",
            "workspace": &bg_name,
            "message": format!("Syncing '{}' to GitHub...", bg_name)
        }));

        // Resolve owner
        let owner = {
            // Try reading from secrets first
            let from_file = (|| -> Option<String> {
                let content = std::fs::read_to_string(&secrets_path).ok()?;
                let v: serde_json::Value = serde_json::from_str(&content).ok()?;
                v["workspace_repo"]["owner"].as_str().map(String::from)
            })();

            match from_file {
                Some(o) if !o.is_empty() => o,
                _ => {
                    // Fetch from GitHub
                    let resp = match http_client
                        .get("https://api.github.com/user")
                        .header("Authorization", format!("Bearer {}", bg_pat))
                        .header("User-Agent", "cthulu-studio")
                        .header("Accept", "application/vnd.github+json")
                        .send()
                        .await
                    {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = bg_app.emit("sync-status", json!({
                                "status": "error",
                                "workspace": &bg_name,
                                "message": format!("GitHub sync failed: {e}")
                            }));
                            return;
                        }
                    };

                    let user: serde_json::Value = match resp.json().await {
                        Ok(u) => u,
                        Err(e) => {
                            let _ = bg_app.emit("sync-status", json!({
                                "status": "error",
                                "workspace": &bg_name,
                                "message": format!("GitHub sync failed: {e}")
                            }));
                            return;
                        }
                    };

                    match user["login"].as_str() {
                        Some(login) => {
                            // Save to secrets
                            let mut secrets: serde_json::Value = std::fs::read_to_string(&secrets_path)
                                .ok()
                                .and_then(|c| serde_json::from_str(&c).ok())
                                .unwrap_or_else(|| json!({}));
                            secrets["workspace_repo"] = json!({ "owner": login, "name": REPO_NAME });
                            let tmp = secrets_path.with_extension("json.tmp");
                            let _ = std::fs::write(&tmp, serde_json::to_string_pretty(&secrets).unwrap_or_default());
                            let _ = std::fs::rename(&tmp, &secrets_path);
                            login.to_string()
                        }
                        None => {
                            let _ = bg_app.emit("sync-status", json!({
                                "status": "error",
                                "workspace": &bg_name,
                                "message": "GitHub user missing login field"
                            }));
                            return;
                        }
                    }
                }
            }
        };

        // Ensure repo exists
        let repo_url = format!("https://api.github.com/repos/{}/{}", owner, REPO_NAME);
        match http_client
            .get(&repo_url)
            .header("Authorization", format!("Bearer {}", bg_pat))
            .header("User-Agent", "cthulu-studio")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
        {
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND => {
                // Create repo
                let create_result = http_client
                    .post("https://api.github.com/user/repos")
                    .header("Authorization", format!("Bearer {}", bg_pat))
                    .header("User-Agent", "cthulu-studio")
                    .header("Accept", "application/vnd.github+json")
                    .json(&json!({
                        "name": REPO_NAME,
                        "private": true,
                        "description": "Cthulu Studio workflow definitions",
                        "auto_init": true,
                    }))
                    .send()
                    .await;

                if let Err(e) = create_result {
                    let _ = bg_app.emit("sync-status", json!({
                        "status": "error",
                        "workspace": &bg_name,
                        "message": format!("Failed to create GitHub repo: {e}")
                    }));
                    return;
                }
            }
            Err(e) => {
                let _ = bg_app.emit("sync-status", json!({
                    "status": "error",
                    "workspace": &bg_name,
                    "message": format!("GitHub API error: {e}")
                }));
                return;
            }
            _ => {} // repo exists
        }

        // Push .gitkeep
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"",
        );
        let put_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}/.gitkeep",
            owner, REPO_NAME, bg_name
        );

        match http_client
            .put(&put_url)
            .header("Authorization", format!("Bearer {}", bg_pat))
            .header("User-Agent", "cthulu-studio")
            .header("Accept", "application/vnd.github+json")
            .json(&json!({
                "message": format!("Create workspace: {}", bg_name),
                "content": encoded,
            }))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() || resp.status() == reqwest::StatusCode::UNPROCESSABLE_ENTITY => {
                let _ = bg_app.emit("sync-status", &json!({
                    "status": "synced",
                    "workspace": &bg_name,
                    "message": format!("'{}' synced to GitHub", bg_name)
                }));
            }
            Ok(resp) => {
                let body = resp.text().await.unwrap_or_default();
                let _ = bg_app.emit("sync-status", &json!({
                    "status": "error",
                    "workspace": &bg_name,
                    "message": format!("GitHub sync error: {}", body)
                }));
            }
            Err(e) => {
                let _ = bg_app.emit("sync-status", json!({
                    "status": "error",
                    "workspace": &bg_name,
                    "message": format!("GitHub sync failed: {e}")
                }));
            }
        }
    });

    Ok(json!({ "ok": true, "name": name }))
}

// ---------------------------------------------------------------------------
// List workspace workflows
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_workspace_workflows(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    workspace: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let clone_path = clone_dir(&state);
    let workspace_dir = clone_path.join(&workspace);

    if !workspace_dir.exists() {
        return Err(format!("Workspace '{}' not found", workspace));
    }

    let mut workflows = Vec::new();
    let entries =
        std::fs::read_dir(&workspace_dir).map_err(|e| format!("Failed to read workspace: {e}"))?;

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
                workflows.push(json!({
                    "name": name,
                    "workspace": workspace,
                    "description": description,
                    "node_count": node_count,
                }));
            }
        }
    }

    workflows.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });

    Ok(json!({
        "workspace": workspace,
        "workflows": workflows,
    }))
}

// ---------------------------------------------------------------------------
// Get workflow
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_workflow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    workspace: String,
    name: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let yaml_path = clone_dir(&state)
        .join(&workspace)
        .join(&name)
        .join("workflow.yaml");

    if !yaml_path.exists() {
        return Err(format!(
            "Workflow '{}/{}' not found",
            workspace, name
        ));
    }

    let content = std::fs::read_to_string(&yaml_path)
        .map_err(|e| format!("Failed to read workflow: {e}"))?;

    let yaml_value: serde_json::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse workflow YAML: {e}"))?;

    Ok(yaml_value)
}

// ---------------------------------------------------------------------------
// Save workflow (local only)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SaveWorkflowRequest {
    flow: serde_json::Value,
}

#[tauri::command]
pub async fn save_workflow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    workspace: String,
    name: String,
    request: SaveWorkflowRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Err("Workflows repo not set up".to_string());
    }

    let workflow_dir = clone_path.join(&workspace).join(&name);
    let _ = std::fs::create_dir_all(&workflow_dir);

    let yaml = serde_yaml::to_string(&request.flow)
        .map_err(|e| format!("Failed to serialize to YAML: {e}"))?;

    let yaml_path = workflow_dir.join("workflow.yaml");

    // Mark as self-write so the file watcher skips this change
    state.workflow_self_writes.mark(yaml_path.clone());

    let tmp_path = yaml_path.with_extension("yaml.tmp");
    std::fs::write(&tmp_path, &yaml).map_err(|e| format!("Failed to write workflow: {e}"))?;
    std::fs::rename(&tmp_path, &yaml_path)
        .map_err(|e| format!("Failed to save workflow: {e}"))?;

    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Publish workflow (save + push to GitHub)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PublishWorkflowRequest {
    flow: serde_json::Value,
}

#[tauri::command]
pub async fn publish_workflow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    workspace: String,
    name: String,
    request: PublishWorkflowRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).unwrap_or_default();
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Err("Workflows repo not set up".to_string());
    }

    let workflow_dir = clone_path.join(&workspace).join(&name);
    let _ = std::fs::create_dir_all(&workflow_dir);

    let yaml = serde_yaml::to_string(&request.flow)
        .map_err(|e| format!("Failed to serialize to YAML: {e}"))?;

    // Save locally
    let yaml_path = workflow_dir.join("workflow.yaml");

    // Mark as self-write so the file watcher skips this change
    state.workflow_self_writes.mark(yaml_path.clone());

    let tmp_path = yaml_path.with_extension("yaml.tmp");
    std::fs::write(&tmp_path, &yaml).map_err(|e| format!("Failed to write workflow: {e}"))?;
    std::fs::rename(&tmp_path, &yaml_path)
        .map_err(|e| format!("Failed to save workflow: {e}"))?;

    // Push to GitHub
    let gh_path = format!("{workspace}/{name}/workflow.yaml");
    let gh_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}",
        owner, REPO_NAME, gh_path
    );

    // Check if file already exists to get SHA
    let existing_sha = {
        let check = state
            .http_client
            .get(&gh_url)
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

    let encoded = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        yaml.as_bytes(),
    );

    let mut body = json!({
        "message": format!("Publish {workspace}/{name}"),
        "content": encoded,
    });

    if let Some(sha) = existing_sha {
        body["sha"] = json!(sha);
    }

    let resp = state
        .http_client
        .put(&gh_url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("GitHub PUT error: {e}"))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub PUT file error: {body}"));
    }

    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Delete workflow
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn delete_workflow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    workspace: String,
    name: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).unwrap_or_default();
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Err("Workflows repo not set up".to_string());
    }

    let workflow_dir = clone_path.join(&workspace).join(&name);
    if !workflow_dir.exists() {
        return Err(format!("Workflow {workspace}/{name} not found"));
    }

    // Remove locally
    std::fs::remove_dir_all(&workflow_dir)
        .map_err(|e| format!("Failed to delete workflow directory: {e}"))?;

    // Delete from GitHub
    let gh_path = format!("{workspace}/{name}/workflow.yaml");
    let gh_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}",
        owner, REPO_NAME, gh_path
    );

    // Get SHA for deletion
    let resp = state
        .http_client
        .get(&gh_url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub API error: {e}"))?;

    if resp.status().is_success() {
        let file_info: serde_json::Value = resp.json().await.unwrap_or_default();
        if let Some(sha) = file_info["sha"].as_str() {
            let _ = state
                .http_client
                .delete(&gh_url)
                .header("Authorization", format!("Bearer {}", pat))
                .header("User-Agent", "cthulu-studio")
                .header("Accept", "application/vnd.github+json")
                .json(&json!({
                    "message": format!("Delete {workspace}/{name}"),
                    "sha": sha,
                }))
                .send()
                .await;
        }
    }

    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// Sync workflows
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn sync_workflows(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).unwrap_or_default();
    let clone_path = clone_dir(&state);

    if !clone_path.exists() {
        return Err("Workflows repo not set up. Call setup_workflows_repo first.".to_string());
    }

    // TODO: Implement full recursive sync via GitHub Contents API
    // For now, return the current workspaces
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

    // Suppress unused variable warnings — these will be used when full sync is implemented
    let _ = pat;
    let _ = owner;

    Ok(json!({
        "ok": true,
        "workspaces": workspaces,
    }))
}

// ---------------------------------------------------------------------------
// Run workflow
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn run_workflow(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    workspace: String,
    name: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;

    let yaml_path = clone_dir(&state)
        .join(&workspace)
        .join(&name)
        .join("workflow.yaml");

    if !yaml_path.exists() {
        return Err(format!(
            "Workflow '{}/{}' not found",
            workspace, name
        ));
    }

    let content = std::fs::read_to_string(&yaml_path)
        .map_err(|e| format!("Failed to read workflow: {e}"))?;

    // Parse YAML → JSON → Flow struct
    let yaml_value: serde_json::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("Failed to parse workflow YAML: {e}"))?;

    // The workflow YAML is stored as a serialized Flow, but may lack some fields.
    // Fill in defaults for required fields that might be missing.
    let mut flow_json = yaml_value.clone();
    if flow_json.get("id").is_none() || flow_json["id"].as_str().unwrap_or("").is_empty() {
        flow_json["id"] = json!(format!("wf:{}:{}", workspace, name));
    }
    if flow_json.get("created_at").is_none() {
        flow_json["created_at"] = json!(chrono::Utc::now().to_rfc3339());
    }
    if flow_json.get("updated_at").is_none() {
        flow_json["updated_at"] = json!(chrono::Utc::now().to_rfc3339());
    }

    let flow: cthulu::flows::Flow = serde_json::from_value(flow_json)
        .map_err(|e| format!("Failed to deserialize workflow as Flow: {e}"))?;

    // Build runner (same pattern as trigger_flow)
    let session_bridge = cthulu::flows::session_bridge::SessionBridge {
        sessions: state.interact_sessions.clone(),
        sessions_path: state.sessions_path.clone(),
        data_dir: state.data_dir.clone(),
        session_streams: state.session_streams.clone(),
    };
    let runner = cthulu::flows::runner::FlowRunner {
        http_client: state.http_client.clone(),
        github_client: state.github_client.clone(),
        events_tx: Some(state.events_tx.clone()),
        sandbox_provider: Some(state.sandbox_provider.clone()),
        agent_repo: Some(state.agent_repo.clone()),
        session_bridge: Some(session_bridge),
    };

    let flow_repo = state.flow_repo.clone();
    let flow_label = format!("{}/{}", workspace, name);

    tokio::spawn(async move {
        match runner.execute(&flow, &*flow_repo, None).await {
            Ok(run) => {
                tracing::info!(
                    workflow = %flow_label,
                    run_id = %run.id,
                    "Workflow execution completed"
                );
            }
            Err(e) => {
                tracing::error!(workflow = %flow_label, error = %e, "Workflow execution failed");
            }
        }
    });

    Ok(json!({ "status": "triggered", "workspace": workspace, "name": name }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_workflow_meta(path: &std::path::Path) -> (Option<String>, usize) {
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
