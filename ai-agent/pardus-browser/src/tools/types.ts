/**
 * Tool execution configuration types
 */

export interface ToolExecutionConfig {
  /** Timeout for the tool call in milliseconds */
  timeout?: number;
  /** Number of retry attempts (default: 0) */
  retries?: number;
  /** Initial delay between retries in milliseconds (default: 1000) */
  retryDelay?: number;
  /** Maximum delay between retries in milliseconds (default: 30000) */
  maxRetryDelay?: number;
  /** Backoff multiplier for retries (default: 2) */
  retryBackoff?: number;
  /** Whether to retry on specific error codes */
  retryableErrors?: string[];
}

export interface ParallelToolGroup {
  /** Tools that can be executed in parallel */
  tools: Array<{
    name: string;
    args: Record<string, unknown>;
    config?: ToolExecutionConfig;
  }>;
  /** Strategy for handling partial failures */
  failureStrategy: 'continue' | 'abort' | 'retry-all';
}

export interface ToolExecutionResult {
  name: string;
  args: Record<string, unknown>;
  success: boolean;
  content?: string;
  error?: string;
  durationMs: number;
  attempts: number;
}

export interface ParallelExecutionResult {
  results: ToolExecutionResult[];
  allSucceeded: boolean;
  anySucceeded: boolean;
  failedCount: number;
  succeededCount: number;
}

/**
 * Check if a tool call can be executed in parallel with others.
 * Tool calls targeting the same browser instance should be sequential
 * to avoid race conditions.
 */
export function canExecuteInParallel(
  tool1: { name: string; args: Record<string, unknown> },
  tool2: { name: string; args: Record<string, unknown> }
): boolean {
  // Get instance IDs from both tools
  const instance1 = tool1.args.instance_id as string | undefined;
  const instance2 = tool2.args.instance_id as string | undefined;

  // If either doesn't target a specific instance, they can be parallel
  if (!instance1 || !instance2) {
    return true;
  }

  // Different instances can run in parallel
  if (instance1 !== instance2) {
    return true;
  }

  // Same instance - check if operations are read-only
  const readOnlyTools = ['browser_get_state', 'browser_list', 'browser_get_cookies', 'browser_get_storage', 'browser_get_action_plan'];
  const isReadOnly1 = readOnlyTools.includes(tool1.name);
  const isReadOnly2 = readOnlyTools.includes(tool2.name);

  // Two read-only operations on same instance can be parallel
  if (isReadOnly1 && isReadOnly2) {
    return true;
  }

  // Everything else on same instance should be sequential
  return false;
}

/**
 * Group tool calls into parallelizable groups.
 * Each group contains tools that can safely execute in parallel.
 */
export function groupToolsForParallelExecution(
  tools: Array<{ name: string; args: Record<string, unknown>; config?: ToolExecutionConfig }>
): ParallelToolGroup[] {
  const groups: ParallelToolGroup[] = [];
  let currentGroup: ParallelToolGroup = { tools: [], failureStrategy: 'continue' };

  for (const tool of tools) {
    // Check if this tool can execute in parallel with current group
    const canParallel = currentGroup.tools.every(t => canExecuteInParallel(t, tool));

    if (canParallel) {
      currentGroup.tools.push(tool);
    } else {
      // Start a new group
      if (currentGroup.tools.length > 0) {
        groups.push(currentGroup);
      }
      currentGroup = { tools: [tool], failureStrategy: 'continue' };
    }
  }

  // Don't forget the last group
  if (currentGroup.tools.length > 0) {
    groups.push(currentGroup);
  }

  return groups;
}
