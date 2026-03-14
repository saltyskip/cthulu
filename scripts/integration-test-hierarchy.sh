#!/usr/bin/env bash
# =============================================================================
# integration-test-hierarchy.sh
# =============================================================================
# Full lifecycle integration test for the Agent Leader/Subagent Hierarchy
# and Task System (Phase 4).
#
# Prerequisites:
#   - Backend running on port 8081: `cargo run -- serve` or `npx nx dev cthulu`
#   - Claude CLI installed and authenticated
#   - ~/.cthulu/cthulu-agents/ directory exists (the sync repo working dir)
#
# What this script does:
#   1. Waits for backend health on port 8081
#   2. Creates 4 agents: CEO, Lead Engineer, Engineer, QA Lead
#   3. Configures heartbeat settings (enable, max_turns=3, auto_permissions=false)
#   4. Sets up hierarchy via reports_to
#   5. Verifies hierarchy via GET /api/agents
#   6. Creates 3 tasks assigned to Lead Engineer, Engineer, QA Lead
#      (each creation auto-triggers a WakeupSource::Assignment heartbeat)
#   7. Polls heartbeat-runs until all finish (timeout: 5 min)
#   8. Prints summary: agent tree, tasks, run results, costs
#   9. Prints instructions to open Studio for visual verification
#
# Usage:
#   chmod +x scripts/integration-test-hierarchy.sh
#   ./scripts/integration-test-hierarchy.sh
# =============================================================================
set -euo pipefail

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------
BASE_URL="http://localhost:8081"
API_URL="${BASE_URL}/api"
WORKING_DIR="${HOME}/.cthulu/cthulu-agents"
POLL_TIMEOUT=300   # 5 minutes
POLL_INTERVAL=5    # seconds between polls

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m' # No Color

# Agent IDs (populated during creation)
CEO_ID=""
LEAD_ENG_ID=""
ENGINEER_ID=""
QA_ID=""

# Task IDs (populated during creation)
TASK_1_ID=""
TASK_2_ID=""
TASK_3_ID=""

# Cleanup tracking
CREATED_AGENT_IDS=()
CREATED_TASK_IDS=()

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

log_info()  { echo -e "${BLUE}[INFO]${NC}  $*"; }
log_ok()    { echo -e "${GREEN}[OK]${NC}    $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
log_err()   { echo -e "${RED}[ERR]${NC}   $*"; }
log_step()  { echo -e "\n${BOLD}${CYAN}=== $* ===${NC}\n"; }

# JSON helpers (using python for portability — jq may not be installed)
json_get() {
    local json="$1" field="$2"
    python3 -c "import json,sys; d=json.loads(sys.stdin.read()); print(d${field})" <<< "$json"
}

json_len() {
    local json="$1"
    python3 -c "import json,sys; d=json.loads(sys.stdin.read()); print(len(d))" <<< "$json"
}

json_pretty() {
    python3 -c "import json,sys; print(json.dumps(json.loads(sys.stdin.read()), indent=2))" <<< "$1"
}

# HTTP helper — returns body, sets $HTTP_CODE
http_post() {
    local url="$1" body="$2"
    local tmpfile
    tmpfile=$(mktemp)
    HTTP_CODE=$(curl -s -o "$tmpfile" -w "%{http_code}" \
        -X POST "$url" \
        -H "Content-Type: application/json" \
        -d "$body")
    HTTP_BODY=$(cat "$tmpfile")
    rm -f "$tmpfile"
}

http_put() {
    local url="$1" body="$2"
    local tmpfile
    tmpfile=$(mktemp)
    HTTP_CODE=$(curl -s -o "$tmpfile" -w "%{http_code}" \
        -X PUT "$url" \
        -H "Content-Type: application/json" \
        -d "$body")
    HTTP_BODY=$(cat "$tmpfile")
    rm -f "$tmpfile"
}

http_get() {
    local url="$1"
    local tmpfile
    tmpfile=$(mktemp)
    HTTP_CODE=$(curl -s -o "$tmpfile" -w "%{http_code}" "$url")
    HTTP_BODY=$(cat "$tmpfile")
    rm -f "$tmpfile"
}

http_delete() {
    local url="$1"
    local tmpfile
    tmpfile=$(mktemp)
    HTTP_CODE=$(curl -s -o "$tmpfile" -w "%{http_code}" -X DELETE "$url")
    HTTP_BODY=$(cat "$tmpfile")
    rm -f "$tmpfile"
}

# Cleanup on exit
cleanup() {
    if [[ "${SKIP_CLEANUP:-}" == "1" ]]; then
        log_warn "SKIP_CLEANUP=1 — leaving agents and tasks in place"
        return
    fi

    log_step "Cleanup"

    for tid in "${CREATED_TASK_IDS[@]}"; do
        log_info "Deleting task $tid"
        http_delete "${API_URL}/agents/tasks/${tid}" 2>/dev/null || true
    done

    for aid in "${CREATED_AGENT_IDS[@]}"; do
        log_info "Deleting agent $aid"
        http_delete "${API_URL}/agents/${aid}" 2>/dev/null || true
    done

    log_ok "Cleanup complete"
}

trap cleanup EXIT

# ---------------------------------------------------------------------------
# Step 1: Wait for backend health
# ---------------------------------------------------------------------------
log_step "Step 1: Waiting for backend on port 8081"

elapsed=0
until curl -sf "${BASE_URL}/health" >/dev/null 2>&1; do
    if (( elapsed >= 60 )); then
        log_err "Backend did not start within 60 seconds"
        log_err "Start it with: cargo run -- serve  (or npx nx dev cthulu)"
        exit 1
    fi
    printf "."
    sleep 1
    (( elapsed++ ))
done
echo ""
log_ok "Backend is healthy"

# Also ensure working directory exists
if [[ ! -d "$WORKING_DIR" ]]; then
    log_info "Creating working directory: $WORKING_DIR"
    mkdir -p "$WORKING_DIR"
fi

# ---------------------------------------------------------------------------
# Step 2: Create 4 agents with hierarchy
# ---------------------------------------------------------------------------
log_step "Step 2: Creating agents"

create_agent() {
    local name="$1" description="$2" prompt="$3" role="$4" reports_to="$5"

    local body
    body=$(AG_NAME="$name" AG_DESC="$description" AG_PROMPT="$prompt" \
           AG_ROLE="$role" AG_REPORTS_TO="$reports_to" AG_WORKDIR="$WORKING_DIR" \
           python3 -c "
import json, os
d = {
    'name': os.environ['AG_NAME'],
    'description': os.environ['AG_DESC'],
    'prompt': os.environ['AG_PROMPT'],
    'permissions': ['Read', 'Write', 'Edit', 'Grep', 'Glob', 'Bash'],
    'working_dir': os.environ['AG_WORKDIR'],
}
role = os.environ.get('AG_ROLE', '')
reports_to = os.environ.get('AG_REPORTS_TO', '')
if role:
    d['role'] = role
if reports_to:
    d['reports_to'] = reports_to
print(json.dumps(d))
")

    http_post "${API_URL}/agents" "$body"

    if [[ "$HTTP_CODE" != "201" ]]; then
        log_err "Failed to create agent (HTTP $HTTP_CODE): $HTTP_BODY"
        exit 1
    fi

    local agent_id
    agent_id=$(json_get "$HTTP_BODY" "['id']")
    CREATED_AGENT_IDS+=("$agent_id")
    log_ok "Created $name (id=$agent_id, role=$role)" >&2
    echo "$agent_id"
}

# --- CEO (root, no reports_to) ---
CEO_ID=$(create_agent \
    "CEO Agent" \
    "Chief Executive Officer — oversees all operations, reviews reports from direct reports" \
    "You are the CEO of this organization. Review the working directory for any status reports or updates from your team. Summarize the current state of operations. If there are pending items that need your attention, note them. Write a brief status update to STATUS.md." \
    "ceo" \
    "")

# --- Lead Engineer (reports to CEO) ---
LEAD_ENG_ID=$(create_agent \
    "Lead Engineer" \
    "CTO — leads engineering team, code review, architecture decisions" \
    "You are the Lead Engineer / CTO. Check the working directory for any new code, tasks, or issues. Review any files that need attention. If a task has been assigned to you, work on it. Write your findings to ENGINEERING-STATUS.md." \
    "cto" \
    "$CEO_ID")

# --- Engineer (reports to Lead Engineer) ---
ENGINEER_ID=$(create_agent \
    "Engineer" \
    "Software engineer — writes code, fixes bugs, implements features" \
    "You are a Software Engineer. Check the working directory for assigned tasks or code to write. If a task has been assigned to you, implement it by creating or modifying files. Document what you did in ENGINEERING-LOG.md." \
    "engineer" \
    "$LEAD_ENG_ID")

# --- QA Lead (reports to CEO) ---
QA_ID=$(create_agent \
    "QA Lead" \
    "Quality assurance lead — reviews code quality, runs tests, reports bugs" \
    "You are the QA Lead. Check the working directory for any new code or changes. Review code quality, look for potential bugs, and document your findings. Write a QA report to QA-REPORT.md." \
    "qa" \
    "$CEO_ID")

echo ""
log_info "Agent tree:"
echo "  CEO ($CEO_ID)"
echo "  ├── Lead Engineer ($LEAD_ENG_ID)"
echo "  │   └── Engineer ($ENGINEER_ID)"
echo "  └── QA Lead ($QA_ID)"

# ---------------------------------------------------------------------------
# Step 3: Configure heartbeat settings via PATCH (update)
# ---------------------------------------------------------------------------
log_step "Step 3: Enabling heartbeats (max_turns=3)"

enable_heartbeat() {
    local agent_id="$1" agent_name="$2"

    local body='{"heartbeat_enabled":true,"heartbeat_interval_secs":600,"max_turns_per_heartbeat":3,"auto_permissions":false}'

    http_put "${API_URL}/agents/${agent_id}" "$body"

    if [[ "$HTTP_CODE" != "200" ]]; then
        log_err "Failed to update heartbeat for $agent_name (HTTP $HTTP_CODE): $HTTP_BODY"
        exit 1
    fi

    log_ok "Heartbeat enabled for $agent_name"
}

enable_heartbeat "$CEO_ID" "CEO"
enable_heartbeat "$LEAD_ENG_ID" "Lead Engineer"
enable_heartbeat "$ENGINEER_ID" "Engineer"
enable_heartbeat "$QA_ID" "QA Lead"

# ---------------------------------------------------------------------------
# Step 4: Verify hierarchy via list
# ---------------------------------------------------------------------------
log_step "Step 4: Verifying hierarchy"

http_get "${API_URL}/agents"
if [[ "$HTTP_CODE" != "200" ]]; then
    log_err "Failed to list agents (HTTP $HTTP_CODE)"
    exit 1
fi

# Count agents from the { "agents": [...] } response
agent_count=$(python3 -c "import json,sys; d=json.loads(sys.stdin.read()); print(len(d.get('agents',[])))" <<< "$HTTP_BODY")

log_info "Total agents in system: $agent_count"

# Verify each agent's role and reports_to
verify_agent() {
    local agent_id="$1" expected_role="$2" expected_reports_to="$3" agent_name="$4"

    http_get "${API_URL}/agents/${agent_id}"
    if [[ "$HTTP_CODE" != "200" ]]; then
        log_err "Failed to get agent $agent_name ($agent_id)"
        exit 1
    fi

    local actual_role actual_reports_to
    actual_role=$(python3 -c "import json,sys; d=json.loads(sys.stdin.read()); print(d.get('role') or '')" <<< "$HTTP_BODY")
    actual_reports_to=$(python3 -c "import json,sys; d=json.loads(sys.stdin.read()); print(d.get('reports_to') or '')" <<< "$HTTP_BODY")

    local ok=true
    if [[ "$actual_role" != "$expected_role" ]]; then
        log_err "$agent_name: expected role='$expected_role', got '$actual_role'"
        ok=false
    fi
    if [[ "$actual_reports_to" != "$expected_reports_to" ]]; then
        log_err "$agent_name: expected reports_to='$expected_reports_to', got '$actual_reports_to'"
        ok=false
    fi

    if $ok; then
        log_ok "$agent_name: role=$actual_role, reports_to=${actual_reports_to:-none}"
    fi
}

verify_agent "$CEO_ID"      "ceo"      ""              "CEO"
verify_agent "$LEAD_ENG_ID" "cto"      "$CEO_ID"       "Lead Engineer"
verify_agent "$ENGINEER_ID" "engineer" "$LEAD_ENG_ID"  "Engineer"
verify_agent "$QA_ID"       "qa"       "$CEO_ID"       "QA Lead"

# ---------------------------------------------------------------------------
# Step 5: Create tasks (auto-triggers heartbeat on assignee)
# ---------------------------------------------------------------------------
log_step "Step 5: Creating tasks (triggers heartbeat via WakeupSource::Assignment)"

create_task() {
    local title="$1" assignee_id="$2" assignee_name="$3"

    local body
    body=$(TASK_TITLE="$title" TASK_ASSIGNEE="$assignee_id" python3 -c "
import json, os
print(json.dumps({
    'title': os.environ['TASK_TITLE'],
    'assignee_agent_id': os.environ['TASK_ASSIGNEE'],
}))
")

    http_post "${API_URL}/agents/tasks" "$body"

    if [[ "$HTTP_CODE" != "201" ]]; then
        log_err "Failed to create task (HTTP $HTTP_CODE): $HTTP_BODY"
        exit 1
    fi

    local task_id
    task_id=$(json_get "$HTTP_BODY" "['id']")
    CREATED_TASK_IDS+=("$task_id")
    log_ok "Created task -> $assignee_name (task_id=$task_id)" >&2
    log_info "  (This auto-triggered a heartbeat run on $assignee_name)" >&2
    echo "$task_id"
}

TASK_1_ID=$(create_task \
    "Review engineering architecture and create ARCHITECTURE.md" \
    "$LEAD_ENG_ID" \
    "Lead Engineer")

# Brief pause so wakeups don't collide
sleep 2

TASK_2_ID=$(create_task \
    "Create a hello-world.py script with unit tests" \
    "$ENGINEER_ID" \
    "Engineer")

sleep 2

TASK_3_ID=$(create_task \
    "Audit working directory for code quality issues and write QA-REPORT.md" \
    "$QA_ID" \
    "QA Lead")

echo ""
log_info "Tasks created:"
echo "  1. Architecture review -> Lead Engineer ($TASK_1_ID)"
echo "  2. Hello-world script  -> Engineer      ($TASK_2_ID)"
echo "  3. QA audit            -> QA Lead        ($TASK_3_ID)"

# ---------------------------------------------------------------------------
# Step 6: Poll heartbeat runs until all complete (or timeout)
# ---------------------------------------------------------------------------
log_step "Step 6: Polling heartbeat runs (timeout: ${POLL_TIMEOUT}s)"

# We expect at least 1 run per agent that received a task
# (Lead Engineer, Engineer, QA Lead — CEO was not assigned a task)
AGENTS_WITH_TASKS=("$LEAD_ENG_ID" "$ENGINEER_ID" "$QA_ID")
AGENT_NAMES=("Lead Engineer" "Engineer" "QA Lead")

wait_for_runs() {
    local start_time
    start_time=$(date +%s)

    while true; do
        local now
        now=$(date +%s)
        local elapsed=$(( now - start_time ))

        if (( elapsed >= POLL_TIMEOUT )); then
            log_err "Timed out after ${POLL_TIMEOUT}s waiting for runs to complete"
            return 1
        fi

        local all_done=true
        local i=0

        for agent_id in "${AGENTS_WITH_TASKS[@]}"; do
            local agent_name="${AGENT_NAMES[$i]}"
            http_get "${API_URL}/agents/${agent_id}/heartbeat-runs"

            if [[ "$HTTP_CODE" != "200" ]]; then
                log_warn "Could not fetch runs for $agent_name (HTTP $HTTP_CODE)"
                all_done=false
                (( i++ ))
                continue
            fi

            # Response is a JSON array directly (not wrapped)
            local run_count
            run_count=$(json_len "$HTTP_BODY")

            if (( run_count == 0 )); then
                log_info "[$agent_name] No runs yet (${elapsed}s elapsed)"
                all_done=false
                (( i++ ))
                continue
            fi

            # Check the most recent run (last element)
            local latest_status
            latest_status=$(python3 -c "
import json, sys
runs = json.loads(sys.stdin.read())
# Most recent is last
print(runs[-1].get('status', 'unknown'))
" <<< "$HTTP_BODY")

            case "$latest_status" in
                succeeded|failed|timed_out|cancelled)
                    log_ok "[$agent_name] Run complete: $latest_status ($run_count total runs)"
                    ;;
                running|queued)
                    log_info "[$agent_name] Run status: $latest_status (${elapsed}s elapsed)"
                    all_done=false
                    ;;
                *)
                    log_warn "[$agent_name] Unknown status: $latest_status"
                    all_done=false
                    ;;
            esac

            (( i++ ))
        done

        if $all_done; then
            return 0
        fi

        sleep "$POLL_INTERVAL"
    done
}

if wait_for_runs; then
    log_ok "All heartbeat runs completed!"
else
    log_warn "Some runs may not have completed — continuing to summary"
fi

# ---------------------------------------------------------------------------
# Step 7: Print summary
# ---------------------------------------------------------------------------
log_step "Step 7: Summary"

echo -e "${BOLD}Agent Hierarchy:${NC}"
echo "  CEO ($CEO_ID)"
echo "  ├── Lead Engineer ($LEAD_ENG_ID)"
echo "  │   └── Engineer ($ENGINEER_ID)"
echo "  └── QA Lead ($QA_ID)"
echo ""

echo -e "${BOLD}Tasks:${NC}"
http_get "${API_URL}/agents/tasks"
if [[ "$HTTP_CODE" == "200" ]]; then
    OUR_TASK_IDS="${TASK_1_ID} ${TASK_2_ID} ${TASK_3_ID}" python3 -c "
import json, sys, os
data = json.loads(sys.stdin.read())
tasks = data.get('tasks', [])
our_task_ids = set(os.environ['OUR_TASK_IDS'].split())
for t in tasks:
    if t['id'] in our_task_ids:
        status = t['status'].upper()
        title = t['title'][:50]
        assignee = t['assignee_agent_id'][:8]
        print(f'  [{status:12s}] {title:50s}  assignee={assignee}...')
" <<< "$HTTP_BODY"
fi
echo ""

echo -e "${BOLD}Heartbeat Run Results:${NC}"
total_cost="0"

for i in "${!AGENTS_WITH_TASKS[@]}"; do
    agent_id="${AGENTS_WITH_TASKS[$i]}"
    agent_name="${AGENT_NAMES[$i]}"

    http_get "${API_URL}/agents/${agent_id}/heartbeat-runs"
    if [[ "$HTTP_CODE" == "200" ]]; then
        agent_cost=$(AGENT_NAME="$agent_name" python3 -c "
import json, sys, os
runs = json.loads(sys.stdin.read())
name = os.environ['AGENT_NAME']
total = 0
if not runs:
    print(f'  {name}: no runs', file=sys.stderr)
else:
    for r in runs:
        status = r.get('status', '?')
        source = r.get('source', '?')
        cost = r.get('cost_usd', 0)
        duration = r.get('duration_secs', 0)
        error = r.get('error', '')
        model = r.get('model', 'n/a')
        err_str = f' error={error}' if error else ''
        print(f'  {name}: status={status} source={source} cost=\${cost:.4f} duration={duration:.1f}s model={model}{err_str}', file=sys.stderr)
        total += cost
print(total)
" <<< "$HTTP_BODY" 2>&1 1>/dev/null) || agent_cost="0"
        # Print the stderr lines (the formatted output)
        AGENT_NAME="$agent_name" python3 -c "
import json, sys, os
runs = json.loads(sys.stdin.read())
name = os.environ['AGENT_NAME']
if not runs:
    print(f'  {name}: no runs')
else:
    for r in runs:
        status = r.get('status', '?')
        source = r.get('source', '?')
        cost = r.get('cost_usd', 0)
        duration = r.get('duration_secs', 0)
        error = r.get('error', '')
        model = r.get('model', 'n/a')
        err_str = f' error={error}' if error else ''
        print(f'  {name}: status={status} source={source} cost=\${cost:.4f} duration={duration:.1f}s model={model}{err_str}')
" <<< "$HTTP_BODY"
        total_cost=$(PREV="$total_cost" ADD="$agent_cost" python3 -c "import os; print(float(os.environ['PREV']) + float(os.environ['ADD']))")
    fi
done

echo ""
echo -e "${BOLD}Total estimated cost:${NC} \$${total_cost}"
echo ""

# Check what files the agents created
echo -e "${BOLD}Files in working directory (${WORKING_DIR}):${NC}"
if [[ -d "$WORKING_DIR" ]]; then
    ls -la "$WORKING_DIR" 2>/dev/null | head -20
else
    echo "  (directory does not exist)"
fi

echo ""

# ---------------------------------------------------------------------------
# Step 8: Visual verification instructions
# ---------------------------------------------------------------------------
log_step "Step 8: Next Steps — Visual Verification"

echo "To verify in Cthulu Studio:"
echo ""
echo "  1. Start Studio:  npx nx dev cthulu-studio"
echo "  2. Open: http://localhost:1420"
echo "  3. Navigate to the Agents tab"
echo "  4. Click 'Org Chart' to see the hierarchy visualization"
echo "  5. Click each agent to see their Dashboard (runs) and Tasks tabs"
echo "  6. Check that:"
echo "     - CEO is the root node"
echo "     - Lead Engineer and QA Lead report to CEO"
echo "     - Engineer reports to Lead Engineer"
echo "     - Tasks appear with correct assignees"
echo "     - Heartbeat runs are visible with 'assignment' source"
echo ""
echo "To skip cleanup and keep agents for UI testing:"
echo "  SKIP_CLEANUP=1 ./scripts/integration-test-hierarchy.sh"
echo ""

log_ok "Integration test complete!"
