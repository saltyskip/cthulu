import { useAgentChat } from "./chat/useAgentChat";
import AgentChatThread from "./chat/AgentChatThread";
import type { PendingPermission } from "../hooks/useGlobalPermissions";

export type AgentChatHandle = ReturnType<typeof useAgentChat>;

interface AgentChatViewProps {
  chat: AgentChatHandle;
  pendingPermissions: PendingPermission[];
  onPermissionResponse: (requestId: string, decision: "allow" | "deny") => void;
  onToggleDebug?: () => void;
  debugActive?: boolean;
}

export default function AgentChatView({ chat, pendingPermissions, onPermissionResponse, onToggleDebug, debugActive }: AgentChatViewProps) {
  return (
    <AgentChatThread
      messages={chat.messages}
      isStreaming={chat.isStreaming}
      resultMeta={chat.resultMeta}
      isDone={chat.isDone}
      onNew={chat.handleSend}
      onCancel={chat.handleCancel}
      attachments={chat.attachments}
      onAddFiles={chat.addFiles}
      onRemoveAttachment={chat.removeAttachment}
      fileInputRef={chat.fileInputRef}
      onToggleDebug={onToggleDebug}
      debugActive={debugActive ?? false}
      onClear={chat.clearMessages}
      onInjectAssistant={chat.injectAssistantMessage}
      gitSnapshot={chat.gitSnapshot}
      pendingPermissions={pendingPermissions}
      onPermissionResponse={onPermissionResponse}
    />
  );
}

export { useAgentChat };
