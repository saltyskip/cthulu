import { memo, useMemo, useState, useEffect, useRef } from "react";
import ShikiHighlighter from "react-shiki";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useTheme } from "../../lib/ThemeContext";
import type { FileOp, PlanOp, MultiRepoSnapshot } from "./FilePreviewContext";
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

// Git status letter badge color
function gitStatusColor(status: string): string {
  switch (status) {
    case "M": return "var(--warning)";
    case "A": return "var(--success)";
    case "D": return "var(--danger)";
    case "R": return "var(--accent)";
    default: return "var(--text-secondary)";
  }
}

// Truncate user message for display
function truncateMsg(text: string, max = 60): string {
  const oneLine = text.replace(/\n/g, " ").trim();
  return oneLine.length > max ? oneLine.slice(0, max) + "…" : oneLine;
}

const FilePreviewPanel = memo(function FilePreviewPanel({
  fileOps,
  plans,
  messages,
  selectedId,
  onSelect,
  gitSnapshot,
}: {
  fileOps: FileOp[];
  plans: PlanOp[];
  messages: ThreadMessageLike[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  gitSnapshot?: MultiRepoSnapshot | null;
}) {
  const { theme: appTheme } = useTheme();
  const shikiTheme = appTheme.shikiTheme;
  const theme = { dark: shikiTheme as string, light: shikiTheme as string };

  // Build steps from messages
  const steps = useMemo(() => groupFileOpsByTask(messages), [messages]);

  // Accordion: track which steps are expanded (latest auto-expands)
  const [expandedSteps, setExpandedSteps] = useState<Set<number>>(new Set());
  const prevStepCountRef = useRef(steps.length);
  useEffect(() => {
    if (steps.length > prevStepCountRef.current) {
      // Collapse previous latest, expand new latest
      setExpandedSteps(new Set([steps.length - 1]));
    } else if (steps.length > 0 && expandedSteps.size === 0) {
      setExpandedSteps(new Set([steps.length - 1]));
    }
    prevStepCountRef.current = steps.length;
  }, [steps.length]);

  const toggleStep = (idx: number) => {
    setExpandedSteps((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  };

  // Find selected item across plans and all file ops
  const selected: SelectedItem | null = useMemo(() => {
    if (selectedId) {
      const plan = plans.find((p) => p.toolCallId === selectedId);
      if (plan) return { kind: "plan", op: plan };
      const file = fileOps.find((f) => f.toolCallId === selectedId);
      if (file) return { kind: "file", op: file };
    }
    // Default to latest item
    const lastFile = fileOps[fileOps.length - 1];
    const lastPlan = plans[plans.length - 1];
    if (!lastFile && !lastPlan) return null;
    if (!lastFile) return { kind: "plan", op: lastPlan };
    if (!lastPlan) return { kind: "file", op: lastFile };
    return { kind: "file", op: lastFile };
  }, [selectedId, fileOps, plans]);

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

  const activeId = selected?.kind === "file" ? selected.op.toolCallId : selected?.op.toolCallId ?? "";

  // Match a file op path to a git file path
  const matchGitFile = (opPath: string, gitPath: string): boolean => {
    const normalizedOp = opPath.replace(/\\/g, "/");
    return normalizedOp.endsWith(gitPath) || gitPath.endsWith(normalizedOp);
  };

  // Build set of file paths that have tool-call ops (for marking clickable git files)
  const fileOpsByPath = useMemo(() => {
    const map = new Map<string, FileOp>();
    for (const op of fileOps) {
      map.set(op.filePath.replace(/\\/g, "/"), op);
    }
    return map;
  }, [fileOps]);

  // Find matching file op for a git file
  const findMatchingOp = (gitPath: string): FileOp | null => {
    for (const [opPath, op] of fileOpsByPath) {
      if (matchGitFile(opPath, gitPath)) return op;
    }
    return null;
  };

  // Render a line-count badge for a file op
  const renderBadge = (op: FileOp): React.ReactNode => {
    if (op.type === "edit") {
      const { added, removed } = editLineCounts(op);
      if (added === 0 && removed === 0) return null;
      return (
        <span className="fr-file-badge">
          {removed > 0 && <span className="fr-file-badge-del">−{removed}</span>}
          {added > 0 && <span className="fr-file-badge-add">+{added}</span>}
        </span>
      );
    } else if (op.type === "write") {
      const lines = writeLineCount(op);
      if (lines > 0) return <span className="fr-file-badge"><span className="fr-file-badge-add">+{lines}</span></span>;
    }
    return null;
  };

  return (
    <div className="fr-preview-split">
      <div className="fr-preview-tree">

        {/* ─── Section 1: Git Status ─── */}
        {gitSnapshot && gitSnapshot.repos.length > 0 && (
          <div className="fr-sidebar-section">
            {/* Branch badges */}
            <div className="fr-git-branches">
              {gitSnapshot.repos.map((repo) => (
                <div key={repo.root || "_root"} className="fr-git-branch">
                  <span className="fr-git-branch-icon">⎇</span>
                  {gitSnapshot.repos.length > 1 && repo.root && (
                    <span className="fr-git-branch-repo">{repo.root}:</span>
                  )}
                  <span className="fr-git-branch-name">{repo.branch}</span>
                  {repo.is_dirty && <span className="fr-git-branch-dirty">•</span>}
                </div>
              ))}
            </div>

            {/* Git file list */}
            {gitSnapshot.repos.some((r) => r.files.length > 0) && (() => {
              const totalFiles = gitSnapshot.repos.reduce((n, r) => n + r.files.length, 0);
              const totalAdded = gitSnapshot.repos.reduce(
                (n, r) => n + r.files.reduce((a, f) => a + f.additions, 0), 0
              );
              const totalDeleted = gitSnapshot.repos.reduce(
                (n, r) => n + r.files.reduce((a, f) => a + f.deletions, 0), 0
              );
              return (
                <div className="fr-git-files">
                  <div className="fr-section-header">
                    <span className="fr-section-title">Changes</span>
                    <span className="fr-section-stats">
                      {totalFiles}
                      {totalAdded > 0 && <span className="fr-file-badge-add"> +{totalAdded}</span>}
                      {totalDeleted > 0 && <span className="fr-file-badge-del"> −{totalDeleted}</span>}
                    </span>
                  </div>
                  {gitSnapshot.repos.map((repo) =>
                    repo.files.map((f) => {
                      const icon = fileIcon(f.path);
                      const matchingOp = findMatchingOp(f.path);
                      return matchingOp ? (
                        <button
                          key={`${repo.root}/${f.path}`}
                          className={`fr-tree-file ${matchingOp.toolCallId === activeId ? "fr-tree-file-active" : ""}`}
                          onClick={() => onSelect(matchingOp.toolCallId)}
                          title={f.path}
                        >
                          <span className="fr-tree-icon" style={{ color: icon.color }}>{icon.icon}</span>
                          <span className="fr-tree-file-name">{basename(f.path)}</span>
                          {renderBadge(matchingOp) ?? <span className="fr-git-status" style={{ color: gitStatusColor(f.status) }}>{f.status}</span>}
                        </button>
                      ) : (
                        <div
                          key={`${repo.root}/${f.path}`}
                          className="fr-tree-file fr-git-only"
                          title={f.path}
                        >
                          <span className="fr-tree-icon" style={{ color: icon.color }}>{icon.icon}</span>
                          <span className="fr-tree-file-name">{basename(f.path)}</span>
                          <span className="fr-git-status" style={{ color: gitStatusColor(f.status) }}>{f.status}</span>
                        </div>
                      );
                    })
                  )}
                </div>
              );
            })()}
          </div>
        )}

        {/* ─── Section 2: Run Log ─── */}
        {(steps.length > 0 || uniquePlans.length > 0) && (
          <div className="fr-sidebar-section fr-sidebar-section-runlog">
            <div className="fr-section-header">
              <span className="fr-section-title">Run log</span>
              <span className="fr-section-stats">{steps.length} step{steps.length !== 1 ? "s" : ""}</span>
            </div>

            {/* Plans — always visible */}
            {uniquePlans.length > 0 && (
              <div className="fr-runlog-group">
                <div className="fr-runlog-group-label">Plans</div>
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

            {/* Step accordion */}
            {steps.map((step, idx) => {
              const isExpanded = expandedSteps.has(idx);
              return (
                <div key={idx} className="fr-runlog-step">
                  <button
                    className={`fr-runlog-step-header ${isExpanded ? "fr-runlog-step-expanded" : ""}`}
                    onClick={() => toggleStep(idx)}
                  >
                    <span className="fr-runlog-caret">{isExpanded ? "▾" : "▸"}</span>
                    <span className="fr-runlog-step-label">
                      {step.userMessage ? truncateMsg(step.userMessage) : `Step ${idx + 1}`}
                    </span>
                    <span className="fr-runlog-step-count">{step.fileOps.length}</span>
                  </button>
                  {isExpanded && (
                    <div className="fr-runlog-step-body">
                      {step.fileOps.map((op) => {
                        const isActive = op.toolCallId === activeId;
                        const icon = fileIcon(op.filePath);
                        return (
                          <button
                            key={op.toolCallId}
                            className={`fr-tree-file ${isActive ? "fr-tree-file-active" : ""}`}
                            onClick={() => onSelect(op.toolCallId)}
                            title={op.filePath}
                          >
                            <span className="fr-tree-icon" style={{ color: icon.color }}>{icon.icon}</span>
                            <span className="fr-tree-file-name">{basename(op.filePath)}</span>
                            {renderBadge(op)}
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
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
