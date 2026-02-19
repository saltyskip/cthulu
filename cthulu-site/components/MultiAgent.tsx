"use client";

import { motion } from "framer-motion";

const terminalOutputs = [
  {
    title: "claude — pr-review",
    lines: [
      { text: "$ claude --allowedTools Read,Grep,Bash", color: "#3fb950" },
      { text: "Cloning bitcoin-portal/RustServer...", color: "#8b949e" },
      { text: "Reading src/handlers/auth.rs...", color: "#8b949e" },
      { text: "⠋ Analyzing PR #142 diff...", color: "#58a6ff" },
      { text: "Found 3 issues in auth middleware", color: "#d29922" },
      { text: "Posting inline comment on L47...", color: "#8b949e" },
      { text: "Posting inline comment on L93...", color: "#8b949e" },
      { text: "gh pr review --request-changes", color: "#3fb950" },
    ],
    x: 0,
    y: 0,
    delay: 0,
  },
  {
    title: "claude — newsletter",
    lines: [
      { text: "$ claude --allowedTools Read,Bash", color: "#3fb950" },
      { text: "Fetching thedefiant.io/feed...", color: "#8b949e" },
      { text: "Fetching blockworks.co/feed...", color: "#8b949e" },
      { text: "⠋ 23 items fetched, filtering...", color: "#58a6ff" },
      { text: "Generating newsletter draft...", color: "#8b949e" },
      { text: "Adding market data table...", color: "#8b949e" },
      { text: "Publishing to Notion...", color: "#bc8cff" },
      { text: "✓ Newsletter published", color: "#3fb950" },
    ],
    x: 52,
    y: 0,
    delay: 2,
  },
  {
    title: "claude — changelog",
    lines: [
      { text: "$ claude --allowedTools Read,Grep", color: "#3fb950" },
      { text: "Querying merged PRs (7 days)...", color: "#8b949e" },
      { text: "Found 12 PRs across 4 repos", color: "#8b949e" },
      { text: "⠋ Categorizing changes...", color: "#58a6ff" },
      { text: "3 features, 7 fixes, 2 chores", color: "#d29922" },
      { text: "Formatting Slack blocks...", color: "#8b949e" },
      { text: "Posted to #the-ark", color: "#bc8cff" },
      { text: "✓ Changelog delivered", color: "#3fb950" },
    ],
    x: 0,
    y: 52,
    delay: 4,
  },
  {
    title: "you@macbook — zsh",
    lines: [
      { text: "$ gh pr review 142 --approve", color: "#3fb950" },
      { text: "✓ Approved  # didn't read it", color: "#f85149" },
      { text: "$ gh pr review 143 --approve", color: "#3fb950" },
      { text: "✓ Approved  # mass deletes prod db?", color: "#f85149" },
      { text: "$ gh pr review 144 --approve", color: "#3fb950" },
      { text: "✓ Approved  # rm -rf / looks fine", color: "#f85149" },
      { text: "$ gh pr review 145 --approve", color: "#3fb950" },
      { text: "✓ Approved  # who needs tests anyway", color: "#f85149" },
    ],
    x: 52,
    y: 52,
    delay: 1,
  },
];

function AnimatedTerminal({
  title,
  lines,
  delay,
}: {
  title: string;
  lines: { text: string; color: string }[];
  delay: number;
}) {
  return (
    <div className="flex h-full flex-col overflow-hidden rounded border border-border bg-[#0d1117]">
      <div className="flex items-center gap-1.5 border-b border-border px-2.5 py-1.5 shrink-0">
        <div className="flex gap-1">
          <div className="h-2 w-2 rounded-full bg-[#f85149]" />
          <div className="h-2 w-2 rounded-full bg-[#d29922]" />
          <div className="h-2 w-2 rounded-full bg-[#3fb950]" />
        </div>
        <span className="ml-1 truncate text-[9px] text-text-secondary font-mono">{title}</span>
      </div>
      <div className="flex-1 overflow-hidden px-2.5 py-2 font-mono">
        {lines.map((line, i) => (
          <div
            key={i}
            className="terminal-line text-[10px] leading-relaxed whitespace-nowrap"
            style={{
              color: line.color,
              animationDelay: `${delay + i * 0.6}s`,
            }}
          >
            {line.text}
          </div>
        ))}
      </div>
    </div>
  );
}

function TerminalChaos() {
  return (
    <div className="relative h-72 overflow-hidden rounded-lg border border-border bg-bg-tertiary p-1.5">
      <div className="grid h-full grid-cols-2 grid-rows-2 gap-1.5">
        {terminalOutputs.map((t, i) => (
          <AnimatedTerminal
            key={i}
            title={t.title}
            lines={t.lines}
            delay={t.delay}
          />
        ))}
      </div>
      {/* Overlay badge */}
      <div className="absolute inset-0 flex items-end justify-center pb-3 pointer-events-none">
        <span className="rounded-full bg-[#0d1117ee] border border-[#f8514944] px-3 py-1 text-xs text-[#f85149]">
          You, pretending you know what&apos;s happening
        </span>
      </div>
    </div>
  );
}

function OrchestrationView() {
  const flows = [
    {
      name: "PR Review Bot",
      status: "running",
      trigger: "GitHub PR",
      lastRun: "12s ago",
      color: "#3fb950",
    },
    {
      name: "Market Brief",
      status: "scheduled",
      trigger: "Every 4h",
      lastRun: "2h ago",
      color: "#58a6ff",
    },
    {
      name: "Dev Changelog",
      status: "idle",
      trigger: "Mondays 9am",
      lastRun: "5d ago",
      color: "#8b949e",
    },
    {
      name: "Newsletter",
      status: "running",
      trigger: "Daily 8am",
      lastRun: "3m ago",
      color: "#3fb950",
    },
  ];

  return (
    <div className="h-72 overflow-hidden rounded-lg border border-border bg-bg-secondary">
      {/* Header bar */}
      <div className="flex items-center justify-between border-b border-border px-4 py-2">
        <span className="text-xs font-medium text-text">Cthulu Dashboard</span>
        <div className="flex items-center gap-2">
          <span className="h-1.5 w-1.5 rounded-full bg-[#3fb950]" />
          <span className="text-[10px] text-text-secondary">4 flows active</span>
        </div>
      </div>
      {/* Flow rows */}
      <div className="divide-y divide-border">
        {flows.map((f) => (
          <div key={f.name} className="flex items-center justify-between px-4 py-3.5">
            <div className="flex items-center gap-3">
              <span
                className="h-2 w-2 rounded-full"
                style={{ backgroundColor: f.color }}
              />
              <div>
                <div className="text-xs font-medium text-text">{f.name}</div>
                <div className="text-[10px] text-text-secondary">{f.trigger}</div>
              </div>
            </div>
            <div className="text-right">
              <div
                className="text-[10px] font-medium"
                style={{ color: f.color }}
              >
                {f.status}
              </div>
              <div className="text-[10px] text-text-secondary">{f.lastRun}</div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function MultiAgent() {
  return (
    <section className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-center text-3xl font-bold sm:text-4xl"
        >
          Multi-agent, minus the chaos
        </motion.h2>
        <p className="mx-auto mt-4 max-w-2xl text-center text-text-secondary">
          Running agents today means opening terminal windows and hoping for the
          best. Cthulu gives you a single control plane for all of them.
        </p>

        <div className="mt-12 grid gap-6 lg:grid-cols-2">
          <motion.div
            initial={{ opacity: 0, x: -20 }}
            whileInView={{ opacity: 1, x: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.5 }}
          >
            <div className="mb-3 flex items-center gap-2">
              <span className="rounded bg-[#f8514922] px-2 py-0.5 text-xs font-medium text-[#f85149]">
                Before
              </span>
              <span className="text-sm text-text-secondary">
                The &ldquo;multi-agent&rdquo; experience today
              </span>
            </div>
            <TerminalChaos />
          </motion.div>

          <motion.div
            initial={{ opacity: 0, x: 20 }}
            whileInView={{ opacity: 1, x: 0 }}
            viewport={{ once: true }}
            transition={{ duration: 0.5 }}
          >
            <div className="mb-3 flex items-center gap-2">
              <span className="rounded bg-[#3fb95022] px-2 py-0.5 text-xs font-medium text-[#3fb950]">
                With Cthulu
              </span>
              <span className="text-sm text-text-secondary">
                One server. All your agents. Full visibility.
              </span>
            </div>
            <OrchestrationView />
          </motion.div>
        </div>
      </div>
    </section>
  );
}
