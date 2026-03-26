import { useState, useEffect, useCallback } from "react";
import {
  listTaskTemplates,
  saveTaskTemplate,
  deleteTaskTemplate,
  runTask,
  getTaskHistory,
  extractTodos,
  type TaskTemplate,
  type TaskRun,
  type TaskRunResponse,
  type ExtractTodosResponse,
} from "../api/client";
import { Button } from "@/components/ui/button";

function getGreeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return "Good morning";
  if (hour < 17) return "Good afternoon";
  return "Good evening";
}

function formatDate(): string {
  return new Date().toLocaleDateString("en-US", {
    weekday: "long",
    year: "numeric",
    month: "long",
    day: "numeric",
  });
}

function formatTime(iso: string): string {
  try {
    return new Date(iso).toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit" });
  } catch {
    return "";
  }
}

const categoryColors: Record<string, string> = {
  shopping: "#10b981",
  research: "#6366f1",
  "data-entry": "#f59e0b",
};

export default function DashboardView() {
  const [tasks, setTasks] = useState<TaskTemplate[]>([]);
  const [history, setHistory] = useState<TaskRun[]>([]);
  const [loading, setLoading] = useState(false);
  const [runningTaskId, setRunningTaskId] = useState<string | null>(null);
  const [lastResult, setLastResult] = useState<TaskRunResponse | null>(null);
  const [customPrompt, setCustomPrompt] = useState("");

  // Repo todo extractor state
  const [repoInput, setRepoInput] = useState("");
  const [pathInput, setPathInput] = useState("");
  const [branchInput, setBranchInput] = useState("");
  const [todoResult, setTodoResult] = useState<ExtractTodosResponse | null>(null);
  const [todoLoading, setTodoLoading] = useState(false);

  const loadTasks = useCallback(async () => {
    try {
      const { tasks: t } = await listTaskTemplates();
      setTasks(t);
    } catch { /* logged */ }
  }, []);

  const loadHistory = useCallback(async () => {
    try {
      const { history: h } = await getTaskHistory();
      setHistory(h);
    } catch { /* logged */ }
  }, []);

  useEffect(() => {
    loadTasks();
    loadHistory();
  }, [loadTasks, loadHistory]);

  const handleRun = async (task: TaskTemplate) => {
    setLoading(true);
    setRunningTaskId(task.id);
    setLastResult(null);
    try {
      const result = await runTask(task.prompt, task.name);
      setLastResult(result);
      await loadHistory();
    } catch (e) {
      setLastResult({
        run_id: "",
        status: "failed",
        error: (e as Error).message,
        started_at: new Date().toISOString(),
        finished_at: new Date().toISOString(),
      });
    } finally {
      setLoading(false);
      setRunningTaskId(null);
    }
  };

  const handleRunCustom = async () => {
    if (!customPrompt.trim()) return;
    setLoading(true);
    setRunningTaskId("custom");
    setLastResult(null);
    try {
      const result = await runTask(customPrompt, "Custom Task");
      setLastResult(result);
      await loadHistory();
    } catch (e) {
      setLastResult({
        run_id: "",
        status: "failed",
        error: (e as Error).message,
        started_at: new Date().toISOString(),
        finished_at: new Date().toISOString(),
      });
    } finally {
      setLoading(false);
      setRunningTaskId(null);
    }
  };

  const handleDelete = async (id: string) => {
    if (!confirm("Delete this task template?")) return;
    await deleteTaskTemplate(id);
    await loadTasks();
  };

  const handleExtractTodos = async () => {
    if (!repoInput.trim() || !pathInput.trim()) return;
    setTodoLoading(true);
    setTodoResult(null);
    try {
      const result = await extractTodos(repoInput, pathInput, branchInput || undefined);
      setTodoResult(result);
      await loadHistory();
    } catch (e) {
      setTodoResult({
        run_id: "",
        status: "failed",
        files: [],
        error: (e as Error).message,
      });
    } finally {
      setTodoLoading(false);
    }
  };

  return (
    <div className="dashboard-view">
      <div className="dashboard-header">
        <h1 className="dashboard-greeting">{getGreeting()}</h1>
        <p className="dashboard-date">{formatDate()}</p>
      </div>

      <div className="dashboard-content">
        {/* Task Templates */}
        <div className="dashboard-section-header">
          <h2>Tasks</h2>
        </div>

        <div className="dashboard-channels">
          {tasks.map((task) => (
            <div key={task.id} className="dashboard-channel">
              <div className="dashboard-channel-header">
                <span className="dashboard-channel-name">
                  {task.category && (
                    <span
                      style={{
                        display: "inline-block",
                        width: 8,
                        height: 8,
                        borderRadius: "50%",
                        background: categoryColors[task.category] || "var(--muted)",
                        marginRight: 8,
                      }}
                    />
                  )}
                  {task.name}
                </span>
                <div style={{ display: "flex", gap: 6 }}>
                  <Button
                    size="sm"
                    onClick={() => handleRun(task)}
                    disabled={loading}
                  >
                    {runningTaskId === task.id ? "Running..." : "Run"}
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => handleDelete(task.id)}
                  >
                    x
                  </Button>
                </div>
              </div>
              <p style={{ margin: "4px 0 0", opacity: 0.7, fontSize: 13 }}>
                {task.description}
              </p>
            </div>
          ))}

          {tasks.length === 0 && (
            <div className="dashboard-empty">No task templates yet.</div>
          )}
        </div>

        {/* Custom task input */}
        <div style={{ marginTop: 16 }}>
          <div className="dashboard-section-header">
            <h2>Run Custom Task</h2>
          </div>
          <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
            <textarea
              value={customPrompt}
              onChange={(e) => setCustomPrompt(e.target.value)}
              placeholder="Describe a task... e.g. 'Find the cheapest flight from SFO to JFK next Friday'"
              style={{
                flex: 1,
                minHeight: 60,
                padding: 8,
                borderRadius: 6,
                border: "1px solid var(--border)",
                background: "var(--bg)",
                color: "var(--fg)",
                fontFamily: "inherit",
                fontSize: 13,
                resize: "vertical",
              }}
            />
            <Button
              onClick={handleRunCustom}
              disabled={loading || !customPrompt.trim()}
              style={{ alignSelf: "flex-end" }}
            >
              {runningTaskId === "custom" ? "Running..." : "Run"}
            </Button>
          </div>
        </div>

        {/* Repo Todo Extractor */}
        <div style={{ marginTop: 24 }}>
          <div className="dashboard-section-header">
            <h2>Extract Todos from Repo</h2>
          </div>
          <p style={{ fontSize: 13, opacity: 0.7, margin: "4px 0 8px" }}>
            Point to a GitHub repo path with markdown files — we'll pull them and create a todo list.
          </p>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            <input
              value={repoInput}
              onChange={(e) => setRepoInput(e.target.value)}
              placeholder="owner/repo (e.g. bitcoin-portal/web-monorepo)"
              style={{
                flex: "2 1 200px",
                padding: 8,
                borderRadius: 6,
                border: "1px solid var(--border)",
                background: "var(--bg)",
                color: "var(--fg)",
                fontSize: 13,
              }}
            />
            <input
              value={pathInput}
              onChange={(e) => setPathInput(e.target.value)}
              placeholder="path (e.g. docs/daily)"
              style={{
                flex: "1 1 150px",
                padding: 8,
                borderRadius: 6,
                border: "1px solid var(--border)",
                background: "var(--bg)",
                color: "var(--fg)",
                fontSize: 13,
              }}
            />
            <input
              value={branchInput}
              onChange={(e) => setBranchInput(e.target.value)}
              placeholder="branch (default: main)"
              style={{
                flex: "1 1 100px",
                padding: 8,
                borderRadius: 6,
                border: "1px solid var(--border)",
                background: "var(--bg)",
                color: "var(--fg)",
                fontSize: 13,
              }}
            />
            <Button
              onClick={handleExtractTodos}
              disabled={todoLoading || !repoInput.trim() || !pathInput.trim()}
            >
              {todoLoading ? "Extracting..." : "Extract Todos"}
            </Button>
          </div>

          {todoResult && (
            <div style={{ marginTop: 12 }}>
              <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: 8 }}>
                <span
                  style={{
                    fontSize: 12,
                    padding: "2px 8px",
                    borderRadius: 4,
                    background: todoResult.status === "completed" ? "var(--success)" : "var(--error)",
                    color: "#fff",
                  }}
                >
                  {todoResult.status}
                </span>
                {todoResult.files.length > 0 && (
                  <span style={{ fontSize: 12, opacity: 0.6 }}>
                    {todoResult.files.length} file{todoResult.files.length !== 1 ? "s" : ""}: {todoResult.files.join(", ")}
                  </span>
                )}
              </div>
              <div
                className="dashboard-channel"
                style={{ whiteSpace: "pre-wrap", fontFamily: "var(--font-mono, monospace)", fontSize: 13 }}
              >
                {todoResult.todos || todoResult.error || "No output"}
              </div>
            </div>
          )}
        </div>

        {/* Last Result */}
        {lastResult && (
          <div style={{ marginTop: 16 }}>
            <div className="dashboard-section-header">
              <h2>Result</h2>
              <span
                style={{
                  fontSize: 12,
                  padding: "2px 8px",
                  borderRadius: 4,
                  background: lastResult.status === "completed" ? "var(--success)" : "var(--error)",
                  color: "#fff",
                }}
              >
                {lastResult.status}
              </span>
            </div>
            <div
              className="dashboard-channel"
              style={{ marginTop: 8, whiteSpace: "pre-wrap", fontFamily: "var(--font-mono, monospace)", fontSize: 13 }}
            >
              {lastResult.result || lastResult.error || "No output"}
            </div>
          </div>
        )}

        {/* History */}
        {history.length > 0 && (
          <div style={{ marginTop: 24 }}>
            <div className="dashboard-section-header">
              <h2>Recent Runs</h2>
            </div>
            <div className="dashboard-channels">
              {history.slice(0, 10).map((run) => (
                <div key={run.id} className="dashboard-channel" style={{ padding: "8px 12px" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <span style={{ fontWeight: 500 }}>{run.task_name || "Task"}</span>
                    <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                      <span
                        style={{
                          fontSize: 11,
                          padding: "1px 6px",
                          borderRadius: 3,
                          background: run.status === "completed" ? "var(--success)" : run.status === "running" ? "var(--accent)" : "var(--error)",
                          color: "#fff",
                        }}
                      >
                        {run.status}
                      </span>
                      <span style={{ fontSize: 12, opacity: 0.6 }}>{formatTime(run.started_at)}</span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
