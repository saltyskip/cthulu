use std::path::PathBuf;

use claude_agent_sdk_rust::{ClaudeAgentOptions, PermissionMode, SystemPrompt};

/// Configuration for creating an agent session.
/// Converted to `ClaudeAgentOptions` via `into_sdk()`.
#[derive(Debug, Default)]
pub struct SessionConfig {
    pub cwd: Option<String>,
    pub system_prompt: Option<String>,
    pub allowed_tools: Vec<String>,
    pub permission_mode: Option<String>,
    pub session_id: Option<String>,
    pub resume: Option<String>,
    pub include_partial_messages: bool,
}

impl SessionConfig {
    pub fn into_sdk(self) -> ClaudeAgentOptions {
        let mut opts = ClaudeAgentOptions::default();

        if let Some(cwd) = &self.cwd {
            opts.cwd = Some(PathBuf::from(cwd));
        }

        if let Some(prompt) = &self.system_prompt {
            opts.system_prompt = Some(SystemPrompt::Text(prompt.clone()));
        }

        opts.allowed_tools = self.allowed_tools;

        if let Some(mode_str) = &self.permission_mode {
            opts.permission_mode = Some(match mode_str.to_lowercase().as_str() {
                "acceptedits" | "accept_edits" => PermissionMode::AcceptEdits,
                "bypasspermissions" | "bypass_permissions" | "bypass" => {
                    PermissionMode::BypassPermissions
                }
                "plan" => PermissionMode::Plan,
                _ => PermissionMode::Default,
            });
        }

        opts.resume = self.resume;
        opts.session_id = self.session_id;
        opts.include_partial_messages = self.include_partial_messages;

        opts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_converts() {
        let config = SessionConfig::default();
        let opts = config.into_sdk();
        assert!(opts.model.is_none());
        assert!(opts.resume.is_none());
    }

    #[test]
    fn test_permission_mode_parsing() {
        let config = SessionConfig {
            permission_mode: Some("bypassPermissions".to_string()),
            ..Default::default()
        };
        let opts = config.into_sdk();
        assert_eq!(
            opts.permission_mode,
            Some(PermissionMode::BypassPermissions)
        );
    }
}
