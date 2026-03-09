---
name: claude-cli-shortcuts
description: Claude CLI shortcuts and patterns for working with the Cthulu codebase -- one-shot reviews, session management, hooks, worktrees, and more.
---

# Claude CLI Shortcuts

## When to Apply

- Running Claude Code from the terminal for quick tasks
- Setting up automation or CI with Claude CLI
- Managing multiple parallel sessions
- Configuring hooks for auto-formatting or linting
- Choosing the right permission mode for a task

## 1. One-Shot Code Review

Pipe a file into `claude -p` for a quick non-interactive review:

```bash
cat cthulu-studio/src/components/Sidebar.tsx | claude -p "Review for React anti-patterns"
```

For machine-parseable output:

```bash
claude -p --output-format json "List all TODO comments in src/"
```

Use for: quick reviews, generating summaries, one-off questions without starting a session.

## 2. Resume Sessions

Continue where you left off instead of losing context:

```bash
claude -c                              # continue most recent session in this directory
claude -r "auth-refactor" "Next step"  # resume a named session
claude -c -p "Run the tests"           # continue via SDK (non-interactive)
```

Name sessions for multi-day features so you can find them later. The session preserves full conversation history and file context.

## 3. Model Selection

Pick the right model for the task:

```bash
claude --model sonnet "Summarize this log file"          # fast, cheap
claude --model opus "Design the new permission system"   # deep reasoning
```

Guidelines for Cthulu:
- **sonnet**: log summarization, simple refactors, formatting, test writing
- **opus**: architecture decisions, complex debugging, security review, multi-file refactors

## 4. Budget-Capped Automation

Prevent runaway agents in automated/CI contexts:

```bash
claude -p --max-turns 5 "Fix lint errors in src/"
claude -p --max-budget-usd 2.00 "Refactor the pipeline module"
claude -p --max-turns 10 --max-budget-usd 5.00 "Add tests for all API endpoints"
```

- `--max-turns`: limits agentic loop iterations
- `--max-budget-usd`: hard dollar cap, stops when reached
- Both are print-mode only (`-p`)

## 5. Custom Subagents

Define project-specific agents inline:

```bash
claude --agents '{
  "flow-reviewer": {
    "description": "Reviews Cthulu flow YAML files for correctness",
    "prompt": "You review workflow YAML files. Check for missing node connections, invalid node types, circular dependencies, and security issues.",
    "tools": ["Read", "Grep", "Glob"],
    "model": "sonnet"
  },
  "rust-checker": {
    "description": "Checks Rust code compiles and passes clippy",
    "prompt": "You verify Rust code quality. Run cargo check and cargo clippy, report any errors.",
    "tools": ["Bash", "Read", "Grep"]
  }
}'
```

Or persist in `.claude/settings.json` under `"agents"` for the whole team.

## 6. Hooks for Auto-Formatting

Add to `.claude/settings.json` to auto-format after every file edit:

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write|Edit",
        "hooks": [
          {
            "type": "command",
            "command": "npx prettier --write \"$CLAUDE_PROJECT_DIR/cthulu-studio/src/**/*.{ts,tsx,css}\"",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

Other hook ideas:
- **PreToolUse** on `Bash`: block destructive commands (`rm -rf`, `git push --force`)
- **PostToolUse** on `Write|Edit`: run `cargo check` after Rust file edits
- **Stop**: run `npx nx build cthulu-studio` before Claude finishes to catch build errors

Cthulu already uses HTTP hooks for interactive sessions (permission gates in `chat.rs`). These command hooks complement that for local CLI usage.

## 7. Parallel Git Worktrees

Run multiple Claude sessions on isolated branches simultaneously:

```bash
claude -w feature-auth "Implement OAuth2 flow"
claude -w fix-sidebar "Fix sidebar overflow bug"
```

Each session gets its own worktree at `.claude/worktrees/<name>` with a clean git state. Changes in one session don't affect the other. Useful for:
- Working on multiple PRs at once
- Trying alternative approaches in parallel
- Isolating risky changes

## 8. Checkpointing

Claude Code auto-tracks file edits. Press `Esc` + `Esc` (or `/rewind`) to open the rewind menu:

- **Restore code and conversation**: undo everything back to that point
- **Restore code only**: revert files but keep the conversation
- **Restore conversation only**: rewind chat but keep current code
- **Summarize from here**: compress later messages to free context

Use when:
- An approach didn't work out — rewind and try differently
- Claude broke the build — restore code to the last working state
- Context is getting long — summarize verbose debugging sessions
- Want to branch off — use `/fork` to create an alternate path

Note: checkpointing only tracks edits made by Claude's file tools, not bash commands like `rm` or `mv`.

## 9. Slash Commands

Create project-specific shortcuts in `.claude/commands/`:

```
.claude/commands/
├── review-pr.md      # /review-pr — fetch and review current PR
├── check-all.md      # /check-all — cargo check && nx build
└── run-flow.md       # /run-flow <name> — trigger a flow run
```

Example `check-all.md`:

```markdown
Run the full verification suite for the Cthulu project:

1. `cargo check` — verify Rust compiles
2. `cargo clippy -- -D warnings` — lint Rust
3. `npx nx build cthulu-studio` — build the Studio frontend

Report any errors found.
```

These appear alongside built-in commands when you type `/` in a session. Also available via `.claude/skills/<name>/SKILL.md` for more complex workflows with supporting files.

## 10. Permission Modes

Choose the right mode for the task:

```bash
claude --permission-mode plan           # read-only exploration, no edits allowed
claude --permission-mode acceptEdits    # auto-approve file edits, still prompt for bash
```

In interactive sessions, toggle with `Shift+Tab`.

| Mode | File edits | Bash commands | Use case |
|------|-----------|--------------|----------|
| default | ask | ask | normal development |
| plan | blocked | blocked | code exploration, architecture review |
| acceptEdits | auto-approve | ask | trusted refactoring |
| bypassPermissions | auto-approve | auto-approve | CI/automation only |

Cthulu's interactive agent sessions currently rely on HTTP hooks for permissions instead of `--permission-mode`. The pipeline executor uses `--dangerously-skip-permissions`. Consider using `--permission-mode acceptEdits` as a safer middle ground for pipeline runs.

## Cthulu-Specific Notes

- The backend spawns Claude CLI in 4 ways: pipeline executor, sandbox executor, interactive chat, and utility spawns. See `.claude/skills/claude-cli-streaming.skill.md` for the streaming protocol details.
- Interactive sessions use `--input-format stream-json` for multi-turn conversation over a persistent process.
- Pipeline runs use `--print --verbose --output-format stream-json` for one-shot execution.
- `--model` is not currently used anywhere in the backend — all executions use the CLI default. Adding per-agent model selection is a potential enhancement.
