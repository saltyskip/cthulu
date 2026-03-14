use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use notify_debouncer_mini::{DebouncedEventKind, new_debouncer};
use tokio::sync::broadcast;

use crate::agents::file_repository::FileAgentRepository;
use crate::api::changes::{ChangeType, ResourceChangeEvent, ResourceType};
use crate::flows::file_repository::FileFlowRepository;
use crate::prompts::file_repository::FilePromptRepository;

/// Self-write guard for workflow YAML files.
/// When the UI saves a workflow, we register the path here so the watcher
/// can skip the event and avoid a re-fetch loop.
pub struct WorkflowSelfWrites {
    paths: std::sync::Mutex<HashSet<PathBuf>>,
}

impl WorkflowSelfWrites {
    pub fn new() -> Self {
        Self {
            paths: std::sync::Mutex::new(HashSet::new()),
        }
    }

    /// Register a path as a self-write (call before writing the file).
    pub fn mark(&self, path: PathBuf) {
        if let Ok(mut set) = self.paths.lock() {
            set.insert(path);
        }
    }

    /// Consume a self-write flag. Returns `true` if this was a self-write.
    pub fn consume(&self, path: &PathBuf) -> bool {
        if let Ok(mut set) = self.paths.lock() {
            set.remove(path)
        } else {
            false
        }
    }
}

/// Watches `~/.cthulu/{flows,agents,prompts,cthulu-workflows/}/` for external
/// file changes and updates the in-memory caches + emits change events.
pub struct FileChangeWatcher {
    /// Keep the debouncer alive — dropping it stops the watcher.
    _debouncer: notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
}

impl FileChangeWatcher {
    /// Start watching the resource directories under `base_dir`.
    /// Returns a `FileChangeWatcher` whose lifetime controls the watcher thread.
    pub fn start(
        base_dir: PathBuf,
        flow_repo: Arc<FileFlowRepository>,
        agent_repo: Arc<FileAgentRepository>,
        prompt_repo: Arc<FilePromptRepository>,
        changes_tx: broadcast::Sender<ResourceChangeEvent>,
        workflow_self_writes: Arc<WorkflowSelfWrites>,
    ) -> anyhow::Result<Self> {
        let flows_dir = base_dir.join("flows");
        let agents_dir = base_dir.join("agents");
        let prompts_dir = base_dir.join("prompts");
        let workflows_dir = base_dir.join("cthulu-workflows");

        // Ensure dirs exist
        std::fs::create_dir_all(&flows_dir)?;
        std::fs::create_dir_all(&agents_dir)?;
        std::fs::create_dir_all(&prompts_dir)?;
        std::fs::create_dir_all(&workflows_dir)?;

        let rt = tokio::runtime::Handle::current();
        let workflows_dir_clone = workflows_dir.clone();

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

                    // ── Workflow YAML handling ──
                    // Path pattern: <workflows_dir>/<workspace>/<name>/workflow.yaml
                    if filename == "workflow.yaml" && !filename.ends_with(".yaml.tmp") {
                        if let Ok(relative) = path.strip_prefix(&workflows_dir_clone) {
                            let components: Vec<_> = relative
                                .components()
                                .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
                                .collect();

                            // Expect: [workspace, workflow_name, "workflow.yaml"]
                            if components.len() == 3 {
                                let workspace = components[0].clone();
                                let wf_name = components[1].clone();
                                let resource_id = format!("{workspace}::{wf_name}");
                                let changes_tx = changes_tx.clone();
                                let self_writes = workflow_self_writes.clone();
                                let path_owned = path.to_path_buf();

                                rt.spawn(async move {
                                    // Check self-write flag
                                    if self_writes.consume(&path_owned) {
                                        tracing::trace!(
                                            resource_id = %resource_id,
                                            "skipping workflow self-write"
                                        );
                                        return;
                                    }

                                    // Determine change type: file exists → Updated, gone → Deleted
                                    let change_type = if path_owned.exists() {
                                        ChangeType::Updated
                                    } else {
                                        ChangeType::Deleted
                                    };

                                    let event = ResourceChangeEvent {
                                        resource_type: ResourceType::Workflow,
                                        change_type,
                                        resource_id: resource_id.clone(),
                                        timestamp: chrono::Utc::now(),
                                    };

                                    tracing::info!(
                                        resource_type = ?ResourceType::Workflow,
                                        ?change_type,
                                        resource_id = %resource_id,
                                        "external workflow file change detected"
                                    );

                                    let _ = changes_tx.send(event);
                                });
                            }
                        }
                        continue;
                    }

                    // ── JSON resource handling (flows, agents, prompts) ──
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
                    let filename = filename.clone();

                    rt.spawn(async move {
                        // Check self-write flag
                        let is_self_write = match resource_type {
                            ResourceType::Flow => flow_repo.consume_self_write(&filename),
                            ResourceType::Agent => agent_repo.consume_self_write(&filename),
                            ResourceType::Prompt => prompt_repo.consume_self_write(&filename),
                            ResourceType::Workflow => false, // handled above
                        };

                        if is_self_write {
                            tracing::trace!(filename, ?resource_type, "skipping self-write");
                            return;
                        }

                        // Try reload first; if that fails, try evict (avoids TOCTOU with path.exists())
                        let reload_id = match resource_type {
                            ResourceType::Flow => flow_repo.reload_file(&filename).await,
                            ResourceType::Agent => agent_repo.reload_file(&filename).await,
                            ResourceType::Prompt => prompt_repo.reload_file(&filename).await,
                            ResourceType::Workflow => None, // handled above
                        };

                        let (change_type, resource_id) = if let Some(id) = reload_id {
                            (ChangeType::Updated, id)
                        } else {
                            // File gone or unparseable — evict from cache
                            let evict_id = match resource_type {
                                ResourceType::Flow => flow_repo.evict_file(&filename).await,
                                ResourceType::Agent => agent_repo.evict_file(&filename).await,
                                ResourceType::Prompt => prompt_repo.evict_file(&filename).await,
                                ResourceType::Workflow => None, // handled above
                            };
                            match evict_id {
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

        // Watch the three JSON directories (non-recursive)
        use notify::RecursiveMode;
        debouncer.watcher().watch(&flows_dir, RecursiveMode::NonRecursive)?;
        debouncer.watcher().watch(&agents_dir, RecursiveMode::NonRecursive)?;
        debouncer.watcher().watch(&prompts_dir, RecursiveMode::NonRecursive)?;

        // Watch the workflows directory (recursive — nested workspace/name/workflow.yaml)
        debouncer.watcher().watch(&workflows_dir, RecursiveMode::Recursive)?;

        tracing::info!(
            flows = %flows_dir.display(),
            agents = %agents_dir.display(),
            prompts = %prompts_dir.display(),
            workflows = %workflows_dir.display(),
            "file change watcher started"
        );

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}
