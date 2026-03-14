import { useState, useCallback } from "react";
import * as api from "../api/client";

interface NewAgentDialogProps {
  onClose: () => void;
  onCreated: (id: string) => void;
}

export function NewAgentDialog({ onClose, onCreated }: NewAgentDialogProps) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = useCallback(async () => {
    if (!name.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const { id } = await api.createAgent({
        name: name.trim(),
        description: description.trim() || undefined,
      });
      onCreated(id);
    } catch (e) {
      setError(typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
      setSaving(false);
    }
  }, [name, description, onCreated]);

  return (
    <div className="cth-dialog-overlay" onClick={onClose}>
      <div className="cth-dialog-content" onClick={e => e.stopPropagation()}>
        <h3 className="cth-dialog-title">Create Agent</h3>
        <div className="cth-dialog-field">
          <label className="cth-dialog-label">Name</label>
          <input
            className="cth-dialog-input"
            type="text"
            value={name}
            onChange={e => setName(e.target.value)}
            placeholder="My Agent"
            autoFocus
            onKeyDown={e => e.key === "Enter" && handleSubmit()}
          />
        </div>
        <div className="cth-dialog-field">
          <label className="cth-dialog-label">Description (optional)</label>
          <input
            className="cth-dialog-input"
            type="text"
            value={description}
            onChange={e => setDescription(e.target.value)}
            placeholder="What does this agent do?"
            onKeyDown={e => e.key === "Enter" && handleSubmit()}
          />
        </div>
        {error && <p className="cth-dialog-error">{error}</p>}
        <div className="cth-dialog-actions">
          <button className="cth-dialog-btn-secondary" onClick={onClose}>Cancel</button>
          <button
            className="cth-dialog-btn-primary"
            onClick={handleSubmit}
            disabled={!name.trim() || saving}
          >
            {saving ? "Creating..." : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
