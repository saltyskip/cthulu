import { createContext, useContext } from "react";

// Context for tool renderers to signal "select this file in the preview panel"
type FilePreviewContextType = ((toolCallId: string) => void) | null;
export const FilePreviewContext = createContext<FilePreviewContextType>(null);
export const useFilePreviewSelect = () => useContext(FilePreviewContext);

// File operation extracted from messages for the preview panel
export interface FileOp {
  toolCallId: string;
  filePath: string;
  type: "edit" | "write";
  oldString?: string;
  newString?: string;
  content?: string;
}

// Plan file extracted from messages (Write to .claude/plans/*.md)
export interface PlanOp {
  toolCallId: string;
  filePath: string;
  content?: string;
}
