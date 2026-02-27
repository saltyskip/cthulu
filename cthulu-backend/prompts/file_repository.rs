use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::sync::RwLock;

use super::SavedPrompt;
use super::repository::PromptRepository;

pub struct FilePromptRepository {
    base_dir: PathBuf,
    prompts: RwLock<HashMap<String, SavedPrompt>>,
    /// Filenames written by this process — used to skip fs-watcher events for our own writes.
    self_writes: std::sync::Mutex<HashSet<String>>,
}

impl FilePromptRepository {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            prompts: RwLock::new(HashMap::new()),
            self_writes: std::sync::Mutex::new(HashSet::new()),
        }
    }

    /// Mark a filename as written by this process (for fs-watcher loop prevention).
    pub fn mark_self_write(&self, filename: &str) {
        self.self_writes.lock().unwrap().insert(filename.to_string());
    }

    /// Consume (remove) a self-write marker. Returns true if the filename was present.
    pub fn consume_self_write(&self, filename: &str) -> bool {
        self.self_writes.lock().unwrap().remove(filename)
    }

    /// Re-read a single prompt JSON file from disk into the cache. Returns the resource ID if successful.
    pub async fn reload_file(&self, filename: &str) -> Option<String> {
        let path = self.prompts_dir().join(filename);
        let content = std::fs::read_to_string(&path).ok()?;
        let prompt: SavedPrompt = serde_json::from_str(&content).ok()?;
        let id = prompt.id.clone();
        self.prompts.write().await.insert(id.clone(), prompt);
        tracing::debug!(prompt_id = %id, filename, "reloaded prompt from disk");
        Some(id)
    }

    /// Remove a prompt from the cache by filename. Returns the resource ID if it was present.
    pub async fn evict_file(&self, filename: &str) -> Option<String> {
        let id = filename.trim_end_matches(".json").to_string();
        let removed = self.prompts.write().await.remove(&id);
        if removed.is_some() {
            tracing::debug!(prompt_id = %id, filename, "evicted prompt from cache");
        }
        removed.map(|p| p.id)
    }

    fn prompts_dir(&self) -> PathBuf {
        self.base_dir.join("prompts")
    }
}

#[async_trait]
impl PromptRepository for FilePromptRepository {
    async fn list_prompts(&self) -> Vec<SavedPrompt> {
        self.prompts.read().await.values().cloned().collect()
    }

    async fn get_prompt(&self, id: &str) -> Option<SavedPrompt> {
        self.prompts.read().await.get(id).cloned()
    }

    async fn save_prompt(&self, prompt: SavedPrompt) -> Result<()> {
        let dir = self.prompts_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create prompts dir: {}", dir.display()))?;

        let filename = format!("{}.json", prompt.id);
        self.mark_self_write(&filename);
        let path = dir.join(&filename);
        let content = serde_json::to_string_pretty(&prompt)
            .context("failed to serialize prompt")?;
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write prompt file: {}", path.display()))?;

        self.prompts.write().await.insert(prompt.id.clone(), prompt);
        Ok(())
    }

    async fn delete_prompt(&self, id: &str) -> Result<bool> {
        let filename = format!("{id}.json");
        self.mark_self_write(&filename);
        let path = self.prompts_dir().join(&filename);
        let existed = self.prompts.write().await.remove(id).is_some();

        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to delete prompt file: {}", path.display()))?;
        }

        Ok(existed)
    }

    async fn load_all(&self) -> Result<()> {
        let prompts_dir = self.prompts_dir();
        std::fs::create_dir_all(&prompts_dir)
            .with_context(|| format!("failed to create prompts dir: {}", prompts_dir.display()))?;

        let mut loaded_prompts = HashMap::new();
        let prompt_entries = std::fs::read_dir(&prompts_dir)
            .with_context(|| format!("failed to read prompts dir: {}", prompts_dir.display()))?;

        for entry in prompt_entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read prompt file: {}", path.display()))?;
            match serde_json::from_str::<SavedPrompt>(&content) {
                Ok(prompt) => {
                    loaded_prompts.insert(prompt.id.clone(), prompt);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Skipping invalid prompt file");
                }
            }
        }

        let prompt_count = loaded_prompts.len();
        if prompt_count > 0 {
            tracing::info!(count = prompt_count, "Loaded saved prompts");
        }
        *self.prompts.write().await = loaded_prompts;

        Ok(())
    }
}
