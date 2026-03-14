/**
 * parseAgentActions.ts
 *
 * Parses raw PTY output from the Studio Assistant to extract structured
 * `create_flow` JSON actions from fenced code blocks.
 *
 * PTY output contains ANSI escape codes, partial writes, and arbitrary text.
 * This module:
 *  1. Strips ANSI escape sequences
 *  2. Finds complete ```json ... ``` blocks
 *  3. Parses the JSON and validates it is a create_flow action
 *  4. Returns the byte offset so the caller can trim already-processed text
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface CreateFlowAction {
  action: "create_flow";
  name: string;
  description: string;
  nodes: CreateFlowNode[];
  edges: "auto" | CreateFlowEdge[];
}

export interface CreateFlowNode {
  node_type: "trigger" | "source" | "filter" | "executor" | "sink";
  kind: string;
  label: string;
  config: Record<string, unknown>;
}

export interface CreateFlowEdge {
  source: string;
  target: string;
}

export interface ParseResult {
  /** Extracted actions (usually 0 or 1 per call). */
  actions: CreateFlowAction[];
  /** Byte offset in the input string up to which we have fully consumed.
   *  The caller should keep text from this offset onward as the new buffer. */
  consumedUpTo: number;
}

// ---------------------------------------------------------------------------
// ANSI stripping
// ---------------------------------------------------------------------------

/**
 * Strip ANSI escape codes from raw PTY text.
 * Covers CSI sequences (\x1b[...X), OSC (\x1b]...BEL/ST), and single-char escapes.
 */
// eslint-disable-next-line no-control-regex
const ANSI_RE = /[\x1b\x9b][[()#;?]*(?:[0-9]{1,4}(?:;[0-9]{0,4})*)?[0-9A-ORZcf-nqry=><~]|\x1b\].*?(?:\x07|\x1b\\)|\x1b[^[\]]/g;

export function stripAnsi(text: string): string {
  return text.replace(ANSI_RE, "");
}

// ---------------------------------------------------------------------------
// Code block extraction
// ---------------------------------------------------------------------------

/**
 * Find all complete ``` json ... ``` blocks in `text`.
 * Returns the parsed objects and the end index of the last block found
 * (so the caller can trim already-processed text).
 */
function extractJsonBlocks(text: string): { objects: unknown[]; lastEndIndex: number } {
  const objects: unknown[] = [];
  let lastEndIndex = 0;

  // Match ```json ... ``` or ``` ... ``` blocks
  // The regex is intentionally greedy for the closing fence to handle
  // nested backticks inside JSON strings — but since JSON strings use
  // double-quotes, this is safe in practice.
  const FENCE_RE = /```(?:json)?\s*\n([\s\S]*?)```/g;
  let match: RegExpExecArray | null;

  while ((match = FENCE_RE.exec(text)) !== null) {
    const jsonStr = match[1].trim();
    if (!jsonStr) continue;

    try {
      const parsed = JSON.parse(jsonStr);
      objects.push(parsed);
      lastEndIndex = match.index + match[0].length;
    } catch {
      // Malformed JSON — skip this block
    }
  }

  return { objects, lastEndIndex };
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

function isCreateFlowAction(obj: unknown): obj is CreateFlowAction {
  if (typeof obj !== "object" || obj === null) return false;
  const o = obj as Record<string, unknown>;
  return (
    o.action === "create_flow" &&
    typeof o.name === "string" &&
    o.name.length > 0 &&
    Array.isArray(o.nodes) &&
    o.nodes.length > 0
  );
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Parse a text buffer (accumulated PTY output with ANSI already stripped or
 * raw — we strip again for safety) and extract any complete `create_flow`
 * actions.
 *
 * @param buffer The accumulated text buffer (may contain ANSI codes).
 * @returns ParseResult with actions and consumedUpTo offset.
 */
export function parseAgentActions(buffer: string): ParseResult {
  const clean = stripAnsi(buffer);
  const { objects, lastEndIndex } = extractJsonBlocks(clean);

  const actions: CreateFlowAction[] = [];
  for (const obj of objects) {
    if (isCreateFlowAction(obj)) {
      actions.push(obj);
    }
  }

  // Map lastEndIndex back to the original buffer.
  // Since ANSI stripping may shorten the string, we need to find the
  // corresponding position in the original buffer.
  // Strategy: find the Nth occurrence of ``` in the original buffer
  // that corresponds to the last closing fence we consumed.
  let consumedUpTo = 0;
  if (lastEndIndex > 0) {
    // Count how many closing ``` we found in the cleaned text up to lastEndIndex
    const cleanedUpTo = clean.slice(0, lastEndIndex);
    const fenceCount = (cleanedUpTo.match(/```/g) || []).length;

    // Find the same number of ``` in the original buffer
    let count = 0;
    let idx = 0;
    while (count < fenceCount && idx < buffer.length) {
      const pos = buffer.indexOf("```", idx);
      if (pos === -1) break;
      count++;
      idx = pos + 3;
    }
    consumedUpTo = idx;
  }

  return { actions, consumedUpTo };
}
