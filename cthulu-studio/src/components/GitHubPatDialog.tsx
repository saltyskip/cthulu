import { useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import * as api from "../api/client";

interface GitHubPatDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
}

export default function GitHubPatDialog({
  open,
  onOpenChange,
  onSaved,
}: GitHubPatDialogProps) {
  const [token, setToken] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await api.saveGithubPat(token);
      setToken("");
      onOpenChange(false);
      onSaved();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="bg-[var(--bg-secondary)] border-[var(--border)] text-[var(--text)]">
        <DialogHeader>
          <DialogTitle>GitHub Personal Access Token</DialogTitle>
          <DialogDescription>
            Enter a GitHub PAT with <code className="text-[var(--accent)]">repo</code> scope
            to store and sync workflows.
          </DialogDescription>
        </DialogHeader>
        <div className="form-group">
          <label className="text-sm text-[var(--text-secondary)]">Token</label>
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="ghp_..."
            className="w-full bg-[var(--bg)] border border-[var(--border)] rounded-md px-3 py-2 text-[var(--text)] text-sm outline-none focus:border-[var(--accent)]"
            onKeyDown={(e) => {
              if (e.key === "Enter" && token.trim()) handleSave();
            }}
            autoFocus
          />
          {error && (
            <p className="text-xs text-[var(--danger)]">{error}</p>
          )}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleSave} disabled={saving || !token.trim()}>
            {saving ? "Validating..." : "Save"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
