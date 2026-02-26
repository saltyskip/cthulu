use serde::Deserialize;

/// Server configuration loaded from environment variables.
pub struct Config {
    pub port: u16,
    pub sentry_dsn: Option<String>,
    pub environment: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self::from_raw_values(
            std::env::var("PORT").ok().as_deref(),
            std::env::var("SENTRY_DSN").ok().as_deref(),
            std::env::var("ENVIRONMENT").ok().as_deref(),
        )
    }

    /// Build a Config from raw string values (as they would come from env vars).
    /// Used directly in tests to avoid mutating process-global environment.
    pub fn from_raw_values(
        port: Option<&str>,
        sentry_dsn: Option<&str>,
        environment: Option<&str>,
    ) -> Self {
        let port = port.and_then(|v| v.parse().ok()).unwrap_or(8081);

        let sentry_dsn = sentry_dsn.filter(|s| !s.is_empty()).map(String::from);

        let environment = environment
            .filter(|s| !s.is_empty())
            .map(String::from)
            .unwrap_or_else(|| "local".to_string());

        Config {
            port,
            sentry_dsn,
            environment,
        }
    }
}

// --- Source and Sink types used by flow runner ---

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SourceConfig {
    Rss {
        url: String,
        #[serde(default = "default_rss_limit")]
        limit: usize,
        #[serde(default)]
        keywords: Vec<String>,
    },
    WebScrape {
        url: String,
        #[serde(default)]
        keywords: Vec<String>,
    },
    GithubMergedPrs {
        repos: Vec<String>,
        #[serde(default = "default_since_days")]
        since_days: u64,
    },
    GoogleSheets {
        spreadsheet_id: String,
        #[serde(default)]
        range: Option<String>,
        #[serde(default)]
        service_account_key_env: Option<String>,
        #[serde(default)]
        limit: Option<usize>,
    },
    WebScraper {
        url: String,
        #[serde(default)]
        base_url: Option<String>,
        items_selector: String,
        #[serde(default)]
        title_selector: Option<String>,
        #[serde(default)]
        url_selector: Option<String>,
        #[serde(default)]
        summary_selector: Option<String>,
        #[serde(default)]
        date_selector: Option<String>,
        #[serde(default)]
        date_format: Option<String>,
        #[serde(default = "default_rss_limit")]
        limit: usize,
    },
}

fn default_rss_limit() -> usize {
    10
}

fn default_since_days() -> u64 {
    7
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SinkConfig {
    Slack {
        webhook_url_env: Option<String>,
        bot_token_env: Option<String>,
        channel: Option<String>,
    },
    Notion {
        token_env: String,
        database_id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_invalid_port_uses_default() {
        let config = Config::from_raw_values(Some("not-a-number"), None, None);
        assert_eq!(config.port, 8081);
    }

    #[test]
    fn test_config_valid_port() {
        let config = Config::from_raw_values(Some("3000"), None, None);
        assert_eq!(config.port, 3000);
    }

    #[test]
    fn test_config_empty_sentry_dsn_is_none() {
        let config = Config::from_raw_values(None, Some(""), None);
        assert!(config.sentry_dsn.is_none());
    }

    #[test]
    fn test_config_present_sentry_dsn() {
        let config = Config::from_raw_values(None, Some("https://sentry.io/123"), None);
        assert_eq!(config.sentry_dsn.as_deref(), Some("https://sentry.io/123"));
    }

    #[test]
    fn test_config_default_environment() {
        let config = Config::from_raw_values(None, None, None);
        assert_eq!(config.environment, "local");
    }

    #[test]
    fn test_config_custom_environment() {
        let config = Config::from_raw_values(None, None, Some("production"));
        assert_eq!(config.environment, "production");
    }
}
