import { memo } from "react";
import type { FileOp } from "./FilePreviewContext";
import { computeDiffLines } from "../../utils/diff";

const FilePreviewPanel = memo(function FilePreviewPanel({
  fileOps,
  selectedId,
  onSelect,
}: {
  fileOps: FileOp[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const op = selectedId ? fileOps.find((f) => f.toolCallId === selectedId) : fileOps[fileOps.length - 1];
  if (!op) return null;

  const diffLines = op.type === "edit" && op.oldString !== undefined && op.newString !== undefined
    ? computeDiffLines(op.oldString, op.newString)
    : null;

  // Build tree structure grouped by directory
  const fileMap = new Map<string, FileOp>();
  for (const f of fileOps) fileMap.set(f.filePath, f);
  const uniqueFiles = [...fileMap.values()];

  // Group by parent directory
  const groups = new Map<string, FileOp[]>();
  for (const f of uniqueFiles) {
    const parts = f.filePath.replace(/\\/g, "/").split("/");
    const name = parts.pop() || f.filePath;
    const dir = parts.length > 0 ? parts.slice(-2).join("/") : "";
    const existing = groups.get(dir) || [];
    existing.push({ ...f, filePath: name }); // store basename for display
    groups.set(dir, existing);
  }

  return (
    <div className="fr-preview-split">
      <div className="fr-preview-tree">
        {[...groups.entries()].map(([dir, files]) => (
          <div key={dir} className="fr-tree-group">
            {dir && <div className="fr-tree-dir">{dir}</div>}
            {files.map((f) => {
              // Find the original toolCallId from fileMap
              const original = uniqueFiles.find((o) => o.filePath.endsWith(f.filePath) && o.toolCallId === f.toolCallId);
              const isActive = original?.toolCallId === op.toolCallId;
              return (
                <button
                  key={f.toolCallId}
                  className={`fr-tree-file ${isActive ? "fr-tree-file-active" : ""}`}
                  onClick={() => onSelect(f.toolCallId)}
                >
                  <span className="fr-tree-icon">{f.type === "edit" ? "✎" : "📄"}</span>
                  {f.filePath}
                </button>
              );
            })}
          </div>
        ))}
      </div>
      <div className="fr-preview-main">
        <div className="fr-preview-path">{op.filePath}</div>
        <div className="fr-preview-body">
          {diffLines ? (
            <div className="fr-preview-diff">
              {diffLines.map((line, i) => (
                <div
                  key={i}
                  className={`fr-diff-line ${
                    line.type === "del" ? "fr-diff-del" : line.type === "add" ? "fr-diff-add" : "fr-diff-ctx"
                  }`}
                >
                  <span className="fr-diff-prefix">
                    {line.type === "del" ? "−" : line.type === "add" ? "+" : " "}
                  </span>
                  {line.text}
                </div>
              ))}
            </div>
          ) : op.content ? (
            <pre className="fr-preview-content">{op.content}</pre>
          ) : (
            <div className="fr-preview-empty">No preview available</div>
          )}
        </div>
      </div>
    </div>
  );
});

export default FilePreviewPanel;
