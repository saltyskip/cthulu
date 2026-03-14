import { useState, useCallback } from "react";
import { useOrg } from "../contexts/OrgContext";
import * as api from "../api/client";

interface NewOrgDialogProps {
  onClose: () => void;
}

export function NewOrgDialog({ onClose }: NewOrgDialogProps) {
  const { createOrg } = useOrg();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = useCallback(async () => {
    if (!name.trim()) return;
    setSaving(true);
    setError(null);
    try {
      // Ensure agent repo is set up before creating org
      try { await api.setupAgentRepo(); } catch { /* may already exist */ }
      await createOrg(name.trim(), description.trim() || undefined);
      onClose();
    } catch (e) {
      setError(typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
    }
    setSaving(false);
  }, [name, description, createOrg, onClose]);

  return (
    <div className="cth-dialog-overlay" onClick={onClose}>
      <div className="cth-dialog-content" onClick={e => e.stopPropagation()}>
        <h3 className="cth-dialog-title">Create Organization</h3>
        <div className="cth-dialog-field">
          <label className="cth-dialog-label">Name</label>
          <input
            className="cth-dialog-input"
            type="text"
            value={name}
            onChange={e => setName(e.target.value)}
            placeholder="My Organization"
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
            placeholder="A brief description"
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
