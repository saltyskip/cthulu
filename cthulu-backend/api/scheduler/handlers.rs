use axum::extract::{Path, State};
use axum::Json;
use hyper::StatusCode;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::api::AppState;

use super::repository::SchedulerRepository;

/// GET /flows/{id}/schedule — compute next run time for a flow's trigger
pub(crate) async fn get_schedule(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let repo = SchedulerRepository::new(state.flow_repo.clone(), state.scheduler.clone());
    let flow = repo.get_flow(&id).await.ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(json!({ "error": "flow not found" })))
    })?;

    let trigger_node = flow.nodes.iter().find(|n| n.node_type == crate::flows::NodeType::Trigger);

    let Some(trigger) = trigger_node else {
        return Ok(Json(json!({
            "flow_id": id,
            "trigger_kind": null,
            "next_run": null,
            "schedule": null,
        })));
    };

    let trigger_kind = trigger.kind.as_str();

    match trigger_kind {
        "cron" => {
            let schedule = trigger.config.get("schedule")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if schedule.is_empty() {
                return Ok(Json(json!({
                    "flow_id": id,
                    "trigger_kind": "cron",
                    "schedule": "",
                    "next_run": null,
                    "error": "no schedule configured",
                })));
            }

            match croner::Cron::new(schedule).parse() {
                Ok(cron) => {
                    let now = chrono::Utc::now();
                    let next = cron.find_next_occurrence(&now, false).ok();
                    let next_runs: Vec<String> = {
                        let mut runs = Vec::new();
                        let mut cursor = now;
                        for _ in 0..5 {
                            if let Ok(n) = cron.find_next_occurrence(&cursor, false) {
                                runs.push(n.to_rfc3339());
                                cursor = n + chrono::Duration::seconds(1);
                            } else {
                                break;
                            }
                        }
                        runs
                    };

                    Ok(Json(json!({
                        "flow_id": id,
                        "trigger_kind": "cron",
                        "enabled": flow.enabled,
                        "schedule": schedule,
                        "next_run": next.map(|n| n.to_rfc3339()),
                        "next_runs": next_runs,
                    })))
                }
                Err(e) => {
                    Ok(Json(json!({
                        "flow_id": id,
                        "trigger_kind": "cron",
                        "schedule": schedule,
                        "next_run": null,
                        "error": format!("invalid cron: {e}"),
                    })))
                }
            }
        }
        "github-pr" => {
            let poll_interval = trigger.config.get("poll_interval")
                .and_then(|v| v.as_u64())
                .unwrap_or(60);
            Ok(Json(json!({
                "flow_id": id,
                "trigger_kind": "github-pr",
                "enabled": flow.enabled,
                "poll_interval_secs": poll_interval,
                "next_run": null,
            })))
        }
        other => {
            Ok(Json(json!({
                "flow_id": id,
                "trigger_kind": other,
                "enabled": flow.enabled,
                "next_run": null,
            })))
        }
    }
}

/// GET /scheduler/status — show which flows have active scheduler tasks
pub(crate) async fn scheduler_status(
    State(state): State<AppState>,
) -> Json<Value> {
    let repo = SchedulerRepository::new(state.flow_repo.clone(), state.scheduler.clone());
    let active_ids = repo.active_flow_ids().await;
    let flows = repo.list_flows().await;

    let flow_statuses: Vec<Value> = flows.iter().map(|f| {
        let is_active = active_ids.contains(&f.id);
        json!({
            "flow_id": f.id,
            "name": f.name,
            "enabled": f.enabled,
            "scheduler_active": is_active,
        })
    }).collect();

    Json(json!({
        "active_count": active_ids.len(),
        "total_flows": flows.len(),
        "flows": flow_statuses,
    }))
}

#[derive(Deserialize)]
pub(crate) struct ValidateCronRequest {
    expression: String,
}

/// POST /validate/cron — validate a cron expression and return next 5 fire times
pub(crate) async fn validate_cron(
    Json(body): Json<ValidateCronRequest>,
) -> Json<Value> {
    let expr = body.expression.trim();

    if expr.is_empty() {
        return Json(json!({
            "valid": false,
            "error": "empty expression",
            "next_runs": [],
        }));
    }

    match croner::Cron::new(expr).parse() {
        Ok(cron) => {
            let now = chrono::Utc::now();
            let mut next_runs = Vec::new();
            let mut cursor = now;
            for _ in 0..5 {
                match cron.find_next_occurrence(&cursor, false) {
                    Ok(n) => {
                        next_runs.push(n.to_rfc3339());
                        cursor = n + chrono::Duration::seconds(1);
                    }
                    Err(_) => break,
                }
            }

            Json(json!({
                "valid": true,
                "expression": expr,
                "next_runs": next_runs,
            }))
        }
        Err(e) => {
            Json(json!({
                "valid": false,
                "expression": expr,
                "error": format!("{e}"),
                "next_runs": [],
            }))
        }
    }
}
