#!/usr/bin/env bash
# mcp-launcher.sh — Claude Desktop entry point for cthulu-mcp.
#
# 1. Ensures the Cthulu backend is running on :8081.
#    If not running, starts it in the background and waits up to 10s.
# 2. Execs into cthulu-mcp (stdio MCP server).
#
# Claude Desktop passes args straight through to this script.
# The script forwards all args to cthulu-mcp unchanged.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
MCP_BINARY="$PROJECT_DIR/target/release/cthulu-mcp"
BACKEND_BINARY="$PROJECT_DIR/target/release/cthulu"
BACKEND_LOG="$PROJECT_DIR/logs/backend.log"
BACKEND_PORT=8081

# ── Ensure log dir exists ─────────────────────────────────────────────────────
mkdir -p "$PROJECT_DIR/logs"

# ── Check / start backend ─────────────────────────────────────────────────────
backend_running() {
    curl -sf "http://localhost:$BACKEND_PORT/health" >/dev/null 2>&1
}

if ! backend_running; then
    # Build the backend release binary if it doesn't exist yet
    if [ ! -f "$BACKEND_BINARY" ]; then
        echo "[mcp-launcher] Backend binary not found — building..." >&2
        cd "$PROJECT_DIR" && cargo build --release --bin cthulu 2>>"$BACKEND_LOG" || {
            echo "[mcp-launcher] Build failed — see $BACKEND_LOG" >&2
            exit 1
        }
    fi

    echo "[mcp-launcher] Starting Cthulu backend on :$BACKEND_PORT ..." >&2
    cd "$PROJECT_DIR"
    # Load .env if present
    if [ -f "$PROJECT_DIR/.env" ]; then
        set -a
        # shellcheck disable=SC1091
        source "$PROJECT_DIR/.env"
        set +a
    fi
    # NOTE: The backend process is intentionally orphaned — it keeps running
    # after the MCP server exits (e.g. when Claude Desktop kills this process).
    # This is by design: the backend is a shared service that other tools
    # (Studio, curl, etc.) also depend on. Users who want to stop it can
    # kill it manually: kill $(lsof -ti:8081)
    "$BACKEND_BINARY" serve >>"$BACKEND_LOG" 2>&1 &
    BACKEND_PID=$!
    echo "[mcp-launcher] Backend started (pid $BACKEND_PID)" >&2

    # Wait up to 10s for backend to be ready
    for i in $(seq 1 20); do
        sleep 0.5
        if backend_running; then
            echo "[mcp-launcher] Backend ready after ${i} × 0.5s" >&2
            break
        fi
        if [ $i -eq 20 ]; then
            echo "[mcp-launcher] Backend did not start in 10s — MCP tools will fail" >&2
        fi
    done
fi

# ── Exec into MCP binary (replaces this process — stdin/stdout pass through) ──
exec "$MCP_BINARY" "$@"
