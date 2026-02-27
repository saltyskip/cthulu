import { useState, useEffect, useCallback, useRef } from "react";
import { STUDIO_ASSISTANT_ID, type Agent } from "../types/flow";
import { getAgent, updateAgent, deleteAgent } from "../api/client";
import { Button } from "@/components/ui/button";
import { FormField } from "@/components/ui/form-field";

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
          <Button variant="ghost" size="icon-xs" onClick={onClose}>
            ×
          </Button>
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
        <Button variant="ghost" size="icon-xs" onClick={onClose} title="Close agent editor">
          ×
        </Button>
      </div>

      <div className="property-fields">
        <FormField label="Name">
          <input
            value={agent.name}
            onChange={(e) => handleChange("name", e.target.value)}
            placeholder="Agent name"
          />
        </FormField>

        <FormField label="Description">
          <input
            value={agent.description}
            onChange={(e) => handleChange("description", e.target.value)}
            placeholder="What does this agent do?"
          />
        </FormField>

        <FormField label="Prompt (file path or inline)">
          <textarea
            value={agent.prompt}
            onChange={(e) => handleChange("prompt", e.target.value)}
            placeholder="examples/my_prompt.md"
            rows={6}
          />
        </FormField>

        <FormField label="Permissions (comma separated)">
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
        </FormField>

        <FormField label="System Prompt">
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
        </FormField>

        <FormField label="Working Directory">
          <input
            value={agent.working_dir || ""}
            onChange={(e) =>
              handleChange("working_dir", e.target.value || null)
            }
            placeholder="Default working directory (optional)"
          />
        </FormField>

        <div style={{ padding: "12px 0", borderTop: "1px solid var(--border)", marginTop: 8 }}>
          <div style={{ fontSize: 10, color: "var(--text-secondary)", marginBottom: 8 }}>
            ID: {agent.id}
          </div>
          {agent.id !== STUDIO_ASSISTANT_ID && (
            <Button variant="destructive" size="sm" onClick={handleDelete}>
              Delete Agent
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}
