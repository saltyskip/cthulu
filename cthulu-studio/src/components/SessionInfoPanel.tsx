import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import ShikiHighlighter from "react-shiki";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { useTheme } from "@/lib/ThemeContext";
import {
  getAgent,
  getSessionStatus,
  killSession,
  deleteAgentSession,
  updateAgent,
  readSessionFile,
  listSessionFiles,
  type SessionStatus,
  type FileTreeEntry,
} from "../api/client";
import type { Agent } from "../types/flow";

interface SessionInfoPanelProps {
  agentId: string;
  sessionId: string;
  onSessionDeleted: () => void;
}

const KNOWN_PERMISSIONS = [
  "Bash", "Read", "Write", "Edit", "Glob", "Grep",
  "WebFetch", "WebSearch", "NotebookEdit",
];

// Navigation items for the left sidebar
type NavId =
  | "overview"
  | "prompts/agent"
  | "prompts/system"
  | `skills/${string}`
  | `config/${string}`;

interface NavItem {
  id: NavId;
  label: string;
  depth: number;
  isFolder?: boolean;
  icon?: string;
}

// Config files to look for in the working directory
const CONFIG_FILES = ["AGENT.md", "CLAUDE.md", ".claude/settings.json"];

export default function SessionInfoPanel({
  agentId,
  sessionId,
  onSessionDeleted,
}: SessionInfoPanelProps) {
  const { theme: appTheme } = useTheme();
  const [agent, setAgent] = useState<Agent | null>(null);
  const [status, setStatus] = useState<SessionStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [killing, setKilling] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [selected, setSelected] = useState<NavId>("overview");
  const [treeWidth, setTreeWidth] = useState(160);

  // Inline editing
  const [editingField, setEditingField] = useState<"name" | "working_dir" | null>(null);
  const [editValue, setEditValue] = useState("");
  const editInputRef = useRef<HTMLInputElement>(null);
  const [editingPerms, setEditingPerms] = useState(false);
  const [draftPerms, setDraftPerms] = useState<string[]>([]);

  // Copy feedback
  const [copied, setCopied] = useState(false);

  // Skills files discovered from the file tree
  const [skillsFiles, setSkillsFiles] = useState<string[]>([]);
  // Config files found
  const [configFiles, setConfigFiles] = useState<string[]>([]);
  // File content for detail view
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [fileLoading, setFileLoading] = useState(false);

  // Expanded folders in nav
  const [expanded, setExpanded] = useState<Set<string>>(new Set(["prompts", "skills", "config"]));

  const shikiTheme = appTheme.shikiTheme;
  const theme = { dark: shikiTheme as string, light: shikiTheme as string };

  // Fetch agent + session status
  useEffect(() => {
    let cancelled = false;
    const fetchAll = async () => {
      try {
        const [a, s] = await Promise.all([
          getAgent(agentId),
          getSessionStatus(agentId, sessionId),
        ]);
        if (cancelled) return;
        setAgent(a);
        setStatus(s);
        setError(null);
      } catch {
        if (!cancelled) setError("Failed to load session info");
      }
    };
    fetchAll();
    const interval = setInterval(fetchAll, 5000);
    return () => { cancelled = true; clearInterval(interval); };
  }, [agentId, sessionId]);

  // Discover skills files and config files from file tree
  useEffect(() => {
    let cancelled = false;
    const discover = async () => {
      try {
        const data = await listSessionFiles(agentId, sessionId);
        if (cancelled) return;

        // Find .skills/ directory
        const skills: string[] = [];
        const findSkills = (entries: FileTreeEntry[], parentPath: string) => {
          for (const e of entries) {
            if (e.type === "directory" && e.name === ".skills" && e.children) {
              for (const child of e.children) {
                if (child.type === "file") {
                  skills.push(child.path);
                }
              }
            }
            if (e.type === "directory" && e.children) {
              findSkills(e.children, e.path);
            }
          }
        };
        findSkills(data.tree, "");
        setSkillsFiles(skills);

        // Find config files at root
        const configs: string[] = [];
        const rootNames = new Set(data.tree.map((e) => e.path));
        for (const cf of CONFIG_FILES) {
          // Check both root and nested
          const findFile = (entries: FileTreeEntry[], target: string): string | null => {
            for (const e of entries) {
              if (e.type === "file" && e.path.endsWith(target)) return e.path;
              if (e.type === "directory" && e.children) {
                const found = findFile(e.children, target);
                if (found) return found;
              }
            }
            return null;
          };
          const found = findFile(data.tree, cf);
          if (found) configs.push(found);
        }
        setConfigFiles(configs);
      } catch { /* ignore */ }
    };
    discover();
  }, [agentId, sessionId]);

  // Load file content when selecting a file-based nav item
  useEffect(() => {
    if (selected === "overview") { setFileContent(null); return; }
    if (selected === "prompts/agent" || selected === "prompts/system") {
      setFileContent(null);
      return;
    }

    // It's a file path (skills/... or config/...)
    const path = selected.startsWith("skills/")
      ? selected.slice("skills/".length)
      : selected.startsWith("config/")
        ? selected.slice("config/".length)
        : null;

    if (!path) return;

    let cancelled = false;
    setFileLoading(true);
    readSessionFile(agentId, sessionId, path)
      .then((data) => {
        if (!cancelled) setFileContent(data.content);
      })
      .catch(() => {
        if (!cancelled) setFileContent("// Error reading file");
      })
      .finally(() => {
        if (!cancelled) setFileLoading(false);
      });
    return () => { cancelled = true; };
  }, [selected, agentId, sessionId]);

  // Focus input when editing starts
  useEffect(() => {
    if (editingField && editInputRef.current) {
      editInputRef.current.focus();
      editInputRef.current.select();
    }
  }, [editingField]);

  const handleKill = useCallback(async () => {
    setKilling(true);
    try {
      await killSession(agentId, sessionId);
      const s = await getSessionStatus(agentId, sessionId);
      setStatus(s);
    } catch {
      setError("Failed to kill session");
    } finally {
      setKilling(false);
    }
  }, [agentId, sessionId]);

  const handleDelete = useCallback(async () => {
    setDeleting(true);
    try {
      await deleteAgentSession(agentId, sessionId);
      onSessionDeleted();
    } catch {
      setError("Failed to delete session");
      setDeleting(false);
      setConfirmDelete(false);
    }
  }, [agentId, sessionId, onSessionDeleted]);

  const startEdit = (field: "name" | "working_dir") => {
    if (!agent) return;
    setEditingField(field);
    setEditValue(field === "name" ? agent.name : (agent.working_dir ?? ""));
  };

  const cancelEdit = () => { setEditingField(null); setEditValue(""); };

  const saveEdit = async () => {
    if (!agent || !editingField) return;
    try {
      const updates = editingField === "name"
        ? { name: editValue }
        : { working_dir: editValue || null };
      const updated = await updateAgent(agentId, updates);
      setAgent(updated);
    } catch { setError("Failed to save"); }
    setEditingField(null);
  };

  const handleEditKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") saveEdit();
    if (e.key === "Escape") cancelEdit();
  };

  const startPermEdit = () => {
    if (!agent) return;
    setEditingPerms(true);
    setDraftPerms([...agent.permissions]);
  };

  const togglePerm = (perm: string) => {
    setDraftPerms((prev) =>
      prev.includes(perm) ? prev.filter((p) => p !== perm) : [...prev, perm]
    );
  };

  const savePerms = async () => {
    if (!agent) return;
    try {
      const updated = await updateAgent(agentId, { permissions: draftPerms });
      setAgent(updated);
    } catch { setError("Failed to save permissions"); }
    setEditingPerms(false);
  };

  const copySessionId = () => {
    navigator.clipboard.writeText(sessionId);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const toggleFolder = (folder: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(folder)) next.delete(folder);
      else next.add(folder);
      return next;
    });
  };

  // Build nav items
  const navItems = useMemo((): NavItem[] => {
    const items: NavItem[] = [];

    items.push({ id: "overview", label: "Overview", depth: 0, icon: "ℹ" });

    // Prompts folder
    const hasPrompts = agent && (agent.prompt || agent.append_system_prompt);
    if (hasPrompts) {
      items.push({ id: "prompts/agent" as NavId, label: "Prompts", depth: 0, isFolder: true, icon: "📝" });
      if (expanded.has("prompts")) {
        if (agent!.prompt) items.push({ id: "prompts/agent", label: "Agent Prompt", depth: 1 });
        if (agent!.append_system_prompt) items.push({ id: "prompts/system", label: "System Prompt", depth: 1 });
      }
    }

    // Skills folder
    if (skillsFiles.length > 0) {
      items.push({ id: `skills/${skillsFiles[0]}` as NavId, label: "Skills", depth: 0, isFolder: true, icon: "⚡" });
      if (expanded.has("skills")) {
        for (const f of skillsFiles) {
          const name = f.split("/").pop() ?? f;
          items.push({ id: `skills/${f}` as NavId, label: name, depth: 1 });
        }
      }
    }

    // Config files folder
    if (configFiles.length > 0) {
      items.push({ id: `config/${configFiles[0]}` as NavId, label: "Config", depth: 0, isFolder: true, icon: "⚙" });
      if (expanded.has("config")) {
        for (const f of configFiles) {
          const name = f.split("/").pop() ?? f;
          items.push({ id: `config/${f}` as NavId, label: name, depth: 1 });
        }
      }
    }

    return items;
  }, [agent, skillsFiles, configFiles, expanded]);

  const handleTreeResize = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = treeWidth;
    const onMove = (ev: MouseEvent) => {
      setTreeWidth(Math.max(100, Math.min(300, startW + ev.clientX - startX)));
    };
    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  }, [treeWidth]);

  if (error && !agent && !status) {
    return (
      <div className="session-info-panel">
        <div className="session-info-error">{error}</div>
      </div>
    );
  }

  const isFileView = selected !== "overview" && selected !== "prompts/agent" && selected !== "prompts/system";
  const filePath = isFileView
    ? selected.startsWith("skills/") ? selected.slice("skills/".length)
      : selected.startsWith("config/") ? selected.slice("config/".length)
      : null
    : null;

  const isMarkdown = filePath ? /\.md$/i.test(filePath) : false;

  return (
    <div className="fv">
      {/* Left: nav tree */}
      <div className="fv-tree" style={{ width: treeWidth }}>
        <div className="fv-tree-header">
          <span>Session Info</span>
        </div>
        <div className="fv-tree-scroll">
          {navItems.map((item) => {
            const isActive = item.id === selected ||
              (item.isFolder && selected.startsWith(item.id.split("/")[0] === "prompts" ? "prompts/" : item.id.split("/")[0] + "/"));
            const folderKey = item.id.split("/")[0];
            return (
              <div
                key={`${item.id}-${item.isFolder ? "folder" : "item"}`}
                className={`fv-item${isActive && !item.isFolder ? " fv-item-active" : ""}`}
                style={{ paddingLeft: 8 + item.depth * 16 }}
                onClick={() => {
                  if (item.isFolder) {
                    toggleFolder(folderKey);
                  } else {
                    setSelected(item.id);
                  }
                }}
              >
                {item.isFolder ? (
                  <span className="fv-icon">
                    {expanded.has(folderKey) ? "▾" : "▸"}
                  </span>
                ) : item.depth === 0 ? (
                  <span className="fv-icon">{item.icon}</span>
                ) : (
                  <span className="fv-icon" style={{ fontSize: 9, opacity: 0.5 }}>•</span>
                )}
                <span className="fv-name">{item.label}</span>
              </div>
            );
          })}
        </div>
      </div>

      <div className="fv-tree-divider" onMouseDown={handleTreeResize} />

      {/* Right: detail pane */}
      <div className="fv-content">
        {selected === "overview" && (
          <div className="session-info-detail">
            {/* Session status */}
            <section className="session-info-section">
              <h3 className="session-info-heading">Session</h3>
              {status ? (
                <div className="session-info-grid">
                  <Row label="Status">
                    <StatusDot alive={status.process_alive} busy={status.busy} />
                    {status.busy ? "Busy" : status.process_alive ? "Idle" : "Stopped"}
                  </Row>
                  <Row label="Messages">{status.message_count}</Row>
                  <Row label="Cost">
                    {status.total_cost > 0 ? `$${status.total_cost.toFixed(4)}` : "—"}
                  </Row>
                  {status.created_at && (
                    <Row label="Created">{new Date(status.created_at).toLocaleString()}</Row>
                  )}
                  {status.busy_since && (
                    <Row label="Busy since">{new Date(status.busy_since).toLocaleTimeString()}</Row>
                  )}
                  {status.working_dir && (
                    <Row label="Working dir">
                      <span className="session-info-mono session-info-truncate" title={status.working_dir}>
                        {status.working_dir}
                      </span>
                    </Row>
                  )}
                  <Row label="Session ID">
                    <span className="session-info-mono session-info-truncate">{sessionId.slice(0, 12)}</span>
                    <button className="session-info-copy-btn" onClick={copySessionId} title="Copy full session ID">
                      {copied ? "Copied!" : "Copy"}
                    </button>
                  </Row>
                </div>
              ) : (
                <div className="session-info-loading">Loading...</div>
              )}
            </section>

            {/* Git worktree */}
            {status?.worktree_group && (
              <section className="session-info-section">
                <h3 className="session-info-heading">Git Worktree</h3>
                <div className="session-info-grid">
                  <Row label="Mode">{status.worktree_group.single_repo ? "Single repo" : "Multi-repo"}</Row>
                  {status.worktree_group.repos.map((repo, i) => (
                    <Row key={i} label={`Branch ${i + 1}`}>
                      <span className="session-info-mono">{repo.branch}</span>
                    </Row>
                  ))}
                  <Row label="Shadow root">
                    <span className="session-info-mono session-info-truncate" title={status.worktree_group.shadow_root}>
                      {status.worktree_group.shadow_root}
                    </span>
                  </Row>
                </div>
              </section>
            )}

            {/* Agent config */}
            {agent && (
              <section className="session-info-section">
                <h3 className="session-info-heading">Agent</h3>
                <div className="session-info-grid">
                  <Row label="Name">
                    {editingField === "name" ? (
                      <input ref={editInputRef} className="session-info-inline-input" value={editValue}
                        onChange={(e) => setEditValue(e.target.value)} onKeyDown={handleEditKeyDown} onBlur={cancelEdit} />
                    ) : (
                      <>{agent.name} <button className="session-info-edit-btn" onClick={() => startEdit("name")} title="Edit name">✎</button></>
                    )}
                  </Row>
                  <Row label="Working dir">
                    {editingField === "working_dir" ? (
                      <input ref={editInputRef} className="session-info-inline-input session-info-mono" value={editValue}
                        onChange={(e) => setEditValue(e.target.value)} onKeyDown={handleEditKeyDown} onBlur={cancelEdit} />
                    ) : (
                      <>
                        <span className="session-info-mono session-info-truncate">{agent.working_dir || "—"}</span>
                        <button className="session-info-edit-btn" onClick={() => startEdit("working_dir")} title="Edit working directory">✎</button>
                      </>
                    )}
                  </Row>
                  <Row label="Permissions">
                    {editingPerms ? (
                      <div className="session-info-perm-editor">
                        <div className="session-info-perm-toggles">
                          {KNOWN_PERMISSIONS.map((p) => (
                            <button key={p}
                              className={`session-info-perm-toggle ${draftPerms.includes(p) ? "session-info-perm-toggle-on" : ""}`}
                              onClick={() => togglePerm(p)}>{p}</button>
                          ))}
                        </div>
                        <div className="session-info-perm-actions">
                          <button className="session-info-btn" onClick={savePerms}>Save</button>
                          <button className="session-info-btn" onClick={() => setEditingPerms(false)}>Cancel</button>
                        </div>
                        <span className="session-info-muted">Changes take effect on next session</span>
                      </div>
                    ) : (
                      <>
                        {agent.permissions.length > 0 ? (
                          <div className="session-info-tags">
                            {agent.permissions.map((p) => (
                              <span key={p} className="session-info-tag">{p}</span>
                            ))}
                          </div>
                        ) : (
                          <span className="session-info-muted">None (default-deny)</span>
                        )}
                        <button className="session-info-edit-btn" onClick={startPermEdit} title="Edit permissions">✎</button>
                      </>
                    )}
                  </Row>
                </div>
              </section>
            )}

            {/* Actions */}
            <section className="session-info-section">
              <h3 className="session-info-heading">Actions</h3>
              <div className="session-info-actions">
                <button className="session-info-btn session-info-btn-warning" onClick={handleKill}
                  disabled={killing || !status?.process_alive}
                  title={status?.process_alive ? "Force-kill the Claude process" : "No active process"}>
                  {killing ? "Killing..." : "Kill Process"}
                </button>
                {!confirmDelete ? (
                  <button className="session-info-btn session-info-btn-danger"
                    onClick={() => setConfirmDelete(true)} disabled={deleting}>
                    Delete Session
                  </button>
                ) : (
                  <div className="session-info-confirm">
                    <span className="session-info-confirm-text">Delete this session?</span>
                    <button className="session-info-btn session-info-btn-danger"
                      onClick={handleDelete} disabled={deleting}>
                      {deleting ? "Deleting..." : "Confirm"}
                    </button>
                    <button className="session-info-btn" onClick={() => setConfirmDelete(false)}>Cancel</button>
                  </div>
                )}
              </div>
            </section>

            {error && <div className="session-info-error">{error}</div>}
          </div>
        )}

        {/* Agent prompt (read-only) */}
        {selected === "prompts/agent" && agent?.prompt && (
          <>
            <div className="fv-content-header">
              <span className="fv-content-path">Agent Prompt</span>
            </div>
            <div className="fv-content-body">
              <div className="fv-markdown">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{agent.prompt}</ReactMarkdown>
              </div>
            </div>
          </>
        )}

        {/* System prompt (read-only) */}
        {selected === "prompts/system" && agent?.append_system_prompt && (
          <>
            <div className="fv-content-header">
              <span className="fv-content-path">System Append Prompt</span>
            </div>
            <div className="fv-content-body">
              <div className="fv-markdown">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>{agent.append_system_prompt}</ReactMarkdown>
              </div>
            </div>
          </>
        )}

        {/* File-based views (skills, config) */}
        {isFileView && (
          <>
            <div className="fv-content-header">
              <span className="fv-content-path">{filePath}</span>
            </div>
            {fileLoading ? (
              <div className="fv-content-empty">Loading...</div>
            ) : fileContent !== null ? (
              <div className="fv-content-body">
                {isMarkdown ? (
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
                                <ShikiHighlighter language={match[1]} theme={theme}
                                  addDefaultStyles={false} showLanguage={false}>
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
                      language={langFromPath(filePath ?? "")}
                      theme={theme}
                      addDefaultStyles={false}
                      showLanguage={false}
                    >
                      {fileContent}
                    </ShikiHighlighter>
                  </div>
                )}
              </div>
            ) : (
              <div className="fv-content-empty">Select a file to view</div>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function langFromPath(path: string): string {
  const EXT_TO_LANG: Record<string, string> = {
    ts: "typescript", tsx: "tsx", js: "javascript", jsx: "jsx",
    rs: "rust", py: "python", go: "go", json: "json",
    yaml: "yaml", yml: "yaml", toml: "toml", md: "markdown",
    html: "html", css: "css", sh: "bash", bash: "bash",
  };
  const name = path.split("/").pop() ?? "";
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  return EXT_TO_LANG[ext] ?? "text";
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="session-info-row">
      <span className="session-info-label">{label}</span>
      <span className="session-info-value">{children}</span>
    </div>
  );
}

function StatusDot({ alive, busy }: { alive: boolean; busy: boolean }) {
  const color = busy ? "var(--warning)" : alive ? "var(--success)" : "var(--text-secondary)";
  return <span className="session-info-dot" style={{ background: color }} />;
}
