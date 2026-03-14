use std::path::{Path, PathBuf};

/// Generate the hook helper script and write it to a temp location.
///
/// The script is invoked by Claude CLI as a command-type hook:
///   `/tmp/cthulu-hook.sh <event> <session_id>`
///
/// It reads JSON from stdin (Claude CLI sends tool info), forwards it to the
/// Unix domain socket, reads the response, and exits with code 0 (allow) or
/// 2 (deny).
///
/// Returns the path to the executable script.
pub fn generate_hook_script(socket_path: &Path) -> Result<PathBuf, String> {
    let script_path = std::env::temp_dir().join("cthulu-hook.sh");
    let socket_display = socket_path.display();

    let script = format!(
        r##"#!/bin/bash
# Cthulu hook helper — communicates with Tauri app via Unix socket.
# Generated at runtime. Do not edit.
set -e

EVENT="$1"
SESSION_ID="$2"
SOCKET="{socket}"

# Read all of stdin (Claude CLI sends the tool payload as JSON)
INPUT=$(cat)

# Build the wire-protocol request envelope
REQUEST=$(printf '{{"event":"%s","session_id":"%s","payload":%s}}' "$EVENT" "$SESSION_ID" "$INPUT")

# Send request and read response via Unix socket using Python 3
RESPONSE=$(/usr/bin/python3 -c "
import socket, sys, json

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    sock.connect('$SOCKET')
except Exception as e:
    # If the socket is unavailable, auto-allow so we don't block the agent
    print(json.dumps({{'decision': 'allow', 'hookSpecificOutput': {{
        'hookEventName': 'PreToolUse',
        'permissionDecision': 'allow',
        'permissionDecisionReason': 'Hook socket unavailable, auto-approved'
    }}}}))
    sys.exit(0)

data = sys.stdin.read()
sock.sendall((data + '\n').encode())

response = b''
while True:
    chunk = sock.recv(4096)
    if not chunk:
        break
    response += chunk
    if b'\n' in response:
        break
sock.close()
print(response.decode().strip())
" <<< "$REQUEST")

# Parse decision from response
DECISION=$(echo "$RESPONSE" | /usr/bin/python3 -c "
import sys, json
try:
    r = json.load(sys.stdin)
    print(r.get('decision', 'allow'))
except:
    print('allow')
" 2>/dev/null || echo "allow")

if [ "$DECISION" = "deny" ]; then
    echo "$RESPONSE"
    exit 2
fi

echo "$RESPONSE"
exit 0
"##,
        socket = socket_display
    );

    std::fs::write(&script_path, &script)
        .map_err(|e| format!("failed to write hook script: {e}"))?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("failed to chmod hook script: {e}"))?;
    }

    tracing::info!(path = %script_path.display(), "generated hook helper script");
    Ok(script_path)
}
