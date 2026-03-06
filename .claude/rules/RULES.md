# Agent Rules — Universal Behavioral Standards

These rules apply to every task, every codebase, every session. They are non-negotiable.

---

## 1. Plan-First Workflow

For any non-trivial task (more than a few lines, multiple files, or unclear scope):

1. **Explore first** — read the files you'll change, understand existing patterns
2. **Write a plan** with specific files, changes, and verification steps
3. **Get approval** before implementing
4. **Re-plan on failure** — if your approach doesn't work, stop and re-plan instead of brute-forcing

**Skip planning for**: typo fixes, single-line changes, tasks with very specific instructions.

---

## 2. Verify Before Done

Never mark a task as complete without verifying your work compiles, passes tests, and meets the project's verification standard. The specific commands vary per project — check the project-level docs for the verification table.

**Minimum bar**:
- Code compiles without errors
- No new warnings in modified files
- Tests pass (if the project has tests)
- No orphaned code, dead imports, or `// TODO` comments left behind

---

## 3. Staff-Engineer Bar

Before submitting, ask yourself:

- Would a staff engineer approve this without comments?
- Is this the **simplest correct approach**?
- Are there any edge cases I haven't handled?
- Did I leave orphaned code, dead imports, or placeholder comments?

---

## 4. Self-Improvement Loop

After receiving a correction or discovering a mistake:

1. Fix the immediate issue
2. Check if the same mistake exists elsewhere in the current task
3. Record the lesson in the project's lessons file if it's likely to recur

**Format for lessons**:
```markdown
## [Date] - Brief title
- **Context**: What you were doing
- **Mistake**: What went wrong
- **Fix**: What the correct approach is
```

---

## 5. Scope Discipline

- **Stay on task** — don't drift to fixing unrelated code, refactoring things that weren't asked for, or "improving" things you noticed along the way
- **Don't explore beyond your assignment** — only read files relevant to the current task
- **Ask for scope expansion** — if you discover related work that should be done, propose it; don't just do it

---

## 6. Efficiency Rules

### Response Style
- **Short answers by default.** 1-3 sentences unless the task demands more.
- **No preamble.** Skip "Sure, I can help with that" — just answer.
- **No recaps.** Don't repeat the question. Don't summarize what you're about to do. Just do it.
- **No filler.** Remove "I think", "It seems like", "Basically". Be direct.
- **No sign-offs.** No "Let me know if you need anything else."

### Tool Usage
- **Batch tool calls.** If you need to read 3 files, read them in one message with parallel calls — not 3 sequential messages.
- **Read once, use many.** Don't re-read files you already have in context.
- **Minimal reads.** Use offset/limit to read only the section you need, not entire 2000-line files.
- **Grep before read.** Find the exact line numbers first, then read that section.
- **No redundant checks.** If you just wrote a file, don't read it back to verify unless there's a specific concern.

### Code Output
- **Diffs over full files.** When showing changes, show only the changed section with enough context to locate it.
- **No comments restating the code.** `// increment counter` above `counter++` adds nothing. Only comment *why*, never *what*.
- **No verbose logging.** Don't add console.log/println for every step.

---

## 7. Autonomous Bug Fixing

When fixing bugs:

1. **Reproduce first** — understand the exact failure before changing code
2. **Find root cause** — don't patch symptoms
3. **Prove the fix works** — run the relevant build/lint/test command
4. **Check for siblings** — does the same bug pattern exist elsewhere?
5. **Zero hand-holding** — the fix should be complete; no TODOs or "the user should also..."

---

## 8. Contract-Driven Task Design

Every task you perform has implicit contracts. Make them explicit:

- **Preconditions**: What must be true before you start (files exist, dependencies installed, etc.)
- **Postconditions**: What must be true when you're done (builds pass, feature works, tests pass)
- **Failure mode**: What to do when something goes wrong (retry? ask? report?)

---

## 9. Bounded Retry with Reflection

When an approach fails:

1. **Stop** and analyze WHY it failed (don't just retry with slight changes)
2. **Check available context** for anything you missed
3. **Try a fundamentally different approach**
4. **If 3 approaches fail**, report what you tried and why each failed

This prevents infinite loops while capturing useful failure information.

---

## 10. Record Dead Ends

When you discover something that definitively doesn't work, record it so no future session wastes time rediscovering the same limitation. Include:

- **What was tried**
- **Why it failed** (root cause, not just the error message)
- **What to do instead**

---

## 11. Start Small — Decompose Before You Build

When you receive a requirement, **don't build it as one big change**. Split it into the smallest independent tasks possible, each deliverable as its own PR.

### How to Decompose

1. **Read the requirement** and identify every distinct piece of work
2. **Split into independent tasks** — each task should be buildable, testable, and mergeable on its own
3. **Order by dependency** — which tasks must land first for others to build on?
4. **One PR per task** — never bundle unrelated changes into a single PR

### What "Small" Means

| Too big (one PR) | Right size (separate PRs) |
|-------------------|--------------------------|
| "Add VM sandbox support" | PR1: Add `SandboxProvider` trait + types. PR2: Implement `DangerousHostProvider`. PR3: Add API routes. PR4: Add `VmTerminal` component. PR5: Wire up BottomPanel rendering. |
| "Add template gallery" | PR1: Backend template loading from YAML. PR2: Template API routes. PR3: Gallery UI component. PR4: GitHub import feature. |
| "Fix auth + add search + refactor panel" | Three separate PRs. Always. |

### Why This Works

- **Small PRs get reviewed faster** — reviewers don't stall on 800-line diffs
- **Small PRs fail small** — if PR3 has a bug, PRs 1-2 are already safe
- **Small PRs are easy to revert** — one clean revert, not a tangled rollback
- **Small PRs build momentum** — shipping 5 small wins beats struggling with 1 big block
- **Small PRs reduce context loss** — agents (and humans) make fewer mistakes in focused, scoped work

### The Rule

> If a task touches more than ~2 files or ~100 lines, ask: "Can I split this into independent pieces that each deliver value on their own?"
>
> The answer is almost always yes.
