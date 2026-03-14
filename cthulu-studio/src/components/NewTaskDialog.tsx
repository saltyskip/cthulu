import { useState, useMemo } from "react";
import * as api from "../api/client";
import type { AgentSummary } from "../types/flow";
import { STUDIO_ASSISTANT_ID } from "../types/flow";

interface NewTaskDialogProps {
  defaultAgentId: string;
  agents: AgentSummary[];
  onClose: () => void;
  onCreated: () => void;
}

export function NewTaskDialog({ defaultAgentId, agents, onClose, onCreated }: NewTaskDialogProps) {
  const [title, setTitle] = useState("");
  const [assigneeId, setAssigneeId] = useState(defaultAgentId);
  const [error, setError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const assignableAgents = useMemo(() => {
    return agents.filter(
      (a) => a.id !== STUDIO_ASSISTANT_ID && !a.subagent_only
    );
  }, [agents]);

  const handleCreate = async () => {
    if (!title.trim()) {
      setError("Title is required");
      return;
    }
    if (!assigneeId) {
      setError("Please select an assignee");
      return;
    }

    setCreating(true);
    setError(null);
    try {
      await api.createTask(title.trim(), assigneeId);
      onCreated();
      onClose();
    } catch (e) {
      setError(typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="dialog-overlay" onClick={onClose}>
      <div className="dialog-content" onClick={(e) => e.stopPropagation()}>
        <h3 className="dialog-title">New Task</h3>

        <div className="dialog-field">
          <label>Title</label>
          <input
            type="text"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="What needs to be done?"
            autoFocus
            onKeyDown={(e) => {
              if (e.key === "Enter" && !creating) handleCreate();
            }}
          />
        </div>

        <div className="dialog-field">
          <label>Assignee</label>
          <select
            value={assigneeId}
            onChange={(e) => setAssigneeId(e.target.value)}
          >
            {assignableAgents.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </div>

        {error && <p className="dialog-error">{error}</p>}

        <div className="dialog-actions">
          <button className="dialog-btn-secondary" onClick={onClose} disabled={creating}>
            Cancel
          </button>
          <button className="dialog-btn-primary" onClick={handleCreate} disabled={creating}>
            {creating ? "Creating..." : "Create Task"}
          </button>
        </div>
      </div>
    </div>
  );
}
