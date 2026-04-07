export interface InstanceInfo {
  id: string;
  port: number;
  ws_url: string;
  running: boolean;
  browser_window_open: boolean;
  current_url: string | null;
  agent_status: AgentStatus;
}

export type AgentStatus =
  | "idle"
  | "connected"
  | "running"
  | "paused"
  | "waiting-challenge"
  | "error";

export type BridgeStatus =
  | "Connecting"
  | "Connected"
  | "Reconnecting"
  | "Disconnected"
  | "Failed";

export interface SemanticNode {
  role: string;
  name: string | null;
  tag: string;
  interactive: boolean;
  is_disabled?: boolean;
  href?: string;
  action?: string;
  element_id?: number;
  selector?: string;
  input_type?: string;
  placeholder?: string;
  is_required?: boolean;
  options?: Array<{ value: string; label: string }>;
  children: SemanticNode[];
}

export interface SemanticTree {
  semanticTree: {
    root: SemanticNode;
    stats: TreeStats;
  };
}

export interface TreeStats {
  landmarks: number;
  links: number;
  headings: number;
  actions: number;
  forms: number;
  images: number;
  iframes: number;
  total_nodes: number;
}

export interface CdpEvent {
  instance_id: string;
  method: string;
  params: Record<string, unknown>;
  timestamp: number;
}

export interface ChallengeInfo {
  url: string;
  status: number;
  kinds: string[];
  risk_score: number;
}

export interface StatusChange {
  instance_id: string;
  old_status: AgentStatus;
  new_status: AgentStatus;
}

// ---------------------------------------------------------------------------
// AI Agent types
// ---------------------------------------------------------------------------

export type AgentRunStatus =
  | "idle"
  | "thinking"
  | "executing_tool"
  | "waiting_challenge"
  | "error";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  content: string;
  toolCalls?: ToolCallInfo[];
  timestamp: number;
  isStreaming?: boolean;
}

export interface ToolCallInfo {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
  result?: {
    success: boolean;
    duration_ms: number;
  };
}

export interface AgentConfig {
  apiKey: string;
  model: string;
  baseURL: string;
  temperature: number;
  maxTokens: number;
  maxRounds: number;
}

export interface AgentThinkingEvent {
  instance_id: string;
  chunk: string;
}

export interface AgentToolCallEvent {
  instance_id: string;
  id: string;
  name: string;
  args: Record<string, unknown>;
}

export interface AgentToolResultEvent {
  instance_id: string;
  id: string;
  name: string;
  success: boolean;
  duration_ms: number;
}

export interface AgentErrorEvent {
  instance_id: string;
  message: string;
}

export interface AgentCompleteEvent {
  instance_id: string;
  content: string;
}

// ---------------------------------------------------------------------------
// Phase 2: Multi-Agent Dashboard types
// ---------------------------------------------------------------------------

export type ViewMode = "grid" | "detail";

export interface GlobalActionEntry {
  id: string;
  instanceId: string;
  timestamp: number;
  type:
    | "navigate"
    | "action_start"
    | "action_complete"
    | "action_fail"
    | "thinking"
    | "tool_call"
    | "tool_result"
    | "error";
  summary: string;
  detail?: string;
}

export interface TakeOverState {
  instanceId: string;
  active: boolean;
  pausedAt: number | null;
}

export interface InstanceState {
  messages: ChatMessage[];
  agentRunStatus: AgentRunStatus;
  agentConnected: boolean;
  tree: SemanticNode | null;
  treeStats: TreeStats | null;
  events: CdpEvent[];
}
