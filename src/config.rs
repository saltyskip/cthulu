use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub github: Option<GithubConfig>,
    #[serde(default)]
    pub tasks: Vec<TaskConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    pub sentry_dsn_env: Option<String>,
    #[serde(default = "default_environment")]
    pub environment: String,
}

fn default_port() -> u16 {
    8081
}

fn default_environment() -> String {
    "local".to_string()
}

#[derive(Debug, Deserialize)]
pub struct GithubConfig {
    pub token_env: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TaskConfig {
    pub name: String,
    pub executor: ExecutorType,
    pub prompt: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub trigger: TriggerConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutorType {
    ClaudeCode,
    ClaudeApi,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TriggerConfig {
    pub github: Option<GithubTriggerConfig>,
    pub cron: Option<CronTriggerConfig>,
    pub webhook: Option<WebhookTriggerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubTriggerConfig {
    pub event: String,
    pub repos: Vec<RepoEntry>,
    #[serde(default = "default_poll_interval")]
    pub poll_interval: u64,
}

fn default_poll_interval() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoEntry {
    pub slug: String,
    pub path: PathBuf,
}

impl RepoEntry {
    pub fn owner_repo(&self) -> Option<(&str, &str)> {
        self.slug.split_once('/')
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CronTriggerConfig {
    pub schedule: String,
    pub working_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebhookTriggerConfig {
    pub path: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;
        let config: Config =
            toml::from_str(&content).with_context(|| "failed to parse cthulu.toml")?;
        Ok(config)
    }

    pub fn github_token(&self) -> Option<String> {
        self.github
            .as_ref()
            .and_then(|g| std::env::var(&g.token_env).ok())
            .filter(|t| !t.is_empty())
    }

    pub fn sentry_dsn(&self) -> String {
        self.server
            .sentry_dsn_env
            .as_ref()
            .and_then(|env_key| std::env::var(env_key).ok())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(toml_str: &str) -> Config {
        toml::from_str(toml_str).unwrap()
    }

    #[test]
    fn test_minimal_config() {
        let config = parse(r#"
            [server]
        "#);
        assert_eq!(config.server.port, 8081);
        assert_eq!(config.server.environment, "local");
        assert!(config.github.is_none());
        assert!(config.tasks.is_empty());
    }

    #[test]
    fn test_full_config() {
        let config = parse(r#"
            [server]
            port = 9090
            environment = "production"
            sentry_dsn_env = "MY_SENTRY"

            [github]
            token_env = "GH_TOKEN"

            [[tasks]]
            name = "pr-review"
            executor = "claude-code"
            prompt = "prompts/pr_review.md"
            permissions = ["Bash", "Read"]

            [tasks.trigger.github]
            event = "pull_request"
            repos = [
              { slug = "owner/repo", path = "/tmp/repo" },
            ]
            poll_interval = 30
        "#);
        assert_eq!(config.server.port, 9090);
        assert_eq!(config.server.environment, "production");
        assert_eq!(config.github.unwrap().token_env, "GH_TOKEN");
        assert_eq!(config.tasks.len(), 1);

        let task = &config.tasks[0];
        assert_eq!(task.name, "pr-review");
        assert_eq!(task.permissions, vec!["Bash", "Read"]);

        let gh = task.trigger.github.as_ref().unwrap();
        assert_eq!(gh.event, "pull_request");
        assert_eq!(gh.repos.len(), 1);
        assert_eq!(gh.repos[0].slug, "owner/repo");
        assert_eq!(gh.poll_interval, 30);
    }

    #[test]
    fn test_default_poll_interval() {
        let config = parse(r#"
            [server]

            [[tasks]]
            name = "test"
            executor = "claude-code"
            prompt = "test.md"

            [tasks.trigger.github]
            event = "pull_request"
            repos = [{ slug = "o/r", path = "/tmp" }]
        "#);
        let gh = config.tasks[0].trigger.github.as_ref().unwrap();
        assert_eq!(gh.poll_interval, 60);
    }

    #[test]
    fn test_cron_trigger() {
        let config = parse(r#"
            [server]

            [[tasks]]
            name = "scheduled"
            executor = "claude-code"
            prompt = "test.md"

            [tasks.trigger.cron]
            schedule = "0 9 * * MON-FRI"
            working_dir = "/tmp/project"
        "#);
        let cron = config.tasks[0].trigger.cron.as_ref().unwrap();
        assert_eq!(cron.schedule, "0 9 * * MON-FRI");
        assert_eq!(cron.working_dir, PathBuf::from("/tmp/project"));
        assert!(config.tasks[0].trigger.github.is_none());
    }

    #[test]
    fn test_webhook_trigger() {
        let config = parse(r#"
            [server]

            [[tasks]]
            name = "webhook-task"
            executor = "claude-api"
            prompt = "test.md"

            [tasks.trigger.webhook]
            path = "/hooks/deploy"
        "#);
        let wh = config.tasks[0].trigger.webhook.as_ref().unwrap();
        assert_eq!(wh.path, "/hooks/deploy");
    }

    #[test]
    fn test_multiple_tasks() {
        let config = parse(r#"
            [server]

            [[tasks]]
            name = "task-a"
            executor = "claude-code"
            prompt = "a.md"
            [tasks.trigger.cron]
            schedule = "0 0 * * *"
            working_dir = "/tmp/a"

            [[tasks]]
            name = "task-b"
            executor = "claude-api"
            prompt = "b.md"
            [tasks.trigger.webhook]
            path = "/hooks/b"
        "#);
        assert_eq!(config.tasks.len(), 2);
        assert_eq!(config.tasks[0].name, "task-a");
        assert_eq!(config.tasks[1].name, "task-b");
    }

    #[test]
    fn test_empty_permissions_default() {
        let config = parse(r#"
            [server]

            [[tasks]]
            name = "test"
            executor = "claude-code"
            prompt = "test.md"
            [tasks.trigger.cron]
            schedule = "0 0 * * *"
            working_dir = "/tmp/test"
        "#);
        assert!(config.tasks[0].permissions.is_empty());
    }

    #[test]
    fn test_repo_entry_owner_repo() {
        let entry = RepoEntry {
            slug: "bitcoin-portal/RustServer".to_string(),
            path: PathBuf::from("/tmp"),
        };
        let (owner, repo) = entry.owner_repo().unwrap();
        assert_eq!(owner, "bitcoin-portal");
        assert_eq!(repo, "RustServer");
    }

    #[test]
    fn test_repo_entry_invalid_slug() {
        let entry = RepoEntry {
            slug: "no-slash-here".to_string(),
            path: PathBuf::from("/tmp"),
        };
        assert!(entry.owner_repo().is_none());
    }

    #[test]
    fn test_invalid_toml_fails() {
        let result: Result<Config, _> = toml::from_str("not valid toml {{{}}}");
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_required_field_fails() {
        // name is required on tasks
        let result: Result<Config, _> = toml::from_str(r#"
            [server]
            [[tasks]]
            executor = "claude-code"
            prompt = "test.md"
            [tasks.trigger.cron]
            schedule = "0 0 * * *"
            working_dir = "/tmp/test"
        "#);
        assert!(result.is_err());
    }
}
