use serde::Deserialize;
use serde_json::{json, Value};

use cthulu::api::AppState;
use cthulu::flows::NodeType;
use cthulu::templates;

// ---------------------------------------------------------------------------
// List templates
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_templates(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let templates = state.template_cache.read().await.clone();
    Ok(json!({ "templates": templates }))
}

// ---------------------------------------------------------------------------
// Get template YAML
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_template_yaml(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    category: String,
    slug: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let yaml_path = state
        .static_dir
        .join("workflows")
        .join(&category)
        .join(format!("{slug}.yaml"));

    let yaml = std::fs::read_to_string(&yaml_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("template not found: {category}/{slug}")
        } else {
            e.to_string()
        }
    })?;

    Ok(json!({ "yaml": yaml }))
}

// ---------------------------------------------------------------------------
// Import template
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn import_template(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    category: String,
    slug: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let yaml_path = state
        .static_dir
        .join("workflows")
        .join(&category)
        .join(format!("{slug}.yaml"));

    let yaml = std::fs::read_to_string(&yaml_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("template not found: {category}/{slug}")
        } else {
            e.to_string()
        }
    })?;

    let mut flow = templates::parse_template_yaml(&yaml)
        .map_err(|e| format!("failed to parse template: {e}"))?;

    // Auto-create agents for executor nodes missing agent_id
    provision_agents_for_executors(&mut flow, &state).await;

    state
        .flow_repo
        .save_flow(flow.clone())
        .await
        .map_err(|e| format!("failed to save flow: {e}"))?;

    // Register with scheduler (it's disabled by default, but register it)
    let _ = state.scheduler.restart_flow(&flow.id).await;

    serde_json::to_value(&flow).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Import from YAML string
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ImportYamlRequest {
    yaml: String,
}

#[tauri::command]
pub async fn import_yaml(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: ImportYamlRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    if request.yaml.trim().is_empty() {
        return Err("yaml field is required and must not be empty".to_string());
    }

    let mut flow = templates::parse_template_yaml(&request.yaml)
        .map_err(|e| format!("failed to parse YAML: {e}"))?;

    // Auto-create agents for executor nodes missing agent_id
    provision_agents_for_executors(&mut flow, &state).await;

    state
        .flow_repo
        .save_flow(flow.clone())
        .await
        .map_err(|e| format!("failed to save flow: {e}"))?;

    let _ = state.scheduler.restart_flow(&flow.id).await;

    Ok(json!({ "flows": [flow] }))
}

// ---------------------------------------------------------------------------
// Import from GitHub
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ImportGithubRequest {
    repo_url: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    branch: String,
}

#[tauri::command]
pub async fn import_github(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    request: ImportGithubRequest,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let branch = if request.branch.is_empty() {
        "main".to_string()
    } else {
        request.branch
    };

    // Parse the GitHub URL
    let url = request
        .repo_url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("github.com/")
        .trim_end_matches('/');

    let parts: Vec<&str> = url.splitn(5, '/').collect();
    if parts.len() < 2 {
        return Err("invalid GitHub URL — expected https://github.com/owner/repo".to_string());
    }

    let owner = parts[0];
    let gh_repo = parts[1];

    let url_sub_path = if parts.len() >= 5 && parts[2] == "tree" {
        parts[4].to_string()
    } else {
        String::new()
    };

    let sub_path = if !request.path.is_empty() {
        request.path.trim_matches('/').to_string()
    } else {
        url_sub_path
    };

    // Fetch YAML files from GitHub
    let yaml_files =
        cthulu::api::templates::repository::fetch_github_yaml_files(
            &state.http_client,
            owner,
            gh_repo,
            &sub_path,
            &branch,
        )
        .await
        .map_err(|e| format!("failed to fetch GitHub repo: {e}"))?;

    if yaml_files.is_empty() {
        return Err("no .yaml or .yml files found in the specified path".to_string());
    }

    let mut imported_flows: Vec<Value> = Vec::new();
    let mut errors: Vec<Value> = Vec::new();

    for (filename, yaml_content) in &yaml_files {
        match templates::parse_template_yaml(yaml_content) {
            Ok(mut flow) => {
                provision_agents_for_executors(&mut flow, &state).await;

                match state.flow_repo.save_flow(flow.clone()).await {
                    Ok(_) => {
                        let _ = state.scheduler.restart_flow(&flow.id).await;
                        imported_flows.push(json!(flow));
                    }
                    Err(e) => {
                        errors.push(
                            json!({ "file": filename, "error": format!("save failed: {e}") }),
                        );
                    }
                }
            }
            Err(e) => {
                errors.push(
                    json!({ "file": filename, "error": format!("parse failed: {e}") }),
                );
            }
        }
    }

    Ok(json!({
        "flows": imported_flows,
        "errors": errors,
        "total_found": yaml_files.len(),
        "imported": imported_flows.len(),
    }))
}

// ---------------------------------------------------------------------------
// Agent auto-provisioning helper
// ---------------------------------------------------------------------------

async fn provision_agents_for_executors(
    flow: &mut cthulu::flows::Flow,
    state: &AppState,
) {
    for node in flow.nodes.iter_mut() {
        if node.node_type != NodeType::Executor {
            continue;
        }
        if node
            .config
            .get("agent_id")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty())
        {
            continue;
        }

        let agent_id = uuid::Uuid::new_v4().to_string();

        let prompt = node
            .config
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let permissions: Vec<String> = node
            .config
            .get("permissions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let agent = cthulu::agents::Agent::builder(&agent_id)
            .name(node.label.clone())
            .description(format!(
                "Auto-created agent for executor node '{}'",
                node.label
            ))
            .prompt(prompt)
            .permissions(permissions)
            .build();

        if let Err(e) = state.agent_repo.save(agent).await {
            tracing::warn!(
                node_label = %node.label,
                error = %e,
                "failed to auto-create agent for executor node"
            );
            continue;
        }

        node.config["agent_id"] = json!(agent_id);
    }
}
