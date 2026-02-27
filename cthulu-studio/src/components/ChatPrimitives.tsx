import { useState } from "react";
import { MessagePrimitive, type ToolCallMessagePartProps } from "@assistant-ui/react";
import { MarkdownTextPrimitive } from "@assistant-ui/react-markdown";
import "@assistant-ui/react-ui/styles/markdown.css";

export function CompactAssistantMessage() {
  return (
    <MessagePrimitive.Root className="fr-msg">
      <MessagePrimitive.Content
        components={{
          Text: CompactMarkdown,
          tools: { Fallback: CompactToolCall },
        }}
      />
    </MessagePrimitive.Root>
  );
}

export function CompactUserMessage() {
  return (
    <MessagePrimitive.Root className="fr-msg fr-msg-user">
      <MessagePrimitive.Content
        components={{ Text: CompactMarkdown }}
      />
    </MessagePrimitive.Root>
  );
}

export function CompactMarkdown() {
  return (
    <div className="fr-md">
      <MarkdownTextPrimitive />
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
