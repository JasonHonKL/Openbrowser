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
