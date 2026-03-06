import { useAgentChat } from "./chat/useAgentChat";
import AgentChatThread from "./chat/AgentChatThread";

export type AgentChatHandle = ReturnType<typeof useAgentChat>;

interface AgentChatViewProps {
  chat: AgentChatHandle;
}

export default function AgentChatView({ chat }: AgentChatViewProps) {
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
      debugMode={chat.debugMode}
      debugEvents={chat.debugEvents}
      onToggleDebug={() => chat.setDebugMode((v) => !v)}
      onClearDebug={chat.clearDebugEvents}
      onClear={chat.clearMessages}
      onInjectAssistant={chat.injectAssistantMessage}
      gitSnapshot={chat.gitSnapshot}
      pendingPermissions={chat.pendingPermissions}
      onPermissionResponse={chat.handlePermissionResponse}
    />
  );
}

export { useAgentChat };
