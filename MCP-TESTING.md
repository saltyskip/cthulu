# cthulu-mcp — Testing Guide

This document shows how to test `cthulu-mcp` at three levels:

1. **Manual pipe tests** — raw JSON-RPC in your terminal (no Claude Desktop needed)
2. **Claude Desktop smoke tests** — natural language prompts that exercise each tool group
3. **What changes between "no MCP" and "with MCP"** — the before/after comparison

---

## Prerequisites

| What | Status check |
|---|---|
| Cthulu backend running | `curl -s http://localhost:8081/api/flows \| head -1` should return JSON |
| SearXNG running | `make searxng-status` → "Health: OK" |
| Binary built | `ls target/release/cthulu-mcp` |
| Claude Desktop config written | `cat ~/Library/Application\ Support/Claude/claude_desktop_config.json` |

If any of the above are missing:

```bash
# Start backend
npm run dev

# Start SearXNG
make searxng-start

# Build binary
make build-mcp

# Register in Claude Desktop
make setup-mcp
```

Then **quit and reopen Claude Desktop**.

---

## Level 1 — Manual pipe tests (terminal)

These use raw JSON-RPC over stdin/stdout. Keep stdin open by appending `; sleep 15` so the server doesn't exit before the tool response arrives.

### Template (copy-paste wrapper)

```bash
# Usage: replace the TOOL_CALL line with any example below
{
  printf '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.1"}}}\n'
  printf '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}\n'
  printf '<TOOL_CALL>\n'
  sleep 15
} | ./target/release/cthulu-mcp \
    --base-url http://localhost:8081 \
    --searxng-url http://127.0.0.1:8888 \
  2>/dev/null | grep '"id":2'
```

### Tool call examples (replace `<TOOL_CALL>`)

**tools/list** — confirm all 30 tools are registered
```json
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
```
Expected: array of 30 tool objects with `name` and `description`.

**web_search via SearXNG** — primary path
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"web_search","arguments":{"query":"rust programming language","num_results":5}}}
```
Expected: `"Found N result(s) via SearXNG:"` with titles, URLs, snippets.

**web_search fallback** — force DDG fallback by pointing at a dead SearXNG URL
```bash
# Add --searxng-url http://127.0.0.1:9999 (port not listening)
```
Expected: `"Found N result(s) via DuckDuckGo (fallback):"`.

**fetch_content**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"fetch_content","arguments":{"url":"https://rust-lang.org"}}}
```
Expected: cleaned plain text from rust-lang.org, truncated at 8 000 chars.

**list_flows**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_flows","arguments":{}}}
```
Expected: JSON array of flow objects from the backend.

**validate_cron**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"validate_cron","arguments":{"expression":"0 9 * * 1-5"}}}
```
Expected: "valid" + next 5 run times.

**get_scheduler_status**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_scheduler_status","arguments":{}}}
```
Expected: list of scheduled flows with next run times.

**get_token_status**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_token_status","arguments":{}}}
```
Expected: token validity and expiry info.

**list_agents**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_agents","arguments":{}}}
```
Expected: JSON array of agents.

**list_prompts**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_prompts","arguments":{}}}
```
Expected: JSON array of saved prompt templates.

**list_templates**
```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_templates","arguments":{}}}
```
Expected: built-in template gallery grouped by category.

---

## Level 2 — Claude Desktop smoke tests

Open Claude Desktop after restarting it. The `cthulu` server should appear in the tool panel (hammer icon). Use these prompts exactly — they cover every tool group.

### 2.1 Connectivity

> "Use the get_token_status tool and tell me if the Claude token is valid."

Expected: green/amber status and an expiry date.

> "Use get_scheduler_status to list which flows are currently scheduled."

Expected: a human-readable list of scheduled flows.

### 2.2 Web search (SearXNG primary path)

> "Search for 'open source AI workflow automation tools 2025' and give me the top 5 results with URLs."

Expected: results labelled **"via SearXNG"**, no rate-limit warnings.

> "Search for 'cron expression best practices' then fetch the content of the first URL and summarise it."

Expected: two tool calls — `web_search` then `fetch_content` — followed by a summary.

### 2.3 Web search (DDG fallback — optional)

Stop SearXNG first: `make searxng-stop`

> "Search for 'python asyncio tutorial'."

Expected: results labelled **"via DuckDuckGo (fallback)"**.

Restart SearXNG after: `make searxng-start`

### 2.4 Flows

> "List all my Cthulu flows. Which ones are enabled?"

Expected: a table or list from `list_flows`.

> "Create a new flow called 'MCP Test Flow' with no nodes. Then immediately get it back and show me its ID."

Expected: `create_flow` → `get_flow` with the returned ID.

> "Validate the cron expression '*/5 * * * *' and tell me when it will next fire."

Expected: valid + next 5 times (every 5 minutes).

> "Delete the flow you just created called 'MCP Test Flow'."

Expected: `delete_flow` success confirmation.

### 2.5 Agents

> "List all agents. Show me the name and description of each."

Expected: table from `list_agents`.

> "Create a new agent called 'Test Agent' with the system prompt 'You are a test assistant.'"

Expected: `create_agent` returns the new agent's ID.

> "Update the agent you just created — change its name to 'Test Agent v2'."

Expected: `update_agent` success.

> "Delete the agent called 'Test Agent v2'."

Expected: `delete_agent` success.

### 2.6 Prompts

> "List all saved prompts in the library."

Expected: `list_prompts` returns an array.

> "Save a new prompt titled 'Daily Stand-up' with the content 'Summarise yesterday's work, today's plan, and any blockers.'"

Expected: `create_prompt` returns a new prompt ID.

> "Update that prompt — add the tag 'productivity'."

Expected: `update_prompt` success.

> "Delete the 'Daily Stand-up' prompt."

Expected: `delete_prompt` success.

### 2.7 Templates

> "Show me all available workflow templates."

Expected: `list_templates` grouped by category (finance, media, research, social).

> "Import the 'crypto-market-brief' template from the finance category."

Expected: `import_template` creates a new flow.

### 2.8 Multi-step reasoning (the real MCP value)

> "I want to build a flow that fetches RSS news about AI every morning at 9 AM and sends a Slack summary. \
> First validate the cron '0 9 * * *', then list my existing flows to see if anything similar already exists, \
> then show me available templates in the media or research category."

Expected: three sequential tool calls — `validate_cron`, `list_flows`, `list_templates` — with the results combined into a plan.

---

## Level 3 — Before / After comparison

This is the core value proposition. The same task, done two ways.

---

### Task: "Find what AI workflow tools exist, then create a matching flow in Cthulu"

#### Without MCP (manual workflow)

1. Open a browser → search DuckDuckGo manually
2. Read 3–5 articles, take notes
3. Open Cthulu Studio in a separate tab
4. Manually design nodes in the canvas
5. Configure cron, sources, executor, sinks one by one
6. Save and enable the flow
7. Switch back to Claude to continue the conversation

**Friction points:**
- Context switching between 3 apps
- Claude has no visibility into what flows already exist
- Claude cannot prevent duplicate flows
- No ability to reason over current state vs desired state

---

#### With MCP (one conversation)

```
You: Research what open source AI workflow tools exist and what makes them different from Cthulu.
     Then check if I already have any related flows, and if not, create one that runs every weekday
     morning and searches for "AI automation news", then show me how to configure an executor node.

Claude: [calls web_search → "open source AI workflow tools comparison 2025"]
        [calls list_flows → checks for duplicates]
        [calls validate_cron → "0 9 * * 1-5"]
        [calls create_flow → {"name":"AI Automation News","nodes":[...],"trigger":{"type":"cron","expression":"0 9 * * 1-5"}}]
        [returns: flow created with ID abc-123, no duplicate found, here's how to configure the executor...]
```

**What changed:**
- Zero context switching
- Claude reasons over live backend state before acting
- Duplicate detection is built into the reasoning chain
- Cron validation happens before flow creation, not after
- The entire workflow is auditable in one conversation thread

---

### Difference table

| Capability | Without MCP | With MCP |
|---|---|---|
| Search the web | Manual browser | `web_search` in-context |
| Fetch and read a page | Copy/paste manually | `fetch_content` in-context |
| Know what flows exist | Open Studio separately | `list_flows` in-context |
| Create / modify flows | Canvas UI | `create_flow` / `update_flow` |
| Validate cron before use | Trial and error | `validate_cron` with preview |
| Reason over agents | View agents tab | `list_agents` + `get_agent` |
| Manage prompts | Prompts panel | `create/update/delete_prompt` |
| Multi-step reasoning | Separate sessions per app | Single conversation thread |
| Duplicate prevention | Manual | Claude checks state first |
| Error recovery | You catch it | Claude retries with corrected params |

---

## Troubleshooting

### "cthulu" server doesn't appear in Claude Desktop
- Quit Claude Desktop completely (Cmd+Q, not just close window)
- Reopen it
- Check `~/Library/Application Support/Claude/claude_desktop_config.json` has the correct binary path
- Verify the binary runs: `./target/release/cthulu-mcp --help`

### Tool calls time out
- Check the backend is running: `curl http://localhost:8081/api/flows`
- Check SearXNG is running: `make searxng-status`

### web_search returns "DuckDuckGo (fallback)" instead of "SearXNG"
- Run `make searxng-status` — container may be stopped
- Run `make searxng-start` to restart it
- The DDG fallback still works; SearXNG just gives higher throughput

### web_search returns 0 results from SearXNG
- DuckDuckGo CAPTCHA is blocking the container IP — this is expected
- Bing and Brave are configured as fallback engines inside SearXNG and should return results
- Check `searxng-settings.yml` has `bing` and `brave` engines enabled

### Re-register after rebuilding the binary
```bash
make setup-mcp   # rewrites the config with current paths
# then restart Claude Desktop
```
