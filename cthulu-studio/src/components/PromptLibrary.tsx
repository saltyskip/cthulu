import { useState, useEffect, useCallback } from "react";
import * as api from "../api/client";
import type { SavedPrompt } from "../types/flow";

interface PromptLibraryProps {
  activePromptId: string | null;
  onSelect: (id: string) => void;
  onCreate: () => void;
}

export default function PromptLibrary({
  activePromptId,
  onSelect,
  onCreate,
}: PromptLibraryProps) {
  const [prompts, setPrompts] = useState<SavedPrompt[]>([]);
  const [collapsed, setCollapsed] = useState(false);

  const loadPrompts = useCallback(async () => {
    try {
      setPrompts(await api.listPrompts());
    } catch {
      /* logged */
    }
  }, []);

  useEffect(() => {
    loadPrompts();
  }, [loadPrompts]);

  // Reload when a prompt is selected (it may have just been created)
  useEffect(() => {
    if (activePromptId) loadPrompts();
  }, [activePromptId, loadPrompts]);

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation();
    try {
      await api.deletePrompt(id);
      loadPrompts();
    } catch {
      /* logged */
    }
  };

  return (
    <div style={{ borderTop: "1px solid var(--border)", flexShrink: 0 }}>
      <div className="sidebar-header">
        <h2
          style={{ cursor: "pointer", userSelect: "none" }}
          onClick={() => setCollapsed((c) => !c)}
        >
          {collapsed ? "\u25b8" : "\u25be"} Prompts
        </h2>
        <button className="ghost" onClick={onCreate}>
          + New
        </button>
      </div>
      {!collapsed && (
        <div style={{ padding: 8 }}>
          {prompts.map((p) => (
            <div
              key={p.id}
              className={`flow-item ${p.id === activePromptId ? "active" : ""}`}
              onClick={() => onSelect(p.id)}
            >
              <div className="flow-item-name">{p.title}</div>
              <div className="flow-item-meta" style={{ display: "flex", alignItems: "center" }}>
                <span>
                  {p.source_flow_name && <>{p.source_flow_name} &middot; </>}
                  {p.tags.length > 0 && p.tags.join(", ")}
                </span>
                <button
                  className="ghost"
                  style={{
                    fontSize: 10,
                    padding: "0 4px",
                    marginLeft: "auto",
                    color: "var(--text-secondary)",
                  }}
                  onClick={(e) => handleDelete(e, p.id)}
                  title="Delete prompt"
                >
                  ✕
                </button>
              </div>
            </div>
          ))}
          {prompts.length === 0 && (
            <div className="flow-item">
              <div className="flow-item-meta">No prompts yet</div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
