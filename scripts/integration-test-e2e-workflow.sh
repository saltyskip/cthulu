#!/usr/bin/env bash
set -euo pipefail

# ── Config ──────────────────────────────────────────────────────────────
BASE_URL="${BASE_URL:-http://localhost:8081}"
TIMEOUT=300  # 5 minutes max for entire test
POLL_INTERVAL=5
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Track resources for cleanup
AGENT_ID=""
FLOW_ID=""

cleanup() {
    echo ""
    echo "═══ Cleanup ═══"
    if [[ -n "$FLOW_ID" ]]; then
        echo "Deleting flow $FLOW_ID..."
        curl -sf -X DELETE "$BASE_URL/api/flows/$FLOW_ID" > /dev/null 2>&1 || true
    fi
    if [[ -n "$AGENT_ID" ]]; then
        echo "Deleting agent $AGENT_ID..."
        curl -sf -X DELETE "$BASE_URL/api/agents/$AGENT_ID" > /dev/null 2>&1 || true
    fi
    echo "Cleanup done."
}

if [[ "${SKIP_CLEANUP:-0}" != "1" ]]; then
    trap cleanup EXIT
fi

# ── Helpers ─────────────────────────────────────────────────────────────
json_val() {
    python3 -c "import json,sys; d=json.load(sys.stdin); print(d$1)" 2>/dev/null
}

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  Cthulu E2E Workflow Integration Test                       ║"
echo "║  Python Script → Claude Code → Slack                        ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# ── Pre-flight checks ─────────────────────────────────────────────────
echo "═══ Pre-flight checks ═══"

if ! command -v python3 &>/dev/null; then
    echo "✗ python3 not found on PATH"
    exit 1
fi
echo "✓ python3 found"

if ! python3 -c "import yaml" 2>/dev/null; then
    echo "✗ pyyaml not installed (pip3 install pyyaml)"
    exit 1
fi
echo "✓ pyyaml available"

if ! command -v curl &>/dev/null; then
    echo "✗ curl not found on PATH"
    exit 1
fi
echo "✓ curl found"
echo ""

# ── Step 1: Wait for backend ───────────────────────────────────────────
echo "═══ Step 1: Waiting for backend at $BASE_URL ═══"
WAIT_START=$(date +%s)
while true; do
    if curl -sf "$BASE_URL/health" > /dev/null 2>&1; then
        echo "✓ Backend is healthy"
        break
    fi
    ELAPSED=$(( $(date +%s) - WAIT_START ))
    if (( ELAPSED > 60 )); then
        echo "✗ Backend not ready after 60s. Is the server running?"
        echo "  Start it with: cargo run -- serve"
        exit 1
    fi
    echo "  Waiting... (${ELAPSED}s)"
    sleep 2
done

# ── Step 2: Create test agent ──────────────────────────────────────────
echo ""
echo "═══ Step 2: Creating test agent ═══"
AGENT_RESPONSE=$(curl -sf -X POST "$BASE_URL/api/agents" \
    -H "Content-Type: application/json" \
    -d '{
        "name": "e2e-test-analyst",
        "model": "claude-sonnet-4-20250514",
        "permissions": [],
        "append_system_prompt": "You are a concise crypto analyst for an automated pipeline. Keep responses under 280 characters. Use emoji.",
        "role": "general"
    }')
AGENT_ID=$(echo "$AGENT_RESPONSE" | json_val "['id']")
echo "✓ Created agent: $AGENT_ID"

# ── Step 3: Create flow ───────────────────────────────────────────────
echo ""
echo "═══ Step 3: Creating E2E workflow ═══"

# Read the YAML and convert to JSON, patching the agent ID
FLOW_JSON=$(python3 <<PYEOF
import json, yaml

with open("$REPO_ROOT/examples/workflows/e2e-python-slack.yaml") as f:
    wf = yaml.safe_load(f)

# Patch agent_id into the claude-code node
for node in wf.get("nodes", []):
    if node.get("kind") == "claude-code":
        node["config"]["agent_id"] = "$AGENT_ID"

# Convert to flow API format
flow = {
    "name": wf["name"],
    "description": wf.get("description", ""),
    "nodes": wf["nodes"],
    "edges": wf.get("edges", [])
}
print(json.dumps(flow))
PYEOF
)

FLOW_RESPONSE=$(curl -sf -X POST "$BASE_URL/api/flows" \
    -H "Content-Type: application/json" \
    -d "$FLOW_JSON")
FLOW_ID=$(echo "$FLOW_RESPONSE" | json_val "['id']")
echo "✓ Created flow: $FLOW_ID"

# ── Step 4: Trigger the flow ──────────────────────────────────────────
echo ""
echo "═══ Step 4: Triggering flow ═══"
TRIGGER_STATUS=$(curl -sf -o /dev/null -w "%{http_code}" \
    -X POST "$BASE_URL/api/flows/$FLOW_ID/trigger")
if [[ "$TRIGGER_STATUS" == "202" || "$TRIGGER_STATUS" == "200" ]]; then
    echo "✓ Flow triggered (HTTP $TRIGGER_STATUS)"
else
    echo "✗ Trigger failed with HTTP $TRIGGER_STATUS"
    exit 1
fi

# ── Step 5: Poll for completion ───────────────────────────────────────
echo ""
echo "═══ Step 5: Polling for run completion (timeout: ${TIMEOUT}s) ═══"
POLL_START=$(date +%s)
RUN_STATUS=""
RUN_ID=""

while true; do
    ELAPSED=$(( $(date +%s) - POLL_START ))
    if (( ELAPSED > TIMEOUT )); then
        echo "✗ Timed out after ${TIMEOUT}s waiting for run to complete"
        echo "  Last known runs:"
        curl -sf "$BASE_URL/api/flows/$FLOW_ID/runs" | python3 -m json.tool 2>/dev/null || true
        exit 1
    fi

    RUNS_JSON=$(curl -sf "$BASE_URL/api/flows/$FLOW_ID/runs" 2>/dev/null || echo "[]")
    RUN_INFO=$(echo "$RUNS_JSON" | python3 -c "
import json, sys
data = json.load(sys.stdin)
runs = data if isinstance(data, list) else data.get('runs', [])
if runs:
    latest = runs[0]
    print(f\"{latest.get('id','?')}|{latest.get('status','?')}\")
else:
    print('|pending')
")

    RUN_ID="${RUN_INFO%%|*}"
    RUN_STATUS="${RUN_INFO##*|}"

    case "$RUN_STATUS" in
        success|completed)
            echo "✓ Run completed successfully! (run: $RUN_ID, ${ELAPSED}s)"
            break
            ;;
        failed|error)
            echo "✗ Run failed! (run: $RUN_ID, ${ELAPSED}s)"
            echo "  Fetching run details..."
            curl -sf "$BASE_URL/api/flows/$FLOW_ID/runs" | python3 -m json.tool 2>/dev/null || true
            exit 1
            ;;
        *)
            echo "  Status: $RUN_STATUS (${ELAPSED}s elapsed)"
            sleep "$POLL_INTERVAL"
            ;;
    esac
done

# ── Step 6: Summary ───────────────────────────────────────────────────
echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  E2E TEST PASSED                                            ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Pipeline: Manual Trigger → Python (BTC fetch) → Claude (analyze) → Slack"
echo "Flow ID:  $FLOW_ID"
echo "Run ID:   $RUN_ID"
echo "Status:   $RUN_STATUS"
echo ""
echo "Check your Slack channel for the posted crypto analysis!"
