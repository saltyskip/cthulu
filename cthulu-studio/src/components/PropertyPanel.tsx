import { useState, useEffect, type RefObject } from "react";
import type { FlowNode } from "../types/flow";
import type { CanvasHandle } from "./Canvas";

interface PropertyPanelProps {
  canvasRef: RefObject<CanvasHandle | null>;
  selectedNodeId: string | null;
}

export default function PropertyPanel({
  canvasRef,
  selectedNodeId,
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

  return (
    <div className="property-panel">
      <h3>
        <span className={`node-type-badge ${node.node_type}`}>
          {node.node_type}
        </span>{" "}
        {node.kind}
      </h3>

      <div className="form-group">
        <label>Label</label>
        <input
          value={node.label}
          onChange={(e) => handleLabelChange(e.target.value)}
        />
      </div>

      {renderConfigFields(node, config, handleConfigChange)}

      <div style={{ marginTop: 24 }}>
        <button className="danger" onClick={handleDelete}>
          Delete Node
        </button>
      </div>
    </div>
  );
}

function renderConfigFields(
  node: FlowNode,
  config: Record<string, unknown>,
  onChange: (key: string, value: unknown) => void
) {
  switch (node.kind) {
    case "cron":
      return (
        <>
          <div className="form-group">
            <label>Schedule (cron)</label>
            <input
              value={(config.schedule as string) || ""}
              onChange={(e) => onChange("schedule", e.target.value)}
              placeholder="0 */4 * * *"
            />
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
    case "rss":
      return (
        <>
          <div className="form-group">
            <label>Feed URL</label>
            <input
              value={(config.url as string) || ""}
              onChange={(e) => onChange("url", e.target.value)}
              placeholder="https://example.com/feed"
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
    case "github-merged-prs":
      return (
        <>
          <div className="form-group">
            <label>Repos (comma separated)</label>
            <input
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
    case "claude-code":
      return (
        <>
          <div className="form-group">
            <label>Prompt (file path or inline)</label>
            <textarea
              value={(config.prompt as string) || ""}
              onChange={(e) => onChange("prompt", e.target.value)}
              placeholder="prompts/my_prompt.md"
            />
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
        </>
      );
    case "slack":
      return (
        <>
          <div className="form-group">
            <label>Webhook URL Env</label>
            <input
              value={(config.webhook_url_env as string) || ""}
              onChange={(e) => onChange("webhook_url_env", e.target.value || null)}
              placeholder="SLACK_WEBHOOK_URL"
            />
          </div>
          <div className="form-group">
            <label>Bot Token Env</label>
            <input
              value={(config.bot_token_env as string) || ""}
              onChange={(e) => onChange("bot_token_env", e.target.value || null)}
              placeholder="SLACK_BOT_TOKEN"
            />
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
    case "notion":
      return (
        <>
          <div className="form-group">
            <label>Token Env</label>
            <input
              value={(config.token_env as string) || ""}
              onChange={(e) => onChange("token_env", e.target.value)}
              placeholder="NOTION_TOKEN"
            />
          </div>
          <div className="form-group">
            <label>Database ID</label>
            <input
              value={(config.database_id as string) || ""}
              onChange={(e) => onChange("database_id", e.target.value)}
            />
          </div>
        </>
      );
    case "github-pr":
      return (
        <>
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
