---
name: react-flow
description: Use when working on Cthulu Studio's visual flow editor -- React Flow canvas, custom node types, edge connections, and drag interactions.
---

# React Flow Patterns (Cthulu Studio)

## When to Apply

- Adding or modifying node types in `cthulu-studio/src/components/NodeTypes/`
- Working on the Canvas component
- Modifying node/edge state management
- Implementing drag-to-resize or interactive UI elements

## Critical Rule: Never Replace Nodes Wholesale

```tsx
// WRONG -- destroys React Flow internals (measured, handleBounds)
setNodes(newArray);

// CORRECT -- spread-merge preserves internal state
setNodes((prev) =>
  prev.map((n) =>
    n.id === targetId ? { ...n, data: { ...n.data, ...updates } } : n
  )
);
```

This applies to any operation that creates new node objects: filtering, mapping, or assigning a fresh array.

## Custom Node Types

Five node types registered in `Canvas.tsx`:

| Type | Component | Purpose |
|------|-----------|---------|
| `trigger` | `TriggerNode` | Cron, GitHub PR, manual, webhook |
| `source` | `SourceNode` | RSS, web-scrape, GitHub PRs, market data |
| `filter` | `FilterNode` | Keyword matching |
| `executor` | `ExecutorNode` | Claude Code / Claude API |
| `sink` | `SinkNode` | Slack, Notion |

## Adding Nodes

Use `addNodeAtScreen()` in `Canvas.tsx`. Executor nodes auto-name as `Executor - E01`, `E02`, etc.:

```tsx
const existingCount = nodes.filter((n) => n.type === "executor").length;
const label = `Executor - E${String(existingCount + 1).padStart(2, "0")}`;
```

## State Management

- Use `useMemo` for computed values (e.g., `executorNodes` derived from `nodes`)
- Use callback form of `setNodes` to access current state
- Avoid `useEffect` for derived state synchronization

## Drag-to-Resize Pattern

Used in BottomPanel and NodeChat input:

```tsx
const dragRef = useRef<{ startY: number; startH: number } | null>(null);

const handleDragStart = (e: React.MouseEvent) => {
  e.preventDefault();
  dragRef.current = { startY: e.clientY, startH: currentHeight };
  const onMove = (ev: MouseEvent) => {
    if (!dragRef.current) return;
    const delta = dragRef.current.startY - ev.clientY;
    setHeight(Math.min(MAX, Math.max(MIN, dragRef.current.startH + delta)));
  };
  const onUp = () => {
    dragRef.current = null;
    document.removeEventListener("mousemove", onMove);
    document.removeEventListener("mouseup", onUp);
    document.body.style.cursor = "";
  };
  document.addEventListener("mousemove", onMove);
  document.addEventListener("mouseup", onUp);
  document.body.style.cursor = "ns-resize";
};
```

## CSS Theming

Components use CSS custom properties (`var(--bg)`, `var(--border)`, `var(--accent)`, `var(--text)`, `var(--text-secondary)`, `var(--bg-secondary)`). Never hardcode colors.
