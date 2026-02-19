"use client";

import { motion } from "framer-motion";

const features = [
  {
    title: "Automated PR Reviews",
    description:
      "Polls GitHub repos, detects new PRs, runs Claude Code reviews with scoped permissions. Posts inline comments and approve/request-changes.",
    icon: "\ud83d\udd0d",
    color: "#58a6ff",
  },
  {
    title: "News Monitoring",
    description:
      "RSS feeds, web scrapers, CSS-selector scrapers. Fetch from any source, filter by keywords, process with AI.",
    icon: "\ud83d\udcf0",
    color: "#d29922",
  },
  {
    title: "Rich Delivery",
    description:
      "Slack (webhooks + Bot API with Block Kit threading) and Notion (database pages with images, callouts, memes, tables).",
    icon: "\ud83d\udce4",
    color: "#3fb950",
  },
  {
    title: "Prompt Templates",
    description:
      "Markdown prompt files with {{variable}} substitution. Battle-tested templates for PR reviews, changelogs, newsletters.",
    icon: "\ud83d\udcdd",
    color: "#bc8cff",
  },
  {
    title: "Scoped Permissions",
    description:
      "Per-task --allowedTools instead of --dangerously-skip-permissions. Each pipeline gets only the tools it needs.",
    icon: "\ud83d\udd12",
    color: "#f85149",
  },
  {
    title: "Market Data",
    description:
      "Built-in CoinGecko, Fear & Greed, S&P 500 integration. Auto-injected via {{market_data}} template variable.",
    icon: "\ud83d\udcc8",
    color: "#d29922",
  },
];

export default function Features() {
  return (
    <section id="features" className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-center text-3xl font-bold sm:text-4xl"
        >
          Features
        </motion.h2>
        <p className="mx-auto mt-4 max-w-2xl text-center text-text-secondary">
          Everything you need to build production-grade AI automation pipelines.
        </p>

        <div className="mt-12 grid gap-6 sm:grid-cols-2 lg:grid-cols-3">
          {features.map((f, i) => (
            <motion.div
              key={f.title}
              initial={{ opacity: 0, y: 20 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true }}
              transition={{ duration: 0.4, delay: i * 0.08 }}
              className="rounded-xl border border-border bg-bg-secondary p-6"
            >
              <div
                className="flex h-10 w-10 items-center justify-center rounded-lg text-xl"
                style={{ background: f.color + "18" }}
              >
                {f.icon}
              </div>
              <h3 className="mt-4 font-semibold text-text">{f.title}</h3>
              <p className="mt-2 text-sm leading-relaxed text-text-secondary">
                {f.description}
              </p>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
}
