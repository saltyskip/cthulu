//! Setup wizard: generates a Slack app manifest and prints configuration instructions.
//!
//! Usage:
//!   cargo run -- setup                 # prompts for bot name
//!   cargo run -- setup --name my-bot   # uses provided name

use serde_json::json;
use std::io::{self, Write};

/// Generate the Slack app manifest JSON for the given bot name.
fn generate_manifest(name: &str) -> serde_json::Value {
    json!({
        "display_information": {
            "name": name,
            "description": "AI-powered code reviewer and assistant powered by Cthulu"
        },
        "features": {
            "app_home": {
                "messages_tab_enabled": true,
                "messages_tab_read_only_enabled": false
            },
            "bot_user": {
                "always_online": true,
                "display_name": name
            }
        },
        "oauth_config": {
            "scopes": {
                "bot": [
                    "app_mentions:read",
                    "channels:history",
                    "chat:write",
                    "im:history",
                    "im:read",
                    "im:write"
                ]
            }
        },
        "settings": {
            "event_subscriptions": {
                "bot_events": [
                    "app_mention",
                    "message.im"
                ]
            },
            "interactivity": {
                "is_enabled": true
            },
            "org_deploy_enabled": false,
            "socket_mode_enabled": true,
            "token_rotation_enabled": false
        }
    })
}

/// Prompt the user for the bot name if not provided via CLI.
fn prompt_bot_name() -> String {
    let default = "cthulu";
    print!("Enter bot display name [{}]: ", default);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let trimmed = input.trim();

    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Run the setup wizard.
pub fn run(name: Option<String>) {
    let bot_name = name.unwrap_or_else(prompt_bot_name);

    let manifest = generate_manifest(&bot_name);
    let manifest_pretty = serde_json::to_string_pretty(&manifest).unwrap();

    println!();
    println!("===================================================");
    println!("  Cthulu Slack App Setup");
    println!("===================================================");
    println!();
    println!("  Bot name: {}", bot_name);
    println!();
    println!("  Copy the JSON below and paste it into Slack's");
    println!("  \"Create New App\" > \"From a manifest\" > JSON tab:");
    println!();
    println!("------------------ COPY START ---------------------");
    println!("{}", manifest_pretty);
    println!("------------------ COPY END -----------------------");
    println!();
    println!("  Scopes included:");
    println!("    app_mentions:read  - Receive @mention events in channels");
    println!("    channels:history   - Read messages in channels the bot joins");
    println!("    chat:write         - Post replies in threads");
    println!("    im:history         - Read DM messages sent to the bot");
    println!("    im:read            - View DM channel info");
    println!("    im:write           - Open DM conversations");
    println!();
    println!("  Events subscribed:");
    println!("    app_mention        - Fires when someone @mentions the bot");
    println!("    message.im         - Fires when someone DMs the bot");
    println!();
    println!("  Socket Mode: ENABLED (no public URL needed)");
    println!();
    println!("===================================================");
    println!("  Next Steps");
    println!("===================================================");
    println!();
    println!("  STEP 1: Create the Slack App");
    println!("    Go to https://api.slack.com/apps");
    println!("    Click \"Create New App\" > \"From a manifest\"");
    println!("    Select your workspace > JSON tab > Paste > Create");
    println!();
    println!("  STEP 2: Generate App-Level Token (for Socket Mode)");
    println!("    In the app settings, go to \"Basic Information\"");
    println!("    Scroll to \"App-Level Tokens\" > \"Generate Token and Scopes\"");
    println!("    Name: \"socket-mode\"");
    println!("    Add scope: connections:write");
    println!("    Click \"Generate\"");
    println!("    Copy the token (starts with xapp-)");
    println!();
    println!("  STEP 3: Install the App & Get Bot Token");
    println!("    Go to \"Install App\" in the sidebar");
    println!("    Click \"Install to Workspace\" > Authorize");
    println!("    Copy the \"Bot User OAuth Token\" (starts with xoxb-)");
    println!();
    println!("  STEP 4: Configure Cthulu");
    println!();
    println!("    Add to your .env file:");
    println!("      SLACK_BOT_TOKEN=xoxb-your-token-here");
    println!("      SLACK_APP_TOKEN=xapp-your-token-here");
    println!();
    println!("    Add to cthulu.toml:");
    println!("      [slack]");
    println!("      bot_token_env = \"SLACK_BOT_TOKEN\"");
    println!("      app_token_env = \"SLACK_APP_TOKEN\"");
    println!();
    println!("  STEP 5: Invite the Bot to a Channel");
    println!("    In Slack, go to the channel you want");
    println!("    Type: /invite @{}", bot_name);
    println!();
    println!("  STEP 6: Start Cthulu & Test");
    println!("    cargo run");
    println!("    In Slack: @{} hello!", bot_name);
    println!("    Or DM the bot directly");
    println!();

    // Also save the manifest to a file for convenience
    let manifest_path = format!("{}-manifest.json", bot_name);
    match std::fs::write(&manifest_path, &manifest_pretty) {
        Ok(()) => {
            println!("  Manifest saved to: {}", manifest_path);
        }
        Err(e) => {
            println!("  (Could not save manifest file: {})", e);
            println!("  Copy the JSON above instead.");
        }
    }
    println!();
}
