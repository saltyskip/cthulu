import { memo, useState, useMemo } from "react";
import ShikiHighlighter from "react-shiki";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useTheme } from "../../lib/ThemeContext";
import type { FileOp, PlanOp } from "./FilePreviewContext";
import { computeDiffLines } from "../../utils/diff";
import { fileIcon } from "../../utils/fileIcons";
import { langFromPath } from "../../utils/langFromPath";
import { SyntaxHighlighter } from "../assistant-ui/shiki-highlighter";
import { useShikiTokens, type Token } from "./useShikiTokens";
import { groupFileOpsByPath, type FileGroup } from "./chatUtils";

function basename(filePath: string): string {
  return filePath.replace(/\\/g, "/").split("/").pop() || filePath;
}

/** Render a single diff (Edit op) with Shiki-highlighted tokens. */
function DiffSection({ op, shikiTheme }: { op: FileOp; shikiTheme: string | Record<string, unknown> }) {
  const lang = useMemo(() => langFromPath(op.filePath), [op.filePath]);
  const diffLines = useMemo(
    () =>
      op.oldString !== undefined && op.newString !== undefined
        ? computeDiffLines(op.oldString, op.newString)
        : null,
    [op.oldString, op.newString],
  );

  const oldTokens = useShikiTokens(op.oldString, lang, shikiTheme);
  const newTokens = useShikiTokens(op.newString, lang, shikiTheme);

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

  if (!diffLines) return null;

  return (
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
  );
}

const FilePreviewPanel = memo(function FilePreviewPanel({
  fileOps,
  plans,
  selectedPath,
  onSelectPath,
}: {
  fileOps: FileOp[];
  plans: PlanOp[];
  selectedPath: string | null;
  onSelectPath: (path: string) => void;
}) {
  const { theme: appTheme } = useTheme();
  const shikiTheme = appTheme.shikiTheme;
  const theme = { dark: shikiTheme as string, light: shikiTheme as string };

  // Group file ops by path with stats
  const fileGroups = useMemo(() => groupFileOpsByPath(fileOps), [fileOps]);

  // Deduplicate plans by filePath (keep latest)
  const uniquePlans = useMemo(() => {
    const planMap = new Map<string, PlanOp>();
    for (const p of plans) planMap.set(p.filePath, p);
    return [...planMap.values()];
  }, [plans]);

  // Resolve selected item
  const isPlan = selectedPath?.startsWith("plan:") ?? false;
  const selectedPlanId = isPlan ? selectedPath!.slice(5) : null;
  const selectedFilePath = isPlan ? null : selectedPath;

  const selectedPlan = selectedPlanId
    ? uniquePlans.find((p) => p.toolCallId === selectedPlanId) ?? null
    : null;
  const selectedGroup = selectedFilePath
    ? fileGroups.find((g) => g.filePath === selectedFilePath) ?? null
    : null;

  // Fallback: select latest if nothing selected
  const effectivePlan = selectedPlan;
  const effectiveGroup = selectedGroup ?? (!isPlan && !selectedPath ? fileGroups[fileGroups.length - 1] ?? null : null);

  // Group files by directory for display
  const dirGroups = useMemo(() => {
    const groups = new Map<string, FileGroup[]>();
    for (const g of fileGroups) {
      const parts = g.filePath.replace(/\\/g, "/").split("/");
      parts.pop(); // remove filename
      const dir = parts.length > 0 ? parts.slice(-2).join("/") : "";
      const existing = groups.get(dir) || [];
      existing.push(g);
      groups.set(dir, existing);
    }
    return groups;
  }, [fileGroups]);

  const hasContent = fileGroups.length > 0 || uniquePlans.length > 0;
  if (!hasContent) return null;

  const activePath = effectivePlan ? `plan:${effectivePlan.toolCallId}` : effectiveGroup?.filePath ?? null;

  return (
    <div className="fr-preview-split">
      <div className="fr-preview-tree">
        {/* Plans section */}
        {uniquePlans.length > 0 && (
          <div className="fr-tree-section">
            <div className="fr-tree-section-label">Plans</div>
            {uniquePlans.map((p) => {
              const planKey = `plan:${p.toolCallId}`;
              const isActive = activePath === planKey;
              const icon = fileIcon(p.filePath);
              return (
                <button
                  key={p.toolCallId}
                  className={`fr-tree-file ${isActive ? "fr-tree-file-active" : ""}`}
                  onClick={() => onSelectPath(planKey)}
                >
                  <span className="fr-tree-icon" style={{ color: icon.color }}>{icon.icon}</span>
                  {basename(p.filePath)}
                </button>
              );
            })}
          </div>
        )}

        {/* Files section */}
        {fileGroups.length > 0 && (
          <div className="fr-tree-section">
            {uniquePlans.length > 0 && <div className="fr-tree-section-label">Files</div>}
            {[...dirGroups.entries()].map(([dir, groups]) => (
              <div key={dir} className="fr-tree-group">
                {dir && <div className="fr-tree-dir">{dir}</div>}
                {groups.map((g) => {
                  const isActive = activePath === g.filePath;
                  const icon = fileIcon(g.filePath);
                  return (
                    <button
                      key={g.filePath}
                      className={`fr-tree-file ${isActive ? "fr-tree-file-active" : ""}`}
                      onClick={() => onSelectPath(g.filePath)}
                    >
                      <span className="fr-tree-icon" style={{ color: icon.color }}>{icon.icon}</span>
                      <span className="fr-tree-name">{basename(g.filePath)}</span>
                      <span className="fr-tree-stats">
                        {g.linesAdded > 0 && <span className="fr-tree-added">+{g.linesAdded}</span>}
                        {g.linesRemoved > 0 && <span className="fr-tree-removed">-{g.linesRemoved}</span>}
                      </span>
                      {g.ops.length > 1 && (
                        <span className="fr-tree-count">×{g.ops.length}</span>
                      )}
                    </button>
                  );
                })}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="fr-preview-main">
        {effectivePlan ? (
          <>
            <div className="fr-preview-path">{basename(effectivePlan.filePath)}</div>
            <div className="fr-preview-body">
              <div className="fr-preview-markdown fr-md">
                <Markdown
                  remarkPlugins={[remarkGfm]}
                  components={{
                    code({ className, children, ...props }) {
                      const match = /language-(\w+)/.exec(className || "");
                      const code = String(children).replace(/\n$/, "");
                      if (match) {
                        return <SyntaxHighlighter language={match[1]} code={code} />;
                      }
                      return <code className={className} {...props}>{children}</code>;
                    },
                  }}
                >
                  {effectivePlan.content || ""}
                </Markdown>
              </div>
            </div>
          </>
        ) : effectiveGroup ? (
          <>
            <div className="fr-preview-path">{effectiveGroup.filePath}</div>
            <div className="fr-preview-body">
              <FileGroupDiffs group={effectiveGroup} shikiTheme={shikiTheme} theme={theme} />
            </div>
          </>
        ) : (
          <div className="fr-preview-body">
            <div className="fr-preview-empty">No preview available</div>
          </div>
        )}
      </div>
    </div>
  );
});

/** Render all ops for a file group — stacked diffs with dividers. */
function FileGroupDiffs({
  group,
  shikiTheme,
  theme,
}: {
  group: FileGroup;
  shikiTheme: string | Record<string, unknown>;
  theme: { dark: string; light: string };
}) {
  const lang = useMemo(() => langFromPath(group.filePath), [group.filePath]);

  return (
    <>
      {group.ops.map((op, i) => (
        <div key={op.toolCallId}>
          {i > 0 && <div className="fr-diff-divider">···</div>}
          {op.type === "edit" ? (
            <DiffSection op={op} shikiTheme={shikiTheme} />
          ) : op.content ? (
            lang ? (
              <ShikiHighlighter
                language={lang}
                theme={theme}
                addDefaultStyles={false}
                showLanguage={false}
                defaultColor="light-dark()"
                className="fr-preview-shiki"
              >
                {op.content}
              </ShikiHighlighter>
            ) : (
              <pre className="fr-preview-content">{op.content}</pre>
            )
          ) : (
            <div className="fr-preview-empty">No content available</div>
          )}
        </div>
      ))}
    </>
  );
}

export default FilePreviewPanel;
