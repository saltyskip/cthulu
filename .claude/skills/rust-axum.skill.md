---
name: rust-axum
description: Use when working on the Rust backend -- Axum 0.8 HTTP server, Tokio async runtime, SSE streaming, process management, and serde serialization.
---

# Rust / Axum Backend Patterns

## When to Apply

- Adding or modifying API routes in `src/server/flow_routes.rs`
- Working with async process management (`LiveClaudeProcess`)
- Implementing SSE streaming endpoints
- Modifying `AppState` or session persistence

## Axum 0.8 Essentials

**Path parameters** use braces, not colons:

```rust
// Correct
.route("/flows/{id}/nodes/{node_id}/interact", post(interact_node))

// Wrong (Express-style)
.route("/flows/:id/nodes/:node_id/interact", post(interact_node))
```

**Handler signature** -- extractors are positional:

```rust
async fn handler(
    State(state): State<AppState>,           // Always first
    Path((id, node_id)): Path<(String, String)>,  // Path params
    Json(body): Json<MyRequest>,             // Request body last
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // ...
}
```

**AppState must derive Clone** -- Axum requires it. All fields should be `Arc<...>` or inherently Clone types.

## SSE Streaming with async_stream

```rust
let stream = async_stream::stream! {
    // All streaming logic lives inside this block
    yield Ok(Event::default().event("text").data("hello"));
};
// NOTHING between the closing }; and the return -- variables are out of scope
Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
```

## Tokio Mutex vs std Mutex

- **`tokio::sync::Mutex`** -- Use in async contexts (holds across `.await` points)
- **`std::sync::Mutex`** -- Use only when lock is held briefly with no `.await` inside

```rust
// AppState uses tokio::sync::Mutex for live_processes
pub live_processes: Arc<tokio::sync::Mutex<HashMap<String, LiveClaudeProcess>>>,
```

## Process Management

**Never derive Clone** on structs with `ChildStdin`, `Child`, or `mpsc::Receiver`:

```rust
// Correct -- no Clone derive
pub struct LiveClaudeProcess {
    pub stdin: tokio::process::ChildStdin,
    pub stdout_lines: tokio::sync::mpsc::UnboundedReceiver<String>,
    pub child: tokio::process::Child,
    pub busy: bool,
}
```

**Single lock pattern** for reading process channels:

```rust
let (line, stderr_batch) = {
    let mut pool = live_processes.lock().await;
    if let Some(proc) = pool.get_mut(&key) {
        let mut errs = Vec::new();
        while let Ok(err) = proc.stderr_lines.try_recv() {
            errs.push(err);
        }
        (proc.stdout_lines.try_recv().ok(), errs)
    } else { break; }
};
// Yield outside the lock
for err in stderr_batch {
    yield Ok(Event::default().event("stderr").data(err));
}
```

## Session Persistence

Sessions stored in `sessions.yaml` via atomic write (temp file + rename):

```rust
let tmp = path.with_extension("yaml.tmp");
std::fs::write(&tmp, yaml_str)?;
std::fs::rename(&tmp, path)?;
```

## Error Response Pattern

```rust
Err((StatusCode::NOT_FOUND, Json(json!({"error": "flow not found"}))))
```
