#!/usr/bin/env bash
# provision-cloud-agent.sh
#
# Provisions a cloud VM with the ADK A2A agent server.
# Usage:
#   ./scripts/provision-cloud-agent.sh <VM_ID> [ANTHROPIC_API_KEY] [VM_MANAGER_HOST]
#
# Example:
#   ./scripts/provision-cloud-agent.sh 0 sk-ant-xxx 34.100.130.60
#
# This script:
#   1. Copies the agent code from static/cloud-agent/ into the VM
#   2. Installs Node.js and dependencies
#   3. Sets environment variables
#   4. Starts the A2A server as a background process

set -euo pipefail

VM_ID="${1:?Usage: $0 <VM_ID> [ANTHROPIC_API_KEY] [VM_MANAGER_HOST]}"
ANTHROPIC_API_KEY="${2:-${ANTHROPIC_API_KEY:-}}"
VM_MANAGER_HOST="${3:-34.100.130.60}"

# Calculate SSH port (base 2222 + VM_ID)
SSH_PORT=$((2222 + VM_ID))
# Calculate web port (base 7700 + VM_ID) — A2A server will bind here
WEB_PORT=$((7700 + VM_ID))

SSH_TARGET="root@${VM_MANAGER_HOST}"
SSH_CMD="ssh -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -p ${SSH_PORT} ${SSH_TARGET}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AGENT_DIR="${SCRIPT_DIR}/../static/cloud-agent"

echo "=== Provisioning VM ${VM_ID} ==="
echo "  SSH: ssh -p ${SSH_PORT} ${SSH_TARGET}"
echo "  A2A: http://${VM_MANAGER_HOST}:${WEB_PORT}"
echo ""

# ── Step 1: Check connectivity ──────────────────────────────────────────
echo "[1/6] Checking SSH connectivity..."
if ! ${SSH_CMD} "echo ok" >/dev/null 2>&1; then
    echo "ERROR: Cannot SSH into VM ${VM_ID} on port ${SSH_PORT}"
    exit 1
fi
echo "  Connected."

# ── Step 2: Install Node.js if not present ──────────────────────────────
echo "[2/6] Checking Node.js installation..."
NODE_VERSION=$(${SSH_CMD} "node --version 2>/dev/null || echo 'none'")
if [[ "${NODE_VERSION}" == "none" ]]; then
    echo "  Installing Node.js..."
    ${SSH_CMD} << 'INSTALL_NODE'
        apt-get update -qq
        apt-get install -y -qq curl
        curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
        apt-get install -y -qq nodejs
INSTALL_NODE
    NODE_VERSION=$(${SSH_CMD} "node --version")
fi
echo "  Node.js ${NODE_VERSION}"

# ── Step 3: Create workspace directory ──────────────────────────────────
echo "[3/6] Setting up directories..."
${SSH_CMD} "mkdir -p /home/agent/workspace /home/agent/server"

# ── Step 4: Copy agent code ────────────────────────────────────────────
echo "[4/6] Copying agent code..."
scp -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
    -P "${SSH_PORT}" \
    "${AGENT_DIR}/package.json" \
    "${AGENT_DIR}/agent.js" \
    "${AGENT_DIR}/server.js" \
    "${SSH_TARGET}:/home/agent/server/"

# ── Step 5: Install dependencies and set environment ────────────────────
echo "[5/6] Installing dependencies..."
${SSH_CMD} << SETUP_ENV
    cd /home/agent/server

    # Write .env file
    cat > .env << ENV_EOF
PORT=${WEB_PORT}
VM_ID=${VM_ID}
AGENT_NAME=cthulu-cloud-agent
ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY}
ENV_EOF

    # Install npm dependencies
    npm install --production 2>&1 | tail -3
SETUP_ENV

# ── Step 6: Start the A2A server ───────────────────────────────────────
echo "[6/6] Starting A2A server..."
${SSH_CMD} << 'START_SERVER'
    # Kill any existing agent server
    pkill -f "node.*server.js" 2>/dev/null || true
    sleep 1

    cd /home/agent/server

    # Source .env
    set -a
    source .env
    set +a

    # Start server in background with output logging
    nohup node server.js > /home/agent/server/agent.log 2>&1 &
    echo $! > /home/agent/server/agent.pid

    # Wait briefly for startup
    sleep 3

    # Check if running
    if kill -0 $(cat /home/agent/server/agent.pid) 2>/dev/null; then
        echo "A2A server started (PID $(cat /home/agent/server/agent.pid))"
    else
        echo "ERROR: A2A server failed to start. Logs:"
        tail -20 /home/agent/server/agent.log
        exit 1
    fi
START_SERVER

echo ""
echo "=== VM ${VM_ID} provisioned ==="
echo "  A2A endpoint: http://${VM_MANAGER_HOST}:${WEB_PORT}"
echo "  Agent card:   http://${VM_MANAGER_HOST}:${WEB_PORT}/.well-known/agent.json"
echo "  Logs:         ssh -p ${SSH_PORT} ${SSH_TARGET} 'tail -f /home/agent/server/agent.log'"
echo ""
