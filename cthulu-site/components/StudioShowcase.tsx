"use client";

import { motion } from "framer-motion";

const nodeTypes = [
  { label: "Cron Trigger", color: "#d29922", type: "trigger" },
  { label: "RSS Source", color: "#58a6ff", type: "source" },
  { label: "Web Scraper", color: "#58a6ff", type: "source" },
  { label: "Keyword Filter", color: "#8b949e", type: "filter" },
  { label: "Agent", color: "#bc8cff", type: "executor" },
  { label: "Slack Sink", color: "#3fb950", type: "sink" },
  { label: "Notion Sink", color: "#3fb950", type: "sink" },
];

function MockNode({
  label,
  color,
  type,
  x,
  y,
}: {
  label: string;
  color: string;
  type: string;
  x: number;
  y: number;
}) {
  return (
    <div
      className="absolute rounded-lg border border-border bg-bg-tertiary px-3 py-2"
      style={{ left: x, top: y, minWidth: 120 }}
    >
      <span
        className="rounded px-1.5 py-0.5 text-[9px] font-semibold uppercase"
        style={{ background: color + "22", color }}
      >
        {type}
      </span>
      <div className="mt-1 text-xs font-medium text-text">{label}</div>
    </div>
  );
}

export default function StudioShowcase() {
  return (
    <section id="studio" className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-center text-3xl font-bold sm:text-4xl"
        >
          Cthulu Studio
        </motion.h2>
        <p className="mx-auto mt-4 max-w-2xl text-center text-text-secondary">
          A visual pipeline builder for designing, monitoring, and triggering
          your automations.
        </p>

        <motion.div
          initial={{ opacity: 0, y: 30 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.6 }}
          className="relative mt-12 overflow-hidden rounded-xl border border-border bg-bg-secondary"
        >
          {/* Title bar */}
          <div className="flex items-center gap-2 border-b border-border px-4 py-3">
            <div className="flex gap-1.5">
              <div className="h-3 w-3 rounded-full bg-[#f85149]" />
              <div className="h-3 w-3 rounded-full bg-[#d29922]" />
              <div className="h-3 w-3 rounded-full bg-[#3fb950]" />
            </div>
            <span className="ml-4 text-xs text-text-secondary">
              Cthulu Studio - crypto-news-brief
            </span>
          </div>

          <div className="flex" style={{ height: 400 }}>
            {/* Sidebar */}
            <div className="w-48 shrink-0 border-r border-border p-3">
              <div className="text-[10px] font-semibold uppercase text-text-secondary">
                Node Types
              </div>
              <div className="mt-2 space-y-1.5">
                {nodeTypes.map((n) => (
                  <div
                    key={n.label}
                    className="flex items-center gap-2 rounded border border-border bg-bg-tertiary px-2 py-1.5 text-xs text-text"
                  >
                    <div
                      className="h-2 w-2 rounded-full"
                      style={{ background: n.color }}
                    />
                    {n.label}
                  </div>
                ))}
              </div>
            </div>

            {/* Canvas area */}
            <div className="relative flex-1 overflow-hidden">
              {/* Grid background */}
              <svg className="absolute inset-0 h-full w-full">
                <defs>
                  <pattern
                    id="grid"
                    width="20"
                    height="20"
                    patternUnits="userSpaceOnUse"
                  >
                    <circle cx="1" cy="1" r="0.5" fill="#21262d" />
                  </pattern>
                </defs>
                <rect width="100%" height="100%" fill="url(#grid)" />
                {/* Connection lines */}
                <line x1="155" y1="45" x2="220" y2="70" stroke="#d2992255" strokeWidth="1.5" />
                <line x1="155" y1="45" x2="220" y2="150" stroke="#d2992255" strokeWidth="1.5" />
                <line x1="340" y1="85" x2="400" y2="120" stroke="#58a6ff55" strokeWidth="1.5" />
                <line x1="340" y1="165" x2="400" y2="120" stroke="#58a6ff55" strokeWidth="1.5" />
                <line x1="540" y1="135" x2="600" y2="70" stroke="#bc8cff55" strokeWidth="1.5" />
                <line x1="540" y1="135" x2="600" y2="190" stroke="#bc8cff55" strokeWidth="1.5" />
              </svg>

              <MockNode label="Every 4h" color="#d29922" type="trigger" x={30} y={25} />
              <MockNode label="RSS Feeds" color="#58a6ff" type="source" x={220} y={50} />
              <MockNode label="Web Scraper" color="#58a6ff" type="source" x={220} y={130} />
              <MockNode label="Agent" color="#bc8cff" type="executor" x={410} y={95} />
              <MockNode label="Slack" color="#3fb950" type="sink" x={600} y={50} />
              <MockNode label="Notion" color="#3fb950" type="sink" x={600} y={170} />

              {/* Minimap */}
              <div className="absolute right-3 bottom-3 h-16 w-24 rounded border border-border bg-bg/80">
                <div className="p-1 text-[7px] text-text-secondary">minimap</div>
              </div>
            </div>

            {/* Property panel */}
            <div className="w-56 shrink-0 border-l border-border p-3">
              <div className="text-[10px] font-semibold uppercase text-text-secondary">
                Properties
              </div>
              <div className="mt-3 space-y-3">
                <div>
                  <div className="text-[10px] text-text-secondary">Type</div>
                  <div className="mt-0.5 rounded border border-border bg-bg-tertiary px-2 py-1 text-xs text-executor">
                    sandboxed-agent
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-text-secondary">Prompt</div>
                  <div className="mt-0.5 rounded border border-border bg-bg-tertiary px-2 py-1 text-xs text-text-secondary">
                    prompts/brief.md
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-text-secondary">Allowed Tools</div>
                  <div className="mt-0.5 rounded border border-border bg-bg-tertiary px-2 py-1 text-xs text-text-secondary">
                    Read, Grep, Bash
                  </div>
                </div>
              </div>

              <div className="mt-6 text-[10px] font-semibold uppercase text-text-secondary">
                Run History
              </div>
              <div className="mt-2 space-y-1">
                <div className="flex items-center gap-2 text-[10px]">
                  <span className="h-1.5 w-1.5 rounded-full bg-sink" />
                  <span className="text-text-secondary">2 min ago</span>
                  <span className="text-sink">Success</span>
                </div>
                <div className="flex items-center gap-2 text-[10px]">
                  <span className="h-1.5 w-1.5 rounded-full bg-sink" />
                  <span className="text-text-secondary">4h ago</span>
                  <span className="text-sink">Success</span>
                </div>
                <div className="flex items-center gap-2 text-[10px]">
                  <span className="h-1.5 w-1.5 rounded-full bg-[#f85149]" />
                  <span className="text-text-secondary">8h ago</span>
                  <span className="text-[#f85149]">Failed</span>
                </div>
              </div>
            </div>
          </div>
        </motion.div>

        {/* Feature callouts */}
        <div className="mt-8 grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          {[
            { label: "Drag & Drop", desc: "Visual node-based editor" },
            { label: "Property Panel", desc: "Configure nodes inline" },
            { label: "Run History", desc: "Per-flow execution logs" },
            { label: "Manual Trigger", desc: "Test pipelines on demand" },
          ].map((item) => (
            <div
              key={item.label}
              className="rounded-lg border border-border bg-bg-secondary px-4 py-3 text-center"
            >
              <div className="text-sm font-medium text-text">{item.label}</div>
              <div className="mt-1 text-xs text-text-secondary">
                {item.desc}
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
