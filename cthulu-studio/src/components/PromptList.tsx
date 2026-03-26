import { useState, useEffect, useCallback } from "react";
import type { SavedPrompt, ActiveView } from "../types/flow";
import { listPrompts, savePrompt, deletePrompt as deletePromptApi } from "../api/client";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "@/components/ui/collapsible";

interface PromptListProps {
  selectedPromptId: string | null;
  onSelectPrompt: (id: string) => void;
  promptListKey: number;
  activeView: ActiveView;
}

export default function PromptList({
  selectedPromptId,
  onSelectPrompt,
  promptListKey,
  activeView,
}: PromptListProps) {
  const [prompts, setPrompts] = useState<SavedPrompt[]>([]);

  const refreshPrompts = useCallback(async () => {
    try {
      setPrompts(await listPrompts());
    } catch {
      // Server may not be reachable
    }
  }, []);

  useEffect(() => {
    refreshPrompts();
  }, [refreshPrompts, promptListKey]);

  async function handleCreatePrompt() {
    try {
      const { id } = await savePrompt({
        title: "New Prompt",
        summary: "",
        source_flow_name: "",
        tags: [],
      });
      await refreshPrompts();
      onSelectPrompt(id);
    } catch (e) {
      console.error("Failed to create prompt:", e);
    }
  }

  async function handleDeletePrompt(e: React.MouseEvent, id: string) {
    e.stopPropagation();
    if (!confirm("Delete this prompt?")) return;
    try {
      await deletePromptApi(id);
      await refreshPrompts();
    } catch (e) {
      console.error("Failed to delete prompt:", e);
    }
  }

  return (
    <Collapsible defaultOpen className="sidebar-section">
      <CollapsibleTrigger asChild>
        <div className="sidebar-section-header">
          <span className="sidebar-chevron">▶</span>
          <h2>Prompts</h2>
          <div style={{ flex: 1 }} />
          <button
            className="ghost sidebar-action-btn"
            onClick={(e) => {
              e.stopPropagation();
              handleCreatePrompt();
            }}
          >
            +
          </button>
        </div>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="sidebar-section-body">
          {prompts.map((p) => (
            <div
              key={p.id}
              className={`sidebar-item${p.id === selectedPromptId && activeView === "prompt-editor" ? " active" : ""}`}
              onClick={() => onSelectPrompt(p.id)}
            >
              <div className="sidebar-item-row">
                <div className="sidebar-item-name">{p.title}</div>
                <button
                  className="ghost sidebar-delete-btn"
                  onClick={(e) => handleDeletePrompt(e, p.id)}
                  title="Delete prompt"
                >
                  ×
                </button>
              </div>
              {p.tags.length > 0 && (
                <div className="sidebar-item-meta">{p.tags.join(", ")}</div>
              )}
            </div>
          ))}
          {prompts.length === 0 && (
            <div className="sidebar-item-empty">No prompts yet</div>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
