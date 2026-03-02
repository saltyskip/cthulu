import { useState, memo } from "react";
import type { TodoItem } from "../ToolRenderers";

const StickyTodoPanel = memo(function StickyTodoPanel({ todos }: { todos: TodoItem[] }) {
  const [collapsed, setCollapsed] = useState(false);
  const completed = todos.filter((t) => t.status === "completed").length;
  const total = todos.length;
  const pct = total > 0 ? Math.round((completed / total) * 100) : 0;

  return (
    <div className="fr-sticky-todo">
      <div className="fr-sticky-todo-header" onClick={() => setCollapsed((v) => !v)}>
        <span className="fr-sticky-todo-caret">{collapsed ? "▸" : "▾"}</span>
        <span className="fr-sticky-todo-title">Tasks</span>
        <span className="fr-sticky-todo-progress">{completed}/{total}</span>
        <div className="fr-sticky-todo-bar">
          <div className="fr-sticky-todo-fill" style={{ width: `${pct}%` }} />
        </div>
      </div>
      {!collapsed && (
        <div className="fr-sticky-todo-list">
          {todos.map((t, i) => (
            <div key={i} className={`fr-todo-item fr-todo-${t.status.replace("_", "-")}`}>
              <span className="fr-todo-check">
                {t.status === "completed" ? "✓" : t.status === "in_progress" ? "●" : "○"}
              </span>
              <span className="fr-todo-text">
                {t.status === "in_progress" && t.activeForm ? t.activeForm : t.content}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
});

export default StickyTodoPanel;
