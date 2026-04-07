import { LLMClient, LLMConfig, Message, getSystemPrompt, compactMessages, truncateToolResult, ContextConfig } from '../llm/index.js';
import { ToolExecutor } from '../tools/executor.js';
import { BrowserManager } from '../core/index.js';
import { BrowserToolName } from '../tools/definitions.js';
import { ToolExecutionConfig, ToolExecutionResult } from '../tools/types.js';

interface AgentOptions {
  /** LLM configuration */
  llmConfig: LLMConfig;
  /** Optional custom instructions appended to system prompt */
  customInstructions?: string;
  /** Maximum number of tool call rounds (default: 50) */
  maxRounds?: number;
  /** Tool execution configuration */
  toolConfig?: {
    /** Enable parallel execution where safe (default: true) */
    parallel?: boolean;
    /** Continue on tool failure (default: true) */
    continueOnError?: boolean;
    /** Default retry configuration for all tools */
    defaultRetryConfig?: ToolExecutionConfig;
  };
  /** Context window management configuration */
  contextConfig?: Partial<ContextConfig>;
}

/**
 * Browser Agent that manages LLM conversation with tool calling
 * 
 * Each Agent instance maintains its own conversation history and
 * can be associated with a specific browser instance if needed.
 */
export class Agent {
  private llm: LLMClient;
  private toolExecutor: ToolExecutor;
  private messages: Message[] = [];
  private maxRounds: number;
  private browserManager: BrowserManager;
  private isRunning = false;
  private toolConfig: AgentOptions['toolConfig'];
  private contextConfig: ContextConfig;
  private abortController: AbortController | null = null;

  constructor(browserManager: BrowserManager, options: AgentOptions) {
    this.browserManager = browserManager;
    this.llm = new LLMClient(options.llmConfig);
    this.toolExecutor = new ToolExecutor(browserManager);
    this.maxRounds = options.maxRounds ?? 50;
    this.toolConfig = {
      parallel: true,
      continueOnError: true,
      ...options.toolConfig,
    };
    this.contextConfig = {
      maxTokens: 100_000,
      keepRecentMessages: 10,
      maxToolResultChars: 6000,
      charsPerToken: 4,
      ...options.contextConfig,
    };

    // Initialize with system prompt
    this.messages.push({
      role: 'system',
      content: getSystemPrompt(options.customInstructions),
    });
  }

  /**
   * Send a user message and process the response, including any tool calls
   * 
   * This is the main entry point for interacting with the agent.
   * It will:
   * 1. Add the user message to conversation
   * 2. Call the LLM with tools
   * 3. Execute any tool calls requested (with parallel execution if enabled)
   * 4. Return the final response
   */
  async chat(userMessage: string): Promise<string> {
    if (this.isRunning) {
      throw new Error('Agent is already processing a message');
    }

    this.isRunning = true;

    try {
      // Add user message
      this.messages.push({
        role: 'user',
        content: userMessage,
      });

      let rounds = 0;

      // Keep looping while the LLM wants to make tool calls
      while (rounds < this.maxRounds) {
        if (this.abortController?.signal.aborted) {
          return '[Agent stopped by user]';
        }

        rounds++;

        // Call LLM
        const response = await this.llm.chat(this.messages);

        // If no tool calls, we're done
        if (!response.toolCalls || response.toolCalls.length === 0) {
          // Add assistant message
          this.messages.push({
            role: 'assistant',
            content: response.content ?? '',
          });

          return response.content ?? '';
        }

        // Add assistant message with tool calls
        this.messages.push({
          role: 'assistant',
          content: response.content ?? '',
          tool_calls: response.toolCalls.map((call) => ({
            id: call.id,
            type: 'function',
            function: {
              name: call.name,
              arguments: JSON.stringify(call.arguments),
            },
          })),
        });

        // Execute tool calls
        const toolResults = await this.executeToolCalls(response.toolCalls);

        // Check if we should continue based on results and configuration
        const hasFailures = toolResults.some(r => !r.success);
        if (hasFailures && !this.toolConfig?.continueOnError) {
          // Abort the conversation due to tool failure
          const errorMessage = 'Tool execution failed. Aborting conversation.';
          this.messages.push({
            role: 'assistant',
            content: errorMessage,
          });
          return errorMessage;
        }

        // Add all tool results to conversation — toolCallId flows from the LLM response
        for (const result of toolResults) {
          const content = result.success
            ? truncateToolResult(result.content || '', this.contextConfig.maxToolResultChars)
            : `Error: ${result.error || 'Unknown error'}\n\nPartial result: ${result.content || 'none'}`;

          this.messages.push({
            role: 'tool',
            tool_call_id: result.toolCallId || 'unknown',
            content,
          });
        }

        // Compact conversation history if approaching context limit
        this.messages = compactMessages(this.messages, this.contextConfig);
      }

      if (rounds >= this.maxRounds) {
        return 'Maximum number of tool call rounds reached. The agent may be stuck in a loop.';
      }

      return '';
    } finally {
      this.isRunning = false;
      this.abortController = null;
    }
  }

  /**
   * Execute tool calls with parallel execution support
   */
  private async executeToolCalls(
    toolCalls: Array<{
      id: string;
      name: string;
      arguments: Record<string, unknown>;
    }>
  ): Promise<ToolExecutionResult[]> {
    if (!this.toolConfig?.parallel) {
      // Sequential execution
      const results: ToolExecutionResult[] = [];

      for (const call of toolCalls) {
        console.log(`[Tool] ${call.name}: ${JSON.stringify(call.arguments)}`);

        const result = await this.toolExecutor.executeTool(
          call.name as BrowserToolName,
          call.arguments,
          this.toolConfig?.defaultRetryConfig
        );

        results.push({
          toolCallId: call.id,
          name: call.name,
          args: call.arguments,
          success: result.success,
          content: result.content,
          error: result.error,
          durationMs: 0,
          attempts: 1,
        });

        // Print result for visibility
        if (result.success) {
          console.log(`[Tool Result] Success`);
        } else {
          console.log(`[Tool Error] ${result.error}`);
        }
      }

      return results;
    } else {
      // Parallel execution with grouping
      const tools = toolCalls.map(call => ({
        toolCallId: call.id,
        name: call.name as BrowserToolName,
        args: call.arguments,
        config: this.toolConfig?.defaultRetryConfig,
      }));

      // Execute with parallel grouping
      const parallelResult = await this.toolExecutor.executeTools(tools, {
        parallel: true,
        continueOnError: this.toolConfig?.continueOnError,
      });

      // Log results
      for (const result of parallelResult.results) {
        console.log(`[Tool] ${result.name}: ${result.success ? 'Success' : 'Failed'} (${result.attempts} attempts, ${result.durationMs}ms)`);
        if (result.error) {
          console.log(`[Tool Error] ${result.error}`);
        }
      }

      return parallelResult.results;
    }
  }

  /**
   * Stream a response for interactive CLI with full tool call support.
   *
   * Yields text chunks as they arrive. Tool calls are buffered and
   * executed after the stream completes, then the loop continues
   * (same as chat() but with streamed text output).
   */
  async *streamChat(userMessage: string): AsyncGenerator<string, string, unknown> {
    if (this.isRunning) {
      throw new Error('Agent is already processing a message');
    }

    this.isRunning = true;
    this.abortController = new AbortController();

    try {
      this.messages.push({ role: 'user', content: userMessage });

      let rounds = 0;

      while (rounds < this.maxRounds) {
        if (this.abortController.signal.aborted) {
          return '[Agent stopped by user]';
        }

        rounds++;

        const result = await this.llm.streamChat(this.messages);

        // Yield any text chunks
        for (const chunk of result.textChunks) {
          yield chunk;
        }

        // No tool calls — done
        if (!result.toolCalls || result.toolCalls.length === 0) {
          this.messages.push({
            role: 'assistant',
            content: result.content ?? '',
          });
          return result.content ?? '';
        }

        // Add assistant message with tool calls
        this.messages.push({
          role: 'assistant',
          content: result.content ?? '',
          tool_calls: result.toolCalls.map(call => ({
            id: call.id,
            type: 'function' as const,
            function: {
              name: call.name,
              arguments: JSON.stringify(call.arguments),
            },
          })),
        });

        // Execute tool calls
        const toolResults = await this.executeToolCalls(result.toolCalls);

        const hasFailures = toolResults.some(r => !r.success);
        if (hasFailures && !this.toolConfig?.continueOnError) {
          const errorMessage = 'Tool execution failed. Aborting conversation.';
          this.messages.push({ role: 'assistant', content: errorMessage });
          yield `\n\n${errorMessage}`;
          return errorMessage;
        }

        // Add tool results
        for (const res of toolResults) {
          const content = res.success
            ? truncateToolResult(res.content || '', this.contextConfig.maxToolResultChars)
            : `Error: ${res.error || 'Unknown error'}\n\nPartial result: ${res.content || 'none'}`;

          this.messages.push({
            role: 'tool',
            tool_call_id: res.toolCallId || 'unknown',
            content,
          });
        }

        // Compact context
        this.messages = compactMessages(this.messages, this.contextConfig);

        // The loop continues — the next iteration will stream the LLM's
        // response to the tool results (which may include more tool calls).
      }

      const limitMsg = 'Maximum number of tool call rounds reached.';
      yield `\n\n${limitMsg}`;
      return limitMsg;
    } finally {
      this.isRunning = false;
      this.abortController = null;
    }
  }

  /**
   * Get conversation history (for debugging)
   */
  getHistory(): Message[] {
    return [...this.messages];
  }

  /**
   * Clear conversation history (except system prompt)
   */
  clearHistory(): void {
    const systemMessage = this.messages[0];
    this.messages = [systemMessage];
  }

  /**
   * Clean up all browser instances managed by this agent
   */
  async cleanup(): Promise<void> {
    await this.browserManager.closeAll();
  }

  /**
   * Update tool configuration at runtime
   */
  setToolConfig(config: Partial<AgentOptions['toolConfig']>): void {
    this.toolConfig = { ...this.toolConfig, ...config };
  }

  /**
   * Get current tool configuration
   */
  getToolConfig(): AgentOptions['toolConfig'] {
    return { ...this.toolConfig };
  }

  stop(): void {
    if (this.abortController) {
      this.abortController.abort();
    }
  }
}
