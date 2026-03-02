import { memo, useMemo } from "react";
import ShikiHighlighter from "react-shiki";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useTheme } from "../../lib/ThemeContext";
import type { FileOp, PlanOp } from "./FilePreviewContext";
import { computeDiffLines } from "../../utils/diff";
import { fileIcon } from "../../utils/fileIcons";
import { SyntaxHighlighter } from "../assistant-ui/shiki-highlighter";
import { useShikiTokens, type Token } from "./useShikiTokens";

const EXT_TO_LANG: Record<string, string> = {
  ts: "typescript", tsx: "tsx", js: "javascript", jsx: "jsx",
  rs: "rust", py: "python", rb: "ruby", go: "go",
  java: "java", kt: "kotlin", swift: "swift", cs: "csharp",
  css: "css", scss: "scss", html: "html", vue: "vue", svelte: "svelte",
  json: "json", yaml: "yaml", yml: "yaml", toml: "toml",
  md: "markdown", sql: "sql", sh: "bash", zsh: "bash", bash: "bash",
  dockerfile: "dockerfile", graphql: "graphql",
};

function langFromPath(filePath: string): string | undefined {
  const ext = filePath.split(".").pop()?.toLowerCase() || "";
  return EXT_TO_LANG[ext];
}

function basename(filePath: string): string {
  return filePath.replace(/\\/g, "/").split("/").pop() || filePath;
}

// Selected item can be a file op or a plan op
type SelectedItem =
  | { kind: "file"; op: FileOp }
  | { kind: "plan"; op: PlanOp };

const FilePreviewPanel = memo(function FilePreviewPanel({
  fileOps,
  plans,
  selectedId,
  onSelect,
}: {
  fileOps: FileOp[];
  plans: PlanOp[];
  selectedId: string | null;
  onSelect: (id: string) => void;
}) {
  const { theme: appTheme } = useTheme();
  const shikiTheme = appTheme.shikiTheme;
  const theme = { dark: shikiTheme as string, light: shikiTheme as string };

  // Find selected item across both plans and files
  const selected: SelectedItem | null = useMemo(() => {
    if (selectedId) {
      const plan = plans.find((p) => p.toolCallId === selectedId);
      if (plan) return { kind: "plan", op: plan };
      const file = fileOps.find((f) => f.toolCallId === selectedId);
      if (file) return { kind: "file", op: file };
    }
    // Default to latest item overall
    const lastFile = fileOps[fileOps.length - 1];
    const lastPlan = plans[plans.length - 1];
    if (!lastFile && !lastPlan) return null;
    if (!lastFile) return { kind: "plan", op: lastPlan };
    if (!lastPlan) return { kind: "file", op: lastFile };
    // Both exist — pick whichever has the later toolCallId (proxy for recency)
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
        // ctx — advance both
        map.push(newTokens?.[newIdx] ?? oldTokens?.[oldIdx] ?? null);
        oldIdx++;
        newIdx++;
      }
    }
    return map;
  }, [diffLines, oldTokens, newTokens]);

  if (!selected) return null;

  // Build unique file list grouped by directory
  const fileMap = new Map<string, FileOp>();
  for (const f of fileOps) fileMap.set(f.filePath, f);
  const uniqueFiles = [...fileMap.values()];

  const groups = new Map<string, FileOp[]>();
  for (const f of uniqueFiles) {
    const parts = f.filePath.replace(/\\/g, "/").split("/");
    const name = parts.pop() || f.filePath;
    const dir = parts.length > 0 ? parts.slice(-2).join("/") : "";
    const existing = groups.get(dir) || [];
    existing.push({ ...f, filePath: name }); // store basename for display
    groups.set(dir, existing);
  }

  // Deduplicate plans by filePath (keep latest)
  const planMap = new Map<string, PlanOp>();
  for (const p of plans) planMap.set(p.filePath, p);
  const uniquePlans = [...planMap.values()];

  const activeId = selected.kind === "file" ? selected.op.toolCallId : selected.op.toolCallId;

  return (
    <div className="fr-preview-split">
      <div className="fr-preview-tree">
        {/* Plans section */}
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
                >
                  <span className="fr-tree-icon" style={{ color: icon.color }}>{icon.icon}</span>
                  {basename(p.filePath)}
                </button>
              );
            })}
          </div>
        )}

        {/* Files section */}
        {uniqueFiles.length > 0 && (
          <div className="fr-tree-section">
            {uniquePlans.length > 0 && <div className="fr-tree-section-label">Files</div>}
            {[...groups.entries()].map(([dir, files]) => (
              <div key={dir} className="fr-tree-group">
                {dir && <div className="fr-tree-dir">{dir}</div>}
                {files.map((f) => {
                  const original = uniqueFiles.find((o) => o.filePath.endsWith(f.filePath) && o.toolCallId === f.toolCallId);
                  const isActive = original?.toolCallId === activeId;
                  const icon = fileIcon(f.filePath);
                  return (
                    <button
                      key={f.toolCallId}
                      className={`fr-tree-file ${isActive ? "fr-tree-file-active" : ""}`}
                      onClick={() => onSelect(f.toolCallId)}
                    >
                      <span className="fr-tree-icon" style={{ color: icon.color }}>{icon.icon}</span>
                      {f.filePath}
                    </button>
                  );
                })}
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="fr-preview-main">
        <div className="fr-preview-path">{selected.kind === "file" ? selected.op.filePath : basename(selected.op.filePath)}</div>
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
