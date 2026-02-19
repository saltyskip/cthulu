"use client";

import { motion } from "framer-motion";

export default function GetStarted() {
  return (
    <section className="px-6 py-20">
      <div className="mx-auto max-w-3xl text-center">
        <motion.h2
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="text-3xl font-bold sm:text-4xl"
        >
          Sign Up for the Waitlist
        </motion.h2>

        <motion.div
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ delay: 0.2 }}
          className="mt-8 rounded-xl border border-border bg-bg-secondary p-6 text-left font-mono text-sm"
        >
          <div className="text-text-secondary">
            <span className="text-sink">$</span> git clone
            https://github.com/saltyskip/cthulu.git
          </div>
          <div className="mt-2 text-text-secondary">
            <span className="text-sink">$</span> cd cthulu && cargo build
            --release
          </div>
          <div className="mt-2 text-text-secondary">
            <span className="text-sink">$</span> cargo run --release
          </div>
        </motion.div>

        {/* Tech badges */}
        <motion.div
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          transition={{ delay: 0.3 }}
          className="mt-8 flex flex-wrap items-center justify-center gap-3"
        >
          {["Rust", "Axum", "Tokio", "React Flow", "Tauri"].map((tech) => (
            <span
              key={tech}
              className="rounded-full border border-border bg-bg-tertiary px-3 py-1 text-xs text-text-secondary"
            >
              {tech}
            </span>
          ))}
        </motion.div>

        <motion.div
          initial={{ opacity: 0, y: 10 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ delay: 0.4 }}
          className="mt-10"
        >
          <a
            href="#waitlist"
            className="inline-block rounded-lg bg-accent px-8 py-3 font-medium text-white transition-opacity hover:opacity-90"
          >
            Sign Up for the Waitlist
          </a>
        </motion.div>
      </div>
    </section>
  );
}
