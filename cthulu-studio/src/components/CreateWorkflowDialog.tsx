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
import type { TemplateMetadata } from "../types/flow";

interface CreateWorkflowDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  workspace: string;
  onCreated: (workspace: string, name: string) => void;
  template?: TemplateMetadata | null;
}

export default function CreateWorkflowDialog({
  open,
  onOpenChange,
  workspace,
  onCreated,
  template,
}: CreateWorkflowDialogProps) {
  const [name, setName] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleCreate = async () => {
    const trimmed = name.trim().replace(/\s+/g, "-").toLowerCase();
    if (!trimmed) return;

    setCreating(true);
    setError(null);
    try {
      if (template) {
        // Import the template to get parsed nodes/edges, then publish as workflow
        const flow = await api.importTemplate(template.category, template.slug);
        await api.publishWorkflow(workspace, trimmed, {
          name: trimmed,
          description: flow.description || "",
          nodes: flow.nodes,
          edges: flow.edges,
        });
      } else {
        // Publish a blank workflow to create the directory + file
        await api.publishWorkflow(workspace, trimmed, {
          name: trimmed,
          description: "",
          nodes: [],
          edges: [],
        });
      }
      setName("");
      onOpenChange(false);
      onCreated(workspace, trimmed);
    } catch (e: unknown) {
      setError(typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
    } finally {
      setCreating(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="bg-[var(--bg-secondary)] border-[var(--border)] text-[var(--text)]">
        <DialogHeader>
          <DialogTitle>
            {template ? "New Workflow from Template" : "New Workflow"}
          </DialogTitle>
          <DialogDescription>
            {template ? (
              <>
                Creating from <strong>{template.icon ?? ""} {template.title}</strong> in{" "}
                <code className="text-[var(--accent)]">{workspace}</code>.
              </>
            ) : (
              <>
                Create a new workflow in <code className="text-[var(--accent)]">{workspace}</code>.
              </>
            )}
          </DialogDescription>
        </DialogHeader>
        <div className="form-group">
          <label className="text-sm text-[var(--text-secondary)]">Name</label>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. crypto-pipeline"
            className="w-full bg-[var(--bg)] border border-[var(--border)] rounded-md px-3 py-2 text-[var(--text)] text-sm outline-none focus:border-[var(--accent)]"
            onKeyDown={(e) => {
              if (e.key === "Enter" && name.trim()) handleCreate();
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
          <Button onClick={handleCreate} disabled={creating || !name.trim()}>
            {creating ? "Creating..." : "Create"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
