use std::path::PathBuf;
use std::sync::Arc;

use notify_debouncer_mini::{DebouncedEventKind, new_debouncer};
use tokio::sync::broadcast;

use crate::agents::file_repository::FileAgentRepository;
use crate::api::changes::{ChangeType, ResourceChangeEvent, ResourceType};
use crate::flows::file_repository::FileFlowRepository;
use crate::prompts::file_repository::FilePromptRepository;

/// Watches `~/.cthulu/{flows,agents,prompts}/` for external file changes
/// and updates the in-memory caches + emits change events.
pub struct FileChangeWatcher {
    /// Keep the debouncer alive — dropping it stops the watcher.
    _debouncer: notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
}

impl FileChangeWatcher {
    /// Start watching the three resource directories under `base_dir`.
    /// Returns a `FileChangeWatcher` whose lifetime controls the watcher thread.
    pub fn start(
        base_dir: PathBuf,
        flow_repo: Arc<FileFlowRepository>,
        agent_repo: Arc<FileAgentRepository>,
        prompt_repo: Arc<FilePromptRepository>,
        changes_tx: broadcast::Sender<ResourceChangeEvent>,
    ) -> anyhow::Result<Self> {
        let flows_dir = base_dir.join("flows");
        let agents_dir = base_dir.join("agents");
        let prompts_dir = base_dir.join("prompts");

        // Ensure dirs exist
        std::fs::create_dir_all(&flows_dir)?;
        std::fs::create_dir_all(&agents_dir)?;
        std::fs::create_dir_all(&prompts_dir)?;

        let rt = tokio::runtime::Handle::current();

        let mut debouncer = new_debouncer(
            std::time::Duration::from_millis(500),
            move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                let events = match events {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!(error = %e, "fs watcher error");
                        return;
                    }
                };

                for event in events {
                    if event.kind != DebouncedEventKind::Any {
                        continue;
                    }

                    let path = &event.path;

                    // Extract filename
                    let filename = match path.file_name().and_then(|f| f.to_str()) {
                        Some(f) => f.to_string(),
                        None => continue,
                    };

                    // Skip non-JSON and temp files
                    if !filename.ends_with(".json") || filename.ends_with(".json.tmp") {
                        continue;
                    }

                    // Determine resource type from parent directory name
                    let parent_name = path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|f| f.to_str())
                        .unwrap_or("");

                    let resource_type = match parent_name {
                        "flows" => ResourceType::Flow,
                        "agents" => ResourceType::Agent,
                        "prompts" => ResourceType::Prompt,
                        _ => continue,
                    };

                    // Clone what we need for the async block
                    let flow_repo = flow_repo.clone();
                    let agent_repo = agent_repo.clone();
                    let prompt_repo = prompt_repo.clone();
                    let changes_tx = changes_tx.clone();
                    let path = path.clone();
                    let filename = filename.clone();

                    rt.spawn(async move {
                        // Check self-write flag
                        let is_self_write = match resource_type {
                            ResourceType::Flow => flow_repo.consume_self_write(&filename),
                            ResourceType::Agent => agent_repo.consume_self_write(&filename),
                            ResourceType::Prompt => prompt_repo.consume_self_write(&filename),
                        };

                        if is_self_write {
                            tracing::trace!(filename, ?resource_type, "skipping self-write");
                            return;
                        }

                        let file_exists = path.exists();

                        let (change_type, resource_id) = if file_exists {
                            // File exists — reload into cache
                            let id = match resource_type {
                                ResourceType::Flow => flow_repo.reload_file(&filename).await,
                                ResourceType::Agent => agent_repo.reload_file(&filename).await,
                                ResourceType::Prompt => prompt_repo.reload_file(&filename).await,
                            };
                            match id {
                                Some(id) => (ChangeType::Updated, id),
                                None => return, // couldn't parse, skip
                            }
                        } else {
                            // File deleted — evict from cache
                            let id = match resource_type {
                                ResourceType::Flow => flow_repo.evict_file(&filename).await,
                                ResourceType::Agent => agent_repo.evict_file(&filename).await,
                                ResourceType::Prompt => prompt_repo.evict_file(&filename).await,
                            };
                            match id {
                                Some(id) => (ChangeType::Deleted, id),
                                None => return, // wasn't in cache, skip
                            }
                        };

                        let event = ResourceChangeEvent {
                            resource_type,
                            change_type,
                            resource_id: resource_id.clone(),
                            timestamp: chrono::Utc::now(),
                        };

                        tracing::info!(
                            ?resource_type,
                            ?change_type,
                            resource_id = %resource_id,
                            "external file change detected"
                        );

                        // Ignore send errors (no subscribers)
                        let _ = changes_tx.send(event);
                    });
                }
            },
        )?;

        // Watch the three directories (non-recursive)
        use notify::RecursiveMode;
        debouncer.watcher().watch(&flows_dir, RecursiveMode::NonRecursive)?;
        debouncer.watcher().watch(&agents_dir, RecursiveMode::NonRecursive)?;
        debouncer.watcher().watch(&prompts_dir, RecursiveMode::NonRecursive)?;

        tracing::info!(
            flows = %flows_dir.display(),
            agents = %agents_dir.display(),
            prompts = %prompts_dir.display(),
            "file change watcher started"
        );

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}
