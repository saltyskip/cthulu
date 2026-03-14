#!/usr/bin/env bash
set -euo pipefail

# Run Cthulu Studio in development mode with Tauri hot-reload.
# The embedded backend starts automatically.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

echo "==> Starting Tauri dev mode (backend + frontend + desktop shell)..."
cd "$ROOT_DIR/cthulu-studio"

# Uses Claude CLI by default. Set AGENT_SDK_ENABLED=1 to use the Rust SDK instead.
# export AGENT_SDK_ENABLED=1

npx tauri dev "$@"
