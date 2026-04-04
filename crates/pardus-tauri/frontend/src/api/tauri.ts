import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  InstanceInfo,
  SemanticNode,
  TreeStats,
  BridgeStatus,
  AgentStatus,
  CdpEvent,
  StatusChange,
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
