import { useState, useEffect, useCallback, useRef } from "react";
import Editor, { type OnMount } from "@monaco-editor/react";
import type { SavedPrompt } from "../types/flow";
import { getPrompt, updatePrompt, deletePrompt } from "../api/client";
import { Button } from "@/components/ui/button";

interface PromptEditorViewProps {
  promptId: string;
  onDeleted: () => void;
  onBack: () => void;
  onTitleChanged?: (id: string, title: string) => void;
}

export default function PromptEditorView({
  promptId,
  onDeleted,
  onBack,
  onTitleChanged,
}: PromptEditorViewProps) {
  const [prompt, setPrompt] = useState<SavedPrompt | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const editorRef = useRef<any>(null);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getPrompt(promptId)
      .then((p) => {
        if (!cancelled) {
          setPrompt(p);
          setLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [promptId]);

  const debouncedSave = useCallback(
    (updates: { title?: string; summary?: string; tags?: string[] }) => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(async () => {
        setSaving(true);
        try {
          await updatePrompt(promptId, updates);
        } catch (e) {
          console.error("Failed to save prompt:", e);
        }
        setSaving(false);
      }, 600);
    },
    [promptId]
  );

  const handleMount: OnMount = useCallback((editor, monaco) => {
    editorRef.current = editor;

    monaco.editor.defineTheme("cthulu-dark", {
      base: "vs-dark",
      inherit: true,
      rules: [],
      colors: {
        "editor.background": "#0d1117",
        "editor.foreground": "#e6edf3",
        "editorLineNumber.foreground": "#8b949e",
        "editorLineNumber.activeForeground": "#e6edf3",
        "editor.selectionBackground": "#264f78",
        "editor.lineHighlightBackground": "#161b22",
        "editorCursor.foreground": "#58a6ff",
        "editorIndentGuide.background": "#21262d",
        "editorWidget.background": "#161b22",
        "editorWidget.border": "#30363d",
        "input.background": "#0d1117",
        "input.border": "#30363d",
        "list.hoverBackground": "#161b22",
        "list.activeSelectionBackground": "#264f78",
      },
    });
    monaco.editor.setTheme("cthulu-dark");
  }, []);

  const handleEditorChange = useCallback(
    (val: string | undefined) => {
      if (val === undefined || !prompt) return;
      setPrompt((prev) => (prev ? { ...prev, summary: val } : prev));
      debouncedSave({ summary: val });
    },
    [prompt, debouncedSave]
  );

  const handleTitleChange = useCallback(
    (title: string) => {
      setPrompt((prev) => (prev ? { ...prev, title } : prev));
      debouncedSave({ title });
      onTitleChanged?.(promptId, title);
    },
    [debouncedSave, promptId, onTitleChanged]
  );

  const handleTagsChange = useCallback(
    (tagsStr: string) => {
      const tags = tagsStr
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean);
      setPrompt((prev) => (prev ? { ...prev, tags } : prev));
      debouncedSave({ tags });
    },
    [debouncedSave]
  );

  const handleDelete = useCallback(async () => {
    if (!prompt) return;
    if (!confirm(`Delete prompt "${prompt.title}"?`)) return;
    try {
      await deletePrompt(promptId);
      onDeleted();
    } catch (e) {
      console.error("Failed to delete prompt:", e);
    }
  }, [prompt, promptId, onDeleted]);

  if (loading) {
    return (
      <div className="prompt-editor-view">
        <div className="prompt-editor-loading">Loading prompt...</div>
      </div>
    );
  }

  if (!prompt) {
    return (
      <div className="prompt-editor-view">
        <div className="prompt-editor-loading">Prompt not found</div>
      </div>
    );
  }

  return (
    <div className="prompt-editor-view">
      <div className="prompt-editor-header">
        <div className="prompt-editor-fields">
          <input
            className="prompt-editor-title"
            value={prompt.title}
            onChange={(e) => handleTitleChange(e.target.value)}
            placeholder="Prompt title"
          />
          <div className="prompt-editor-meta-row">
            <input
              className="prompt-editor-tags"
              value={prompt.tags.join(", ")}
              onChange={(e) => handleTagsChange(e.target.value)}
              placeholder="Tags (comma separated)"
            />
            {prompt.source_flow_name && (
              <span className="prompt-editor-source">
                from: {prompt.source_flow_name}
              </span>
            )}
          </div>
        </div>
        <div className="prompt-editor-actions">
          {saving && (
            <span className="prompt-editor-saving">saving...</span>
          )}
          <Button variant="destructive" size="sm" onClick={handleDelete}>
            Delete
          </Button>
        </div>
      </div>
      <div className="prompt-editor-body">
        <Editor
          language="markdown"
          value={prompt.summary}
          onChange={handleEditorChange}
          onMount={handleMount}
          theme="cthulu-dark"
          options={{
            minimap: { enabled: false },
            fontSize: 13,
            fontFamily: '"SF Mono", "Fira Code", "Cascadia Code", monospace',
            lineNumbers: "off",
            scrollBeyondLastLine: false,
            wordWrap: "on",
            tabSize: 2,
            automaticLayout: true,
            folding: false,
            renderLineHighlight: "none",
            padding: { top: 12, bottom: 12 },
            lineDecorationsWidth: 12,
            overviewRulerLanes: 0,
            hideCursorInOverviewRuler: true,
            overviewRulerBorder: false,
            scrollbar: {
              verticalScrollbarSize: 8,
            },
          }}
        />
      </div>
    </div>
  );
}
