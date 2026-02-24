# AI Workflow Guide

How AI agents should work in this monorepo. The root [CLAUDE.md](../CLAUDE.md) covers **what the rules are**; this document covers **how to work**.

---

## Plan Before You Code

For any non-trivial task (more than a few lines, multiple files, or unclear scope):

1. **Explore first** -- read the files you'll change, understand existing patterns
2. **Write a plan** with specific files, changes, and verification steps
3. **Get approval** before implementing
4. **Re-plan on failure** -- if your approach doesn't work, stop and re-plan instead of brute-forcing

**Skip planning for**: typo fixes, single-line changes, tasks with very specific instructions.

---

## Subagent Strategy

Use subagents to parallelize work and protect context:

- **Offload research** -- use Explore agents for codebase investigation
- **One task per subagent** -- keep scopes narrow and focused
- **Run independent agents in parallel** -- don't serialize what can be parallelized
- **Don't duplicate work** -- if you delegate research to a subagent, don't also search yourself

---

## Self-Improvement Loop

After receiving a correction or discovering a mistake:

1. Fix the immediate issue
2. Check if the lesson applies elsewhere in the current task
3. Record the lesson in `.claude/lessons.md` if it's likely to recur

**Format for lessons**:

```markdown
## [Date] - Brief title
- **Context**: What you were doing
- **Mistake**: What went wrong
- **Fix**: What the correct approach is
```

At the start of each session, review `.claude/lessons.md` for recent entries.

---

## Verification Before Done

Never mark a task as complete without verifying your work:

### Build & Lint Checklist

| Change type | Verification command |
|-------------|---------------------|
| Rust backend (any .rs file) | `cargo check` (fast) or `cargo build` (full) |
| Rust logic | `cargo test` |
| Rust style | `cargo clippy -- -D warnings` |
| Studio component | `npx nx build cthulu-studio` |
| Studio + backend together | `cargo check && npx nx build cthulu-studio` |
| Site page/section | `npx nx build cthulu-site` |
| New API endpoint | `cargo check`, restart server, test with `curl` |
| Flow config / YAML | Start server, load flow in Studio, verify in UI |

### Staff-Engineer Bar

Before submitting, ask yourself:

- Would a staff engineer approve this without comments?
- Is this the **simplest correct approach**?
- Are there any edge cases I haven't handled?
- Did I leave orphaned code, dead imports, or `// TODO` comments?

---

## Autonomous Bug Fixing

When fixing bugs:

1. **Reproduce first** -- understand the exact failure before changing code
2. **Find root cause** -- don't patch symptoms
3. **Prove the fix works** -- run the relevant build/lint/test command
4. **Check for siblings** -- does the same bug pattern exist elsewhere?
5. **Zero hand-holding** -- the fix should be complete; no TODOs or "the user should also..."

---

## Task Tracking

For multi-step work:

1. **Create tasks upfront** -- break the work into trackable units before starting
2. **Mark progress** -- set tasks to `in_progress` when starting, `completed` when done
3. **Capture discoveries** -- if you find additional work needed, create new tasks
4. **One in-progress at a time** -- complete current task before starting the next

---

## Session Start Checklist

At the beginning of each session:

1. Review `.claude/lessons.md` for recent lessons
2. Read `CLAUDE.md` for project rules and architecture
3. For non-trivial tasks, plan before coding
4. Run `cargo check` to ensure the codebase compiles before making changes
