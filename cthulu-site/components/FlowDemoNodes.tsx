"use client";

import { Handle, Position } from "@xyflow/react";

const nodeColors: Record<string, string> = {
  trigger: "#d29922",
  source: "#58a6ff",
  executor: "#bc8cff",
  sink: "#3fb950",
};

function DemoNode({
  data,
}: {
  data: { label: string; type: string; icon: string };
}) {
  const color = nodeColors[data.type] || "#8b949e";

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
          background: "#161b22",
          borderColor: "#30363d",
          minWidth: 160,
        }}
      >
        <div className="flex items-center gap-2">
          <span
            className="rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase"
            style={{ background: color + "22", color }}
          >
            {data.type}
          </span>
        </div>
        <div className="mt-1.5 flex items-center gap-2">
          <span className="text-base">{data.icon}</span>
          <span className="text-sm font-medium text-[#e6edf3]">
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

export const demoNodeTypes = {
  demo: DemoNode,
};
