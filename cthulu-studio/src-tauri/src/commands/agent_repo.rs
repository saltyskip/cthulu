use base64::Engine as _;
use serde_json::{json, Value};

use cthulu::api::AppState;

const REPO_NAME: &str = "cthulu-agents";
const SECRETS_KEY: &str = "agent_repo";

/// Helper: get the local sync path (~/.cthulu/cthulu-agents).
fn sync_dir(state: &AppState) -> std::path::PathBuf {
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

/// Read the repo owner from secrets.json under the `agent_repo` key.
fn read_owner(state: &AppState) -> Option<String> {
    let content = std::fs::read_to_string(&state.secrets_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v[SECRETS_KEY]["owner"].as_str().map(String::from)
}

// ---------------------------------------------------------------------------
// Org management
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_orgs(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let sync = sync_dir(&state);
    if !sync.exists() {
        return Ok(json!({ "orgs": [] }));
    }
    let mut orgs = Vec::new();
    let entries = std::fs::read_dir(&sync)
        .map_err(|e| format!("Failed to read sync directory: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "projects" {
                continue;
            }
            let org_json_path = path.join("org.json");
            let org_meta: serde_json::Value = if org_json_path.exists() {
                let content = std::fs::read_to_string(&org_json_path).unwrap_or_default();
                serde_json::from_str(&content).unwrap_or_else(|_| json!({ "name": &name }))
            } else {
                json!({ "name": &name })
            };
            orgs.push(json!({
                "slug": &name,
                "name": org_meta["name"].as_str().unwrap_or(&name),
                "description": org_meta["description"].as_str().unwrap_or(""),
            }));
        }
    }
    orgs.sort_by(|a, b| {
        let an = a["name"].as_str().unwrap_or("");
        let bn = b["name"].as_str().unwrap_or("");
        an.cmp(bn)
    });
    Ok(json!({ "orgs": orgs }))
}

#[tauri::command]
pub async fn create_org(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    name: String,
    description: Option<String>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).ok_or_else(|| {
        "Agent repo not set up. Call setup_agent_repo first.".to_string()
    })?;
    let slug = name.trim().to_lowercase()
        .chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        return Err("Org name cannot be empty".to_string());
    }
    let org_dir = sync_dir(&state).join(&slug);
    if org_dir.exists() {
        return Err(format!("Org '{}' already exists", slug));
    }
    std::fs::create_dir_all(org_dir.join("projects"))
        .map_err(|e| format!("Failed to create org directory: {e}"))?;
    let org_meta = json!({
        "name": name.trim(),
        "description": description.as_deref().unwrap_or(""),
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let org_json_str = serde_json::to_string_pretty(&org_meta).unwrap_or_default();
    std::fs::write(org_dir.join("org.json"), &org_json_str)
        .map_err(|e| format!("Failed to write org.json: {e}"))?;
    // Push org.json to GitHub
    let encoded = base64::engine::general_purpose::STANDARD.encode(org_json_str.as_bytes());
    let put_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}/org.json",
        owner, REPO_NAME, slug
    );
    let resp = state.http_client.put(&put_url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .json(&json!({ "message": format!("Create org: {}", name.trim()), "content": encoded }))
        .send().await
        .map_err(|e| format!("GitHub PUT error: {e}"))?;
    if !resp.status().is_success() && resp.status() != reqwest::StatusCode::UNPROCESSABLE_ENTITY {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub PUT error: {body}"));
    }
    // Push .gitkeep in projects/
    let encoded_empty = base64::engine::general_purpose::STANDARD.encode(b"");
    let gitkeep_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}/projects/.gitkeep",
        owner, REPO_NAME, slug
    );
    let _ = state.http_client.put(&gitkeep_url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .json(&json!({ "message": format!("Create org projects dir: {}", slug), "content": encoded_empty }))
        .send().await;
    Ok(json!({ "ok": true, "slug": slug, "name": name.trim() }))
}

#[tauri::command]
pub async fn delete_org(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    slug: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let org_dir = sync_dir(&state).join(&slug);
    if org_dir.exists() {
        std::fs::remove_dir_all(&org_dir)
            .map_err(|e| format!("Failed to remove org directory: {e}"))?;
    }
    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// 1. Setup agent repo
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn setup_agent_repo(
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
                "description": "Cthulu Studio agent definitions",
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

    // 3. Save repo config to secrets.json under `agent_repo` key
    {
        let secrets_path = &state.secrets_path;
        let mut secrets: serde_json::Value = if secrets_path.exists() {
            let content = std::fs::read_to_string(secrets_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
        } else {
            json!({})
        };

        secrets[SECRETS_KEY] = json!({
            "owner": username,
            "name": REPO_NAME,
        });

        let tmp_path = secrets_path.with_extension("json.tmp");
        let json_str = serde_json::to_string_pretty(&secrets).unwrap_or_default();
        let _ = std::fs::write(&tmp_path, &json_str);
        let _ = std::fs::rename(&tmp_path, secrets_path);
    }

    // 4. Create local sync directory if it doesn't exist
    let local_dir = sync_dir(&state);
    if !local_dir.exists() {
        std::fs::create_dir_all(&local_dir)
            .map_err(|e| format!("Failed to create local agents directory: {e}"))?;
    }

    // Create default org using the GitHub username
    let default_org_dir = local_dir.join(&username);
    if !default_org_dir.exists() {
        std::fs::create_dir_all(default_org_dir.join("projects"))
            .map_err(|e| format!("Failed to create default org directory: {e}"))?;
    }

    // Write org.json for the default org
    let org_json_path = default_org_dir.join("org.json");
    if !org_json_path.exists() {
        let org_meta = json!({
            "name": &username,
            "description": "Default organization",
            "created_at": chrono::Utc::now().to_rfc3339(),
        });
        let org_json_str = serde_json::to_string_pretty(&org_meta).unwrap_or_default();
        let _ = std::fs::write(&org_json_path, &org_json_str);

        // Push org.json to GitHub
        let encoded_org = base64::engine::general_purpose::STANDARD.encode(org_json_str.as_bytes());
        let org_put_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}/org.json",
            username, REPO_NAME, username
        );
        let _ = state.http_client.put(&org_put_url)
            .header("Authorization", format!("Bearer {}", pat))
            .header("User-Agent", "cthulu-studio")
            .header("Accept", "application/vnd.github+json")
            .json(&json!({ "message": format!("Create default org: {}", username), "content": encoded_org }))
            .send().await;

        // Push .gitkeep in projects/
        let encoded_empty = base64::engine::general_purpose::STANDARD.encode(b"");
        let gitkeep_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}/projects/.gitkeep",
            username, REPO_NAME, username
        );
        let _ = state.http_client.put(&gitkeep_url)
            .header("Authorization", format!("Bearer {}", pat))
            .header("User-Agent", "cthulu-studio")
            .header("Accept", "application/vnd.github+json")
            .json(&json!({ "message": format!("Create default org projects dir: {}", username), "content": encoded_empty }))
            .send().await;
    }

    Ok(json!({
        "repo_url": repo_url,
        "created": created,
        "username": username,
    }))
}

// ---------------------------------------------------------------------------
// 2. List agent projects
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_agent_projects(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    org: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let projects_dir = sync_dir(&state).join(&org).join("projects");

    if !projects_dir.exists() {
        return Ok(json!({ "projects": [] }));
    }

    let mut projects = Vec::new();
    let entries = std::fs::read_dir(&projects_dir)
        .map_err(|e| format!("Failed to read projects directory: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                // Read project.json for metadata
                let project_json_path = path.join("project.json");
                let project_meta: serde_json::Value = if project_json_path.exists() {
                    let content = std::fs::read_to_string(&project_json_path).unwrap_or_default();
                    serde_json::from_str(&content).unwrap_or_else(|_| json!({}))
                } else {
                    json!({})
                };
                projects.push(json!({
                    "slug": &name,
                    "name": project_meta["name"].as_str().unwrap_or(&name),
                    "working_dir": project_meta["working_dir"].as_str().unwrap_or(""),
                    "color": project_meta.get("color").and_then(|v| v.as_str()),
                    "status": project_meta["status"].as_str().unwrap_or("active"),
                }));
            }
        }
    }

    projects.sort_by(|a, b| {
        let an = a["name"].as_str().unwrap_or("");
        let bn = b["name"].as_str().unwrap_or("");
        an.cmp(bn)
    });
    Ok(json!({ "projects": projects }))
}

// ---------------------------------------------------------------------------
// 3. Create agent project
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn create_agent_project(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    org: String,
    project: String,
    working_dir: Option<String>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = require_pat(&state).await?;

    // Validate project name: lowercase alphanumeric + hyphens, no leading/trailing hyphens
    let name = project.trim().to_string();
    if name.is_empty() {
        return Err("Project name cannot be empty".to_string());
    }
    let valid = !name.starts_with('-')
        && !name.ends_with('-')
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(
            "Invalid project name: must be lowercase alphanumeric with hyphens, no leading/trailing hyphens"
                .to_string(),
        );
    }

    let projects_dir = sync_dir(&state).join(&org).join("projects");
    let project_dir = projects_dir.join(&name);
    if project_dir.exists() {
        return Err(format!("Project '{}' already exists", name));
    }

    // Create local directory with .gitkeep
    std::fs::create_dir_all(&project_dir)
        .map_err(|e| format!("Failed to create project directory: {e}"))?;
    let gitkeep = project_dir.join(".gitkeep");
    let _ = std::fs::write(&gitkeep, "");

    // Write project.json with metadata
    let project_meta = json!({
        "name": &name,
        "working_dir": working_dir.as_deref().unwrap_or(""),
        "color": null,
        "status": "active",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let project_json_str = serde_json::to_string_pretty(&project_meta).unwrap_or_default();
    std::fs::write(project_dir.join("project.json"), &project_json_str)
        .map_err(|e| format!("Failed to write project.json: {e}"))?;

    // Push .gitkeep to GitHub
    let owner = read_owner(&state).ok_or_else(|| {
        "Agent repo not set up. Call setup_agent_repo first.".to_string()
    })?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(b"");
    let put_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}/projects/{}/.gitkeep",
        owner, REPO_NAME, org, name
    );

    let resp = state
        .http_client
        .put(&put_url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .json(&json!({
            "message": format!("Create project: {}/{}", org, name),
            "content": encoded,
        }))
        .send()
        .await
        .map_err(|e| format!("GitHub PUT error: {e}"))?;

    if !resp.status().is_success() && resp.status() != reqwest::StatusCode::UNPROCESSABLE_ENTITY {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("GitHub PUT error: {body}"));
    }

    // Push project.json to GitHub
    let encoded_project = base64::engine::general_purpose::STANDARD.encode(project_json_str.as_bytes());
    let project_json_url = format!(
        "https://api.github.com/repos/{}/{}/contents/{}/projects/{}/project.json",
        owner, REPO_NAME, org, name
    );

    let _ = state
        .http_client
        .put(&project_json_url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .json(&json!({
            "message": format!("Create project metadata: {}/{}", org, name),
            "content": encoded_project,
        }))
        .send()
        .await;

    Ok(json!({ "ok": true, "name": name }))
}

// ---------------------------------------------------------------------------
// 4. Publish agent
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn publish_agent(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
    org: String,
    project: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).ok_or_else(|| {
        "Agent repo not set up. Call setup_agent_repo first.".to_string()
    })?;

    // Get the agent from the repository
    let mut agent = state
        .agent_repo
        .get(&id)
        .await
        .ok_or_else(|| format!("Agent '{}' not found", id))?;

    // Read project metadata to get working_dir
    let project_dir = sync_dir(&state).join(&org).join("projects").join(&project);
    let project_json_path = project_dir.join("project.json");
    let project_working_dir: Option<String> = if project_json_path.exists() {
        let content = std::fs::read_to_string(&project_json_path).unwrap_or_default();
        let meta: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
        meta["working_dir"].as_str().filter(|s| !s.is_empty()).map(String::from)
    } else {
        None
    };

    // Update the project field (just the project name, not org-prefixed)
    agent.project = Some(project.clone());

    // Auto-set working_dir from project if the project has one
    if let Some(ref wd) = project_working_dir {
        agent.working_dir = Some(wd.clone());
    }

    // Auto-grant full permissions so the agent can execute anything in the workspace
    agent.permissions = vec![
        "Bash".to_string(),
        "Read".to_string(),
        "Write".to_string(),
        "Edit".to_string(),
        "MultiEdit".to_string(),
        "Grep".to_string(),
        "Glob".to_string(),
        "TodoRead".to_string(),
        "TodoWrite".to_string(),
        "WebFetch".to_string(),
        "mcp__desktop-commander__execute_command".to_string(),
        "mcp__desktop-commander__read_file".to_string(),
        "mcp__desktop-commander__write_file".to_string(),
    ];
    agent.auto_permissions = true;

    state
        .agent_repo
        .save(agent.clone())
        .await
        .map_err(|e| format!("Failed to save agent: {e}"))?;

    // Build agent.json: serialize agent WITHOUT prompt and project fields
    let mut agent_json: serde_json::Value =
        serde_json::to_value(&agent).map_err(|e| format!("Failed to serialize agent: {e}"))?;
    if let Some(obj) = agent_json.as_object_mut() {
        obj.remove("prompt");
        obj.remove("project");
    }
    let agent_json_str = serde_json::to_string_pretty(&agent_json)
        .map_err(|e| format!("Failed to serialize agent JSON: {e}"))?;

    // Build prompt.md: the agent's prompt text
    let prompt_md = agent.prompt.clone();

    // Write to local sync dir
    let agent_dir = sync_dir(&state)
        .join(&org)
        .join("projects")
        .join(&project)
        .join(&id);
    std::fs::create_dir_all(&agent_dir)
        .map_err(|e| format!("Failed to create agent sync directory: {e}"))?;

    std::fs::write(agent_dir.join("agent.json"), &agent_json_str)
        .map_err(|e| format!("Failed to write agent.json: {e}"))?;
    std::fs::write(agent_dir.join("prompt.md"), &prompt_md)
        .map_err(|e| format!("Failed to write prompt.md: {e}"))?;

    // Push agent.json to GitHub
    let gh_base = format!(
        "https://api.github.com/repos/{}/{}/contents/{}/projects/{}/{}",
        owner, REPO_NAME, org, project, id
    );

    for (filename, content) in [("agent.json", agent_json_str.as_str()), ("prompt.md", prompt_md.as_str())] {
        let gh_url = format!("{}/{}", gh_base, filename);

        // Check if file exists to get SHA for update
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

        let encoded = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());

        let mut body = json!({
            "message": format!("Publish agent {}/{}/{}", org, project, id),
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
            return Err(format!("GitHub PUT file error for {}: {}", filename, body));
        }
    }

    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// 5. Unpublish agent
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn unpublish_agent(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
    id: String,
    org: String,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).ok_or_else(|| {
        "Agent repo not set up. Call setup_agent_repo first.".to_string()
    })?;

    // Get the agent to find its project
    let mut agent = state
        .agent_repo
        .get(&id)
        .await
        .ok_or_else(|| format!("Agent '{}' not found", id))?;

    let project = agent
        .project
        .clone()
        .ok_or_else(|| format!("Agent '{}' is not published to any project", id))?;

    // Delete both files from GitHub
    let gh_base = format!(
        "https://api.github.com/repos/{}/{}/contents/{}/projects/{}/{}",
        owner, REPO_NAME, org, project, id
    );

    for filename in ["agent.json", "prompt.md"] {
        let gh_url = format!("{}/{}", gh_base, filename);

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
                let del_resp = state
                    .http_client
                    .delete(&gh_url)
                    .header("Authorization", format!("Bearer {}", pat))
                    .header("User-Agent", "cthulu-studio")
                    .header("Accept", "application/vnd.github+json")
                    .json(&json!({
                        "message": format!("Unpublish agent {}/{}/{}", org, project, id),
                        "sha": sha,
                    }))
                    .send()
                    .await
                    .map_err(|e| format!("GitHub DELETE error: {e}"))?;

                if !del_resp.status().is_success() {
                    let body = del_resp.text().await.unwrap_or_default();
                    tracing::warn!("Failed to delete {} from GitHub: {}", filename, body);
                }
            }
        }
    }

    // Remove from local sync dir
    let agent_dir = sync_dir(&state)
        .join(&org)
        .join("projects")
        .join(&project)
        .join(&id);
    if agent_dir.exists() {
        let _ = std::fs::remove_dir_all(&agent_dir);
    }

    // Clear project field on the agent
    agent.project = None;
    state
        .agent_repo
        .save(agent)
        .await
        .map_err(|e| format!("Failed to save agent: {e}"))?;

    Ok(json!({ "ok": true }))
}

// ---------------------------------------------------------------------------
// 6. Sync agent repo
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn sync_agent_repo(
    state: tauri::State<'_, AppState>,
    ready: tauri::State<'_, crate::ReadySignal>,
) -> Result<Value, String> {
    crate::wait_ready(&ready).await?;
    let pat = require_pat(&state).await?;
    let owner = read_owner(&state).ok_or_else(|| {
        "Agent repo not set up. Call setup_agent_repo first.".to_string()
    })?;

    let local_dir = sync_dir(&state);
    if !local_dir.exists() {
        std::fs::create_dir_all(&local_dir)
            .map_err(|e| format!("Failed to create local agents directory: {e}"))?;
    }

    // Fetch top-level directory from GitHub (list orgs)
    let root_url = format!(
        "https://api.github.com/repos/{}/{}/contents",
        owner, REPO_NAME
    );

    let root_resp = state
        .http_client
        .get(&root_url)
        .header("Authorization", format!("Bearer {}", pat))
        .header("User-Agent", "cthulu-studio")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub API error: {e}"))?;

    if root_resp.status() == 404 {
        return Ok(json!({ "ok": true, "synced": 0, "projects": [] }));
    }

    if !root_resp.status().is_success() {
        let body = root_resp.text().await.unwrap_or_default();
        return Err(format!("GitHub API error listing root: {body}"));
    }

    let root_list: Vec<serde_json::Value> = root_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse root listing: {e}"))?;

    let mut synced_count = 0u32;
    let mut seen_ids = std::collections::HashSet::new();
    let mut project_names = Vec::new();

    // Iterate over top-level dirs (orgs)
    for org_entry in &root_list {
        let org_name = match org_entry["name"].as_str() {
            Some(n) => n,
            None => continue,
        };
        if org_entry["type"].as_str() != Some("dir") {
            continue;
        }
        // Skip hidden directories
        if org_name.starts_with('.') {
            continue;
        }

        // Create local org directory
        let local_org_dir = local_dir.join(org_name);
        if !local_org_dir.exists() {
            let _ = std::fs::create_dir_all(&local_org_dir);
        }

        // Download org.json if it exists
        let org_json_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}/org.json",
            owner, REPO_NAME, org_name
        );
        let org_json_resp = state
            .http_client
            .get(&org_json_url)
            .header("Authorization", format!("Bearer {}", pat))
            .header("User-Agent", "cthulu-studio")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await;
        if let Ok(resp) = org_json_resp {
            if resp.status().is_success() {
                let file_info: serde_json::Value = resp.json().await.unwrap_or_default();
                if let Some(b64) = file_info["content"].as_str() {
                    let cleaned = b64.replace('\n', "").replace('\r', "");
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&cleaned) {
                        if let Ok(content) = String::from_utf8(bytes) {
                            let _ = std::fs::write(local_org_dir.join("org.json"), &content);
                        }
                    }
                }
            }
        }

        // Fetch {org}/projects/ directory from GitHub
        let projects_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}/projects",
            owner, REPO_NAME, org_name
        );

        let projects_resp = state
            .http_client
            .get(&projects_url)
            .header("Authorization", format!("Bearer {}", pat))
            .header("User-Agent", "cthulu-studio")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await;

        let projects_list: Vec<serde_json::Value> = match projects_resp {
            Ok(resp) if resp.status().is_success() => {
                resp.json().await.unwrap_or_default()
            }
            _ => continue,
        };

        // Create local projects dir for this org
        let local_projects_dir = local_org_dir.join("projects");
        if !local_projects_dir.exists() {
            let _ = std::fs::create_dir_all(&local_projects_dir);
        }

        for project_entry in &projects_list {
            let project_name = match project_entry["name"].as_str() {
                Some(n) => n,
                None => continue,
            };
            if project_entry["type"].as_str() != Some("dir") {
                continue;
            }

            project_names.push(format!("{}/{}", org_name, project_name));

            // Create local project directory
            let local_project_dir = local_projects_dir.join(project_name);
            if !local_project_dir.exists() {
                let _ = std::fs::create_dir_all(&local_project_dir);
            }

            // Download project.json if it exists
            let project_json_url = format!(
                "https://api.github.com/repos/{}/{}/contents/{}/projects/{}/project.json",
                owner, REPO_NAME, org_name, project_name
            );
            let project_json_resp = state
                .http_client
                .get(&project_json_url)
                .header("Authorization", format!("Bearer {}", pat))
                .header("User-Agent", "cthulu-studio")
                .header("Accept", "application/vnd.github+json")
                .send()
                .await;
            if let Ok(resp) = project_json_resp {
                if resp.status().is_success() {
                    let file_info: serde_json::Value = resp.json().await.unwrap_or_default();
                    if let Some(b64) = file_info["content"].as_str() {
                        let cleaned = b64.replace('\n', "").replace('\r', "");
                        if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(&cleaned) {
                            if let Ok(content) = String::from_utf8(bytes) {
                                let _ = std::fs::write(local_project_dir.join("project.json"), &content);
                            }
                        }
                    }
                }
            }

            // List agents in this project
            let agents_url = format!(
                "https://api.github.com/repos/{}/{}/contents/{}/projects/{}",
                owner, REPO_NAME, org_name, project_name
            );

            let agents_resp = state
                .http_client
                .get(&agents_url)
                .header("Authorization", format!("Bearer {}", pat))
                .header("User-Agent", "cthulu-studio")
                .header("Accept", "application/vnd.github+json")
                .send()
                .await;

            let agents_list: Vec<serde_json::Value> = match agents_resp {
                Ok(resp) if resp.status().is_success() => {
                    resp.json().await.unwrap_or_default()
                }
                _ => continue,
            };

            for agent_entry in &agents_list {
                let agent_id = match agent_entry["name"].as_str() {
                    Some(n) => n,
                    None => continue,
                };
                if agent_entry["type"].as_str() != Some("dir") {
                    continue;
                }

                // Skip duplicates
                if seen_ids.contains(agent_id) {
                    tracing::warn!(
                        "Duplicate agent ID '{}' found in {}/{}, skipping",
                        agent_id,
                        org_name,
                        project_name
                    );
                    continue;
                }
                seen_ids.insert(agent_id.to_string());

                // Create local agent directory
                let local_agent_dir = local_project_dir.join(agent_id);
                if !local_agent_dir.exists() {
                    let _ = std::fs::create_dir_all(&local_agent_dir);
                }

                // Download agent.json
                let agent_json_url = format!(
                    "https://api.github.com/repos/{}/{}/contents/{}/projects/{}/{}/agent.json",
                    owner, REPO_NAME, org_name, project_name, agent_id
                );

                let agent_json_resp = state
                    .http_client
                    .get(&agent_json_url)
                    .header("Authorization", format!("Bearer {}", pat))
                    .header("User-Agent", "cthulu-studio")
                    .header("Accept", "application/vnd.github+json")
                    .send()
                    .await;

                let agent_json_content = match agent_json_resp {
                    Ok(resp) if resp.status().is_success() => {
                        let file_info: serde_json::Value = resp.json().await.unwrap_or_default();
                        match file_info["content"].as_str() {
                            Some(b64) => {
                                let cleaned = b64.replace('\n', "").replace('\r', "");
                                match base64::engine::general_purpose::STANDARD.decode(&cleaned) {
                                    Ok(bytes) => String::from_utf8(bytes).ok(),
                                    Err(_) => None,
                                }
                            }
                            None => None,
                        }
                    }
                    _ => None,
                };

                let agent_json_content = match agent_json_content {
                    Some(c) => c,
                    None => {
                        tracing::warn!(
                            "Failed to download agent.json for {}/{}/{}",
                            org_name,
                            project_name,
                            agent_id
                        );
                        continue;
                    }
                };

                // Download prompt.md
                let prompt_url = format!(
                    "https://api.github.com/repos/{}/{}/contents/{}/projects/{}/{}/prompt.md",
                    owner, REPO_NAME, org_name, project_name, agent_id
                );

                let prompt_resp = state
                    .http_client
                    .get(&prompt_url)
                    .header("Authorization", format!("Bearer {}", pat))
                    .header("User-Agent", "cthulu-studio")
                    .header("Accept", "application/vnd.github+json")
                    .send()
                    .await;

                let prompt_content = match prompt_resp {
                    Ok(resp) if resp.status().is_success() => {
                        let file_info: serde_json::Value = resp.json().await.unwrap_or_default();
                        match file_info["content"].as_str() {
                            Some(b64) => {
                                let cleaned = b64.replace('\n', "").replace('\r', "");
                                match base64::engine::general_purpose::STANDARD.decode(&cleaned) {
                                    Ok(bytes) => String::from_utf8(bytes).ok(),
                                    Err(_) => None,
                                }
                            }
                            None => None,
                        }
                    }
                    _ => None,
                };

                let prompt_content = prompt_content.unwrap_or_default();

                // Save files locally
                let _ = std::fs::write(local_agent_dir.join("agent.json"), &agent_json_content);
                let _ = std::fs::write(local_agent_dir.join("prompt.md"), &prompt_content);

                // Parse agent.json and merge prompt + project
                let mut agent: cthulu::agents::Agent = match serde_json::from_str(&agent_json_content) {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse agent.json for {}/{}/{}: {}",
                            org_name,
                            project_name,
                            agent_id,
                            e
                        );
                        continue;
                    }
                };

                agent.prompt = prompt_content;
                agent.project = Some(project_name.to_string());

                // Save to runtime store
                if let Err(e) = state.agent_repo.save(agent).await {
                    tracing::warn!(
                        "Failed to save synced agent {}/{}/{}: {}",
                        org_name,
                        project_name,
                        agent_id,
                        e
                    );
                    continue;
                }

                synced_count += 1;
            }
        }
    }

    // Reload all agents from disk to ensure consistency
    if let Err(e) = state.agent_repo.load_all().await {
        tracing::warn!("Failed to reload agent repository after sync: {}", e);
    }

    Ok(json!({
        "ok": true,
        "synced": synced_count,
        "projects": project_names,
    }))
}
