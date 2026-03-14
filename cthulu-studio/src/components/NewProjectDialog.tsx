import { useState, useCallback } from "react";
import { useOrg } from "../contexts/OrgContext";
import * as api from "../api/client";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";

interface NewProjectDialogProps {
  onClose: () => void;
  onCreated: () => void;
}

export function NewProjectDialog({ onClose, onCreated }: NewProjectDialogProps) {
  const { selectedOrgSlug } = useOrg();
  const [name, setName] = useState("");
  const [workingDir, setWorkingDir] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleBrowse = useCallback(async () => {
    try {
      const selected = await open({ directory: true, multiple: false, title: "Choose Working Directory" });
      if (selected && typeof selected === "string") {
        setWorkingDir(selected);
      }
    } catch {
      // user cancelled
    }
  }, []);

  const handleSubmit = useCallback(async () => {
    if (!name.trim() || !selectedOrgSlug) return;
    setSaving(true);
    setError(null);
    try {
      const slug = name.trim().toLowerCase().replace(/[^a-z0-9-]/g, "-").replace(/^-+|-+$/g, "");
      await api.createAgentProject(selectedOrgSlug, slug, workingDir.trim() || undefined);
      onCreated();
    } catch (e) {
      setError(typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
      setSaving(false);
    }
  }, [name, workingDir, selectedOrgSlug, onCreated]);

  return (
    <div className="cth-dialog-overlay" onClick={onClose}>
      <div className="cth-dialog-content" onClick={e => e.stopPropagation()}>
        <h3 className="cth-dialog-title">Create Project</h3>
        <div className="cth-dialog-field">
          <label className="cth-dialog-label">Project Name</label>
          <input
            className="cth-dialog-input"
            type="text"
            value={name}
            onChange={e => setName(e.target.value)}
            placeholder="my-project"
            autoFocus
            onKeyDown={e => e.key === "Enter" && handleSubmit()}
          />
          <p className="cth-dialog-hint">
            Lowercase letters, numbers, and hyphens only
          </p>
        </div>
        <div className="cth-dialog-field">
          <label className="cth-dialog-label">Working Directory</label>
          <div className="cth-dialog-dir-row">
            <input
              className="cth-dialog-input cth-dialog-dir-input"
              type="text"
              value={workingDir}
              onChange={e => setWorkingDir(e.target.value)}
              placeholder="/path/to/workspace"
              onKeyDown={e => e.key === "Enter" && handleSubmit()}
            />
            <button
              type="button"
              className="cth-dialog-browse-btn"
              onClick={handleBrowse}
              title="Browse"
            >
              <FolderOpen size={14} />
            </button>
          </div>
          <p className="cth-dialog-hint">
            Agents published to this project will use this directory and receive full permissions
          </p>
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
