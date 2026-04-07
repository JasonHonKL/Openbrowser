import OpenAI from 'openai';
import { browserTools } from '../tools/definitions.js';

export interface LLMConfig {
  /** OpenAI API key */
  apiKey: string;
  /** Base URL for API (default: https://api.openai.com/v1) */
  baseURL?: string;
  /** Model to use (default: gpt-4) */
  model?: string;
  /** Temperature (default: 0.7) */
  temperature?: number;
  /** Max tokens (default: 4000) */
  maxTokens?: number;
}

export interface Message {
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  tool_calls?: Array<{
    id: string;
    type: 'function';
    function: {
      name: string;
      arguments: string;
    };
  }>;
  tool_call_id?: string;
  name?: string;
}

export class LLMClient {
  private client: OpenAI;
  private config: Required<LLMConfig>;

  constructor(config: LLMConfig) {
    this.config = {
      apiKey: config.apiKey,
      baseURL: config.baseURL ?? 'https://api.openai.com/v1',
      model: config.model ?? 'gpt-4',
      temperature: config.temperature ?? 0.7,
      maxTokens: config.maxTokens ?? 4000,
    };

    this.client = new OpenAI({
      apiKey: this.config.apiKey,
      baseURL: this.config.baseURL,
    });
  }

  /**
   * Send a conversation to the LLM with browser tools enabled
   */
  async chat(messages: Message[]): Promise<{
    content: string | null;
    toolCalls?: Array<{
      id: string;
      name: string;
      arguments: Record<string, unknown>;
    }>;
  }> {
    const response = await this.client.chat.completions.create({
      model: this.config.model,
      messages: messages as OpenAI.Chat.ChatCompletionMessageParam[],
      tools: browserTools as unknown as OpenAI.Chat.ChatCompletionTool[],
      tool_choice: 'auto',
      temperature: this.config.temperature,
      max_tokens: this.config.maxTokens,
    });

    const choice = response.choices[0];
    const message = choice.message;

    // Parse tool calls if present
    const toolCalls = message.tool_calls?.map((call) => {
      let arguments_: Record<string, unknown> = {};
      try {
        arguments_ = JSON.parse(call.function.arguments) as Record<string, unknown>;
      } catch {
        arguments_ = {};
      }
      return {
        id: call.id,
        name: call.function.name,
        arguments: arguments_,
      };
    });

    return {
      content: message.content,
      toolCalls,
    };
  }

  /**
   * Stream a conversation (for interactive CLI).
   * Buffers tool call chunks and returns the full result including tool calls.
   */
  async streamChat(messages: Message[]): Promise<{
    content: string | null;
    toolCalls?: Array<{
      id: string;
      name: string;
      arguments: Record<string, unknown>;
    }>;
    textChunks: string[];
  }> {
    const stream = await this.client.chat.completions.create({
      model: this.config.model,
      messages: messages as OpenAI.Chat.ChatCompletionMessageParam[],
      tools: browserTools as unknown as OpenAI.Chat.ChatCompletionTool[],
      tool_choice: 'auto',
      temperature: this.config.temperature,
      max_tokens: this.config.maxTokens,
      stream: true,
    });

    const textChunks: string[] = [];
    let fullContent = '';

    // Accumulate tool call fragments from stream chunks
    const toolCallAccum = new Map<number, { id: string; name: string; argsStr: string }>();

    for await (const chunk of stream) {
      const choice = chunk.choices[0];
      if (!choice) continue;

      const delta = choice.delta;

      // Text content
      if (delta?.content) {
        textChunks.push(delta.content);
        fullContent += delta.content;
      }

      // Tool call deltas — accumulate by index
      if (delta?.tool_calls) {
        for (const tc of delta.tool_calls) {
          const idx = tc.index ?? 0;
          if (!toolCallAccum.has(idx)) {
            toolCallAccum.set(idx, {
              id: tc.id ?? '',
              name: tc.function?.name ?? '',
              argsStr: '',
            });
          }
          const entry = toolCallAccum.get(idx)!;
          if (tc.id) entry.id = tc.id;
          if (tc.function?.name) entry.name = tc.function.name;
          if (tc.function?.arguments) entry.argsStr += tc.function.arguments;
        }
      }
    }

    // Assemble tool calls
    const toolCalls: Array<{ id: string; name: string; arguments: Record<string, unknown> }> = [];
    for (const [_, tc] of toolCallAccum) {
      let parsedArgs: Record<string, unknown> = {};
      try {
        parsedArgs = JSON.parse(tc.argsStr) as Record<string, unknown>;
      } catch {
        parsedArgs = {};
      }
      toolCalls.push({ id: tc.id, name: tc.name, arguments: parsedArgs });
    }

    return {
      content: fullContent || null,
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
      textChunks,
    };
  }

  getModel(): string {
    return this.config.model;
  }
}
