import { useState, useCallback, useEffect, useMemo } from "react";
import { getGitDiff, readSessionFile, listSessionFiles } from "../api/client";
import type { MultiRepoSnapshot, GitFileStatus } from "./chat/FilePreviewContext";
import { useShikiTokens, type Token } from "./chat/useShikiTokens";
import { useTheme } from "../lib/ThemeContext";
import { langFromPath } from "../utils/langFromPath";

interface ChangesPanelProps {
  agentId: string;
  sessionId: string;
  gitSnapshot: MultiRepoSnapshot | null;
  hookChangedFiles: string[];
}

function statusBadgeColor(status: string): string {
  switch (status) {
    case "M": return "var(--warning)";
    case "A": case "?": return "var(--success)";
    case "D": return "var(--danger)";
    case "R": return "var(--accent)";
    default: return "var(--text-secondary)";
  }
}

function statusLabel(status: string): string {
  switch (status) {
    case "M": return "M";
    case "A": return "A";
    case "D": return "D";
    case "?": return "U";
    case "R": return "R";
    default: return status;
  }
}

/** Parsed line from a unified diff. */
interface ParsedDiffLine {
  type: "add" | "del" | "ctx" | "meta" | "hunk";
  text: string;
}

/** Parse a unified diff into structured lines + old/new source for tokenization. */
function parseUnifiedDiff(diff: string): {
  lines: ParsedDiffLine[];
  oldSource: string;
  newSource: string;
} {
  const lines: ParsedDiffLine[] = [];
  const oldLines: string[] = [];
  const newLines: string[] = [];

  for (const raw of diff.split("\n")) {
    if (raw.startsWith("diff ") || raw.startsWith("index ") || raw.startsWith("--- ") || raw.startsWith("+++ ") || raw.startsWith("new file") || raw.startsWith("deleted file")) {
      lines.push({ type: "meta", text: raw });
    } else if (raw.startsWith("@@")) {
      lines.push({ type: "hunk", text: raw });
    } else if (raw.startsWith("+")) {
      lines.push({ type: "add", text: raw.slice(1) });
      newLines.push(raw.slice(1));
    } else if (raw.startsWith("-")) {
      lines.push({ type: "del", text: raw.slice(1) });
      oldLines.push(raw.slice(1));
    } else {
      const text = raw.startsWith(" ") ? raw.slice(1) : raw;
      lines.push({ type: "ctx", text });
      oldLines.push(text);
      newLines.push(text);
    }
  }

  return { lines, oldSource: oldLines.join("\n"), newSource: newLines.join("\n") };
}

/** Highlighted diff view using shiki tokens. */
function HighlightedDiff({ diff, filePath }: { diff: string; filePath: string }) {
  const { theme: appTheme } = useTheme();
  const lang = useMemo(() => langFromPath(filePath), [filePath]);

  const { lines, oldSource, newSource } = useMemo(() => parseUnifiedDiff(diff), [diff]);

  const oldTokens = useShikiTokens(oldSource || undefined, lang, appTheme.shikiTheme);
  const newTokens = useShikiTokens(newSource || undefined, lang, appTheme.shikiTheme);

  const tokenMap = useMemo(() => {
    const map: (Token[] | null)[] = [];
    let oldIdx = 0;
    let newIdx = 0;
    for (const line of lines) {
      if (line.type === "del") {
        map.push(oldTokens?.[oldIdx] ?? null);
        oldIdx++;
      } else if (line.type === "add") {
        map.push(newTokens?.[newIdx] ?? null);
        newIdx++;
      } else if (line.type === "ctx") {
        map.push(newTokens?.[newIdx] ?? oldTokens?.[oldIdx] ?? null);
        oldIdx++;
        newIdx++;
      } else {
        map.push(null);
      }
    }
    return map;
  }, [lines, oldTokens, newTokens]);

  return (
    <pre className="changes-diff-content">
      {lines.map((line, i) => {
        const tokens = tokenMap[i];
        const cls =
          line.type === "add" ? "changes-diff-add"
          : line.type === "del" ? "changes-diff-del"
          : line.type === "hunk" ? "changes-diff-hunk"
          : line.type === "meta" ? "changes-diff-meta"
          : "changes-diff-ctx";
        return (
          <div key={i} className={cls}>
            {line.type === "meta" || line.type === "hunk" ? (
              line.text
            ) : (
              <>
                <span className="changes-diff-prefix">
                  {line.type === "add" ? "+" : line.type === "del" ? "−" : " "}
                </span>
                {tokens
                  ? tokens.map((t, j) => (
                      <span key={j} style={t.color ? { color: t.color } : undefined}>
                        {t.content}
                      </span>
                    ))
                  : line.text || " "}
              </>
            )}
          </div>
        );
      })}
    </pre>
  );
}

/** Strip an absolute path's working-dir prefix to get a relative path. */
function makeRelative(absPath: string, workingDir: string): string {
  if (workingDir && absPath.startsWith(workingDir)) {
    let rel = absPath.slice(workingDir.length);
    if (rel.startsWith("/")) rel = rel.slice(1);
    return rel || absPath;
  }
  return absPath;
}

export default function ChangesPanel({
  agentId,
  sessionId,
  gitSnapshot,
  hookChangedFiles,
}: ChangesPanelProps) {
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [selectedRepoRoot, setSelectedRepoRoot] = useState<string>(".");
  const [diff, setDiff] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [workingDir, setWorkingDir] = useState<string>("");

  // Fetch working dir once so we can make hook paths relative
  useEffect(() => {
    listSessionFiles(agentId, sessionId)
      .then((data) => setWorkingDir(data.root || ""))
      .catch(() => {});
  }, [agentId, sessionId]);

  const hasGit = gitSnapshot && gitSnapshot.repos.length > 0;

  const handleGitFileClick = useCallback(async (file: GitFileStatus, repoRoot: string) => {
    setSelectedPath(file.path);
    setSelectedRepoRoot(repoRoot);
    setError(null);
    setDiff(null);
    setFileContent(null);

    if (file.status === "D") {
      setDiff(null);
      setError("File deleted");
      return;
    }

    setLoading(true);
    try {
      const result = await getGitDiff(agentId, sessionId, file.path, repoRoot === "." ? undefined : repoRoot);
      setDiff(result.diff || null);
      if (!result.diff) setError("No diff available");
    } catch {
      setError("Failed to load diff");
    } finally {
      setLoading(false);
    }
  }, [agentId, sessionId]);

  const handleHookFileClick = useCallback(async (absPath: string) => {
    const relPath = makeRelative(absPath, workingDir);
    setSelectedPath(relPath);
    setSelectedRepoRoot(".");
    setError(null);
    setDiff(null);
    setFileContent(null);

    setLoading(true);
    try {
      // Try git diff first — works if session has git integration
      const result = await getGitDiff(agentId, sessionId, relPath);
      if (result.diff) {
        setDiff(result.diff);
        setLoading(false);
        return;
      }
    } catch {
      // No git integration — fall through to file content
    }
    try {
      const result = await readSessionFile(agentId, sessionId, relPath);
      setFileContent(result.content);
    } catch {
      setError("Failed to read file");
    } finally {
      setLoading(false);
    }
  }, [agentId, sessionId, workingDir]);

  const hasChanges = hasGit
    ? gitSnapshot.repos.some((r) => r.files.length > 0)
    : hookChangedFiles.length > 0;

  return (
    <div className="changes-panel">
      <div className="changes-file-list">
        {hasGit ? (
          gitSnapshot.repos.map((repo) => (
            <div key={repo.root}>
              <div className="changes-repo-header">
                <svg width="12" height="12" viewBox="0 0 16 16" fill="currentColor" style={{ flexShrink: 0 }}>
                  <path d="M11.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zm-2.25.75a2.25 2.25 0 1 1 3 2.122V6A2.5 2.5 0 0 1 10 8.5H6A1.5 1.5 0 0 0 4.5 10v1.128a2.251 2.251 0 1 1-1.5 0V5.372a2.25 2.25 0 1 1 1.5 0v1.836A2.99 2.99 0 0 1 6 7h4a1.5 1.5 0 0 0 1.5-1.5v-.628A2.25 2.25 0 0 1 9.5 3.25zM4.25 12a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5zM3.5 3.25a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0z"/>
                </svg>
                <span className="changes-branch-name">{repo.branch}</span>
                {repo.root !== "." && <span className="changes-repo-root">{repo.root}</span>}
                {repo.is_dirty && <span className="changes-dirty-dot" />}
              </div>
              {repo.files.length === 0 ? (
                <div className="changes-empty-repo">No changes</div>
              ) : (
                repo.files.map((file) => {
                  const isSelected = selectedPath === file.path && selectedRepoRoot === repo.root;
                  return (
                    <div
                      key={`${repo.root}:${file.path}`}
                      className={`changes-file ${isSelected ? "changes-file-active" : ""}`}
                      onClick={() => handleGitFileClick(file, repo.root)}
                    >
                      <span className="changes-status-badge" style={{ color: statusBadgeColor(file.status) }}>
                        {statusLabel(file.status)}
                      </span>
                      <span className="changes-file-name">{file.path}</span>
                      {(file.additions > 0 || file.deletions > 0) && (
                        <span className="changes-file-stats">
                          {file.additions > 0 && <span style={{ color: "var(--success)" }}>+{file.additions}</span>}
                          {file.deletions > 0 && <span style={{ color: "var(--danger)" }}> -{file.deletions}</span>}
                        </span>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          ))
        ) : hookChangedFiles.length > 0 ? (
          hookChangedFiles.map((absPath) => {
            const relPath = makeRelative(absPath, workingDir);
            const isSelected = selectedPath === relPath;
            return (
              <div
                key={absPath}
                className={`changes-file ${isSelected ? "changes-file-active" : ""}`}
                onClick={() => handleHookFileClick(absPath)}
              >
                <span className="changes-status-badge" style={{ color: "var(--accent)" }}>M</span>
                <span className="changes-file-name">{relPath}</span>
              </div>
            );
          })
        ) : null}
        {!hasChanges && (
          <div className="changes-empty">No changes detected</div>
        )}
      </div>
      <div className="changes-diff-view">
        {!selectedPath ? (
          <div className="changes-diff-placeholder">Select a file to view changes</div>
        ) : loading ? (
          <div className="changes-diff-placeholder">Loading...</div>
        ) : error ? (
          <div className="changes-diff-placeholder">{error}</div>
        ) : diff ? (
          <HighlightedDiff diff={diff} filePath={selectedPath} />
        ) : fileContent != null ? (
          <pre className="changes-diff-content">
            {fileContent.split("\n").map((line, i) => (
              <div key={i} className="changes-diff-ctx">{line || " "}</div>
            ))}
          </pre>
        ) : (
          <div className="changes-diff-placeholder">No changes available</div>
        )}
      </div>
    </div>
  );
}
