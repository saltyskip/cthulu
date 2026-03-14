import { useState, useEffect, useCallback, useMemo } from "react";
import * as api from "../api/client";
import type { Task, TaskStatus, AgentSummary } from "../types/flow";
import { NewTaskDialog } from "./NewTaskDialog";
import { Plus, Trash2, CheckCircle2, Circle, Loader2, XCircle } from "lucide-react";

const STATUS_LABELS: Record<TaskStatus, string> = {
  todo: "To Do",
  in_progress: "In Progress",
  done: "Done",
  cancelled: "Cancelled",
};

const STATUS_COLORS: Record<TaskStatus, string> = {
  todo: "var(--text-secondary)",
  in_progress: "var(--accent)",
  done: "#22c55e",
  cancelled: "#ef4444",
};

const STATUS_ICONS: Record<TaskStatus, typeof Circle> = {
  todo: Circle,
  in_progress: Loader2,
  done: CheckCircle2,
  cancelled: XCircle,
};

type FilterTab = "all" | TaskStatus;

function relativeTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

interface TaskListProps {
  agentId: string;
  agents: AgentSummary[];
}

export function TaskList({ agentId, agents }: TaskListProps) {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [filter, setFilter] = useState<FilterTab>("all");
  const [showNewDialog, setShowNewDialog] = useState(false);
  const [loading, setLoading] = useState(true);

  const loadTasks = useCallback(() => {
    setLoading(true);
    api.listTasks(agentId)
      .then(setTasks)
      .catch((e) => {
        console.error("Failed to load tasks:", typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
      })
      .finally(() => setLoading(false));
  }, [agentId]);

  useEffect(() => {
    loadTasks();
  }, [loadTasks]);

  const filteredTasks = useMemo(() => {
    if (filter === "all") return tasks;
    return tasks.filter((t) => t.status === filter);
  }, [tasks, filter]);

  const handleStatusChange = useCallback(async (taskId: string, newStatus: TaskStatus) => {
    try {
      await api.updateTask(taskId, { status: newStatus });
      loadTasks();
    } catch (e) {
      console.error("Failed to update task:", typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
    }
  }, [loadTasks]);

  const handleDelete = useCallback(async (taskId: string) => {
    try {
      await api.deleteTask(taskId);
      loadTasks();
    } catch (e) {
      console.error("Failed to delete task:", typeof e === "string" ? e : (e instanceof Error ? e.message : String(e)));
    }
  }, [loadTasks]);

  const filters: { id: FilterTab; label: string }[] = [
    { id: "all", label: "All" },
    { id: "todo", label: "To Do" },
    { id: "in_progress", label: "In Progress" },
    { id: "done", label: "Done" },
  ];

  return (
    <div className="task-list">
      <div className="task-list-header">
        <h3>Tasks</h3>
        <button className="primary" onClick={() => setShowNewDialog(true)}>
          <Plus size={14} style={{ marginRight: 4, verticalAlign: "middle" }} />
          New Task
        </button>
      </div>

      <div className="task-list-filters">
        {filters.map((f) => (
          <button
            key={f.id}
            className={`task-filter-btn${filter === f.id ? " active" : ""}`}
            onClick={() => setFilter(f.id)}
          >
            {f.label}
          </button>
        ))}
      </div>

      <div className="task-list-items">
        {loading ? (
          <div className="task-list-empty">
            <Loader2 size={20} style={{ animation: "spin 1s linear infinite" }} />
            Loading tasks...
          </div>
        ) : filteredTasks.length === 0 ? (
          <div className="task-list-empty">
            <Circle size={20} />
            No tasks found
          </div>
        ) : (
          filteredTasks.map((task) => {
            const StatusIcon = STATUS_ICONS[task.status];
            return (
              <div key={task.id} className="task-item">
                <StatusIcon
                  size={16}
                  style={{ color: STATUS_COLORS[task.status], flexShrink: 0 }}
                />
                <span className="task-item-title" title={task.title}>
                  {task.title}
                </span>
                <span
                  className="task-item-status"
                  style={{
                    color: STATUS_COLORS[task.status],
                    background: `color-mix(in srgb, ${STATUS_COLORS[task.status]} 12%, transparent)`,
                  }}
                >
                  {STATUS_LABELS[task.status]}
                </span>
                <span className="task-item-time">{relativeTime(task.created_at)}</span>
                <div className="task-item-actions">
                  <select
                    className="task-status-select"
                    value={task.status}
                    onChange={(e) => handleStatusChange(task.id, e.target.value as TaskStatus)}
                  >
                    <option value="todo">To Do</option>
                    <option value="in_progress">In Progress</option>
                    <option value="done">Done</option>
                    <option value="cancelled">Cancelled</option>
                  </select>
                  <button onClick={() => handleDelete(task.id)} title="Delete task">
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>

      {showNewDialog && (
        <NewTaskDialog
          defaultAgentId={agentId}
          agents={agents}
          onClose={() => setShowNewDialog(false)}
          onCreated={loadTasks}
        />
      )}
    </div>
  );
}
