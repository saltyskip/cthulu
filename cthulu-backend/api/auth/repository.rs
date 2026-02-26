/// Read the OAuth token from the same sources as startup:
/// 1. macOS Keychain (`security find-generic-password -s "Claude Code-credentials"`)
/// 2. CLAUDE_CODE_OAUTH_TOKEN env var
pub fn read_oauth_token() -> Option<String> {
    if let Some(raw) = read_keychain_raw() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(token) = v["claudeAiOauth"]["accessToken"].as_str() {
                return Some(token.to_string());
            }
        }
    }

    // Fall back to env var
    std::env::var("CLAUDE_CODE_OAUTH_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
}

/// Read the full credentials JSON blob from the macOS Keychain.
/// Returns the raw JSON string (the whole `{"claudeAiOauth": {...}}` object)
/// so it can be written verbatim to ~/.claude/.credentials.json in VMs.
/// Returns None on non-macOS or if the Keychain entry doesn't exist.
pub fn read_full_credentials() -> Option<String> {
    let raw = read_keychain_raw()?;
    // Validate it's parseable JSON before returning
    if serde_json::from_str::<serde_json::Value>(&raw).is_ok() {
        Some(raw)
    } else {
        None
    }
}

/// Read the raw JSON string from `security find-generic-password`.
fn read_keychain_raw() -> Option<String> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() { None } else { Some(raw) }
}
