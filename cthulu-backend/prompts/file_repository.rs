use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::sync::RwLock;

use super::SavedPrompt;
use super::repository::PromptRepository;

pub struct FilePromptRepository {
    base_dir: PathBuf,
    prompts: RwLock<HashMap<String, SavedPrompt>>,
}

impl FilePromptRepository {
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self {
            base_dir: base_dir.as_ref().to_path_buf(),
            prompts: RwLock::new(HashMap::new()),
        }
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

        let path = dir.join(format!("{}.json", prompt.id));
        let content = serde_json::to_string_pretty(&prompt)
            .context("failed to serialize prompt")?;
        std::fs::write(&path, content)
            .with_context(|| format!("failed to write prompt file: {}", path.display()))?;

        self.prompts.write().await.insert(prompt.id.clone(), prompt);
        Ok(())
    }

    async fn delete_prompt(&self, id: &str) -> Result<bool> {
        let path = self.prompts_dir().join(format!("{id}.json"));
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
