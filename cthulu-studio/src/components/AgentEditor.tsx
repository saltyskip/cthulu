import { useState, useEffect, useCallback, useRef } from "react";
import type { Agent } from "../types/flow";
import { getAgent, updateAgent, deleteAgent } from "../api/client";

interface AgentEditorProps {
  agentId: string;
  onClose: () => void;
  onDeleted: () => void;
}

export default function AgentEditor({
  agentId,
  onClose,
  onDeleted,
}: AgentEditorProps) {
  const [agent, setAgent] = useState<Agent | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getAgent(agentId)
      .then((a) => {
        if (!cancelled) {
          setAgent(a);
          setLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [agentId]);

  const debouncedSave = useCallback(
    (updated: Agent) => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(async () => {
        setSaving(true);
        try {
          await updateAgent(updated.id, {
            name: updated.name,
            description: updated.description,
            prompt: updated.prompt,
            permissions: updated.permissions,
            append_system_prompt: updated.append_system_prompt,
            working_dir: updated.working_dir,
          });
        } catch (e) {
          console.error("Failed to save agent:", e);
        }
        setSaving(false);
      }, 500);
    },
    []
  );

  function handleChange<K extends keyof Agent>(key: K, value: Agent[K]) {
    if (!agent) return;
    const updated = { ...agent, [key]: value };
    setAgent(updated);
    debouncedSave(updated);
  }

  async function handleDelete() {
    if (!agent) return;
    if (!confirm(`Delete agent "${agent.name}"?`)) return;
    try {
      await deleteAgent(agent.id);
      onDeleted();
    } catch (e) {
      console.error("Failed to delete agent:", e);
    }
  }

  if (loading) {
    return (
      <div className="property-panel">
        <div className="sidebar-header">
          <h2>Agent</h2>
        </div>
        <div style={{ padding: 12, fontSize: 12, color: "var(--text-secondary)" }}>
          Loading...
        </div>
      </div>
    );
  }

  if (!agent) {
    return (
      <div className="property-panel">
        <div className="sidebar-header">
          <h2>Agent</h2>
          <button className="ghost" onClick={onClose}>
            ×
          </button>
        </div>
        <div style={{ padding: 12, fontSize: 12, color: "var(--text-secondary)" }}>
          Agent not found
        </div>
      </div>
    );
  }

  return (
    <div className="property-panel">
      <div className="sidebar-header">
        <h2>
          Agent
          {saving && (
            <span
              style={{
                fontSize: 10,
                fontWeight: "normal",
                color: "var(--text-secondary)",
                marginLeft: 6,
              }}
            >
              saving...
            </span>
          )}
        </h2>
        <button className="ghost" onClick={onClose} title="Close agent editor">
          ×
        </button>
      </div>

      <div className="property-fields">
        <div className="form-group">
          <label>Name</label>
          <input
            value={agent.name}
            onChange={(e) => handleChange("name", e.target.value)}
            placeholder="Agent name"
          />
        </div>

        <div className="form-group">
          <label>Description</label>
          <input
            value={agent.description}
            onChange={(e) => handleChange("description", e.target.value)}
            placeholder="What does this agent do?"
          />
        </div>

        <div className="form-group">
          <label>Prompt (file path or inline)</label>
          <textarea
            value={agent.prompt}
            onChange={(e) => handleChange("prompt", e.target.value)}
            placeholder="examples/my_prompt.md"
            rows={6}
          />
        </div>

        <div className="form-group">
          <label>Permissions (comma separated)</label>
          <input
            value={agent.permissions.join(", ")}
            onChange={(e) =>
              handleChange(
                "permissions",
                e.target.value
                  .split(",")
                  .map((s) => s.trim())
                  .filter(Boolean)
              )
            }
            placeholder="Bash, Read, Grep, Glob"
          />
        </div>

        <div className="form-group">
          <label>System Prompt</label>
          <textarea
            value={agent.append_system_prompt || ""}
            onChange={(e) =>
              handleChange(
                "append_system_prompt",
                e.target.value || null
              )
            }
            placeholder="Additional instructions appended to Claude's system prompt"
            rows={4}
          />
        </div>

        <div className="form-group">
          <label>Working Directory</label>
          <input
            value={agent.working_dir || ""}
            onChange={(e) =>
              handleChange("working_dir", e.target.value || null)
            }
            placeholder="Default working directory (optional)"
          />
        </div>

        <div style={{ padding: "12px 0", borderTop: "1px solid var(--border)", marginTop: 8 }}>
          <div style={{ fontSize: 10, color: "var(--text-secondary)", marginBottom: 8 }}>
            ID: {agent.id}
          </div>
          <button className="danger" onClick={handleDelete}>
            Delete Agent
          </button>
        </div>
      </div>
    </div>
  );
}
