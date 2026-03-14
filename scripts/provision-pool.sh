#!/usr/bin/env bash
# provision-pool.sh
#
# Provision all VMs in the agent pool (default: VM 0-4).
# Usage:
#   ./scripts/provision-pool.sh [ANTHROPIC_API_KEY] [VM_MANAGER_HOST] [POOL_SIZE]
#
# Example:
#   ./scripts/provision-pool.sh sk-ant-xxx 34.100.130.60 5

set -euo pipefail

ANTHROPIC_API_KEY="${1:?Usage: $0 <ANTHROPIC_API_KEY> [VM_MANAGER_HOST] [POOL_SIZE]}"
VM_MANAGER_HOST="${2:-34.100.130.60}"
POOL_SIZE="${3:-5}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=== Provisioning cloud agent pool ==="
echo "  Host: ${VM_MANAGER_HOST}"
echo "  Pool size: ${POOL_SIZE}"
echo ""

FAILED=0
for i in $(seq 0 $((POOL_SIZE - 1))); do
    echo "──────────────────────────────────────"
    if ! "${SCRIPT_DIR}/provision-cloud-agent.sh" "${i}" "${ANTHROPIC_API_KEY}" "${VM_MANAGER_HOST}"; then
        echo "WARNING: VM ${i} provisioning failed"
        FAILED=$((FAILED + 1))
    fi
done

echo ""
echo "=== Pool provisioning complete ==="
echo "  Succeeded: $((POOL_SIZE - FAILED)) / ${POOL_SIZE}"
if [ ${FAILED} -gt 0 ]; then
    echo "  Failed: ${FAILED}"
    exit 1
fi
