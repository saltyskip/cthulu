//! CthuluMcpServer — the root rmcp server struct.
//!
//! All tools use the rmcp pattern:
//!   #[tool_router] on impl CthuluMcpServer
//!   #[tool(description = "...")] on each method
//!   Parameters<T> wrapper for typed inputs (T: Deserialize + JsonSchema)
//!   #[tool_handler] on impl ServerHandler

pub mod agents;
pub mod flows;
pub mod prompts;
pub mod search;
pub mod utility;

use std::sync::Arc;

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Content, Implementation, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    schemars, serde,
    tool, tool_handler, tool_router,
    ErrorData as McpError,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::client::CthuluClient;
use crate::search::SearchClient;

// ── Server struct ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CthuluMcpServer {
    pub cthulu: Arc<CthuluClient>,
    pub search: Arc<SearchClient>,
    tool_router: ToolRouter<Self>,
}

impl CthuluMcpServer {
    pub fn new(cthulu: Arc<CthuluClient>, search: Arc<SearchClient>) -> Self {
        Self {
            cthulu,
            search,
            tool_router: Self::tool_router(),
        }
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

pub fn ok(text: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(text.into())]))
}

pub fn err(e: impl std::fmt::Display) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

// ── Parameter structs ─────────────────────────────────────────────────────────

#[derive(Deserialize, JsonSchema)]
pub struct IdParam {
    #[schemars(description = "Resource ID")]
    pub id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct BodyParam {
    #[schemars(description = "Resource definition as a JSON string")]
    pub body: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct IdBodyParam {
    #[schemars(description = "Resource ID")]
    pub id: String,
    #[schemars(description = "Updated fields as a JSON string")]
    pub body: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct WebSearchParam {
    #[schemars(description = "Search query string")]
    pub query: String,
    #[schemars(description = "Maximum number of results to return (default: 10)")]
    pub max_results: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
pub struct FetchContentParam {
    #[schemars(description = "URL of the webpage to fetch")]
    pub url: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct TriggerFlowParam {
    #[schemars(description = "Flow ID")]
    pub id: String,
    #[schemars(description = "Optional trigger context as a JSON string (e.g. PR info)")]
    pub body: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct AgentIdParam {
    #[schemars(description = "Agent ID")]
    pub agent_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SessionParam {
    #[schemars(description = "Agent ID")]
    pub agent_id: String,
    #[schemars(description = "Session ID")]
    pub session_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ChatParam {
    #[schemars(description = "Agent ID")]
    pub agent_id: String,
    #[schemars(description = "Message to send to the agent")]
    pub message: String,
    #[schemars(description = "Session ID to continue (omit to create a new session)")]
    pub session_id: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TemplateCategorySlug {
    #[schemars(description = "Template category (finance, media, research, social)")]
    pub category: String,
    #[schemars(description = "Template slug (e.g. crypto-market-brief)")]
    pub slug: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct CronParam {
    #[schemars(description = "Cron expression (5 fields: min hour dom month dow). Example: 0 9 * * 1-5")]
    pub expression: String,
}

// ── Tool impls ────────────────────────────────────────────────────────────────

#[tool_router]
impl CthuluMcpServer {
    // ── Web Search ────────────────────────────────────────────────────────────

    #[tool(description = "\
        Search the web using DuckDuckGo via a self-hosted SearXNG instance (unlimited, \
        no rate limit). Falls back to direct DuckDuckGo scraping (rate-limited to \
        30 req/min) when SearXNG is unavailable. Returns titles, URLs and snippets.")]
    async fn web_search(
        &self,
        Parameters(p): Parameters<WebSearchParam>,
    ) -> Result<CallToolResult, McpError> {
        search::web_search(self, p.query, p.max_results).await
    }

    #[tool(description = "\
        Fetch and parse the text content of a webpage URL. \
        Strips scripts, styles, navigation and boilerplate. \
        Truncates at 8 000 characters. \
        Rate-limited to 20 fetches/min on the DuckDuckGo fallback path.")]
    async fn fetch_content(
        &self,
        Parameters(p): Parameters<FetchContentParam>,
    ) -> Result<CallToolResult, McpError> {
        search::fetch_content(self, p.url).await
    }

    // ── Flows ─────────────────────────────────────────────────────────────────

    #[tool(description = "List all Cthulu flows with their id, name, enabled status and trigger type. Flows are stored at ~/.cthulu/flows/<id>.json.")]
    async fn list_flows(&self) -> Result<CallToolResult, McpError> {
        flows::list_flows(self).await
    }

    #[tool(description = "Get the raw JSON definition of a flow (nodes, edges, config, version). Use describe_flow for a human-readable summary.")]
    async fn get_flow(
        &self,
        Parameters(p): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        flows::get_flow(self, p.id).await
    }

    #[tool(description = "\
        Return a human-readable description of a flow's pipeline: DAG shape, \
        every node's kind + key config, executor prompt preview, cron schedule, \
        sources (RSS URLs, repos, etc.) and sinks. \
        Much more useful than get_flow for understanding what a flow does.")]
    async fn describe_flow(
        &self,
        Parameters(p): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        flows::describe_flow(self, p.id).await
    }

    #[tool(description = "\
        Create a new Cthulu flow. Supply a JSON object with at minimum a 'name' field. \
        Optionally include 'description', 'enabled', 'nodes' array, and 'edges' array. \
        Use get_node_types to see valid node_type values and their config schemas. \
        Example minimal body: {\"name\": \"My Flow\"}. \
        Example with a cron trigger: {\"name\": \"Daily Brief\", \"nodes\": [{\"id\": \"t1\", \
        \"node_type\": \"trigger\", \"kind\": \"cron\", \"config\": {\"schedule\": \"0 9 * * 1-5\"}, \
        \"position\": {\"x\": 0, \"y\": 0}, \"label\": \"Weekday 9am\"}], \"edges\": []}")]
    async fn create_flow(
        &self,
        Parameters(p): Parameters<BodyParam>,
    ) -> Result<CallToolResult, McpError> {
        flows::create_flow(self, p.body).await
    }

    #[tool(description = "\
        Update an existing flow by ID. \
        Supply the full or partial flow JSON. \
        Include 'version' (the current version integer from get_flow) to avoid overwrite conflicts. \
        To enable/disable: {\"enabled\": true}. \
        To rename: {\"name\": \"New Name\"}. \
        To swap a node's cron schedule: fetch the flow, modify nodes[i].config.schedule, send back.")]
    async fn update_flow(
        &self,
        Parameters(p): Parameters<IdBodyParam>,
    ) -> Result<CallToolResult, McpError> {
        flows::update_flow(self, p.id, p.body).await
    }

    #[tool(description = "Permanently delete a flow and all its run history. This cannot be undone.")]
    async fn delete_flow(
        &self,
        Parameters(p): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        flows::delete_flow(self, p.id).await
    }

    #[tool(description = "\
        Manually trigger a flow run right now, even if the flow is disabled. \
        Optionally pass a JSON body for webhook/PR trigger context (e.g. {\"pr\": 42, \"repo\": \"owner/repo\"}).")]
    async fn trigger_flow(
        &self,
        Parameters(p): Parameters<TriggerFlowParam>,
    ) -> Result<CallToolResult, McpError> {
        flows::trigger_flow(self, p.id, p.body).await
    }

    #[tool(description = "Get the last 100 run records for a flow (status, start/end times, node results, cost).")]
    async fn get_flow_runs(
        &self,
        Parameters(p): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        flows::get_flow_runs(self, p.id).await
    }

    #[tool(description = "Get the next scheduled run times for a cron-triggered flow.")]
    async fn get_flow_schedule(
        &self,
        Parameters(p): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        flows::get_flow_schedule(self, p.id).await
    }

    #[tool(description = "\
        List all workflow JSON files on disk at ~/.cthulu/flows/ with file size and last-modified time. \
        Works even when the backend is not running. \
        Use this to audit what's on disk or find recently modified flows.")]
    async fn list_workflow_files(&self) -> Result<CallToolResult, McpError> {
        flows::list_workflow_files(self).await
    }

    // ── Agents ────────────────────────────────────────────────────────────────

    #[tool(description = "List all Cthulu agents with their id, name and description.")]
    async fn list_agents(&self) -> Result<CallToolResult, McpError> {
        agents::list_agents(self).await
    }

    #[tool(description = "Get the full configuration of an agent (system prompt, permissions, working directory).")]
    async fn get_agent(
        &self,
        Parameters(p): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        agents::get_agent(self, p.id).await
    }

    #[tool(description = "\
        Create a new Cthulu agent. \
        Supply a JSON object with 'name', 'prompt', and optional 'permissions' and 'working_dir'.")]
    async fn create_agent(
        &self,
        Parameters(p): Parameters<BodyParam>,
    ) -> Result<CallToolResult, McpError> {
        agents::create_agent(self, p.body).await
    }

    #[tool(description = "Update an existing agent's name, system prompt, permissions or working directory.")]
    async fn update_agent(
        &self,
        Parameters(p): Parameters<IdBodyParam>,
    ) -> Result<CallToolResult, McpError> {
        agents::update_agent(self, p.id, p.body).await
    }

    #[tool(description = "Delete an agent. Cannot delete the built-in 'studio-assistant' agent.")]
    async fn delete_agent(
        &self,
        Parameters(p): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        agents::delete_agent(self, p.id).await
    }

    #[tool(description = "List all active and historical chat sessions for an agent.")]
    async fn list_agent_sessions(
        &self,
        Parameters(p): Parameters<AgentIdParam>,
    ) -> Result<CallToolResult, McpError> {
        agents::list_agent_sessions(self, p.agent_id).await
    }

    #[tool(description = "Create a new interactive chat session for an agent (max 5 concurrent sessions per agent).")]
    async fn create_agent_session(
        &self,
        Parameters(p): Parameters<AgentIdParam>,
    ) -> Result<CallToolResult, McpError> {
        agents::create_agent_session(self, p.agent_id).await
    }

    #[tool(description = "Delete a specific chat session for an agent.")]
    async fn delete_agent_session(
        &self,
        Parameters(p): Parameters<SessionParam>,
    ) -> Result<CallToolResult, McpError> {
        agents::delete_agent_session(self, p.agent_id, p.session_id).await
    }

    #[tool(description = "\
        Retrieve the full JSONL event log for a completed or active session. \
        Each line is a JSON object representing one Claude stream event.")]
    async fn get_session_log(
        &self,
        Parameters(p): Parameters<SessionParam>,
    ) -> Result<CallToolResult, McpError> {
        agents::get_session_log(self, p.agent_id, p.session_id).await
    }

    #[tool(description = "\
        Send a message to an agent and wait for the response. \
        Creates a new session if session_id is not provided. \
        Polls until the agent finishes responding (timeout: 120 s). \
        Returns the session_id and the last assistant turn as text.")]
    async fn chat_with_agent(
        &self,
        Parameters(p): Parameters<ChatParam>,
    ) -> Result<CallToolResult, McpError> {
        agents::chat_with_agent(self, p.agent_id, p.message, p.session_id).await
    }

    // ── Prompts ───────────────────────────────────────────────────────────────

    #[tool(description = "List all saved prompt templates in the Cthulu prompt library.")]
    async fn list_prompts(&self) -> Result<CallToolResult, McpError> {
        prompts::list_prompts(self).await
    }

    #[tool(description = "Get the full content and metadata of a saved prompt template.")]
    async fn get_prompt(
        &self,
        Parameters(p): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        prompts::get_prompt(self, p.id).await
    }

    #[tool(description = "\
        Save a new prompt template to the Cthulu prompt library. \
        Supply a JSON object with 'title' and 'content' fields. Optional: 'tags' array.")]
    async fn create_prompt(
        &self,
        Parameters(p): Parameters<BodyParam>,
    ) -> Result<CallToolResult, McpError> {
        prompts::create_prompt(self, p.body).await
    }

    #[tool(description = "Update the title, content or tags of an existing prompt template.")]
    async fn update_prompt(
        &self,
        Parameters(p): Parameters<IdBodyParam>,
    ) -> Result<CallToolResult, McpError> {
        prompts::update_prompt(self, p.id, p.body).await
    }

    #[tool(description = "Delete a prompt template from the library.")]
    async fn delete_prompt(
        &self,
        Parameters(p): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        prompts::delete_prompt(self, p.id).await
    }

    // ── Utility ───────────────────────────────────────────────────────────────

    #[tool(description = "\
        Return the complete node type schema: all valid node_type values (trigger/source/filter/executor/sink), \
        their kind variants, and every config field each kind accepts. \
        ALWAYS call this before create_flow or update_flow to get correct field names and values. \
        Also includes prompt template variables ({{content}}, {{item_count}}, etc.) \
        and the workflow file storage path (~/.cthulu/flows/).")]
    async fn get_node_types(&self) -> Result<CallToolResult, McpError> {
        utility::get_node_types(self).await
    }

    #[tool(description = "\
        List all built-in workflow templates grouped by category \
        (finance, media, research, social). Each entry includes a slug, \
        description and node summary. Use import_template to instantiate one.")]
    async fn list_templates(&self) -> Result<CallToolResult, McpError> {
        utility::list_templates(self).await
    }

    #[tool(description = "\
        Instantiate a built-in workflow template as a new flow. \
        Use list_templates to find available category/slug pairs. \
        Example: category='finance', slug='crypto-market-brief'.")]
    async fn import_template(
        &self,
        Parameters(p): Parameters<TemplateCategorySlug>,
    ) -> Result<CallToolResult, McpError> {
        utility::import_template(self, p.category, p.slug).await
    }

    #[tool(description = "\
        Validate a 5-field cron expression and preview the next 5 scheduled run times. \
        Example: '0 9 * * 1-5' (9 AM weekdays). Always validate before embedding in a flow.")]
    async fn validate_cron(
        &self,
        Parameters(p): Parameters<CronParam>,
    ) -> Result<CallToolResult, McpError> {
        utility::validate_cron(self, p.expression).await
    }

    #[tool(description = "Get an overview of the Cthulu scheduler: which flows are actively scheduled, their next run times and trigger types.")]
    async fn get_scheduler_status(&self) -> Result<CallToolResult, McpError> {
        utility::get_scheduler_status(self).await
    }

    #[tool(description = "\
        Check the Claude OAuth token status. \
        Returns: status (valid/expired/missing), expires_at (ISO timestamp), \
        is_expired (bool), subscription_type, and rate_limit_tier. \
        The token is stored in the macOS Keychain under 'Claude Code-credentials'. \
        If expired, tell the user to run 'claude' in their terminal to re-authenticate, \
        or use the refresh_token action in Cthulu Studio. \
        DO NOT attempt to curl or shell out — this tool returns everything needed.")]
    async fn get_token_status(&self) -> Result<CallToolResult, McpError> {
        utility::get_token_status(self).await
    }
}

// ── ServerHandler impl ────────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for CthuluMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::LATEST,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "cthulu-mcp".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            instructions: Some(
                "MCP server for Cthulu — an AI workflow automation platform that runs \
Claude Code agents in visual DAG pipelines.\n\
\n\
## Architecture\n\
Flows are stored as JSON files at ~/.cthulu/flows/<uuid>.json. \
Each flow is a DAG: trigger -> sources -> (filters) -> executor -> sinks.\n\
The backend REST API runs at http://localhost:8081.\n\
\n\
## Recommended workflow for creating a flow\n\
1. Call get_node_types  -- learn the schema (node kinds, config fields, prompt variables)\n\
2. Call validate_cron   -- verify any cron expression before embedding it\n\
3. Call list_flows      -- check for duplicates\n\
4. Call create_flow     -- supply full nodes + edges JSON\n\
5. Call describe_flow   -- confirm the DAG looks right\n\
\n\
## Node types (summary — call get_node_types for full schema)\n\
trigger  : cron | github-pr | webhook | manual\n\
source   : rss | web-scrape | web-scraper | github-merged-prs | market-data | google-sheets\n\
filter   : keyword\n\
executor : claude-code | vm-sandbox\n\
sink     : slack | notion\n\
\n\
## Key rules\n\
- node_type values MUST be lowercase (trigger, source, executor, sink)\n\
- Every node needs: id, node_type, kind, config, position {x,y}, label\n\
- Executor prompt can be inline text or a relative path like 'examples/prompts/brief.md'\n\
- Prompt template variables: {{content}}, {{item_count}}, {{timestamp}}, {{market_data}}\n\
- update_flow requires 'version' (the integer from get_flow) to avoid overwrite conflicts\n\
\n\
## Tools by category\n\
Flows   : list_flows, get_flow, describe_flow, create_flow, update_flow,\n\
          delete_flow, trigger_flow, get_flow_runs, get_flow_schedule, list_workflow_files\n\
Agents  : list_agents, get_agent, create_agent, update_agent, delete_agent,\n\
          list_agent_sessions, create_agent_session, delete_agent_session,\n\
          get_session_log, chat_with_agent\n\
Prompts : list_prompts, get_prompt, create_prompt, update_prompt, delete_prompt\n\
Utility : get_node_types, list_templates, import_template,\n\
          validate_cron, get_scheduler_status, get_token_status\n\
Search  : web_search, fetch_content"
                    .into(),
            ),
        }
    }
}
