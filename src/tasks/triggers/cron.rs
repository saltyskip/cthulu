use std::collections::HashMap;

use anyhow::{Context, Result};
use chrono::Utc;
use croner::Cron;

use crate::config::CronTriggerConfig;
use crate::tasks::context::render_prompt;
use crate::tasks::executors::Executor;

/// A trigger that fires on a cron schedule.
///
/// Uses the `croner` crate to parse standard cron expressions (5-field)
/// and calculates sleep durations between executions. Each tick renders
/// the prompt template with timing context variables and invokes the
/// configured executor.
///
/// ## Supported cron format
///
/// ```text
/// * * * * *
/// | | | | |
/// | | | | +-- Day of week (0-7, SUN-SAT)
/// | | | +---- Month (1-12, JAN-DEC)
/// | | +------ Day of month (1-31)
/// | +-------- Hour (0-23)
/// +---------- Minute (0-59)
/// ```
///
/// ## Context variables injected into prompts
///
/// | Variable        | Description                          |
/// |-----------------|--------------------------------------|
/// | `{{timestamp}}` | ISO 8601 UTC time of execution       |
/// | `{{schedule}}`  | The cron expression from config      |
/// | `{{task_name}}` | Name of the task being executed       |
///
#[derive(Debug)]
pub struct CronTrigger {
    config: CronTriggerConfig,
    cron: Cron,
}

impl CronTrigger {
    /// Parse the cron expression from config and create a new trigger.
    ///
    /// Returns an error if the expression is invalid.
    pub fn new(config: CronTriggerConfig) -> Result<Self> {
        let cron = Cron::new(&config.schedule)
            .parse()
            .with_context(|| format!("invalid cron expression: {}", config.schedule))?;
        Ok(Self { config, cron })
    }

    /// Run the cron loop forever, sleeping until the next occurrence
    /// and executing the task each time it fires.
    pub async fn run_loop(
        &self,
        task_name: &str,
        prompt_template: &str,
        executor: &dyn Executor,
    ) {
        tracing::info!(
            task = %task_name,
            schedule = %self.config.schedule,
            working_dir = %self.config.working_dir.display(),
            "Cron trigger started"
        );

        loop {
            let now = Utc::now();

            let next = match self.cron.find_next_occurrence(&now, false) {
                Ok(next) => next,
                Err(e) => {
                    tracing::error!(
                        task = %task_name,
                        error = %e,
                        "Failed to calculate next cron occurrence, retrying in 60s"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    continue;
                }
            };

            let duration = (next - now).to_std().unwrap_or(std::time::Duration::ZERO);

            tracing::info!(
                task = %task_name,
                next = %next.format("%Y-%m-%d %H:%M:%S UTC"),
                sleep_secs = duration.as_secs(),
                "Next execution in {}",
                humanize_duration(duration)
            );

            tokio::time::sleep(duration).await;

            // Build context for template rendering
            let execution_time = Utc::now();
            let mut context = HashMap::new();
            context.insert("timestamp".to_string(), execution_time.to_rfc3339());
            context.insert("schedule".to_string(), self.config.schedule.clone());
            context.insert("task_name".to_string(), task_name.to_string());

            let rendered_prompt = render_prompt(prompt_template, &context);

            tracing::info!(task = %task_name, "Cron fired, executing task");

            match executor.execute(&rendered_prompt, &self.config.working_dir).await {
                Ok(()) => {
                    tracing::info!(task = %task_name, "Cron task completed successfully");
                }
                Err(e) => {
                    tracing::error!(
                        task = %task_name,
                        error = %e,
                        "Cron task execution failed"
                    );
                }
            }
        }
    }

    /// Returns the parsed cron expression (useful for validation/display).
    pub fn schedule(&self) -> &str {
        self.cron.as_str()
    }
}

fn humanize_duration(d: std::time::Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CronTriggerConfig;
    use std::path::PathBuf;

    fn make_config(schedule: &str) -> CronTriggerConfig {
        CronTriggerConfig {
            schedule: schedule.to_string(),
            working_dir: PathBuf::from("/tmp/test"),
        }
    }

    #[test]
    fn test_valid_cron_expression() {
        let trigger = CronTrigger::new(make_config("0 9 * * MON-FRI"));
        assert!(trigger.is_ok());
    }

    #[test]
    fn test_every_minute() {
        let trigger = CronTrigger::new(make_config("* * * * *"));
        assert!(trigger.is_ok());
    }

    #[test]
    fn test_complex_expression() {
        // Every 15 minutes during business hours on weekdays
        let trigger = CronTrigger::new(make_config("*/15 9-17 * * 1-5"));
        assert!(trigger.is_ok());
    }

    #[test]
    fn test_invalid_cron_expression() {
        let trigger = CronTrigger::new(make_config("not a cron expression"));
        assert!(trigger.is_err());
        let err = trigger.unwrap_err().to_string();
        assert!(err.contains("invalid cron expression"));
    }

    #[test]
    fn test_invalid_field_value() {
        // 61 minutes is invalid
        let trigger = CronTrigger::new(make_config("61 * * * *"));
        assert!(trigger.is_err());
    }

    #[test]
    fn test_schedule_returns_expression() {
        let trigger = CronTrigger::new(make_config("30 6 * * *")).unwrap();
        assert_eq!(trigger.schedule(), "30 6 * * *");
    }

    #[test]
    fn test_humanize_seconds() {
        assert_eq!(humanize_duration(std::time::Duration::from_secs(45)), "45s");
    }

    #[test]
    fn test_humanize_minutes() {
        assert_eq!(humanize_duration(std::time::Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn test_humanize_hours() {
        assert_eq!(humanize_duration(std::time::Duration::from_secs(7500)), "2h 5m");
    }

    #[test]
    fn test_context_variables_populated() {
        // Verify the context map we'd build has the right keys
        let mut context = HashMap::new();
        context.insert("timestamp".to_string(), "2025-01-01T09:00:00Z".to_string());
        context.insert("schedule".to_string(), "0 9 * * MON-FRI".to_string());
        context.insert("task_name".to_string(), "stale-pr-check".to_string());

        let template = "Running {{task_name}} at {{timestamp}} (schedule: {{schedule}})";
        let rendered = render_prompt(template, &context);
        assert_eq!(
            rendered,
            "Running stale-pr-check at 2025-01-01T09:00:00Z (schedule: 0 9 * * MON-FRI)"
        );
    }
}
