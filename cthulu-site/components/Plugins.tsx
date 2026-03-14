"use client";

import { motion } from "framer-motion";

interface PluginNode {
  label: string;
  kind: string;
  type: "trigger" | "source" | "executor" | "sink";
  description: string;
}

const pluginCategories: {
  type: "trigger" | "source" | "executor" | "sink";
  label: string;
  tagline: string;
  nodes: PluginNode[];
}[] = [
  {
    type: "trigger",
    label: "Triggers",
    tagline: "Start pipelines from any event",
    nodes: [
      {
        label: "Cron",
        kind: "cron",
        type: "trigger",
        description: "Schedule on any cron expression — hourly, daily, weekly",
      },
      {
        label: "GitHub PR",
        kind: "github-pr",
        type: "trigger",
        description: "Fire on new or updated pull requests",
      },
      {
        label: "Webhook",
        kind: "webhook",
        type: "trigger",
        description: "Accept HTTP POST from any external system",
      },
      {
        label: "Manual",
        kind: "manual",
        type: "trigger",
        description: "One-click run from the Studio UI",
      },
    ],
  },
  {
    type: "source",
    label: "Sources",
    tagline: "Pull data from anywhere",
    nodes: [
      {
        label: "RSS Feed",
        kind: "rss",
        type: "source",
        description: "Parse and filter any Atom or RSS feed",
      },
      {
        label: "Web Scraper",
        kind: "web-scrape",
        type: "source",
        description: "Extract content with CSS selectors",
      },
      {
        label: "GitHub PRs",
        kind: "github-merged-prs",
        type: "source",
        description: "Fetch merged pull requests and diffs",
      },
      {
        label: "Market Data",
        kind: "market-data",
        type: "source",
        description: "CoinGecko, Fear & Greed, S&P 500",
      },
    ],
  },
  {
    type: "executor",
    label: "Executors",
    tagline: "Process with AI agents",
    nodes: [
      {
        label: "Claude Code",
        kind: "claude-code",
        type: "executor",
        description: "Full agent with filesystem, Bash, and tool access",
      },
      {
        label: "Claude API",
        kind: "claude-api",
        type: "executor",
        description: "Direct API calls for fast, structured responses",
      },
    ],
  },
  {
    type: "sink",
    label: "Sinks",
    tagline: "Deliver results everywhere",
    nodes: [
      {
        label: "Slack",
        kind: "slack",
        type: "sink",
        description: "Webhooks and Bot API with Block Kit threading",
      },
      {
        label: "Notion",
        kind: "notion",
        type: "sink",
        description: "Rich pages with images, tables, and callouts",
      },
    ],
  },
];

const colorMap: Record<string, string> = {
  trigger: "var(--trigger-color)",
  source: "var(--source-color)",
  executor: "var(--executor-color)",
  sink: "var(--sink-color)",
};

export default function Plugins() {
  return (
    <section id="plugins" className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-center text-3xl font-bold sm:text-4xl"
        >
          Plugin Ecosystem
        </motion.h2>
        <p className="mx-auto mt-4 max-w-2xl text-center text-text-secondary">
          Mix and match {pluginCategories.reduce((sum, c) => sum + c.nodes.length, 0)}+ built-in nodes
          to build any pipeline. Drag, connect, ship.
        </p>

        <div className="mt-14 space-y-12">
          {pluginCategories.map((cat, catIdx) => (
            <motion.div
              key={cat.type}
              initial={{ opacity: 0, y: 24 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.5, delay: catIdx * 0.1 }}
            >
              {/* Category header */}
              <div className="mb-5 flex items-center gap-3">
                <div
                  className="h-3 w-3 rounded-full"
                  style={{ background: colorMap[cat.type] }}
                />
                <h3 className="text-lg font-semibold text-text">
                  {cat.label}
                </h3>
                <span className="text-sm text-text-secondary">
                  — {cat.tagline}
                </span>
              </div>

              {/* Node cards */}
              <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
                {cat.nodes.map((node, nodeIdx) => (
                  <motion.div
                    key={node.kind}
                    initial={{ opacity: 0, scale: 0.95 }}
                    whileInView={{ opacity: 1, scale: 1 }}
                    whileHover={{ y: -2, transition: { duration: 0.2 } }}
                    viewport={{ once: true }}
                    transition={{ duration: 0.3, delay: nodeIdx * 0.06 }}
                    className="group relative rounded-xl border border-border bg-bg-secondary p-5 transition-colors hover:border-border/80"
                  >
                    {/* Glow accent on hover */}
                    <div
                      className="pointer-events-none absolute inset-0 rounded-xl opacity-0 transition-opacity group-hover:opacity-100"
                      style={{
                        background: `radial-gradient(ellipse at top left, color-mix(in srgb, ${colorMap[node.type]} 6%, transparent), transparent 70%)`,
                      }}
                    />

                    <div className="relative">
                      <div className="flex items-center justify-between">
                        <span className="text-sm font-semibold text-text">
                          {node.label}
                        </span>
                        <span
                          className="rounded-full px-2 py-0.5 text-[10px] font-medium uppercase tracking-wider"
                          style={{
                            color: colorMap[node.type],
                            background: `color-mix(in srgb, ${colorMap[node.type]} 10%, transparent)`,
                          }}
                        >
                          {node.type}
                        </span>
                      </div>
                      <p className="mt-2 text-xs leading-relaxed text-text-secondary">
                        {node.description}
                      </p>
                    </div>
                  </motion.div>
                ))}
              </div>
            </motion.div>
          ))}
        </div>

        {/* Extensibility callout */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.5 }}
          className="mt-14 rounded-xl border border-border bg-bg-secondary p-8 text-center"
        >
          <h3 className="text-lg font-semibold text-text">
            Build your own plugins
          </h3>
          <p className="mx-auto mt-2 max-w-lg text-sm text-text-secondary">
            Every node type follows the same interface. Add custom sources,
            executors, or sinks — register them in the pipeline and they appear
            in the Studio drag-and-drop palette.
          </p>
          <div className="mt-6 inline-flex items-center gap-4 rounded-lg border border-border bg-bg-tertiary px-5 py-3">
            <code className="text-xs text-text-secondary">
              <span style={{ color: "var(--executor-color)" }}>impl</span>{" "}
              <span style={{ color: "var(--accent)" }}>Source</span>{" "}
              <span style={{ color: "var(--executor-color)" }}>for</span>{" "}
              <span className="text-text">MyCustomSource</span>{" "}
              <span className="text-text-secondary">{"{ ... }"}</span>
            </code>
          </div>
        </motion.div>
      </div>
    </section>
  );
}
