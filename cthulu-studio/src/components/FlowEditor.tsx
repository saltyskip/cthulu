import { useRef, useImperativeHandle, forwardRef, useCallback } from "react";
import Editor, { type OnMount, type OnChange } from "@monaco-editor/react";
import { registerFlowSchema } from "../lib/flow-schema";

export interface FlowEditorHandle {
  revealNode: (nodeId: string) => void;
}

interface FlowEditorProps {
  value: string;
  onChange: (text: string) => void;
}

const FlowEditor = forwardRef<FlowEditorHandle, FlowEditorProps>(
  function FlowEditor({ value, onChange }, ref) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const editorRef = useRef<any>(null);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const monacoRef = useRef<any>(null);

    const handleMount: OnMount = useCallback((editor, monaco) => {
      editorRef.current = editor;
      monacoRef.current = monaco;

      registerFlowSchema(monaco);

      // Define custom dark theme matching our CSS variables
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

    const handleChange: OnChange = useCallback(
      (val) => {
        if (val !== undefined) onChange(val);
      },
      [onChange]
    );

    useImperativeHandle(ref, () => ({
      revealNode(nodeId: string) {
        const editor = editorRef.current;
        const monaco = monacoRef.current;
        if (!editor || !monaco) return;

        const model = editor.getModel();
        if (!model) return;

        // Find the node's "id": "<nodeId>" in the text
        const text = model.getValue();
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
        editor.focus();
      },
    }));

    return (
      <Editor
        language="json"
        value={value}
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
    );
  }
);

export default FlowEditor;
