import { useRef, useImperativeHandle, forwardRef, useCallback, useEffect } from "react";
import Editor, { type OnMount, type OnChange } from "@monaco-editor/react";
import { registerFlowSchema } from "../lib/flow-schema";
import { applyMonacoTheme } from "../lib/monaco-theme";
import { useTheme } from "../lib/ThemeContext";

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

const FlowEditor = forwardRef<FlowEditorHandle, FlowEditorProps>(
  function FlowEditor({ defaultValue, onChange }, ref) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const editorRef = useRef<any>(null);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const monacoRef = useRef<any>(null);
    const { theme: appTheme } = useTheme();

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
        if (val !== undefined) onChange(val);
      },
      [onChange]
    );

    useImperativeHandle(ref, () => ({
      setText(text: string) {
        const editor = editorRef.current;
        if (!editor) return;
        const model = editor.getModel();
        if (!model) return;
        // Only push if text actually differs — avoids cursor jump
        if (text === model.getValue()) return;
        editor.executeEdits("external-update", [{
          range: model.getFullModelRange(),
          text,
          forceMoveMarkers: false, // preserve cursor position
        }]);
      },

      getText() {
        return editorRef.current?.getModel()?.getValue() ?? "";
      },

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
    );
  }
);

export default FlowEditor;
