import { Message } from './client.js';
import { CORE_PROMPT } from './prompts.js';

export interface ContextConfig {
  /** Max approximate tokens before compaction triggers (default: 100000) */
  maxTokens: number;
  /** Number of recent messages to always keep in full (default: 10) */
  keepRecentMessages: number;
  /** Max chars for a tool result before truncation (default: 6000) */
  maxToolResultChars: number;
  /** Characters per token for approximation (default: 4) */
  charsPerToken: number;
}

const DEFAULT_CONTEXT_CONFIG: ContextConfig = {
  maxTokens: 100_000,
  keepRecentMessages: 10,
  maxToolResultChars: 6000,
  charsPerToken: 4,
};

/**
 * Approximate token count for a string.
 * Uses character-based heuristic; good enough for budgeting.
 */
function estimateTokens(text: string, charsPerToken: number): number {
  return Math.ceil(text.length / charsPerToken);
}

/**
 * Truncate a string to a maximum character length, appending a marker.
 */
function truncate(str: string, maxChars: number): string {
  if (str.length <= maxChars) return str;
  const half = Math.floor((maxChars - 40) / 2);
  return str.slice(0, half) +
    '\n\n... [truncated, use browser_get_state for full page content] ...\n\n' +
    str.slice(-half);
}

/**
 * Summarize a tool result message for compaction.
 * Keeps the essential info (which tool, success/fail) but drops the large content.
 */
function summarizeToolMessage(msg: Message): string {
  if (msg.role !== 'tool' || !msg.content) return msg.content ?? '';

  // Keep the first ~300 chars (usually the header/metadata), drop the full page tree
  const content = msg.content;
  if (content.length <= 500) return content;

  // Find the "---" separator that typically precedes "## Page Content"
  const separatorIdx = content.indexOf('\n---\n');
  if (separatorIdx !== -1) {
    // Keep everything before the separator (metadata/stats) plus a note
    return content.slice(0, separatorIdx) +
      '\n\n[Page content omitted to save context. Use browser_get_state to retrieve it.]';
  }

  // Fallback: keep first 300 chars
  return content.slice(0, 300) +
    '\n\n[Content truncated for context management.]';
}

/**
 * Estimate the total token count of a message array.
 */
export function estimateMessageTokens(messages: Message[], charsPerToken: number = 4): number {
  let total = 0;
  for (const msg of messages) {
    // Overhead per message (role, metadata) ~4 tokens
    total += 4;
    if (msg.content) total += estimateTokens(msg.content, charsPerToken);
    if (msg.tool_calls) {
      for (const tc of msg.tool_calls) {
        total += estimateTokens(tc.function.name + tc.function.arguments, charsPerToken);
        total += 4; // overhead per tool call
      }
    }
  }
  return total;
}

/**
 * Compact a message array to fit within a token budget.
 *
 * Strategy:
 * 1. Always keep the system prompt (messages[0])
 * 2. Always keep the N most recent messages in full
 * 3. For older messages:
 *    - user/assistant messages: keep but truncate if very long
 *    - tool messages: summarize (drop page content, keep metadata)
 * 4. If still over budget, remove oldest non-system messages
 */
export function compactMessages(
  messages: Message[],
  config: Partial<ContextConfig> = {}
): Message[] {
  const cfg = { ...DEFAULT_CONTEXT_CONFIG, ...config };

  if (messages.length <= 1) return messages;

  // Step 1: Truncate large tool results in-place
  const truncated = messages.map(msg => {
    if (msg.role === 'tool' && msg.content && msg.content.length > cfg.maxToolResultChars) {
      return { ...msg, content: truncate(msg.content, cfg.maxToolResultChars) };
    }
    return msg;
  });

  // Check if we're within budget after truncation
  const currentTokens = estimateMessageTokens(truncated, cfg.charsPerToken);
  if (currentTokens <= cfg.maxTokens) {
    return truncated;
  }

  // Step 2: Need compaction — downgrade system prompt to core + summarize older tool messages
  const systemMsg = { ...truncated[0], content: CORE_PROMPT };
  const recentStart = Math.max(1, truncated.length - cfg.keepRecentMessages);
  const olderMessages = truncated.slice(1, recentStart);
  const recentMessages = truncated.slice(recentStart);

  const summarized = olderMessages.map(msg => {
    if (msg.role === 'tool') {
      return { ...msg, content: summarizeToolMessage(msg) };
    }
    // For very long user/assistant messages, truncate
    if (msg.content && msg.content.length > cfg.maxToolResultChars) {
      return { ...msg, content: truncate(msg.content, cfg.maxToolResultChars) };
    }
    return msg;
  });

  const result = [systemMsg, ...summarized, ...recentMessages];

  // Step 3: If STILL over budget, drop oldest messages (keep system + recent)
  let finalTokens = estimateMessageTokens(result, cfg.charsPerToken);
  if (finalTokens > cfg.maxTokens) {
    // Drop from the summarized section until we fit
    let dropFrom = 1; // start after system message
    while (dropFrom < result.length - cfg.keepRecentMessages && finalTokens > cfg.maxTokens) {
      const dropped = result[dropFrom];

      if (dropped.role === 'assistant' && dropped.tool_calls && dropped.tool_calls.length > 0) {
        const toolCallCount = dropped.tool_calls.length;
        const removed = result.splice(dropFrom, 1 + toolCallCount);
        let removedTokens = 0;
        for (const msg of removed) {
          removedTokens += estimateTokens(
            (msg.content ?? '') + (msg.tool_calls?.map(tc => tc.function.arguments).join('') ?? ''),
            cfg.charsPerToken
          );
        }
        finalTokens -= removedTokens;
        continue;
      }

      if (dropped.role === 'tool' && dropFrom > 1) {
        const prev = result[dropFrom - 1];
        if (prev.role === 'assistant' && prev.tool_calls && prev.tool_calls.length > 0) {
          const toolCallCount = prev.tool_calls.length;
          const removed = result.splice(dropFrom - 1, 1 + toolCallCount);
          let removedTokens = 0;
          for (const msg of removed) {
            removedTokens += estimateTokens(
              (msg.content ?? '') + (msg.tool_calls?.map(tc => tc.function.arguments).join('') ?? ''),
              cfg.charsPerToken
            );
          }
          finalTokens -= removedTokens;
          continue;
        }
      }

      const removed = result.splice(dropFrom, 1);
      finalTokens -= estimateTokens(
        (removed[0].content ?? '') + (removed[0].tool_calls?.map(tc => tc.function.arguments).join('') ?? ''),
        cfg.charsPerToken
      );
    }

    // Add a note that context was compacted
    if (result.length > 1 && result[1].role !== 'system') {
      result.splice(1, 0, {
        role: 'user',
        content: '[System note: Earlier conversation history was compacted to fit the context window.]',
      });
    }
  }

  return result;
}

/**
 * Truncate a tool result string to the configured max length.
 * Use this before returning tool results to the agent.
 */
export function truncateToolResult(content: string, maxChars: number = 6000): string {
  return truncate(content, maxChars);
}
