//! Tauri event bridge — replaces SSE streams with native Tauri event emission.
//!
//! The backend uses `tokio::sync::broadcast` channels for streaming data to
//! connected clients. In web mode, axum SSE handlers subscribe and stream to
//! the browser. In the Tauri desktop app, we instead spawn background tokio
//! tasks that subscribe to the same channels and emit Tauri events, which the
//! frontend receives via `listen()` / `onEvent()`.
//!
//! The emitted payloads are identical to what the SSE handlers send, so the
//! frontend parsing logic stays unchanged.

use tauri::Emitter;
use tokio::sync::broadcast;

/// Spawn long-lived background tasks that bridge each global broadcast channel
/// to Tauri events.
///
/// Call this once after creating the `AppState` (and the Tauri `AppHandle` is
/// available). Each bridge runs as an independent `tokio::spawn` task that
/// loops forever until the corresponding broadcast channel is closed.
///
/// Per-session streams (chat events, session logs) are handled directly by
/// the Tauri command layer when a session begins.
pub fn start_event_bridges(app_handle: tauri::AppHandle, state: &cthulu::api::AppState) {
    // Bridge 1: Resource changes (flow/agent/prompt CRUD notifications)
    //
    // SSE equivalent: GET /api/changes  (cthulu::api::changes::stream_changes)
    // The SSE handler emits named events like "flow_change", "agent_change",
    // "prompt_change" with the serialized ResourceChangeEvent as data.
    // We emit a single "resource-change" Tauri event with the full event object
    // so the frontend can dispatch by resource_type.
    {
        let mut rx = state.changes_tx.subscribe();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let _ = handle.emit("resource-change", &event);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(lagged = n, "resource-change bridge lagged");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("resource-change bridge: channel closed");
                        break;
                    }
                }
            }
        });
    }

    // Bridge 2: Flow run events (run_started, node_started, node_completed, etc.)
    //
    // SSE equivalent: GET /api/flows/{id}/runs/stream  (cthulu::api::flows::handlers::stream_runs)
    // The SSE handler filters by flow_id and emits named events (e.g. "run_started").
    // We emit ALL run events on a single "run-event" Tauri event — the frontend
    // can filter by flow_id client-side. This avoids needing to spawn a new
    // bridge per-flow.
    {
        let mut rx = state.events_tx.subscribe();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let _ = handle.emit("run-event", &event);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(lagged = n, "run-event bridge lagged");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("run-event bridge: channel closed");
                        break;
                    }
                }
            }
        });
    }

    // Bridge 3: Hook events (permission requests, file changes, stop signals)
    //
    // SSE equivalent: GET /api/hooks/stream  (cthulu::api::hooks::routes::global_hook_stream)
    // The SSE handler parses each String as JSON, extracts a "type" field for
    // the SSE event name, and sends the raw JSON as data. We emit the raw JSON
    // string on the "hook-event" Tauri event — the frontend already knows how
    // to parse the JSON and dispatch by type.
    {
        let mut rx = state.global_hook_tx.subscribe();
        let handle = app_handle.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(data) => {
                        let _ = handle.emit("hook-event", &data);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(lagged = n, "hook-event bridge lagged");
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("hook-event bridge: channel closed");
                        break;
                    }
                }
            }
        });
    }
}
