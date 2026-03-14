"use client";

import { useState } from "react";
import { motion } from "framer-motion";
import Link from "next/link";

const WAITLIST_API_URL = ""; // Configure when ready

type FormStatus = "idle" | "submitting" | "success" | "error";

export default function GetStarted() {
  const [email, setEmail] = useState("");
  const [status, setStatus] = useState<FormStatus>("idle");

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!email.trim()) return;
    setStatus("submitting");
    try {
      if (WAITLIST_API_URL) {
        await fetch(WAITLIST_API_URL, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ email: email.trim() }),
        });
      }
      setStatus("success");
      setEmail("");
    } catch {
      setStatus("error");
    }
  };

  return (
    <section id="waitlist" className="px-6 py-20">
      <div className="mx-auto max-w-3xl text-center">
        {/* Waitlist Form */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          className="rounded-xl border border-border bg-bg-secondary p-8"
        >
          <h2 className="text-3xl font-bold sm:text-4xl">
            Get Early Access
          </h2>
          <p className="mt-3 text-text-secondary">
            Join the waitlist to be among the first to automate workflows with
            AI-powered DAG pipelines.
          </p>

          {status === "success" ? (
            <p className="mt-6 text-sm font-medium text-accent">
              You&apos;re on the list! We&apos;ll be in touch.
            </p>
          ) : (
            <form onSubmit={handleSubmit} className="mt-6 flex gap-3 sm:flex-row flex-col items-center justify-center">
              <input
                type="email"
                required
                placeholder="you@example.com"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                className="w-full max-w-sm rounded-lg border border-border bg-bg-tertiary px-4 py-3 text-sm text-text placeholder:text-text-secondary focus:border-accent focus:outline-none"
              />
              <button
                type="submit"
                disabled={status === "submitting"}
                className="shrink-0 rounded-lg bg-accent px-6 py-3 text-sm font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50"
              >
                {status === "submitting" ? "Joining..." : "Join Waitlist"}
              </button>
            </form>
          )}

          {status === "error" && (
            <p className="mt-3 text-sm text-red-400">
              Something went wrong. Please try again.
            </p>
          )}
        </motion.div>

        {/* Separator */}
        <div className="mt-12 mb-8 flex items-center gap-4">
          <div className="h-px flex-1 bg-border" />
          <span className="text-xs font-medium uppercase tracking-wider text-text-secondary">
            For Developers
          </span>
          <div className="h-px flex-1 bg-border" />
        </div>

        {/* Terminal / Install commands */}
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          whileInView={{ opacity: 1, y: 0 }}
          viewport={{ once: true }}
          transition={{ delay: 0.2 }}
          className="rounded-xl border border-border bg-bg-secondary p-6 text-left font-mono text-sm"
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
          <Link
            href="#waitlist"
            className="inline-block rounded-lg bg-accent px-8 py-3 font-medium text-primary-foreground transition-opacity hover:opacity-90"
          >
            Sign Up for the Waitlist
          </Link>
        </motion.div>
      </div>
    </section>
  );
}
