import { useState, useEffect } from "react";
import MDEditor from "@uiw/react-md-editor";
import * as api from "../api/client";
import type { SavedPrompt } from "../types/flow";

interface PromptEditorProps {
  promptId: string;
}

export default function PromptEditor({ promptId }: PromptEditorProps) {
  const [prompt, setPrompt] = useState<SavedPrompt | null>(null);
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [tagsText, setTagsText] = useState("");
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const prompts = await api.listPrompts();
        const found = prompts.find((p) => p.id === promptId);
        if (found && !cancelled) {
          setPrompt(found);
          setTitle(found.title);
          setSummary(found.summary);
          setTagsText(found.tags.join(", "));
          setDirty(false);
        }
      } catch {
        /* logged */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [promptId]);

  const handleSave = async () => {
    setSaving(true);
    try {
      const tags = tagsText
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean);
      await api.updatePrompt(promptId, { title, summary, tags });
      setDirty(false);
    } catch {
      /* logged */
    }
    setSaving(false);
  };

  if (!prompt) {
    return (
      <div className="canvas-container">
        <div className="empty-state">
          <p>Loading prompt...</p>
        </div>
      </div>
    );
  }

  return (
    <div className="canvas-container" style={{ padding: 24, overflow: "auto" }}>
      <div style={{ maxWidth: 900 }}>
        <div style={{ display: "flex", gap: 12, marginBottom: 16, alignItems: "flex-end" }}>
          <div style={{ flex: 1 }}>
            <label style={{
              display: "block", fontSize: 11, fontWeight: 600,
              textTransform: "uppercase", color: "var(--text-secondary)",
              marginBottom: 4, letterSpacing: "0.5px",
            }}>Title</label>
            <input
              value={title}
              onChange={(e) => { setTitle(e.target.value); setDirty(true); }}
              placeholder="Prompt title"
              style={{
                width: "100%", background: "var(--bg)", border: "1px solid var(--border)",
                borderRadius: 6, padding: "8px 10px", color: "var(--text)", fontSize: 13,
              }}
            />
          </div>
          <div style={{ flex: 1 }}>
            <label style={{
              display: "block", fontSize: 11, fontWeight: 600,
              textTransform: "uppercase", color: "var(--text-secondary)",
              marginBottom: 4, letterSpacing: "0.5px",
            }}>Tags</label>
            <input
              value={tagsText}
              onChange={(e) => { setTagsText(e.target.value); setDirty(true); }}
              placeholder="tag1, tag2, tag3"
              style={{
                width: "100%", background: "var(--bg)", border: "1px solid var(--border)",
                borderRadius: 6, padding: "8px 10px", color: "var(--text)", fontSize: 13,
              }}
            />
          </div>
          <button
            className="primary"
            onClick={handleSave}
            disabled={saving || !dirty}
          >
            {saving ? "Saving..." : "Save"}
          </button>
        </div>

        {prompt.source_flow_name && (
          <div style={{ fontSize: 12, color: "var(--text-secondary)", marginBottom: 12 }}>
            Source: {prompt.source_flow_name}
          </div>
        )}

        <div data-color-mode="dark">
          <MDEditor
            value={summary}
            onChange={(val) => { setSummary(val || ""); setDirty(true); }}
            height={600}
            preview="edit"
          />
        </div>
      </div>
    </div>
  );
}
