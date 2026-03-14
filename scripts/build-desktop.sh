#!/usr/bin/env bash
set -euo pipefail

# Build the Cthulu Studio desktop application.
# Produces platform-specific installers in cthulu-studio/src-tauri/target/release/bundle/

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

echo "==> Building frontend..."
cd "$ROOT_DIR"
npx nx build cthulu-studio

echo "==> Building Tauri desktop app..."
cd "$ROOT_DIR/cthulu-studio"
npx tauri build "$@"

echo ""
echo "==> Build complete! Artifacts:"
ls -la src-tauri/target/release/bundle/*/ 2>/dev/null || echo "  (check src-tauri/target/release/bundle/)"
