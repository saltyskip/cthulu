import { memo, useMemo, useState, useEffect, useRef } from "react";
import ShikiHighlighter from "react-shiki";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useTheme } from "../../lib/ThemeContext";
import type { FileOp, PlanOp } from "./FilePreviewContext";
import type { TaskStep } from "./chatUtils";
import { groupFileOpsByTask } from "./chatUtils";
import { computeDiffLines } from "../../utils/diff";
import { fileIcon } from "../../utils/fileIcons";
import { langFromPath } from "../../utils/langFromPath";
import { SyntaxHighlighter } from "../assistant-ui/shiki-highlighter";
import { useShikiTokens, type Token } from "./useShikiTokens";
import type { ThreadMessageLike } from "@assistant-ui/react";

function basename(filePath: string): string {
  return filePath.replace(/\\/g, "/").split("/").pop() || filePath;
}

/** Count changed lines for an edit op. */
function editLineCounts(op: FileOp): { added: number; removed: number } {
  if (op.type !== "edit" || !op.oldString || !op.newString) return { added: 0, removed: 0 };
  const lines = computeDiffLines(op.oldString, op.newString);
  let added = 0, removed = 0;
  for (const l of lines) {
    if (l.type === "add") added++;
    else if (l.type === "del") removed++;
  }
  return { added, removed };
}

/** Count lines for a write op. */
function writeLineCount(op: FileOp): number {
  if (op.type !== "write" || !op.content) return 0;
  return op.content.split("\n").length;
}

// Selected item can be a file op or a plan op
type SelectedItem =
  | { kind: "file"; op: FileOp }
  | { kind: "plan"; op: PlanOp };

const FilePreviewPanel = memo(function FilePreviewPanel({
  fileOps,
  plans,
  messages,
  selectedId,
  onSelect,
}: {
  fileOps: FileOp[];
  plans: PlanOp[];
  messages: ThreadMessageLike[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const { theme: appTheme } = useTheme();
  const shikiTheme = appTheme.shikiTheme;
  const theme = { dark: shikiTheme as string, light: shikiTheme as string };

  // Build steps from messages
  const steps = useMemo(() => groupFileOpsByTask(messages), [messages]);

  // Active step state — auto-advance to latest
  const [activeStep, setActiveStep] = useState(0);
  const prevStepCountRef = useRef(steps.length);
  useEffect(() => {
    if (steps.length > prevStepCountRef.current) {
      setActiveStep(steps.length - 1);
    }
    prevStepCountRef.current = steps.length;
  }, [steps.length]);

  // Clamp activeStep
  const clampedStep = Math.min(activeStep, Math.max(0, steps.length - 1));
  const currentStep: TaskStep | undefined = steps[clampedStep];

  // The file ops to show: current step's ops (or empty)
  const stepFileOps = currentStep?.fileOps ?? [];

  // Find selected item across plans and current step's file ops
  const allFileOps = stepFileOps; // only show current step
  const selected: SelectedItem | null = useMemo(() => {
    if (selectedId) {
      const plan = plans.find((p) => p.toolCallId === selectedId);
      if (plan) return { kind: "plan", op: plan };
      const file = allFileOps.find((f) => f.toolCallId === selectedId);
      if (file) return { kind: "file", op: file };
      // Also check all fileOps (in case selected from a different step)
      const anyFile = fileOps.find((f) => f.toolCallId === selectedId);
      if (anyFile) return { kind: "file", op: anyFile };
    }
    // Default to latest item in current step
    const lastFile = allFileOps[allFileOps.length - 1];
    const lastPlan = plans[plans.length - 1];
    if (!lastFile && !lastPlan) return null;
    if (!lastFile) return { kind: "plan", op: lastPlan };
    if (!lastPlan) return { kind: "file", op: lastFile };
    return { kind: "file", op: lastFile };
  }, [selectedId, allFileOps, fileOps, plans]);

  const fileOp = selected?.kind === "file" ? selected.op : null;
  const planOp = selected?.kind === "plan" ? selected.op : null;

  const lang = useMemo(() => fileOp ? langFromPath(fileOp.filePath) : undefined, [fileOp?.filePath]);

  const diffLines = fileOp && fileOp.type === "edit" && fileOp.oldString !== undefined && fileOp.newString !== undefined
    ? computeDiffLines(fileOp.oldString, fileOp.newString)
    : null;

  // Tokenize old and new strings for syntax-highlighted diffs
  const oldTokens = useShikiTokens(fileOp?.oldString, lang, shikiTheme);
  const newTokens = useShikiTokens(fileOp?.newString, lang, shikiTheme);

  // Build a mapping from diff line index → tokenized line
  const diffTokenMap = useMemo(() => {
    if (!diffLines) return null;
    const map: (Token[] | null)[] = [];
    let oldIdx = 0;
    let newIdx = 0;
    for (const line of diffLines) {
      if (line.type === "del") {
        map.push(oldTokens?.[oldIdx] ?? null);
        oldIdx++;
      } else if (line.type === "add") {
        map.push(newTokens?.[newIdx] ?? null);
        newIdx++;
      } else {
        // ctx — advance both
        map.push(newTokens?.[newIdx] ?? oldTokens?.[oldIdx] ?? null);
        oldIdx++;
        newIdx++;
      }
    }
    return map;
  }, [diffLines, oldTokens, newTokens]);

  if (!selected && plans.length === 0 && steps.length === 0) return null;

  // Deduplicate plans by filePath (keep latest)
  const planMap = new Map<string, PlanOp>();
  for (const p of plans) planMap.set(p.filePath, p);
  const uniquePlans = [...planMap.values()];

  // Group step file ops by directory for tree display
  const groups = new Map<string, FileOp[]>();
  for (const f of stepFileOps) {
    const parts = f.filePath.replace(/\\/g, "/").split("/");
    const name = parts.pop() || f.filePath;
    const dir = parts.length > 0 ? parts.slice(-2).join("/") : "";
    const existing = groups.get(dir) || [];
    existing.push({ ...f, filePath: name }); // store basename for display
    groups.set(dir, existing);
  }

  const activeId = selected?.kind === "file" ? selected.op.toolCallId : selected?.op.toolCallId ?? "";

  // Truncate user message for display
  const truncateMsg = (text: string, max = 80) => {
    const oneLine = text.replace(/\n/g, " ").trim();
    return oneLine.length > max ? oneLine.slice(0, max) + "…" : oneLine;
  };

  return (
    <div className="fr-preview-split">
      <div className="fr-preview-tree">
        {/* Stepper — sticky context bar at top */}
        {steps.length > 0 && (
          <div className="fr-stepper" title={currentStep?.userMessage}>
            <button
              className="fr-stepper-btn"
              disabled={clampedStep <= 0}
              onClick={() => setActiveStep((s) => Math.max(0, s - 1))}
            >
              ‹
            </button>
            <span className="fr-stepper-label">
              Step {clampedStep + 1} of {steps.length}
            </span>
            <button
              className="fr-stepper-btn"
              disabled={clampedStep >= steps.length - 1}
              onClick={() => setActiveStep((s) => Math.min(steps.length - 1, s + 1))}
            >
              ›
            </button>
          </div>
        )}

        {/* Plans section — always visible, step-independent */}
        {uniquePlans.length > 0 && (
          <div className="fr-tree-section">
            <div className="fr-tree-section-label">Plans</div>
            {uniquePlans.map((p) => {
              const isActive = p.toolCallId === activeId;
              const icon = fileIcon(p.filePath);
              return (
                <button
                  key={p.toolCallId}
                  className={`fr-tree-file ${isActive ? "fr-tree-file-active" : ""}`}
                  onClick={() => onSelect(p.toolCallId)}
                  title={p.filePath}
                >
                  <span className="fr-tree-icon" style={{ color: icon.color }}>{icon.icon}</span>
                  <span className="fr-tree-file-name">{basename(p.filePath)}</span>
                </button>
              );
            })}
          </div>
        )}

        {/* File list for active step */}
        {stepFileOps.length > 0 && (
          <div className="fr-tree-section fr-tree-section-files">
            <div className="fr-tree-section-label">Changed files</div>
            {[...groups.entries()].map(([dir, files]) => (
              <div key={dir} className="fr-tree-group">
                {dir && <div className="fr-tree-dir">{dir}</div>}
                {files.map((f) => {
                  const original = stepFileOps.find((o) => o.filePath.endsWith(f.filePath) && o.toolCallId === f.toolCallId);
                  const isActive = original?.toolCallId === activeId;
                  const icon = fileIcon(f.filePath);
                  // Line count badge
                  let badge: React.ReactNode = null;
                  if (original) {
                    if (original.type === "edit") {
                      const { added, removed } = editLineCounts(original);
                      badge = (
                        <span className="fr-file-badge">
                          {removed > 0 && <span className="fr-file-badge-del">−{removed}</span>}
                          {added > 0 && <span className="fr-file-badge-add">+{added}</span>}
                        </span>
                      );
                    } else if (original.type === "write") {
                      const lines = writeLineCount(original);
                      if (lines > 0) {
                        badge = <span className="fr-file-badge"><span className="fr-file-badge-add">+{lines}</span></span>;
                      }
                    }
                  }
                  return (
                    <button
                      key={f.toolCallId}
                      className={`fr-tree-file ${isActive ? "fr-tree-file-active" : ""}`}
                      onClick={() => onSelect(f.toolCallId)}
                      title={original?.filePath}
                    >
                      <span className="fr-tree-icon" style={{ color: icon.color }}>{icon.icon}</span>
                      <span className="fr-tree-file-name">{f.filePath}</span>
                      {badge}
                    </button>
                  );
                })}
              </div>
            ))}
          </div>
        )}

        {/* Summary footer */}
        {stepFileOps.length > 0 && (() => {
          let totalAdded = 0, totalRemoved = 0;
          for (const op of stepFileOps) {
            if (op.type === "edit") {
              const c = editLineCounts(op);
              totalAdded += c.added;
              totalRemoved += c.removed;
            } else if (op.type === "write" && op.content) {
              totalAdded += op.content.split("\n").length;
            }
          }
          return (
            <div className="fr-tree-summary">
              {stepFileOps.length} file{stepFileOps.length !== 1 ? "s" : ""} changed
              {totalAdded > 0 && <span className="fr-file-badge-add"> +{totalAdded}</span>}
              {totalRemoved > 0 && <span className="fr-file-badge-del"> −{totalRemoved}</span>}
            </div>
          );
        })()}
      </div>

      <div className="fr-preview-main">
        <div className="fr-preview-path">{selected?.kind === "file" ? selected.op.filePath : selected ? basename(selected.op.filePath) : ""}</div>
        <div className="fr-preview-body">
          {planOp ? (
            // Render plan as formatted markdown
            <div className="fr-preview-markdown fr-md">
              <Markdown
                remarkPlugins={[remarkGfm]}
                components={{
                  code({ className, children, ...props }) {
                    const match = /language-(\w+)/.exec(className || "");
                    const code = String(children).replace(/\n$/, "");
                    if (match) {
                      return (
                        <SyntaxHighlighter language={match[1]} code={code} />
                      );
                    }
                    return <code className={className} {...props}>{children}</code>;
                  },
                }}
              >
                {planOp.content || ""}
              </Markdown>
            </div>
          ) : fileOp && diffLines ? (
            <div className="fr-preview-diff">
              {diffLines.map((line, i) => {
                const tokens = diffTokenMap?.[i];
                return (
                  <div
                    key={i}
                    className={`fr-diff-line ${
                      line.type === "del" ? "fr-diff-del" : line.type === "add" ? "fr-diff-add" : "fr-diff-ctx"
                    }`}
                  >
                    <span className="fr-diff-prefix">
                      {line.type === "del" ? "−" : line.type === "add" ? "+" : " "}
                    </span>
                    {tokens
                      ? tokens.map((t, j) => (
                          <span key={j} style={t.color ? { color: t.color } : undefined}>
                            {t.content}
                          </span>
                        ))
                      : line.text}
                  </div>
                );
              })}
            </div>
          ) : fileOp?.content ? (
            lang ? (
              <ShikiHighlighter
                language={lang}
                theme={theme}
                addDefaultStyles={false}
                showLanguage={false}
                defaultColor="light-dark()"
                className="fr-preview-shiki"
              >
                {fileOp.content}
              </ShikiHighlighter>
            ) : (
              <pre className="fr-preview-content">{fileOp.content}</pre>
            )
          ) : (
            <div className="fr-preview-empty">No preview available</div>
          )}
        </div>
      </div>
    </div>
  );
});

export default FilePreviewPanel;
