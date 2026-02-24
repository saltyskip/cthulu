# Cthulu Studio - AI Assistant Guide

Visual flow editor for Cthulu workflows. Built with React 19, TypeScript, Vite, React Flow (@xyflow/react 12.6), and Tauri for desktop distribution.

**Parent Documentation**: [Root CLAUDE.md](../CLAUDE.md)

---

## Critical Rules

1. **Never replace React Flow nodes wholesale** -- always spread-merge to preserve internal measurements. See root CLAUDE.md rule 1.
2. **No `useEffect` for derived state** -- use `useMemo` for computed values. See root CLAUDE.md rule 2.
3. **CSS variables for theming** -- use `var(--bg)`, `var(--border)`, `var(--accent)`, `var(--text)`, `var(--text-secondary)`, `var(--bg-secondary)`. Never hardcode colors.
4. **Monospace font stack** -- `"SF Mono", "Fira Code", "Cascadia Code", monospace` for code/chat areas.

---

## Architecture

```
src/
├── App.tsx                     # Root: manages flow state, bottom panel, layout
├── main.tsx                    # Entry point
├── styles.css                  # All styles (CSS variables, component styles)
├── api/
│   ├── client.ts               # REST API client (flows, sessions, scheduler)
│   ├── interactStream.ts       # SSE streaming for agent chat
│   ├── runStream.ts            # SSE streaming for flow runs
│   └── logger.ts               # Client-side logger
├── components/
│   ├── Canvas.tsx              # React Flow canvas + node type registry
│   ├── BottomPanel.tsx         # VS Code-like tabbed panel (Console, Log, Executors)
│   ├── NodeChat.tsx            # Per-node agent chat with SSE streaming
│   ├── PropertyPanel.tsx       # Node config editor + CronFields preview
│   ├── FlowList.tsx            # Sidebar flow list with status dots
│   ├── TopBar.tsx              # Flow name, enable/disable, run, next-run display
│   ├── Sidebar.tsx             # Left sidebar with Add Nodes section
│   ├── InteractPanel.tsx       # Flow-level agent chat (legacy, may be removed)
│   ├── Console.tsx             # Console output panel
│   ├── RunHistory.tsx          # Run history viewer
│   ├── RunLog.tsx              # Run log output
│   ├── PromptEditor.tsx        # Markdown prompt editor (@uiw/react-md-editor)
│   ├── PromptLibrary.tsx       # Prompt template library
│   ├── ErrorBoundary.tsx       # Error boundary wrapper
│   └── NodeTypes/
│       ├── TriggerNode.tsx     # Trigger node (cron, github-pr, manual, webhook)
│       ├── SourceNode.tsx      # Source node (rss, web-scrape, market-data, etc.)
│       ├── FilterNode.tsx      # Filter node (keyword matching)
│       ├── ExecutorNode.tsx    # Executor node (Claude Code / Claude API)
│       └── SinkNode.tsx        # Sink node (slack, notion)
├── types/
│   └── flow.ts                 # TypeScript type definitions (Flow, Node, Edge, etc.)
└── utils/
    └── validateNode.ts         # Node validation logic (required fields per type)
```

---

## Key Patterns

### Node State Management

```tsx
// CORRECT -- spread-merge preserves React Flow internals
setNodes((prev) =>
  prev.map((n) => n.id === id ? { ...n, data: { ...n.data, label: newLabel } } : n)
);

// WRONG -- destroys measured/handleBounds
setNodes(nodes.map((n) => ({ id: n.id, data: newData, position: n.position })));
```

### Executor Auto-Naming

New executors get sequential names: `Executor - E01`, `Executor - E02`, etc. in `Canvas.tsx`:

```tsx
const count = nodes.filter((n) => n.type === "executor").length;
const label = `Executor - E${String(count + 1).padStart(2, "0")}`;
```

### Bottom Panel (VS Code-like)

`BottomPanel.tsx` renders tabbed panel at the bottom with drag-to-resize:
- **Console** tab -- console output
- **Log** tab -- run log
- **Executor tabs** -- one per executor node, auto-opens when clicking node on canvas

Tabs are compact: 28px height, 11px font, gaps between tabs.

### SSE Streaming

Two SSE streams:
- `startNodeInteract()` in `interactStream.ts` -- per-node agent chat
- `startRunStream()` in `runStream.ts` -- flow run output

Both use `EventSource`-like pattern with `fetch` + `ReadableStream` for better control.

### Drag-to-Resize

Used on BottomPanel (panel height) and NodeChat input (textarea height). Pattern:
1. `onMouseDown` captures start position and current size
2. `mousemove` listener computes delta, clamps to min/max, sets state
3. `mouseup` cleans up listeners and resets cursor

### API Client Convention

All API functions in `client.ts` follow:
```typescript
export async function doSomething(flowId: string, ...args): Promise<ResponseType> {
  const res = await fetch(`${API}/flows/${flowId}/endpoint`, { method, body, headers });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}
```

Base URL: `http://localhost:8081/api` (configured at top of `client.ts`).

---

## Verification

Before marking frontend work as complete:

```bash
npx nx build cthulu-studio   # TypeScript + Vite build
```
