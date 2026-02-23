#!/usr/bin/env bash
# Start backend, wait for health, then start studio.
# Ctrl+C kills the entire process tree cleanly.
set -e

BACKEND_PID=""
STUDIO_PID=""

# Kill a process and all its descendants
killtree() {
  local pid=$1
  # Get all child PIDs recursively
  local children
  children=$(pgrep -P "$pid" 2>/dev/null || true)
  for child in $children; do
    killtree "$child"
  done
  kill "$pid" 2>/dev/null || true
}

cleanup() {
  trap - INT TERM EXIT
  echo ""
  echo "Shutting down..."
  [ -n "$STUDIO_PID" ] && killtree "$STUDIO_PID"
  [ -n "$BACKEND_PID" ] && killtree "$BACKEND_PID"
  # Brief wait, then force-kill anything left on ports
  sleep 1
  lsof -ti:8081 | xargs kill -9 2>/dev/null || true
  lsof -ti:1420 | xargs kill -9 2>/dev/null || true
  exit 0
}

trap cleanup INT TERM EXIT

# Start backend
npx nx dev cthulu &
BACKEND_PID=$!

echo "Waiting for backend on :8081..."
until curl -sf http://localhost:8081/health >/dev/null 2>&1; do
  if ! kill -0 "$BACKEND_PID" 2>/dev/null; then
    echo "Backend failed to start"
    exit 1
  fi
  sleep 0.5
done
echo "Backend ready"

# Start studio
npx nx dev cthulu-studio &
STUDIO_PID=$!

# Wait — when either child exits or we get a signal, cleanup runs
wait
