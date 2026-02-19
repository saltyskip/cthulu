"use client";

import { motion } from "framer-motion";

const jsonConfig = `<span class="toml-comment">// pipeline.json — A complete agent pipeline</span>
{
  <span class="toml-key">"name"</span>: <span class="toml-string">"crypto-news-brief"</span>,
  <span class="toml-key">"trigger"</span>: {
    <span class="toml-key">"type"</span>: <span class="toml-string">"cron"</span>,
    <span class="toml-key">"schedule"</span>: <span class="toml-string">"0 */4 * * *"</span>
  },
  <span class="toml-key">"sources"</span>: [
    {
      <span class="toml-key">"type"</span>: <span class="toml-string">"rss"</span>,
      <span class="toml-key">"urls"</span>: [
        <span class="toml-string">"https://thedefiant.io/feed"</span>,
        <span class="toml-string">"https://blockworks.co/feed"</span>,
        <span class="toml-string">"https://www.dlnews.com/arc/rss"</span>
      ]
    },
    {
      <span class="toml-key">"type"</span>: <span class="toml-string">"web-scraper"</span>,
      <span class="toml-key">"url"</span>: <span class="toml-string">"https://cointelegraph.com"</span>,
      <span class="toml-key">"item_selector"</span>: <span class="toml-string">"article.post-card"</span>
    }
  ],
  <span class="toml-key">"filter"</span>: {
    <span class="toml-key">"keywords"</span>: [<span class="toml-string">"bitcoin"</span>, <span class="toml-string">"ethereum"</span>, <span class="toml-string">"regulation"</span>],
    <span class="toml-key">"match_on"</span>: <span class="toml-string">"title_or_summary"</span>
  },
  <span class="toml-key">"agent"</span>: {
    <span class="toml-key">"prompt_file"</span>: <span class="toml-string">"prompts/brief.md"</span>,
    <span class="toml-key">"allowed_tools"</span>: [<span class="toml-string">"Read"</span>, <span class="toml-string">"Grep"</span>, <span class="toml-string">"Bash(curl)"</span>]
  },
  <span class="toml-key">"sinks"</span>: [
    { <span class="toml-key">"type"</span>: <span class="toml-string">"slack"</span> },
    { <span class="toml-key">"type"</span>: <span class="toml-string">"notion"</span>, <span class="toml-key">"database_id"</span>: <span class="toml-string">"your-notion-db-id"</span> }
  ]
}`;

export default function ConfigExample() {
  return (
    <section className="px-6 py-20">
      <div className="mx-auto max-w-6xl">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-center text-3xl font-bold sm:text-4xl"
        >
          Define Pipelines in JSON
        </motion.h2>
        <p className="mx-auto mt-4 max-w-2xl text-center text-text-secondary">
          Version-controlled pipeline definitions you can PR review and deploy
          like infrastructure.
        </p>

        <motion.div
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ duration: 0.5 }}
          className="mt-10"
        >
          <div className="rounded-xl border border-border bg-bg-secondary overflow-hidden">
            {/* File tab */}
            <div className="flex items-center border-b border-border px-4 py-2">
              <span className="rounded bg-bg-tertiary px-2 py-0.5 text-xs text-text-secondary">
                pipeline.json
              </span>
            </div>
            <pre
              className="overflow-x-auto p-6 text-sm leading-relaxed"
              dangerouslySetInnerHTML={{ __html: jsonConfig }}
            />
          </div>
        </motion.div>
      </div>
    </section>
  );
}
