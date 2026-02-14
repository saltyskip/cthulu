pub mod context;
pub mod executors;
pub mod triggers;

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::config::{ExecutorType, TaskConfig};
use crate::github::client::{GithubClient, HttpGithubClient};
use crate::tasks::executors::claude_code::ClaudeCodeExecutor;
use crate::tasks::executors::Executor;
use crate::tasks::triggers::github::GithubPrTrigger;

pub struct TaskState {
    pub reviews_completed: Mutex<u64>,
    pub active_reviews: Mutex<u64>,
    pub seen_prs: Mutex<std::collections::HashMap<String, HashSet<u64>>>,
}

impl TaskState {
    pub fn new() -> Self {
        Self {
            reviews_completed: Mutex::new(0),
            active_reviews: Mutex::new(0),
            seen_prs: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

pub async fn spawn_task(
    task: TaskConfig,
    github_token: Option<String>,
    http_client: Arc<reqwest::Client>,
    task_state: Arc<TaskState>,
) {
    if let Some(gh_config) = &task.trigger.github {
        let Some(token) = github_token else {
            tracing::warn!(task = %task.name, "GitHub trigger configured but no GITHUB_TOKEN set -- skipping");
            return;
        };

        let prompt_template = match std::fs::read_to_string(&task.prompt) {
            Ok(content) => content,
            Err(e) => {
                tracing::error!(task = %task.name, prompt = %task.prompt, error = %e, "Failed to read prompt file");
                return;
            }
        };

        let executor: Box<dyn Executor> = match task.executor {
            ExecutorType::ClaudeCode => Box::new(ClaudeCodeExecutor::new(task.permissions.clone())),
            ExecutorType::ClaudeApi => {
                tracing::error!(task = %task.name, "claude-api executor not yet implemented");
                return;
            }
        };

        let github_client: Arc<dyn GithubClient> = Arc::new(HttpGithubClient::new(
            (*http_client).clone(),
            token,
        ));

        let trigger = GithubPrTrigger::new(
            github_client,
            gh_config.clone(),
            task_state.clone(),
        );

        tracing::info!(task = %task.name, "Starting GitHub PR trigger");

        if let Err(e) = trigger.seed().await {
            tracing::error!(task = %task.name, error = %e, "Failed to seed GitHub trigger");
            return;
        }

        let task_name = task.name.clone();
        tokio::spawn(async move {
            trigger
                .poll_loop(
                    &task_name,
                    &prompt_template,
                    executor.as_ref(),
                    &task_state,
                )
                .await;
        });
    } else if task.trigger.cron.is_some() {
        tracing::warn!(task = %task.name, "Cron trigger not yet implemented -- skipping");
    } else if task.trigger.webhook.is_some() {
        tracing::warn!(task = %task.name, "Webhook trigger not yet implemented -- skipping");
    } else {
        tracing::warn!(task = %task.name, "No trigger configured -- skipping");
    }
}
