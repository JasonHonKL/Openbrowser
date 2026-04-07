#!/usr/bin/env node

import { Agent } from "./agent/index.js";
import { BrowserManager } from "./core/index.js";
import { LLMConfig } from "./llm/index.js";

interface JsonRpcRequest {
  jsonrpc: "2.0";
  id?: number;
  method: string;
  params?: Record<string, unknown>;
}

interface JsonRpcResponse {
  jsonrpc: "2.0";
  id: number;
  result?: unknown;
  error?: { code: number; message: string };
}

interface JsonRpcNotification {
  jsonrpc: "2.0";
  method: string;
  params: Record<string, unknown>;
}

type JsonRpcMessage = JsonRpcRequest | JsonRpcResponse;

let agent: Agent | null = null;
let browserManager: BrowserManager | null = null;
const pendingRequests = new Map<
  number,
  { resolve: (v: unknown) => void; reject: (e: Error) => void }
>();

function send(message: JsonRpcResponse | JsonRpcNotification): void {
  const line = JSON.stringify(message);
  process.stdout.write(line + "\n");
}

function notify(method: string, params: Record<string, unknown>): void {
  send({ jsonrpc: "2.0", method, params });
}

function respond(id: number, result: unknown): void {
  send({ jsonrpc: "2.0", id, result });
}

function respondError(id: number, code: number, message: string): void {
  send({ jsonrpc: "2.0", id, error: { code, message } });
}

async function handleRequest(req: JsonRpcRequest): Promise<void> {
  const id = req.id ?? 0;
  const params = req.params ?? {};

  switch (req.method) {
    case "agent.init": {
      try {
        if (agent) {
          respondError(id, -32000, "Agent already initialized");
          return;
        }

        const apiKey = params.apiKey as string | undefined;
        if (!apiKey) {
          respondError(id, -32000, "Missing required param: apiKey");
          return;
        }

        const llmConfig: LLMConfig = {
          apiKey,
          baseURL: (params.baseURL as string) || undefined,
          model: (params.model as string) || "gpt-4",
          temperature: (params.temperature as number) ?? 0.7,
          maxTokens: (params.maxTokens as number) ?? 4000,
        };

        browserManager = new BrowserManager();
        agent = new Agent(browserManager, {
          llmConfig,
          maxRounds: (params.maxRounds as number) ?? 50,
          customInstructions: (params.customInstructions as string) || undefined,
        });

        respond(id, { ok: true });
      } catch (e) {
        respondError(id, -32001, e instanceof Error ? e.message : String(e));
      }
      break;
    }

    case "agent.chat": {
      if (!agent) {
        respondError(id, -32002, "Agent not initialized. Call agent.init first.");
        return;
      }

      const message = params.message as string;
      if (!message) {
        respondError(id, -32003, "Missing required param: message");
        return;
      }

      (async () => {
        try {
          notify("agent.status", { status: "thinking" });

          const gen = agent!.streamChat(message);
          let fullContent = "";

          for await (const chunk of gen) {
            fullContent += chunk;
            notify("agent.thinking", { chunk });
          }

          respond(id, { content: fullContent });
          notify("agent.status", { status: "idle" });
        } catch (e) {
          const msg = e instanceof Error ? e.message : String(e);
          respondError(id, -32004, msg);
          notify("agent.error", { message: msg });
          notify("agent.status", { status: "error" });
        }
      })();

      break;
    }

    case "agent.stop": {
      if (!agent) {
        respondError(id, -32005, "Agent not initialized");
        return;
      }
      agent.stop();
      respond(id, { ok: true });
      notify("agent.status", { status: "idle" });
      break;
    }

    case "agent.clearHistory": {
      if (!agent) {
        respondError(id, -32006, "Agent not initialized");
        return;
      }
      agent.clearHistory();
      respond(id, { ok: true });
      notify("agent.history_cleared", {});
      break;
    }

    case "agent.getHistory": {
      if (!agent) {
        respondError(id, -32007, "Agent not initialized");
        return;
      }
      const history = agent.getHistory().map((m) => ({
        role: m.role,
        content: m.content,
        tool_calls: m.tool_calls?.map((tc) => ({
          id: tc.id,
          type: tc.type,
          name: tc.function.name,
          arguments: tc.function.arguments,
        })),
        tool_call_id: m.tool_call_id,
        name: m.name,
      }));
      respond(id, { history });
      break;
    }

    case "agent.shutdown": {
      respond(id, { ok: true });
      (async () => {
        if (browserManager) {
          await browserManager.closeAll();
        }
        setTimeout(() => process.exit(0), 100);
      })();
      break;
    }

    default:
      respondError(id, -32601, `Method not found: ${req.method}`);
  }
}

process.stdin.setEncoding("utf-8");
process.stdin.resume();

let buffer = "";

process.stdin.on("data", (data: string) => {
  buffer += data;
  const lines = buffer.split("\n");
  buffer = lines.pop() ?? "";

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;

    try {
      const msg = JSON.parse(trimmed) as JsonRpcMessage;

      if ("id" in msg && msg.id !== undefined && "method" in msg) {
        handleRequest(msg as JsonRpcRequest);
      }
    } catch {
      // ignore malformed lines
    }
  }
});

process.stdin.on("close", () => {
  browserManager?.closeAll().finally(() => process.exit(0));
});

process.on("SIGINT", () => {
  browserManager?.closeAll().finally(() => process.exit(0));
});

process.on("SIGTERM", () => {
  browserManager?.closeAll().finally(() => process.exit(0));
});
