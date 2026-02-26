/// REST endpoints for the template gallery.
///
/// GET  /api/templates                         — list all templates (metadata + raw YAML)
/// GET  /api/templates/{cat}/{slug}             — get raw YAML for a single template
/// POST /api/templates/{cat}/{slug}/import      — parse YAML → Flow, save, return Flow
/// POST /api/templates/import-yaml             — parse raw YAML body → Flow, save, return Flow
/// POST /api/templates/import-github           — fetch all workflow YAMLs from a GitHub repo,
///                                               import each one, return array of imported Flows
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::api::AppState;
use crate::templates;

use super::repository::TemplateRepository;

/// List all templates across all categories.
/// Returns an array of `TemplateMetadata` objects.
pub(crate) async fn list_templates(State(state): State<AppState>) -> impl IntoResponse {
    let repo = TemplateRepository::new(state.flow_repo.clone(), state.static_dir.clone());
    let templates = repo.list_templates();
    Json(json!({ "templates": templates }))
}

/// Return the raw YAML for a single template.
pub(crate) async fn get_template_yaml(
    State(state): State<AppState>,
    Path((category, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    let repo = TemplateRepository::new(state.flow_repo.clone(), state.static_dir.clone());

    match repo.get_template_yaml(&category, &slug) {
        Ok(yaml) => (
            StatusCode::OK,
            [("content-type", "text/yaml; charset=utf-8")],
            yaml,
        )
            .into_response(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
            StatusCode::NOT_FOUND,
            [("content-type", "application/json")],
            json!({ "error": format!("template not found: {category}/{slug}") }).to_string(),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "application/json")],
            json!({ "error": e.to_string() }).to_string(),
        )
            .into_response(),
    }
}

/// Parse the template YAML into a Flow, persist it, and return the new Flow.
/// The imported flow is always set to `enabled: false` (safe default).
pub(crate) async fn import_template(
    State(state): State<AppState>,
    Path((category, slug)): Path<(String, String)>,
) -> impl IntoResponse {
    let repo = TemplateRepository::new(state.flow_repo.clone(), state.static_dir.clone());

    let yaml = match repo.get_template_yaml(&category, &slug) {
        Ok(y) => y,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": format!("template not found: {category}/{slug}") })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let flow = match templates::parse_template_yaml(&yaml) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("failed to parse template: {e}") })),
            )
                .into_response();
        }
    };

    match repo.save_imported_flow(flow.clone()).await {
        Ok(_) => {
            // Start scheduler for the new flow (it's disabled, but register it)
            let _ = state.scheduler.restart_flow(&flow.id).await;
            tracing::info!(
                flow_id = %flow.id,
                flow_name = %flow.name,
                template = %format!("{category}/{slug}"),
                "imported template as new flow"
            );
            Json(json!(flow)).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save flow: {e}") })),
        )
            .into_response(),
    }
}

// ── Body types ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub(crate) struct ImportYamlBody {
    /// Raw YAML content of the workflow file.
    yaml: String,
}

#[derive(Deserialize)]
pub(crate) struct ImportGithubBody {
    /// GitHub repo URL, e.g. "https://github.com/owner/repo" or
    /// "https://github.com/owner/repo/tree/main/workflows".
    /// We normalise to the GitHub Contents API automatically.
    repo_url: String,
    /// Optional sub-path within the repo to scan for YAML files (default: root).
    #[serde(default)]
    path: String,
    /// Optional branch/tag/sha (default: "main").
    #[serde(default)]
    branch: String,
}

// ── Handlers ───────────────────────────────────────────────────────────────

/// POST /api/templates/import-yaml
/// Body: `{ "yaml": "<raw YAML string>" }`
/// Parses the YAML as a workflow, saves it as a new disabled Flow, returns the Flow.
pub(crate) async fn import_yaml(
    State(state): State<AppState>,
    Json(body): Json<ImportYamlBody>,
) -> impl IntoResponse {
    let repo = TemplateRepository::new(state.flow_repo.clone(), state.static_dir.clone());

    if body.yaml.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "yaml field is required and must not be empty" })),
        )
            .into_response();
    }

    let flow = match templates::parse_template_yaml(&body.yaml) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({ "error": format!("failed to parse YAML: {e}") })),
            )
                .into_response();
        }
    };

    match repo.save_imported_flow(flow.clone()).await {
        Ok(_) => {
            let _ = state.scheduler.restart_flow(&flow.id).await;
            tracing::info!(flow_id = %flow.id, flow_name = %flow.name, "imported flow from uploaded YAML");
            Json(json!({ "flows": [flow] })).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("failed to save flow: {e}") })),
        )
            .into_response(),
    }
}

/// POST /api/templates/import-github
/// Body: `{ "repo_url": "https://github.com/owner/repo", "path": "", "branch": "main" }`
///
/// Uses the GitHub Contents API (no auth required for public repos) to list files,
/// then fetches every `.yaml` / `.yml` file and imports each as a new disabled Flow.
/// Returns `{ "flows": [...], "errors": [...] }`.
pub(crate) async fn import_github(
    State(state): State<AppState>,
    Json(body): Json<ImportGithubBody>,
) -> impl IntoResponse {
    let repo = TemplateRepository::new(state.flow_repo.clone(), state.static_dir.clone());
    let branch = if body.branch.is_empty() { "main".to_string() } else { body.branch.clone() };

    // Parse the GitHub URL into (owner, repo, sub_path).
    // Accepted formats:
    //   https://github.com/owner/repo
    //   https://github.com/owner/repo/tree/branch/path/to/dir
    //   github.com/owner/repo
    let url = body.repo_url.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("github.com/")
        .trim_end_matches('/');

    let parts: Vec<&str> = url.splitn(5, '/').collect();
    if parts.len() < 2 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid GitHub URL — expected https://github.com/owner/repo" })),
        ).into_response();
    }

    let owner = parts[0];
    let gh_repo = parts[1];

    // If the URL contains /tree/branch/sub_path, extract sub_path
    let url_sub_path = if parts.len() >= 5 && parts[2] == "tree" {
        parts[4].to_string() // e.g. "workflows"
    } else {
        String::new()
    };

    let sub_path = if !body.path.is_empty() {
        body.path.trim_matches('/').to_string()
    } else {
        url_sub_path
    };

    // Recursively fetch all YAML files from the GitHub Contents API
    let yaml_files = match super::repository::fetch_github_yaml_files(
        &state.http_client,
        owner,
        gh_repo,
        &sub_path,
        &branch,
    ).await {
        Ok(files) => files,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": format!("failed to fetch GitHub repo: {e}") })),
            ).into_response();
        }
    };

    if yaml_files.is_empty() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "no .yaml or .yml files found in the specified path" })),
        ).into_response();
    }

    let mut imported_flows: Vec<serde_json::Value> = Vec::new();
    let mut errors: Vec<serde_json::Value> = Vec::new();

    for (filename, yaml_content) in &yaml_files {
        match templates::parse_template_yaml(yaml_content) {
            Ok(flow) => {
                match repo.save_imported_flow(flow.clone()).await {
                    Ok(_) => {
                        let _ = state.scheduler.restart_flow(&flow.id).await;
                        tracing::info!(
                            flow_id = %flow.id,
                            flow_name = %flow.name,
                            file = %filename,
                            "imported flow from GitHub"
                        );
                        imported_flows.push(json!(flow));
                    }
                    Err(e) => {
                        errors.push(json!({ "file": filename, "error": format!("save failed: {e}") }));
                    }
                }
            }
            Err(e) => {
                errors.push(json!({ "file": filename, "error": format!("parse failed: {e}") }));
            }
        }
    }

    Json(json!({
        "flows": imported_flows,
        "errors": errors,
        "total_found": yaml_files.len(),
        "imported": imported_flows.len(),
    })).into_response()
}
