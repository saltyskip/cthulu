import { useState, useRef, useImperativeHandle, forwardRef, useCallback, useEffect } from "react";
import Editor, { type OnMount, type OnChange } from "@monaco-editor/react";
import { registerFlowSchema } from "../lib/flow-schema";
import { applyMonacoTheme } from "../lib/monaco-theme";
import { useTheme } from "../lib/ThemeContext";
import yaml from "js-yaml";

export type EditorFormat = "json" | "yaml";

export interface FlowEditorHandle {
  revealNode: (nodeId: string) => void;
  /** Push text into the editor from an external source (preserves undo stack). */
  setText: (text: string) => void;
  /** Read the current editor text without triggering a render. */
  getText: () => string;
}

interface FlowEditorProps {
  defaultValue: string;
  onChange: (text: string) => void;
}

/** Convert a JSON string to pretty YAML. Returns null on failure. */
function jsonToYaml(jsonStr: string): string | null {
  try {
    const obj = JSON.parse(jsonStr);
    return yaml.dump(obj, { indent: 2, lineWidth: 120, noRefs: true });
  } catch {
    return null;
  }
}

/** Convert a YAML string to pretty JSON. Returns null on failure. */
function yamlToJson(yamlStr: string): string | null {
  try {
    const obj = yaml.load(yamlStr);
    return JSON.stringify(obj, null, 2);
  } catch {
    return null;
  }
}

const FlowEditor = forwardRef<FlowEditorHandle, FlowEditorProps>(
  function FlowEditor({ defaultValue, onChange }, ref) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const editorRef = useRef<any>(null);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const monacoRef = useRef<any>(null);
    const { theme: appTheme } = useTheme();

    const [format, setFormat] = useState<EditorFormat>("json");
    // Track format in a ref so imperative methods always see latest
    const formatRef = useRef(format);
    formatRef.current = format;

    // Suppress onChange while we're doing a format switch
    const suppressChange = useRef(false);

    const handleMount: OnMount = useCallback((editor, monaco) => {
      editorRef.current = editor;
      monacoRef.current = monaco;

      registerFlowSchema(monaco);
      applyMonacoTheme(monaco, appTheme);
    }, [appTheme]);

    useEffect(() => {
      if (monacoRef.current) applyMonacoTheme(monacoRef.current, appTheme);
    }, [appTheme]);

    const handleChange: OnChange = useCallback(
      (val) => {
        if (val === undefined || suppressChange.current) return;
        // Always emit JSON to the parent regardless of current format
        if (formatRef.current === "yaml") {
          const json = yamlToJson(val);
          if (json) onChange(json);
          // If YAML is invalid mid-edit, just skip — don't emit broken JSON
        } else {
          onChange(val);
        }
      },
      [onChange]
    );

    /** Replace editor text without triggering onChange */
    const setEditorText = useCallback((text: string) => {
      const editor = editorRef.current;
      if (!editor) return;
      const model = editor.getModel();
      if (!model) return;
      if (text === model.getValue()) return;
      suppressChange.current = true;
      editor.executeEdits("format-switch", [{
        range: model.getFullModelRange(),
        text,
        forceMoveMarkers: false,
      }]);
      suppressChange.current = false;
    }, []);

    const switchFormat = useCallback((newFormat: EditorFormat) => {
      if (newFormat === formatRef.current) return;
      const editor = editorRef.current;
      const monaco = monacoRef.current;
      if (!editor || !monaco) return;

      const model = editor.getModel();
      if (!model) return;
      const current = model.getValue();

      if (newFormat === "yaml") {
        const yamlStr = jsonToYaml(current);
        if (!yamlStr) return; // invalid JSON, can't convert
        setEditorText(yamlStr);
        monaco.editor.setModelLanguage(model, "yaml");
      } else {
        const jsonStr = yamlToJson(current);
        if (!jsonStr) return; // invalid YAML, can't convert
        setEditorText(jsonStr);
        monaco.editor.setModelLanguage(model, "json");
      }
      setFormat(newFormat);
    }, [setEditorText]);

    useImperativeHandle(ref, () => ({
      setText(text: string) {
        const editor = editorRef.current;
        if (!editor) return;
        const model = editor.getModel();
        if (!model) return;

        // text is always JSON from the parent
        let displayText = text;
        if (formatRef.current === "yaml") {
          displayText = jsonToYaml(text) ?? text;
        }

        if (displayText === model.getValue()) return;
        suppressChange.current = true;
        editor.executeEdits("external-update", [{
          range: model.getFullModelRange(),
          text: displayText,
          forceMoveMarkers: false,
        }]);
        suppressChange.current = false;
      },

      getText() {
        const val = editorRef.current?.getModel()?.getValue() ?? "";
        if (formatRef.current === "yaml") {
          return yamlToJson(val) ?? val;
        }
        return val;
      },

      revealNode(nodeId: string) {
        const editor = editorRef.current;
        const monaco = monacoRef.current;
        if (!editor || !monaco) return;

        const model = editor.getModel();
        if (!model) return;

        const text = model.getValue();

        if (formatRef.current === "json") {
          // Find the node's "id": "<nodeId>" in the text
          const needle = `"id": "${nodeId}"`;
          const idx = text.indexOf(needle);
          if (idx === -1) return;

          // Walk backwards to find the opening { of this node object
          let braceCount = 0;
          let startIdx = idx;
          for (let i = idx; i >= 0; i--) {
            if (text[i] === "}") braceCount++;
            if (text[i] === "{") {
              if (braceCount === 0) {
                startIdx = i;
                break;
              }
              braceCount--;
            }
          }

          // Walk forwards to find the closing }
          braceCount = 0;
          let endIdx = idx;
          for (let i = startIdx; i < text.length; i++) {
            if (text[i] === "{") braceCount++;
            if (text[i] === "}") {
              braceCount--;
              if (braceCount === 0) {
                endIdx = i + 1;
                break;
              }
            }
          }

          const startPos = model.getPositionAt(startIdx);
          const endPos = model.getPositionAt(endIdx);

          editor.revealLineInCenter(startPos.lineNumber);
          editor.setSelection(
            new monaco.Range(
              startPos.lineNumber,
              startPos.column,
              endPos.lineNumber,
              endPos.column
            )
          );
        } else {
          // YAML: search for `id: <nodeId>` pattern
          const needle = `id: ${nodeId}`;
          const idx = text.indexOf(needle);
          if (idx === -1) return;
          const pos = model.getPositionAt(idx);
          editor.revealLineInCenter(pos.lineNumber);
          editor.setPosition(pos);
        }
        editor.focus();
      },
    }));

    return (
      <div className="flow-editor-wrapper">
        <div className="flow-editor-toolbar">
          <button
            className={`flow-editor-format-btn${format === "json" ? " active" : ""}`}
            onClick={() => switchFormat("json")}
          >
            JSON
          </button>
          <button
            className={`flow-editor-format-btn${format === "yaml" ? " active" : ""}`}
            onClick={() => switchFormat("yaml")}
          >
            YAML
          </button>
        </div>
        <Editor
          language={format === "json" ? "json" : "yaml"}
          defaultValue={defaultValue}
          onChange={handleChange}
          onMount={handleMount}
          theme="cthulu-dark"
          options={{
            minimap: { enabled: false },
            fontSize: 12,
            fontFamily: '"SF Mono", "Fira Code", "Cascadia Code", monospace',
            lineNumbers: "on",
            scrollBeyondLastLine: false,
            wordWrap: "on",
            tabSize: 2,
            automaticLayout: true,
            folding: true,
            bracketPairColorization: { enabled: true },
            renderLineHighlight: "line",
            padding: { top: 8 },
          }}
        />
      </div>
    );
  }
);

export default FlowEditor;
