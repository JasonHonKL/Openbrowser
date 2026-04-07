import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  InstanceInfo,
  SemanticNode,
  TreeStats,
  BridgeStatus,
  AgentStatus,
  AgentConfig,
  CdpEvent,
  StatusChange,
  AgentThinkingEvent,
  AgentToolCallEvent,
  AgentToolResultEvent,
  AgentErrorEvent,
  AgentCompleteEvent,
} from "../types";

// ---------------------------------------------------------------------------
// Instance management
// ---------------------------------------------------------------------------

export async function listInstances(): Promise<InstanceInfo[]> {
  return invoke("list_instances");
}

export async function spawnInstance(): Promise<InstanceInfo> {
  return invoke("spawn_instance");
}

export async function killInstance(id: string): Promise<void> {
  return invoke("kill_instance", { id });
}

export async function killAllInstances(): Promise<void> {
  return invoke("kill_all_instances");
}

// ---------------------------------------------------------------------------
// CDP bridge
// ---------------------------------------------------------------------------

export async function connectInstance(instanceId: string): Promise<void> {
  return invoke("connect_instance", { instanceId });
}

export async function disconnectInstance(instanceId: string): Promise<void> {
  return invoke("disconnect_instance", { instanceId });
}

export async function executeCdp(
  instanceId: string,
  method: string,
  params: Record<string, unknown>,
): Promise<unknown> {
  return invoke("execute_cdp", { instanceId, method, params });
}

export async function getSemanticTree(
  instanceId: string,
): Promise<{ semanticTree: { root: SemanticNode; stats: TreeStats } }> {
  return invoke("get_semantic_tree", { instanceId });
}

export async function getBridgeStatus(
  instanceId: string,
): Promise<BridgeStatus> {
  return invoke("get_bridge_status", { instanceId });
}

export async function getInstanceEvents(
  instanceId: string,
  limit?: number,
  since?: number,
): Promise<{ method: string; params: Record<string, unknown>; timestamp: number }[]> {
  return invoke("get_instance_events", { instanceId, limit: limit ?? 100, since });
}

// ---------------------------------------------------------------------------
// Agent status
// ---------------------------------------------------------------------------

export async function setAgentStatus(
  instanceId: string,
  status: AgentStatus,
): Promise<void> {
  return invoke("set_agent_status", { instanceId, status });
}

// ---------------------------------------------------------------------------
// AI Agent
// ---------------------------------------------------------------------------

export async function startAgent(
  instanceId: string,
  config: AgentConfig,
): Promise<void> {
  return invoke("start_agent", { instanceId, config });
}

export async function sendAgentMessage(
  instanceId: string,
  message: string,
): Promise<{ content?: string; error?: string }> {
  return invoke("send_agent_message", { instanceId, message });
}

export async function stopAgent(instanceId: string): Promise<void> {
  return invoke("stop_agent", { instanceId });
}

export async function clearAgentHistory(instanceId: string): Promise<void> {
  return invoke("clear_agent_history", { instanceId });
}

export async function shutdownAgent(instanceId: string): Promise<void> {
  return invoke("shutdown_agent", { instanceId });
}

export async function getAgentStatus(
  instanceId: string,
): Promise<string | null> {
  return invoke("get_agent_status", { instanceId });
}

export async function isAgentRunning(instanceId: string): Promise<boolean> {
  return invoke("is_agent_running", { instanceId });
}

// ---------------------------------------------------------------------------
// Browser windows
// ---------------------------------------------------------------------------

export async function openBrowserWindow(
  instanceId: string,
  url?: string,
): Promise<void> {
  return invoke("open_browser_window", { instanceId, url });
}

export async function closeBrowserWindow(instanceId: string): Promise<void> {
  return invoke("close_browser_window", { instanceId });
}

// ---------------------------------------------------------------------------
// Agent resume (take-over flow)
// ---------------------------------------------------------------------------

export async function resumeAgent(
  instanceId: string,
  message: string,
): Promise<{ content?: string; error?: string }> {
  return invoke("resume_agent", { instanceId, message });
}

// ---------------------------------------------------------------------------
// Event listeners
// ---------------------------------------------------------------------------

type UnlistenFn = () => void;

export function onCdpEvent(
  handler: (event: CdpEvent) => void,
): Promise<UnlistenFn> {
  return listen<CdpEvent>("cdp-event", (e) => handler(e.payload));
}

export function onAgentStatusChanged(
  handler: (event: StatusChange) => void,
): Promise<UnlistenFn> {
  return listen<StatusChange>("agent-status-changed", (e) => handler(e.payload));
}

export async function openChallengeWindow(
  url: string,
  title?: string,
): Promise<string> {
  return invoke("open_challenge_window", { url, title });
}

export function onChallengeDetected(
  handler: (info: { url: string; status: number; kinds: string[]; risk_score: number }) => void,
): Promise<UnlistenFn> {
  return listen<{ url: string; status: number; kinds: string[]; risk_score: number }>(
    "challenge-detected",
    (e) => handler(e.payload),
  );
}

export function onChallengeSolved(
  handler: (info: { url: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ url: string }>("challenge-solved", (e) => handler(e.payload));
}

export function onChallengeFailed(
  handler: (info: { challenge_url: string; reason: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ challenge_url: string; reason: string }>(
    "challenge-failed",
    (e) => handler(e.payload),
  );
}

export function onCdpBridgeConnected(
  handler: (info: { instance_id: string; port: number }) => void,
): Promise<UnlistenFn> {
  return listen<{ instance_id: string; port: number }>(
    "cdp-bridge-connected",
    (e) => handler(e.payload),
  );
}

export function onCdpBridgeDisconnected(
  handler: (info: { instance_id: string; port: number }) => void,
): Promise<UnlistenFn> {
  return listen<{ instance_id: string; port: number }>(
    "cdp-bridge-disconnected",
    (e) => handler(e.payload),
  );
}

// ---------------------------------------------------------------------------
// AI Agent event listeners
// ---------------------------------------------------------------------------

export function onAgentThinking(
  handler: (event: AgentThinkingEvent) => void,
): Promise<UnlistenFn> {
  return listen<AgentThinkingEvent>("agent-thinking", (e) => handler(e.payload));
}

export function onAgentToolCall(
  handler: (event: AgentToolCallEvent) => void,
): Promise<UnlistenFn> {
  return listen<AgentToolCallEvent>("agent-tool-call", (e) => handler(e.payload));
}

export function onAgentToolResult(
  handler: (event: AgentToolResultEvent) => void,
): Promise<UnlistenFn> {
  return listen<AgentToolResultEvent>("agent-tool-result", (e) => handler(e.payload));
}

export function onAgentError(
  handler: (event: AgentErrorEvent) => void,
): Promise<UnlistenFn> {
  return listen<AgentErrorEvent>("agent-error", (e) => handler(e.payload));
}

export function onAgentComplete(
  handler: (event: AgentCompleteEvent) => void,
): Promise<UnlistenFn> {
  return listen<AgentCompleteEvent>("agent-complete", (e) => handler(e.payload));
}

export function onAgentShutdown(
  handler: (event: { instance_id: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ instance_id: string }>("agent-shutdown", (e) => handler(e.payload));
}

export function onAgentHistoryCleared(
  handler: (event: { instance_id: string }) => void,
): Promise<UnlistenFn> {
  return listen<{ instance_id: string }>("agent-history-cleared", (e) => handler(e.payload));
}
