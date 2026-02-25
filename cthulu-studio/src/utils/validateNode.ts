import type { FlowNode } from "../types/flow";

function isValidCron(expr: string): boolean {
  const tokens = expr.trim().split(/\s+/);
  return tokens.length >= 5 && tokens.length <= 6;
}

const UUID_RE = /^[0-9a-f]{8}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{12}$/i;
function isValidUuid(s: string): boolean {
  return UUID_RE.test(s.trim());
}

export function validateNode(node: FlowNode): string[] {
  const errors: string[] = [];
  const cfg = node.config;

  switch (node.kind) {
    case "cron":
      if (!cfg.schedule || !(cfg.schedule as string).trim()) {
        errors.push("Schedule is required");
      } else if (!isValidCron(cfg.schedule as string)) {
        errors.push("Schedule must be a valid cron expression (5-6 tokens)");
      }
      break;
    case "rss":
      if (!cfg.url || !(cfg.url as string).trim()) {
        errors.push("Feed URL is required");
      }
      break;
    case "web-scrape":
      if (!cfg.url || !(cfg.url as string).trim()) {
        errors.push("Page URL is required");
      }
      break;
    case "web-scraper":
      if (!cfg.url || !(cfg.url as string).trim()) {
        errors.push("Page URL is required");
      }
      if (!cfg.items_selector || !(cfg.items_selector as string).trim()) {
        errors.push("Items selector is required");
      }
      break;
    case "github-merged-prs":
      if (!Array.isArray(cfg.repos) || cfg.repos.length === 0) {
        errors.push("Repos is required");
      }
      break;
    case "keyword":
      if (!Array.isArray(cfg.keywords) || cfg.keywords.length === 0) {
        errors.push("Keywords is required");
      }
      break;
    case "claude-code":
      if (!cfg.prompt || !(cfg.prompt as string).trim()) {
        errors.push("Prompt is required");
      }
      break;
    case "vm-sandbox":
      // No required fields — tier has a default, api_key is optional
      break;
    case "slack":
      if (
        (!cfg.webhook_url_env || !(cfg.webhook_url_env as string).trim()) &&
        (!cfg.bot_token_env || !(cfg.bot_token_env as string).trim())
      ) {
        errors.push("Webhook URL or Bot Token is required");
      }
      break;
    case "notion":
      if (!cfg.token_env || !(cfg.token_env as string).trim()) {
        errors.push("Token env is required");
      }
      if (!cfg.database_id || !(cfg.database_id as string).trim()) {
        errors.push("Database ID is required");
      } else if (!isValidUuid(cfg.database_id as string)) {
        errors.push("Database ID must be a valid UUID (e.g. 30aac5ee-1a2b-3c4d-5e6f-1234567890ab)");
      }
      break;
  }

  return errors;
}

export function validateFlow(nodes: FlowNode[]): Record<string, string[]> {
  const result: Record<string, string[]> = {};
  for (const node of nodes) {
    const errors = validateNode(node);
    if (errors.length > 0) {
      result[node.id] = errors;
    }
  }
  return result;
}
