#!/bin/bash
# Build Cthulu locally, then docker build copies the artifacts.
#
# Usage:
#   ./scripts/build.sh                                          # build only
#   ./scripts/build.sh --docker                                 # build + docker image
#   ./scripts/build.sh --docker --google-client-id YOUR_ID      # with Google OAuth

set -euo pipefail
cd "$(dirname "$0")/.."

DOCKER=false
GOOGLE_CLIENT_ID="${VITE_GOOGLE_CLIENT_ID:-}"

while [[ $# -gt 0 ]]; do
  case $1 in
    --docker) DOCKER=true; shift ;;
    --google-client-id) GOOGLE_CLIENT_ID="$2"; shift 2 ;;
    *) echo "Unknown: $1"; exit 1 ;;
  esac
done

echo "=== 1. Building Rust backend (linux/amd64) ==="
cargo build --release --target x86_64-unknown-linux-gnu
echo "    Binary: target/x86_64-unknown-linux-gnu/release/cthulu"

echo ""
echo "=== 2. Building frontend ==="
export VITE_AUTH_ENABLED=true
if [ -n "$GOOGLE_CLIENT_ID" ]; then
  export VITE_GOOGLE_CLIENT_ID="$GOOGLE_CLIENT_ID"
  echo "    Google OAuth: enabled"
fi
cd cthulu-studio
npx vite build
cd ..
echo "    Output: cthulu-studio/dist/"

echo ""
echo "=== Build complete ==="

if [ "$DOCKER" = true ]; then
  echo ""
  echo "=== 3. Building Docker image ==="
  docker build --platform linux/amd64 -t cthulu:latest .
  echo ""
  echo "Done! Run with:"
  echo "  docker run -p 8081:8081 \\"
  echo "    -e GOOGLE_CLIENT_ID=your-id \\"
  echo "    -e GOOGLE_CLIENT_SECRET=your-secret \\"
  echo "    -v cthulu-data:/data \\"
  echo "    cthulu:latest"
fi
