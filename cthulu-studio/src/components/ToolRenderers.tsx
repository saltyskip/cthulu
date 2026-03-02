import { useState, useEffect, useCallback, useRef } from "react";
import {
  makeAssistantToolUI,
  type ToolCallMessagePartProps,
} from "@assistant-ui/react";
import { useAuiState } from "@assistant-ui/store";
import { useFilePreviewSelect } from "./chat/FilePreviewContext";
import { computeDiffLines } from "../utils/diff";
import { fileIcon } from "../utils/fileIcons";

// Helpers

function basename(filePath: string): { dir: string; name: string } {
  const parts = filePath.replace(/\\/g, "/").split("/");
  const name = parts.pop() || filePath;
  // Show last 2 directory segments to keep it short
  const tail = parts.length > 2 ? parts.slice(-2) : parts;
  const dir = tail.length > 0
    ? (parts.length > 2 ? "…/" : "") + tail.join("/") + "/"
    : "";
  return { dir, name };
}

function FilePath({ path, op }: { path: string; op?: string }) {
  const { name } = basename(path);
  return (
    <span className="fr-tool-file" title={path}>
      <span className="fr-tool-file-name">{name}</span>
      {op && <span className="fr-tool-file-op">{op}</span>}
    </span>
  );
}

function ToolShell({
  icon,
  iconColor,
  nerdFont = false,
  label,
  labelNode,
  badge,
  dir,
  done,
  error,
  children,
  defaultOpen = false,
}: {
  icon: string;
  iconColor?: string;
  nerdFont?: boolean;
  label?: string;
  labelNode?: React.ReactNode;
  badge?: string;
  dir?: string;
  done?: boolean;
  error?: boolean;
  children?: React.ReactNode;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const hasBody = !!children;

  return (
    <div className="fr-tool">
      <div
        className="fr-tool-row"
        onClick={() => hasBody && setOpen((v) => !v)}
        style={{ cursor: hasBody ? "pointer" : "default" }}
      >
        <span
          className={`fr-tool-icon ${nerdFont ? "fr-tool-icon-nerd" : ""}`}
          style={iconColor ? { color: iconColor } : undefined}
        >{icon}</span>
        {labelNode ?? <span className="fr-tool-name">{label}</span>}
        {badge && <span className="fr-tool-badge">{badge}</span>}
        <span className="fr-tool-spacer" />
        {dir && <span className="fr-tool-file-dir">{dir}</span>}
        {error && <span className="fr-tool-err">✗</span>}
        {done && !error && <span className="fr-tool-done">✓</span>}
        {hasBody && (
          <span className="fr-tool-caret">{open ? "▾" : "▸"}</span>
        )}
      </div>
      {open && children && (
        <div className="fr-tool-detail">{children}</div>
      )}
    </div>
  );
}

// Tool Renderers

export function EditToolRenderer(props: ToolCallMessagePartProps) {
  const args = props.args as {
    file_path?: string;
    old_string?: string;
    new_string?: string;
    replace_all?: boolean;
  };
  const hasResult = props.result !== undefined;
  const filePath = args.file_path || "unknown";
  const selectFile = useFilePreviewSelect();
  const diffLines =
    args.old_string !== undefined && args.new_string !== undefined
      ? computeDiffLines(args.old_string, args.new_string)
      : null;

  const handleClick = selectFile
    ? () => selectFile(props.toolCallId)
    : undefined;

  const fi = fileIcon(filePath);
  const { dir } = basename(filePath);

  return (
    <ToolShell
      icon={fi.icon}
      iconColor={fi.color}
      nerdFont
      labelNode={
        <span className={selectFile ? "fr-tool-clickable" : ""} onClick={handleClick}>
          <FilePath path={filePath} op="Edit" />
        </span>
      }
      badge={args.replace_all ? "replace_all" : undefined}
      dir={dir || undefined}
      done={hasResult}
    >
      {diffLines && (
        <div className="fr-diff">
          {diffLines.map((line, i) => (
            <div
              key={i}
              className={`fr-diff-line ${
                line.type === "del"
                  ? "fr-diff-del"
                  : line.type === "add"
                    ? "fr-diff-add"
                    : "fr-diff-ctx"
              }`}
            >
              <span className="fr-diff-prefix">
                {line.type === "del" ? "−" : line.type === "add" ? "+" : " "}
              </span>
              {line.text}
            </div>
          ))}
        </div>
      )}
    </ToolShell>
  );
}

export function WriteToolRenderer(props: ToolCallMessagePartProps) {
  const args = props.args as { file_path?: string; content?: string };
  const hasResult = props.result !== undefined;
  const filePath = args.file_path || "unknown";
  const selectFile = useFilePreviewSelect();

  const handleClick = selectFile
    ? () => selectFile(props.toolCallId)
    : undefined;

  const fi = fileIcon(filePath);
  const { dir } = basename(filePath);

  return (
    <ToolShell
      icon={fi.icon}
      iconColor={fi.color}
      nerdFont
      labelNode={
        <span className={selectFile ? "fr-tool-clickable" : ""} onClick={handleClick}>
          <FilePath path={filePath} op="Write" />
        </span>
      }
      dir={dir || undefined}
      done={hasResult}
    >
      {args.content && (
        <pre>{args.content}</pre>
      )}
    </ToolShell>
  );
}

/**
 * Detect consecutive Read calls for the same file.
 * Returns "hidden" if this part is absorbed by an earlier one,
 * or "first:N" if this is the first in a group of N.
 */
function useReadGroup(toolCallId: string, filePath: string): string {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return useAuiState((s: any) => {
    const parts = s.message?.parts;
    if (!Array.isArray(parts)) return "first:1";

    // Find our index
    let idx = -1;
    for (let i = 0; i < parts.length; i++) {
      if (parts[i]?.type === "tool-call" && parts[i].toolCallId === toolCallId) {
        idx = i;
        break;
      }
    }
    if (idx < 0) return "first:1";

    // Check if previous part is a same-file Read → we're hidden
    if (idx > 0) {
      const prev = parts[idx - 1];
      if (
        prev?.type === "tool-call" &&
        prev.toolName === "Read" &&
        prev.args?.file_path === filePath
      ) {
        return "hidden";
      }
    }

    // We're the first — count consecutive same-file Reads after us
    let count = 1;
    for (let i = idx + 1; i < parts.length; i++) {
      const p = parts[i];
      if (
        p?.type === "tool-call" &&
        p.toolName === "Read" &&
        p.args?.file_path === filePath
      ) {
        count++;
      } else {
        break;
      }
    }
    return `first:${count}`;
  });
}

export function ReadToolRenderer(props: ToolCallMessagePartProps) {
  const args = props.args as { file_path?: string };
  const hasResult = props.result !== undefined;
  const filePath = args.file_path || "unknown";

  const groupInfo = useReadGroup(props.toolCallId, filePath);
  if (groupInfo === "hidden") return null;

  const count = parseInt(groupInfo.split(":")[1], 10);
  const resultText =
    typeof props.result === "string"
      ? props.result
      : props.result !== undefined
        ? JSON.stringify(props.result, null, 2)
        : null;

  const fi = fileIcon(filePath);
  const { dir } = basename(filePath);

  return (
    <ToolShell
      icon={fi.icon}
      iconColor={fi.color}
      nerdFont
      labelNode={<FilePath path={filePath} op="Read" />}
      badge={count > 1 ? `×${count}` : undefined}
      dir={dir || undefined}
      done={hasResult}
    >
      {resultText && <pre>{resultText}</pre>}
    </ToolShell>
  );
}

export function BashToolRenderer(props: ToolCallMessagePartProps) {
  const args = props.args as { command?: string; description?: string };
  const hasResult = props.result !== undefined;
  const command = args.command || "";
  const truncated =
    command.length > 120 ? command.slice(0, 117) + "..." : command;
  const resultText =
    typeof props.result === "string"
      ? props.result
      : props.result !== undefined
        ? JSON.stringify(props.result, null, 2)
        : null;
  // Simple heuristic: error if result contains common error patterns
  const isError =
    resultText != null &&
    /(?:error|Error|ERR!|FAILED|panic|command not found)/i.test(
      resultText.slice(0, 500),
    );

  return (
    <ToolShell
      icon={"\ue795"}
      nerdFont
      iconColor="#89e051"
      labelNode={<span className="fr-tool-cmd">{truncated}</span>}
      done={hasResult}
      error={isError}
    >
      {resultText && <pre>{resultText}</pre>}
    </ToolShell>
  );
}

export function GlobGrepToolRenderer(props: ToolCallMessagePartProps) {
  const args = props.args as { pattern?: string; glob?: string; path?: string };
  const hasResult = props.result !== undefined;
  const pattern = args.pattern || args.glob || "";
  const resultText =
    typeof props.result === "string"
      ? props.result
      : props.result !== undefined
        ? JSON.stringify(props.result, null, 2)
        : null;

  return (
    <ToolShell
      icon={"\uf002"}
      nerdFont
      label={props.toolName}
      badge={pattern}
      done={hasResult}
    >
      {resultText && <pre>{resultText}</pre>}
    </ToolShell>
  );
}

export function AgentToolRenderer(props: ToolCallMessagePartProps) {
  const args = props.args as {
    description?: string;
    prompt?: string;
    subagent_type?: string;
    model?: string;
    run_in_background?: boolean;
  };
  const hasResult = props.result !== undefined;
  const description = args.description || "Subagent task";
  const agentType = args.subagent_type || "general-purpose";
  const resultText =
    typeof props.result === "string"
      ? props.result
      : props.result !== undefined
        ? JSON.stringify(props.result, null, 2)
        : null;

  const typeLabel =
    agentType === "Explore" ? "Explore" :
    agentType === "Plan" ? "Plan" :
    agentType === "general-purpose" ? "General" : agentType;

  return (
    <ToolShell
      icon={"\uf544"}
      nerdFont
      label={description}
      badge={typeLabel}
      done={hasResult}
    >
      {args.prompt && (
        <div className="fr-agent-prompt">{args.prompt.length > 300 ? args.prompt.slice(0, 297) + "..." : args.prompt}</div>
      )}
      {(args.model || args.run_in_background) && (
        <div className="fr-agent-meta">
          {args.model && <span className="fr-tool-badge">{args.model}</span>}
          {args.run_in_background && <span className="fr-tool-badge">background</span>}
        </div>
      )}
      {resultText && (
        <>
          <div className="fr-tool-sep" />
          <pre className="fr-tool-result">{resultText}</pre>
        </>
      )}
    </ToolShell>
  );
}

// TodoWrite is rendered as a sticky footer panel in AgentChatThread,
// so the inline version is intentionally hidden.
export function TodoWriteToolRenderer(_props: ToolCallMessagePartProps) {
  return null;
}

// Shared todo types used by the sticky panel
export interface TodoItem {
  content: string;
  status: string;
  activeForm?: string;
}

export function WebSearchToolRenderer(props: ToolCallMessagePartProps) {
  const args = props.args as { query?: string };
  const hasResult = props.result !== undefined;
  const query = args.query || "";

  return (
    <ToolShell icon={"\uf0ac"} nerdFont label={query || "Web Search"} done={hasResult}>
      {hasResult && (
        <pre className="fr-tool-result">
          {typeof props.result === "string" ? props.result : JSON.stringify(props.result, null, 2)}
        </pre>
      )}
    </ToolShell>
  );
}

export function WebFetchToolRenderer(props: ToolCallMessagePartProps) {
  const args = props.args as { url?: string; prompt?: string };
  const hasResult = props.result !== undefined;
  const url = args.url || "";
  const truncatedUrl = url.length > 80 ? url.slice(0, 77) + "..." : url;

  return (
    <ToolShell icon={"\uf0ac"} nerdFont labelNode={<span className="fr-tool-cmd">{truncatedUrl}</span>} badge={args.prompt ? "+" : undefined} done={hasResult}>
      {hasResult && (
        <pre className="fr-tool-result">
          {typeof props.result === "string" ? props.result : JSON.stringify(props.result, null, 2)}
        </pre>
      )}
    </ToolShell>
  );
}

// AskUserQuestion types
interface AskOption {
  label: string;
  description?: string;
}

interface AskQuestion {
  question: string;
  header?: string;
  options: AskOption[];
  multiSelect?: boolean;
}

interface AskUserQuestionArgs {
  questions?: AskQuestion[];
}

interface AskUserQuestionResult {
  answer: string;
}

export const AskUserQuestionToolUI = makeAssistantToolUI<
  AskUserQuestionArgs,
  AskUserQuestionResult
>({
  toolName: "AskUserQuestion",
  render: ({ args, result, addResult }) => {
    return (
      <AskUserQuestionInner
        args={args}
        result={result}
        addResult={addResult}
      />
    );
  },
});

function AskUserQuestionInner({
  args,
  result,
  addResult,
}: {
  args: AskUserQuestionArgs;
  result: AskUserQuestionResult | undefined;
  addResult: (result: AskUserQuestionResult) => void;
}) {
  const questions = args.questions || [];
  const q = questions[0]; // Claude Code sends one question at a time
  const [selected, setSelected] = useState<number | null>(null);
  const [sent, setSent] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  // Lock when the user has answered OR when a user message follows this one
  // (isLast=false means another message exists after this assistant message,
  // which must be a user message — so the question was bypassed or answered).
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const isLast = useAuiState((s: any) => s.message?.isLast ?? true);
  const done = !!result || sent || !isLast;

  // Scroll into view and steal focus from composer when the question appears
  useEffect(() => {
    if (!done && containerRef.current) {
      // Blur the composer input so keyboard shortcuts (1-9, Enter) work immediately
      if (document.activeElement instanceof HTMLElement) {
        document.activeElement.blur();
      }
      containerRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const select = useCallback((oi: number) => {
    if (done) return;
    setSelected((prev) => (prev === oi ? null : oi));
  }, [done]);

  const submit = useCallback(() => {
    if (!q || selected === null || done) return;
    setSent(true);
    const label = q.options[selected]?.label ?? "";
    addResult({ answer: label });
  }, [q, selected, done, addResult]);

  // Keyboard: 1-9 selects option, Enter submits selection
  useEffect(() => {
    if (done || !q) return;
    const handler = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;

      if (e.key === "Enter" && selected !== null) {
        e.preventDefault();
        submit();
        return;
      }

      const num = parseInt(e.key, 10);
      if (num >= 1 && num <= q.options.length) {
        e.preventDefault();
        select(num - 1);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [done, q, selected, select, submit]);

  if (!q) return null;

  // Done state: show which option was chosen (or all dimmed if bypassed)
  if (done) {
    const chosenLabel = result?.answer ?? (selected !== null ? q.options[selected]?.label : null);
    return (
      <div className="fr-ask fr-ask-done" ref={containerRef}>
        {q.header && <div className="fr-ask-header">{q.header}</div>}
        <div className="fr-ask-question">{q.question}</div>
        <div className="fr-ask-options">
          {q.options.map((opt, oi) => {
            const wasChosen = chosenLabel != null && opt.label === chosenLabel;
            return (
              <div
                key={oi}
                className={`fr-ask-opt ${wasChosen ? "fr-ask-opt-selected" : "fr-ask-opt-dimmed"}`}
              >
                <span className="fr-ask-key">{oi + 1}</span>
                <span className="fr-ask-indicator">{wasChosen ? "●" : "○"}</span>
                <span>
                  <span className="fr-ask-opt-label">{opt.label}</span>
                </span>
                {wasChosen && <span className="fr-tool-done">✓</span>}
              </div>
            );
          })}
        </div>
      </div>
    );
  }

  // Active state: show interactive options
  return (
    <div className="fr-ask fr-ask-active" ref={containerRef}>
      {q.header && <div className="fr-ask-header">{q.header}</div>}
      <div className="fr-ask-question">{q.question}</div>
      <div className="fr-ask-options">
        {q.options.map((opt, oi) => {
          const isSelected = selected === oi;
          return (
            <button
              key={oi}
              className={`fr-ask-opt ${isSelected ? "fr-ask-opt-selected" : ""}`}
              onClick={() => select(oi)}
            >
              <span className="fr-ask-key">{oi + 1}</span>
              <span className="fr-ask-indicator">
                {isSelected ? "●" : "○"}
              </span>
              <span>
                <span className="fr-ask-opt-label">{opt.label}</span>
                {opt.description && (
                  <span className="fr-ask-opt-desc">{opt.description}</span>
                )}
              </span>
            </button>
          );
        })}
      </div>
      <div className="fr-ask-footer">
        <button
          className="fr-ask-submit"
          disabled={selected === null}
          onClick={submit}
        >
          Answer
        </button>
        <span className="fr-ask-hint">
          Press 1-{q.options.length} to select, Enter to submit
        </span>
      </div>
    </div>
  );
}
