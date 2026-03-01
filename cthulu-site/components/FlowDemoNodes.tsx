"use client";

import { Handle, Position } from "@xyflow/react";

const nodeColors: Record<string, string> = {
  trigger: "var(--trigger-color)",
  source: "var(--source-color)",
  executor: "var(--executor-color)",
  sink: "var(--sink-color)",
};

function DemoNode({
  data,
}: {
  data: { label: string; type: string; icon: string };
}) {
  const color = nodeColors[data.type] || "var(--text-secondary)";

  return (
    <>
      {data.type !== "trigger" && (
        <Handle
          type="target"
          position={Position.Left}
          id="in"
          style={{ background: color, border: "none", width: 8, height: 8 }}
        />
      )}
      <div
        className="rounded-lg border px-4 py-3"
        style={{
          background: "var(--bg-secondary)",
          borderColor: "var(--border)",
          minWidth: 160,
        }}
      >
        <div className="flex items-center gap-2">
          <span
            className="rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase"
            style={{ background: `color-mix(in srgb, ${color} 13%, transparent)`, color }}
          >
            {data.type}
          </span>
        </div>
        <div className="mt-1.5 flex items-center gap-2">
          <span className="text-base">{data.icon}</span>
          <span className="text-sm font-medium" style={{ color: "var(--text)" }}>
            {data.label}
          </span>
        </div>
      </div>
      {data.type !== "sink" && (
        <Handle
          type="source"
          position={Position.Right}
          id="out"
          style={{ background: color, border: "none", width: 8, height: 8 }}
        />
      )}
    </>
  );
}

function DemoNodeVertical({
  data,
}: {
  data: { label: string; type: string; icon: string };
}) {
  const color = nodeColors[data.type] || "var(--text-secondary)";

  return (
    <>
      {data.type !== "trigger" && (
        <Handle
          type="target"
          position={Position.Top}
          id="in-top"
          style={{ background: color, border: "none", width: 8, height: 8 }}
        />
      )}
      <div
        className="rounded-lg border px-3 py-2"
        style={{
          background: "var(--bg-secondary)",
          borderColor: "var(--border)",
          width: 140,
        }}
      >
        <div className="flex items-center gap-2">
          <span
            className="rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase"
            style={{ background: `color-mix(in srgb, ${color} 13%, transparent)`, color }}
          >
            {data.type}
          </span>
        </div>
        <div className="mt-1 flex items-center gap-2">
          <span className="text-sm">{data.icon}</span>
          <span className="text-xs font-medium" style={{ color: "var(--text)" }}>
            {data.label}
          </span>
        </div>
      </div>
      {data.type !== "sink" && (
        <Handle
          type="source"
          position={Position.Bottom}
          id="out-bottom"
          style={{ background: color, border: "none", width: 8, height: 8 }}
        />
      )}
    </>
  );
}

export const demoNodeTypes = {
  demo: DemoNode,
  demoVertical: DemoNodeVertical,
};
