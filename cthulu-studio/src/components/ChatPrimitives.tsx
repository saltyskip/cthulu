import { useState } from "react";
import { MessagePrimitive, useMessage, type ToolCallMessagePartProps } from "@assistant-ui/react";
import { MarkdownTextPrimitive } from "@assistant-ui/react-markdown";
import remarkGfm from "remark-gfm";
import { SyntaxHighlighter } from "./assistant-ui/shiki-highlighter";
import { ToolGroup } from "./assistant-ui/tool-group";
import {
  EditToolRenderer,
  WriteToolRenderer,
  ReadToolRenderer,
  BashToolRenderer,
  GlobGrepToolRenderer,
  AgentToolRenderer,
  TodoWriteToolRenderer,
  WebSearchToolRenderer,
  WebFetchToolRenderer,
} from "./ToolRenderers";
import "@assistant-ui/react-ui/styles/markdown.css";

function MessageTimestamp() {
  const message = useMessage();
  const createdAt = message?.createdAt;
  if (!createdAt) return null;
  const d = new Date(createdAt);
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return <span className="fr-msg-time">{time}</span>;
}

export function CompactAssistantMessage() {
  return (
    <MessagePrimitive.Root className="fr-msg">
      <MessageTimestamp />
      <MessagePrimitive.Content
        components={{
          Text: CompactMarkdown,
          tools: {
            by_name: {
              Edit: EditToolRenderer,
              Write: WriteToolRenderer,
              Read: ReadToolRenderer,
              Bash: BashToolRenderer,
              Glob: GlobGrepToolRenderer,
              Grep: GlobGrepToolRenderer,
              Agent: AgentToolRenderer,
              TodoWrite: TodoWriteToolRenderer,
              WebSearch: WebSearchToolRenderer,
              WebFetch: WebFetchToolRenderer,
            },
            Fallback: CompactToolCall,
          },
          ToolGroup,
        }}
      />
    </MessagePrimitive.Root>
  );
}

export function CompactUserMessage() {
  return (
    <MessagePrimitive.Root className="fr-msg fr-msg-user">
      <MessageTimestamp />
      <MessagePrimitive.Content
        components={{ Text: CompactMarkdown }}
      />
    </MessagePrimitive.Root>
  );
}

export function CompactMarkdown() {
  return (
    <div className="fr-md">
      <MarkdownTextPrimitive
        remarkPlugins={[remarkGfm]}
        components={{ SyntaxHighlighter }}
      />
    </div>
  );
}

export function CompactToolCall(props: ToolCallMessagePartProps) {
  const [open, setOpen] = useState(false);
  const hasResult = props.result !== undefined;

  return (
    <div className="fr-tool">
      <div className="fr-tool-row" onClick={() => setOpen((v) => !v)}>
        <span className="fr-tool-caret">{open ? "▾" : "▸"}</span>
        <span className="fr-tool-name">{props.toolName}</span>
        {hasResult && <span className="fr-tool-done">✓</span>}
      </div>
      {open && (
        <div className="fr-tool-detail">
          <pre>{JSON.stringify(props.args, null, 2)}</pre>
          {hasResult && (
            <>
              <div className="fr-tool-sep" />
              <pre className="fr-tool-result">
                {typeof props.result === "string"
                  ? props.result
                  : JSON.stringify(props.result, null, 2)}
              </pre>
            </>
          )}
        </div>
      )}
    </div>
  );
}
