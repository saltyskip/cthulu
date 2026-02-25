import { useState, useEffect, useRef, type RefObject } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import * as api from "../api/client";
import type { FlowNode } from "../types/flow";
import type { CanvasHandle } from "./Canvas";

interface PropertyPanelProps {
  canvasRef: RefObject<CanvasHandle | null>;
  selectedNodeId: string | null;
  nodeValidationErrors?: Record<string, string[]>;
}

export default function PropertyPanel({
  canvasRef,
  selectedNodeId,
  nodeValidationErrors,
}: PropertyPanelProps) {
  const [node, setNode] = useState<FlowNode | null>(null);
  const [config, setConfig] = useState<Record<string, unknown>>({});

  // Read node from Canvas when selection changes
  useEffect(() => {
    if (!selectedNodeId) {
      setNode(null);
      return;
    }
    const n = canvasRef.current?.getNode(selectedNodeId) ?? null;
    setNode(n);
    if (n) setConfig({ ...n.config });
  }, [selectedNodeId, canvasRef]);

  if (!node) {
    return (
      <div className="property-panel">
        <div className="empty-state">
          <p>Select a node to edit</p>
        </div>
      </div>
    );
  }

  const handleLabelChange = (label: string) => {
    canvasRef.current?.updateNodeData(node.id, { label });
    setNode((prev) => prev ? { ...prev, label } : prev);
  };

  const handleConfigChange = (key: string, value: unknown) => {
    const newConfig = { ...config, [key]: value };
    setConfig(newConfig);
    canvasRef.current?.updateNodeData(node.id, { config: newConfig });
  };

  const handleDelete = () => {
    canvasRef.current?.deleteNode(node.id);
  };

  const errors = node ? (nodeValidationErrors?.[node.id] ?? []) : [];

  return (
    <div className="property-panel">
      <h3>
        <span className={`node-type-badge ${node.node_type}`}>
          {node.node_type}
        </span>{" "}
        {node.kind}
      </h3>

      {errors.length > 0 && (
        <div className="validation-summary">
          {errors.map((err, i) => (
            <div key={i}>{err}</div>
          ))}
        </div>
      )}

      <div className="form-group">
        <label>Label</label>
        <input
          value={node.label}
          onChange={(e) => handleLabelChange(e.target.value)}
        />
      </div>

      {renderConfigFields(node, config, handleConfigChange, errors)}

      <div style={{ marginTop: 24 }}>
        <button className="danger" onClick={handleDelete}>
          Delete Node
        </button>
      </div>
    </div>
  );
}

function fieldHasError(errors: string[], keyword: string): string | undefined {
  return errors.find((e) => e.toLowerCase().includes(keyword.toLowerCase()));
}

function renderConfigFields(
  node: FlowNode,
  config: Record<string, unknown>,
  onChange: (key: string, value: unknown) => void,
  errors: string[] = []
) {
  switch (node.kind) {
    case "cron": {
      const scheduleErr = fieldHasError(errors, "schedule");
      return (
        <CronFields
          config={config}
          onChange={onChange}
          scheduleErr={scheduleErr}
        />
      );
    }
    case "rss": {
      const urlErr = fieldHasError(errors, "feed url");
      return (
        <>
          <div className="form-group">
            <label>Feed URL</label>
            <input
              className={urlErr ? "input-error" : ""}
              value={(config.url as string) || ""}
              onChange={(e) => onChange("url", e.target.value)}
              placeholder="https://example.com/feed"
            />
            {urlErr && <span className="field-error">{urlErr}</span>}
          </div>
          <div className="form-group">
            <label>Limit</label>
            <input
              type="number"
              value={(config.limit as number) || 10}
              onChange={(e) => onChange("limit", parseInt(e.target.value) || 10)}
            />
          </div>
          <div className="form-group">
            <label>Keywords (comma separated, optional)</label>
            <input
              value={
                Array.isArray(config.keywords)
                  ? (config.keywords as string[]).join(", ")
                  : ""
              }
              onChange={(e) =>
                onChange(
                  "keywords",
                  e.target.value.split(",").map((s) => s.trim()).filter(Boolean)
                )
              }
              placeholder="bitcoin, crypto, regulation"
            />
          </div>
        </>
      );
    }
    case "github-merged-prs": {
      const reposErr = fieldHasError(errors, "repos");
      return (
        <>
          <div className="form-group">
            <label>Repos (comma separated)</label>
            <input
              className={reposErr ? "input-error" : ""}
              value={
                Array.isArray(config.repos)
                  ? (config.repos as string[]).join(", ")
                  : ""
              }
              onChange={(e) =>
                onChange(
                  "repos",
                  e.target.value.split(",").map((s) => s.trim()).filter(Boolean)
                )
              }
              placeholder="owner/repo-a, owner/repo-b"
            />
            {reposErr && <span className="field-error">{reposErr}</span>}
          </div>
          <div className="form-group">
            <label>Since Days</label>
            <input
              type="number"
              value={(config.since_days as number) || 7}
              onChange={(e) =>
                onChange("since_days", parseInt(e.target.value) || 7)
              }
            />
          </div>
        </>
      );
    }
    case "claude-code": {
      const promptErr = fieldHasError(errors, "prompt");
      return (
        <ClaudeCodeFields
          config={config}
          onChange={onChange}
          promptErr={promptErr}
        />
      );
    }
    case "vm-sandbox": {
      const promptErr = fieldHasError(errors, "prompt");
      return (
        <>
          <div className="form-group">
            <label>VM Tier</label>
            <select
              value={(config.tier as string) || "nano"}
              onChange={(e) => onChange("tier", e.target.value)}
            >
              <option value="nano">nano (1 vCPU, 512 MB)</option>
              <option value="micro">micro (2 vCPU, 1024 MB)</option>
            </select>
          </div>
          <ClaudeCodeFields
            config={config}
            onChange={onChange}
            promptErr={promptErr}
          />
          <div className="form-group">
            <label style={{ fontSize: "11px", color: "var(--text-secondary)" }}>
              This agent runs in a sandboxed Firecracker VM. Enable the flow to
              provision VMs. Click the node to open an interactive terminal.
            </label>
          </div>
        </>
      );
    }
    case "web-scrape": {
      const urlErr = fieldHasError(errors, "page url");
      return (
        <>
          <div className="form-group">
            <label>Page URL</label>
            <input
              className={urlErr ? "input-error" : ""}
              value={(config.url as string) || ""}
              onChange={(e) => onChange("url", e.target.value)}
              placeholder="https://example.gov/news"
            />
            {urlErr && <span className="field-error">{urlErr}</span>}
          </div>
          <div className="form-group">
            <label>Keywords (comma separated)</label>
            <input
              value={
                Array.isArray(config.keywords)
                  ? (config.keywords as string[]).join(", ")
                  : ""
              }
              onChange={(e) =>
                onChange(
                  "keywords",
                  e.target.value.split(",").map((s) => s.trim()).filter(Boolean)
                )
              }
              placeholder="bitcoin, crypto, regulation"
            />
          </div>
        </>
      );
    }
    case "web-scraper": {
      const urlErr = fieldHasError(errors, "page url");
      const selectorErr = fieldHasError(errors, "items selector");
      return (
        <>
          <div className="form-group">
            <label>Page URL</label>
            <input
              className={urlErr ? "input-error" : ""}
              value={(config.url as string) || ""}
              onChange={(e) => onChange("url", e.target.value)}
              placeholder="https://www.sec.gov/news/pressreleases"
            />
            {urlErr && <span className="field-error">{urlErr}</span>}
          </div>
          <div className="form-group">
            <label>Base URL</label>
            <input
              value={(config.base_url as string) || ""}
              onChange={(e) => onChange("base_url", e.target.value || null)}
              placeholder="https://www.sec.gov"
            />
          </div>
          <div className="form-group">
            <label>Items Selector</label>
            <input
              className={selectorErr ? "input-error" : ""}
              value={(config.items_selector as string) || ""}
              onChange={(e) => onChange("items_selector", e.target.value)}
              placeholder="div.press-release"
            />
            {selectorErr && <span className="field-error">{selectorErr}</span>}
          </div>
          <div className="form-group">
            <label>Title Selector</label>
            <input
              value={(config.title_selector as string) || ""}
              onChange={(e) => onChange("title_selector", e.target.value || null)}
              placeholder="h3 a"
            />
          </div>
          <div className="form-group">
            <label>URL Selector</label>
            <input
              value={(config.url_selector as string) || ""}
              onChange={(e) => onChange("url_selector", e.target.value || null)}
              placeholder="h3 a"
            />
          </div>
          <div className="form-group">
            <label>Summary Selector</label>
            <input
              value={(config.summary_selector as string) || ""}
              onChange={(e) => onChange("summary_selector", e.target.value || null)}
              placeholder="p.summary"
            />
          </div>
          <div className="form-group">
            <label>Limit</label>
            <input
              type="number"
              value={(config.limit as number) || 10}
              onChange={(e) => onChange("limit", parseInt(e.target.value) || 10)}
            />
          </div>
        </>
      );
    }
    case "keyword": {
      const kwErr = fieldHasError(errors, "keywords");
      return (
        <>
          <div className="form-group">
            <label>Keywords (comma separated)</label>
            <input
              className={kwErr ? "input-error" : ""}
              value={
                Array.isArray(config.keywords)
                  ? (config.keywords as string[]).join(", ")
                  : ""
              }
              onChange={(e) =>
                onChange(
                  "keywords",
                  e.target.value.split(",").map((s) => s.trim()).filter(Boolean)
                )
              }
              placeholder="bitcoin, crypto, sec, etf"
            />
            {kwErr && <span className="field-error">{kwErr}</span>}
          </div>
          <div className="form-group">
            <label>Require All</label>
            <select
              value={config.require_all ? "true" : "false"}
              onChange={(e) => onChange("require_all", e.target.value === "true")}
            >
              <option value="false">Any keyword (OR)</option>
              <option value="true">All keywords (AND)</option>
            </select>
          </div>
          <div className="form-group">
            <label>Match Field</label>
            <select
              value={(config.field as string) || "title_or_summary"}
              onChange={(e) => onChange("field", e.target.value)}
            >
              <option value="title_or_summary">Title or Summary</option>
              <option value="title">Title only</option>
              <option value="summary">Summary only</option>
            </select>
          </div>
        </>
      );
    }
    case "slack": {
      const slackErr = fieldHasError(errors, "webhook url or bot token");
      return (
        <>
          <div className="form-group">
            <label>Webhook URL Env</label>
            <input
              className={slackErr ? "input-error" : ""}
              value={(config.webhook_url_env as string) || ""}
              onChange={(e) => onChange("webhook_url_env", e.target.value || null)}
              placeholder="SLACK_WEBHOOK_URL"
            />
          </div>
          <div className="form-group">
            <label>Bot Token Env</label>
            <input
              className={slackErr ? "input-error" : ""}
              value={(config.bot_token_env as string) || ""}
              onChange={(e) => onChange("bot_token_env", e.target.value || null)}
              placeholder="SLACK_BOT_TOKEN"
            />
            {slackErr && <span className="field-error">{slackErr}</span>}
          </div>
          <div className="form-group">
            <label>Channel</label>
            <input
              value={(config.channel as string) || ""}
              onChange={(e) => onChange("channel", e.target.value || null)}
              placeholder="#channel-name"
            />
          </div>
        </>
      );
    }
    case "notion": {
      const tokenErr = fieldHasError(errors, "token env");
      const dbErr = fieldHasError(errors, "database id");
      return (
        <>
          <div className="form-group">
            <label>Token Env</label>
            <input
              className={tokenErr ? "input-error" : ""}
              value={(config.token_env as string) || ""}
              onChange={(e) => onChange("token_env", e.target.value)}
              placeholder="NOTION_TOKEN"
            />
            {tokenErr && <span className="field-error">{tokenErr}</span>}
          </div>
          <div className="form-group">
            <label>Database ID</label>
            <input
              className={dbErr ? "input-error" : ""}
              value={(config.database_id as string) || ""}
              onChange={(e) => onChange("database_id", e.target.value)}
              placeholder="30aac5ee-1a2b-3c4d-5e6f-1234567890ab"
            />
            {dbErr && <span className="field-error">{dbErr}</span>}
          </div>
        </>
      );
    }
    case "github-pr": {
      const repos = (config.repos as { slug: string; path: string }[]) || [];
      const updateRepo = (index: number, field: "slug" | "path", value: string) => {
        const updated = repos.map((r, i) => i === index ? { ...r, [field]: value } : r);
        onChange("repos", updated);
      };
      const addRepo = () => onChange("repos", [...repos, { slug: "", path: "." }]);
      const removeRepo = (index: number) => onChange("repos", repos.filter((_, i) => i !== index));
      return (
        <>
          <div className="form-group">
            <label>Repositories</label>
            {repos.map((repo, i) => (
              <div key={i} style={{ display: "flex", gap: "4px", marginBottom: "4px" }}>
                <input
                  style={{ flex: 1 }}
                  placeholder="owner/repo"
                  value={repo.slug}
                  onChange={(e) => updateRepo(i, "slug", e.target.value)}
                />
                <input
                  style={{ flex: 1 }}
                  placeholder="local path"
                  value={repo.path}
                  onChange={(e) => updateRepo(i, "path", e.target.value)}
                />
                <button className="ghost" style={{ padding: "2px 6px" }} onClick={() => removeRepo(i)}>×</button>
              </div>
            ))}
            <button className="ghost" style={{ fontSize: "12px" }} onClick={addRepo}>+ Add repo</button>
          </div>
          <div className="form-group">
            <label>Poll Interval (seconds)</label>
            <input
              type="number"
              value={(config.poll_interval as number) || 60}
              onChange={(e) =>
                onChange("poll_interval", parseInt(e.target.value) || 60)
              }
            />
          </div>
          <div className="form-group">
            <label>Skip Drafts</label>
            <select
              value={config.skip_drafts === false ? "false" : "true"}
              onChange={(e) => onChange("skip_drafts", e.target.value === "true")}
            >
              <option value="true">Yes</option>
              <option value="false">No</option>
            </select>
          </div>
          <div className="form-group">
            <label>Review on Push</label>
            <select
              value={config.review_on_push ? "true" : "false"}
              onChange={(e) =>
                onChange("review_on_push", e.target.value === "true")
              }
            >
              <option value="false">No</option>
              <option value="true">Yes</option>
            </select>
          </div>
        </>
      );
    }
    default:
      return (
        <div className="form-group">
          <label>Config (JSON)</label>
          <textarea
            value={JSON.stringify(config, null, 2)}
            onChange={(e) => {
              try {
                const parsed = JSON.parse(e.target.value);
                Object.keys(parsed).forEach((key) => onChange(key, parsed[key]));
              } catch {
                // Invalid JSON, ignore
              }
            }}
          />
        </div>
      );
  }
}

function ClaudeCodeFields({
  config,
  onChange,
  promptErr,
}: {
  config: Record<string, unknown>;
  onChange: (key: string, value: unknown) => void;
  promptErr: string | undefined;
}) {
  const handleImport = async () => {
    try {
      const selected = await open({
        title: "Select Prompt File",
        multiple: false,
        filters: [
          { name: "Markdown", extensions: ["md"] },
          { name: "Text", extensions: ["txt"] },
          { name: "All Files", extensions: ["*"] },
        ],
      });
      if (selected) {
        onChange("prompt", selected);
      }
    } catch {
      // User cancelled or Tauri not available
    }
  };

  return (
    <>
      <div className="form-group">
        <label>Prompt (file path or inline)</label>
        <textarea
          className={promptErr ? "input-error" : ""}
          value={(config.prompt as string) || ""}
          onChange={(e) => onChange("prompt", e.target.value)}
          placeholder="examples/my_prompt.md"
        />
        {promptErr && <span className="field-error">{promptErr}</span>}
        <button
          className="ghost"
          style={{ marginTop: 6, fontSize: 11 }}
          onClick={handleImport}
        >
          Import from Library
        </button>
      </div>
      <div className="form-group">
        <label>Permissions (comma separated)</label>
        <input
          value={
            Array.isArray(config.permissions)
              ? (config.permissions as string[]).join(", ")
              : ""
          }
          onChange={(e) =>
            onChange(
              "permissions",
              e.target.value.split(",").map((s) => s.trim()).filter(Boolean)
            )
          }
          placeholder="Bash, Read, Grep, Glob"
        />
      </div>
      <div className="form-group">
        <label>System Prompt</label>
        <textarea
          value={(config.append_system_prompt as string) || ""}
          onChange={(e) =>
            onChange(
              "append_system_prompt",
              e.target.value || null
            )
          }
          placeholder="Additional instructions appended to Claude's system prompt"
          rows={4}
        />
      </div>
    </>
  );
}

function CronFields({
  config,
  onChange,
  scheduleErr,
}: {
  config: Record<string, unknown>;
  onChange: (key: string, value: unknown) => void;
  scheduleErr: string | undefined;
}) {
  const [cronPreview, setCronPreview] = useState<api.CronValidation | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const schedule = (config.schedule as string) || "";

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);

    if (!schedule.trim()) {
      setCronPreview(null);
      return;
    }

    debounceRef.current = setTimeout(() => {
      api.validateCron(schedule.trim()).then(setCronPreview).catch(() => {});
    }, 400);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [schedule]);

  const formatTime = (iso: string) => {
    try {
      const d = new Date(iso);
      return d.toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      });
    } catch {
      return iso;
    }
  };

  return (
    <>
      <div className="form-group">
        <label>Schedule (cron)</label>
        <input
          className={scheduleErr || (cronPreview && !cronPreview.valid) ? "input-error" : ""}
          value={schedule}
          onChange={(e) => onChange("schedule", e.target.value)}
          placeholder="0 */4 * * *"
        />
        {scheduleErr && <span className="field-error">{scheduleErr}</span>}
        {cronPreview && !cronPreview.valid && !scheduleErr && (
          <span className="field-error">{cronPreview.error}</span>
        )}
        {cronPreview && cronPreview.valid && cronPreview.next_runs.length > 0 && (
          <div className="cron-preview">
            <span className="cron-preview-label">Next runs:</span>
            {cronPreview.next_runs.map((t, i) => (
              <span key={i} className="cron-preview-time">{formatTime(t)}</span>
            ))}
          </div>
        )}
      </div>
      <div className="form-group">
        <label>Working Directory</label>
        <input
          value={(config.working_dir as string) || "."}
          onChange={(e) => onChange("working_dir", e.target.value)}
        />
      </div>
    </>
  );
}
