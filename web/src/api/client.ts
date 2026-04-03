const BASE = "";

async function request<T>(path: string, opts?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...opts,
  });
  const data = await res.json();
  if (!res.ok) throw new Error(data.error ?? "Request failed");
  return data;
}

// Types
export interface TabInfo {
  id: number;
  url: string;
  title: string | null;
  state: string;
  can_go_back: boolean;
  can_go_forward: boolean;
  history_len: number;
  memory_usage_mb: number;
  memory_limit_mb: number;
}

export interface PageSnapshot {
  url: string;
  status: number;
  content_type: string | null;
  title: string | null;
  html: string;
}

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
  children: SemanticNode[];
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

export interface SemanticTree {
  root: SemanticNode;
  stats: TreeStats;
}

export interface NetworkRecord {
  id: number;
  method: string;
  type: string;
  url: string;
  status: number | null;
  content_type: string | null;
  body_size: number | null;
  timing_ms: number | null;
}

export interface CookieEntry {
  name: string;
  value: string;
  domain: string;
  path: string;
  http_only: boolean;
  secure: boolean;
}

export type ServerEvent =
  | { type: "navigation.started"; data: { tab_id: number; url: string } }
  | { type: "navigation.completed"; data: { tab_id: number; status: number; url: string } }
  | { type: "navigation.failed"; data: { tab_id: number; error: string } }
  | { type: "tab.created"; data: { id: number; url: string } }
  | { type: "tab.closed"; data: { id: number } }
  | { type: "tab.activated"; data: { id: number } }
  | { type: "semantic.updated"; data: { tab_id: number; stats: TreeStats } };

// API
export const api = {
  // Pages
  navigate: (url: string) => request<TabInfo>("/api/pages/navigate", { method: "POST", body: JSON.stringify({ url }) }),
  reload: () => request<TabInfo>("/api/pages/reload", { method: "POST" }),
  currentPage: () => request<PageSnapshot>("/api/pages/current"),

  // Tabs
  listTabs: () => request<TabInfo[]>("/api/tabs"),
  createTab: (url: string) => request<TabInfo>("/api/tabs", { method: "POST", body: JSON.stringify({ url }) }),
  closeTab: (id: number) => request<{ ok: boolean }>(`/api/tabs/${id}`, { method: "DELETE" }),
  activateTab: (id: number) => request<TabInfo>(`/api/tabs/${id}/activate`, { method: "POST" }),

  // Semantic
  semanticTree: () => request<SemanticTree>("/api/semantic/tree"),
  semanticTreeFlat: () => request<SemanticNode[]>("/api/semantic/tree?format=flat"),
  semanticElement: (id: number) => request<SemanticNode>(`/api/semantic/element/${id}`),

  // Interact
  click: (element_id?: number, selector?: string) =>
    request<{ ok: boolean }>("/api/interact/click", { method: "POST", body: JSON.stringify({ element_id, selector }) }),
  typeText: (value: string, element_id?: number, selector?: string) =>
    request<{ ok: boolean }>("/api/interact/type", { method: "POST", body: JSON.stringify({ element_id, selector, value }) }),
  submit: (formSelector: string, fields: Record<string, string>) =>
    request<{ ok: boolean }>("/api/interact/submit", { method: "POST", body: JSON.stringify({ form_selector: formSelector, fields }) }),
  scroll: (direction: "up" | "down") =>
    request<{ ok: boolean }>("/api/interact/scroll", { method: "POST", body: JSON.stringify({ direction }) }),

  // Network
  networkRequests: () => request<NetworkRecord[]>("/api/network/requests"),
  clearNetwork: () => request<{ ok: boolean }>("/api/network/requests", { method: "DELETE" }),

  // Cookies
  listCookies: () => request<CookieEntry[]>("/api/cookies"),
  setCookie: (name: string, value: string, domain: string, path = "/") =>
    request<{ ok: boolean }>("/api/cookies", { method: "POST", body: JSON.stringify({ name, value, domain, path }) }),
  deleteCookie: (name: string) => request<{ deleted: boolean }>(`/api/cookies/${name}`, { method: "DELETE" }),
  clearCookies: () => request<{ ok: boolean }>("/api/cookies", { method: "DELETE" }),
};
