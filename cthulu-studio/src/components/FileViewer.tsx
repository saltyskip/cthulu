import { useState, useCallback, useEffect, useRef, useMemo } from "react";
import ShikiHighlighter from "react-shiki";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useTheme } from "@/lib/ThemeContext";
import { readSessionFile, listSessionFiles, type FileTreeEntry } from "../api/client";

interface FileViewerProps {
  agentId: string;
  sessionId: string;
  changedFiles: string[];
}

const EXT_TO_LANG: Record<string, string> = {
  ts: "typescript", tsx: "tsx", js: "javascript", jsx: "jsx",
  rs: "rust", py: "python", go: "go", rb: "ruby",
  json: "json", yaml: "yaml", yml: "yaml", toml: "toml",
  md: "markdown", html: "html", css: "css", scss: "scss",
  sh: "bash", bash: "bash", zsh: "bash",
  sql: "sql", graphql: "graphql",
  svg: "xml", xml: "xml",
  dockerfile: "dockerfile",
  c: "c", cpp: "cpp", h: "c", hpp: "cpp",
  java: "java", kt: "kotlin", swift: "swift",
};

const EXT_TO_ICON: Record<string, string> = {
  ts: "\ue628", tsx: "\ue7ba", js: "\ue781", jsx: "\ue7ba",
  rs: "\ue7a8", py: "\ue73c", go: "\ue626", rb: "\ue739",
  json: "\ue60b", yaml: "\ue60b", yml: "\ue60b", toml: "\ue60b",
  md: "\ue73e", html: "\ue736", css: "\ue749", scss: "\ue749",
  sh: "\ue795", bash: "\ue795", zsh: "\ue795",
  svg: "\ue60b", xml: "\ue60b",
  c: "\ue61e", cpp: "\ue61d", h: "\ue61e", hpp: "\ue61d",
  java: "\ue738", kt: "\ue634", swift: "\ue755",
  lock: "\uf023", sql: "\ue706", graphql: "\ue662",
};

const FOLDER_ICON = "\uf07b";
const FOLDER_OPEN_ICON = "\uf07c";
const FILE_ICON = "\uf15b";
const CHANGED_DOT = "\uf111";

function iconForFile(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return EXT_TO_ICON[ext] ?? FILE_ICON;
}

function langFromPath(path: string): string {
  const name = path.split("/").pop() ?? "";
  if (name.toLowerCase() === "dockerfile") return "dockerfile";
  if (name.toLowerCase() === "makefile") return "makefile";
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return EXT_TO_LANG[ext] ?? "text";
}

function isMarkdown(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return ext === "md" || ext === "mdx";
}

/** Flatten tree into visible entries respecting expanded state */
interface FlatEntry {
  entry: FileTreeEntry;
  depth: number;
}

function flattenTree(
  entries: FileTreeEntry[],
  expanded: Set<string>,
  depth: number = 0,
): FlatEntry[] {
  const result: FlatEntry[] = [];
  for (const entry of entries) {
    result.push({ entry, depth });
    if (entry.type === "directory" && expanded.has(entry.path) && entry.children) {
      result.push(...flattenTree(entry.children, expanded, depth + 1));
    }
  }
  return result;
}

export default function FileViewer({ agentId, sessionId, changedFiles }: FileViewerProps) {
  const { theme: appTheme } = useTheme();
  const [tree, setTree] = useState<FileTreeEntry[]>([]);
  const [focusedPath, setFocusedPath] = useState<string | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const treeRef = useRef<HTMLDivElement>(null);

  const shikiTheme = appTheme.shikiTheme;
  const theme = { dark: shikiTheme as string, light: shikiTheme as string };
  const changedPaths = new Set(changedFiles);

  const flatList = useMemo(() => flattenTree(tree, expanded), [tree, expanded]);

  // Auto-expand top-level dirs on first load
  useEffect(() => {
    if (tree.length > 0 && expanded.size === 0) {
      const topDirs = new Set(
        tree.filter((e) => e.type === "directory").map((e) => e.path)
      );
      if (topDirs.size > 0) setExpanded(topDirs);
    }
  }, [tree]); // eslint-disable-line react-hooks/exhaustive-deps

  const refresh = useCallback(async () => {
    try {
      const data = await listSessionFiles(agentId, sessionId);
      setTree(data.tree);
    } catch { /* server unreachable */ }
  }, [agentId, sessionId]);

  useEffect(() => { refresh(); }, [refresh]);

  useEffect(() => {
    if (changedFiles.length > 0) refresh();
  }, [changedFiles.length, refresh]);

  const handleSelect = useCallback(async (path: string) => {
    setSelectedPath(path);
    setFocusedPath(path);
    setLoading(true);
    try {
      const data = await readSessionFile(agentId, sessionId, path);
      setFileContent(data.content);
    } catch {
      setFileContent("// Error reading file");
    } finally {
      setLoading(false);
    }
  }, [agentId, sessionId]);

  useEffect(() => {
    if (selectedPath && changedPaths.has(selectedPath)) {
      handleSelect(selectedPath);
    }
  }, [changedFiles.length]); // eslint-disable-line react-hooks/exhaustive-deps

  const toggleExpand = useCallback((path: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (flatList.length === 0) return;
      const idx = focusedPath ? flatList.findIndex((f) => f.entry.path === focusedPath) : -1;

      if (e.key === "ArrowDown") {
        e.preventDefault();
        const next = Math.min(idx + 1, flatList.length - 1);
        setFocusedPath(flatList[next].entry.path);
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        const next = Math.max(idx <= 0 ? 0 : idx - 1, 0);
        setFocusedPath(flatList[next].entry.path);
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        if (idx < 0) return;
        const item = flatList[idx];
        if (item.entry.type === "directory") {
          if (!expanded.has(item.entry.path)) {
            toggleExpand(item.entry.path);
          } else if (item.entry.children?.length) {
            // Already expanded — move into first child
            setFocusedPath(item.entry.children[0].path);
          }
        }
      } else if (e.key === "ArrowLeft") {
        e.preventDefault();
        if (idx < 0) return;
        const item = flatList[idx];
        if (item.entry.type === "directory" && expanded.has(item.entry.path)) {
          toggleExpand(item.entry.path);
        } else if (item.depth > 0) {
          // Move to parent directory
          for (let i = idx - 1; i >= 0; i--) {
            if (flatList[i].depth < item.depth && flatList[i].entry.type === "directory") {
              setFocusedPath(flatList[i].entry.path);
              break;
            }
          }
        }
      } else if (e.key === "Enter") {
        e.preventDefault();
        if (idx < 0) return;
        const item = flatList[idx];
        if (item.entry.type === "directory") {
          toggleExpand(item.entry.path);
        } else {
          handleSelect(item.entry.path);
        }
      }
    },
    [flatList, focusedPath, expanded, toggleExpand, handleSelect]
  );

  // Scroll focused item into view
  useEffect(() => {
    if (!focusedPath || !treeRef.current) return;
    const el = treeRef.current.querySelector(`[data-path="${CSS.escape(focusedPath)}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }, [focusedPath]);

  return (
    <div className="fv">
      {/* Left: file tree */}
      <div
        className="fv-tree"
        tabIndex={0}
        onKeyDown={handleKeyDown}
        ref={treeRef}
      >
        <div className="fv-tree-header">
          <span>Files</span>
          <button className="fv-refresh" onClick={refresh} title="Refresh">↻</button>
        </div>
        <div className="fv-tree-scroll">
          {flatList.length === 0 ? (
            <div className="fv-empty">No files</div>
          ) : (
            flatList.map(({ entry, depth }) => {
              const isDir = entry.type === "directory";
              const isChanged = changedPaths.has(entry.path);
              const isFocused = entry.path === focusedPath;
              const isSelected = entry.path === selectedPath;
              return (
                <div
                  key={entry.path}
                  data-path={entry.path}
                  className={`fv-item${isSelected ? " fv-item-active" : ""}${isFocused ? " fv-item-focused" : ""}`}
                  style={{ paddingLeft: 8 + depth * 16 }}
                  onClick={() => {
                    if (isDir) {
                      toggleExpand(entry.path);
                      setFocusedPath(entry.path);
                    } else {
                      handleSelect(entry.path);
                    }
                  }}
                >
                  <span className="fv-icon">
                    {isDir
                      ? expanded.has(entry.path) ? FOLDER_OPEN_ICON : FOLDER_ICON
                      : iconForFile(entry.name)}
                  </span>
                  <span className="fv-name">{entry.name}</span>
                  {isChanged && <span className="fv-changed">{CHANGED_DOT}</span>}
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* Right: rendered content */}
      <div className="fv-content">
        {!selectedPath ? (
          <div className="fv-content-empty">Select a file to preview</div>
        ) : loading ? (
          <div className="fv-content-empty">Loading...</div>
        ) : fileContent !== null ? (
          <>
            <div className="fv-content-header">
              <span className="fv-content-path">{selectedPath}</span>
            </div>
            <div className="fv-content-body">
              {isMarkdown(selectedPath) ? (
                <div className="fv-markdown">
                  <ReactMarkdown
                    remarkPlugins={[remarkGfm]}
                    components={{
                      code({ className, children, ...props }) {
                        const match = /language-(\w+)/.exec(className || "");
                        const code = String(children).replace(/\n$/, "");
                        if (match) {
                          return (
                            <div className="fv-md-code-block">
                              <ShikiHighlighter
                                language={match[1]}
                                theme={theme}
                                addDefaultStyles={false}
                                showLanguage={false}
                              >
                                {code}
                              </ShikiHighlighter>
                            </div>
                          );
                        }
                        return <code className={className} {...props}>{children}</code>;
                      },
                    }}
                  >
                    {fileContent}
                  </ReactMarkdown>
                </div>
              ) : (
                <div className="fv-shiki">
                  <ShikiHighlighter
                    language={langFromPath(selectedPath)}
                    theme={theme}
                    addDefaultStyles={false}
                    showLanguage={false}
                  >
                    {fileContent}
                  </ShikiHighlighter>
                </div>
              )}
            </div>
          </>
        ) : null}
      </div>
    </div>
  );
}

