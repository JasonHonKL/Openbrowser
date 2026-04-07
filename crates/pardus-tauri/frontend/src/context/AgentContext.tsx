import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  useRef,
  type ReactNode,
} from "react";
import type {
  InstanceInfo,
  CdpEvent,
  SemanticNode,
  TreeStats,
  ChatMessage,
  AgentConfig,
  AgentRunStatus,
  ViewMode,
  GlobalActionEntry,
  TakeOverState,
  InstanceState,
} from "../types";
import * as api from "../api/tauri";
import { classifyEvent, type ActionEntry } from "../utils/classifyEvent";

// ---------------------------------------------------------------------------
// Context interface
// ---------------------------------------------------------------------------

interface AgentContextValue {
  instances: InstanceInfo[];
  selectedId: string | null;
  select: (id: string | null) => void;

  // View mode
  viewMode: ViewMode;
  setViewMode: (mode: ViewMode) => void;

  // Per-instance data (selected instance getters for backward compat)
  tree: SemanticNode | null;
  stats: TreeStats | null;
  events: CdpEvent[];
  loading: boolean;
  error: string | null;
  refreshInstances: () => Promise<void>;
  refreshTree: () => Promise<void>;

  // Agent
  messages: ChatMessage[];
  agentRunStatus: AgentRunStatus;
  agentConfig: AgentConfig;
  setAgentConfig: (config: AgentConfig) => void;
  sendMessage: (text: string) => Promise<void>;
  startAgent: (config: AgentConfig) => Promise<void>;
  stopAgent: () => Promise<void>;
  shutdownAgent: () => Promise<void>;
  clearHistory: () => Promise<void>;
  agentConnected: boolean;

  // Phase 2 — multi-instance
  getInstanceState: (id: string) => InstanceState | undefined;
  globalActions: GlobalActionEntry[];
  takeOver: TakeOverState | null;
  startTakeOver: (instanceId: string) => Promise<void>;
  endTakeOver: (instanceId: string) => Promise<void>;
}

const AgentContext = createContext<AgentContextValue | null>(null);

export function useAgent(): AgentContextValue {
  const ctx = useContext(AgentContext);
  if (!ctx) throw new Error("useAgent must be used within AgentProvider");
  return ctx;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const MAX_EVENTS = 1000;
const MAX_GLOBAL_ACTIONS = 500;

function debounce<T extends (...args: never[]) => void>(
  fn: T,
  ms: number,
): (...args: Parameters<T>) => void {
  let timer: ReturnType<typeof setTimeout>;
  return (...args: Parameters<T>) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}

const DEFAULT_AGENT_CONFIG: AgentConfig = {
  apiKey: "",
  model: "gpt-4",
  baseURL: "https://api.openai.com/v1",
  temperature: 0.7,
  maxTokens: 4000,
  maxRounds: 50,
};

function loadAgentConfig(): AgentConfig {
  try {
    const saved = localStorage.getItem("pardus-agent-config");
    if (saved) {
      return { ...DEFAULT_AGENT_CONFIG, ...JSON.parse(saved) };
    }
  } catch {
    // ignore
  }
  return DEFAULT_AGENT_CONFIG;
}

function saveAgentConfig(config: AgentConfig) {
  try {
    localStorage.setItem("pardus-agent-config", JSON.stringify(config));
  } catch {
    // ignore
  }
}

function emptyInstanceState(): InstanceState {
  return {
    messages: [],
    agentRunStatus: "idle",
    agentConnected: false,
    tree: null,
    treeStats: null,
    events: [],
  };
}

function classifyGlobalAction(event: CdpEvent): GlobalActionEntry | null {
  const entry = classifyEvent(event);
  if (!entry) return null;
  return {
    id: entry.id,
    instanceId: event.instance_id,
    timestamp: entry.timestamp,
    type: entry.type === "navigate" ? "navigate"
      : entry.type === "action_start" ? "action_start"
      : entry.type === "action_complete" ? "action_complete"
      : "action_fail",
    summary: entry.summary,
    detail: entry.detail,
  };
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export function AgentProvider({ children }: { children: ReactNode }) {
  // --- Core state ---
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // --- Per-instance state map ---
  const [instanceStates, setInstanceStates] = useState<Map<string, InstanceState>>(new Map());
  const instanceStatesRef = useRef<Map<string, InstanceState>>(new Map());

  // Keep ref in sync
  useEffect(() => {
    instanceStatesRef.current = instanceStates;
  }, [instanceStates]);

  // Selected ID ref for use in stable callbacks
  const selectedIdRef = useRef<string | null>(null);
  useEffect(() => {
    selectedIdRef.current = selectedId;
  }, [selectedId]);

  // --- Phase 2 state ---
  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [globalActions, setGlobalActions] = useState<GlobalActionEntry[]>([]);
  const [takeOver, setTakeOver] = useState<TakeOverState | null>(null);
  const [agentConfig, setAgentConfigState] = useState<AgentConfig>(loadAgentConfig);

  // Sending lock per instance
  const sendingRefs = useRef<Map<string, boolean>>(new Map());

  // --- Refresh helpers ---
  const refreshInstances = useCallback(async () => {
    try {
      const list = await api.listInstances();
      setInstances(list);
    } catch {
      // silent
    }
  }, []);

  const refreshTreeForInstance = useCallback(async (instanceId: string) => {
    try {
      const result = await api.getSemanticTree(instanceId);
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(instanceId);
        if (state) {
          next.set(instanceId, {
            ...state,
            tree: result.semanticTree.root,
            treeStats: result.semanticTree.stats,
          });
        }
        return next;
      });
    } catch {
      // silent — may not be connected yet
    }
  }, []);

  // Debounced tree refresh (selected: 300ms, non-selected: 1000ms)
  const debouncedRefreshers = useRef<Map<string, ReturnType<typeof debounce>>>(
    new Map(),
  );

  const getDebouncedRefresh = useCallback(
    (instanceId: string) => {
      const key = instanceId;
      const existing = debouncedRefreshers.current.get(key);
      if (existing) return existing;
      const d = debounce(() => refreshTreeForInstance(instanceId), 500);
      debouncedRefreshers.current.set(key, d);
      return d;
    },
    [refreshTreeForInstance],
  );

  // --- Backward-compatible getters (selected instance) ---
  const selectedState = selectedId
    ? instanceStates.get(selectedId) ?? emptyInstanceState()
    : emptyInstanceState();

  const tree = selectedState.tree;
  const stats = selectedState.treeStats;
  const events = selectedState.events;
  const messages = selectedState.messages;
  const agentRunStatus = selectedState.agentRunStatus;
  const agentConnected = selectedState.agentConnected;

  // --- refreshTree (selected) ---
  const refreshTree = useCallback(async () => {
    if (!selectedId) return;
    setLoading(true);
    setError(null);
    try {
      const result = await api.getSemanticTree(selectedId);
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(selectedId);
        if (state) {
          next.set(selectedId, {
            ...state,
            tree: result.semanticTree.root,
            treeStats: result.semanticTree.stats,
          });
        }
        return next;
      });
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [selectedId]);

  const refreshTreeRef = useRef(refreshTree);
  refreshTreeRef.current = refreshTree;

  // --- Poll instances ---
  useEffect(() => {
    refreshInstances();
    const interval = setInterval(refreshInstances, 3000);
    return () => clearInterval(interval);
  }, [refreshInstances]);

  // --- Register event listeners ONCE (no selectedId dependency) ---
  useEffect(() => {
    const unsubs: Array<() => void> = [];

    api.onCdpEvent((event) => {
      const iid = event.instance_id;

      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;

        const updated = [...state.events, event];
        if (updated.length > MAX_EVENTS) {
          updated.splice(0, updated.length - MAX_EVENTS);
        }
        next.set(iid, { ...state, events: updated });
        return next;
      });

      // Feed global action stream
      const action = classifyGlobalAction(event);
      if (action) {
        setGlobalActions((prev) => [...prev.slice(-MAX_GLOBAL_ACTIONS), action]);
      }

      // Debounced tree refresh for this instance
      if (
        event.method === "Page.frameNavigated" ||
        event.method.startsWith("Pardus.action")
      ) {
        // We don't know if it's selected here, use a moderate debounce
        const dr = getDebouncedRefresh(iid);
        dr();
      }
    }).then((u) => { if (u) unsubs.push(u); });

    api.onAgentStatusChanged((change) => {
      refreshInstances();
    }).then((u) => { if (u) unsubs.push(u); });

    api.onAgentThinking((event) => {
      const iid = event.instance_id;
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        const newMessages = (() => {
          const last = state.messages[state.messages.length - 1];
          if (last && last.role === "assistant" && last.isStreaming) {
            const updated = [...state.messages];
            updated[updated.length - 1] = {
              ...last,
              content: last.content + event.chunk,
            };
            return updated;
          }
          return [
            ...state.messages,
            {
              id: `msg-${Date.now()}`,
              role: "assistant" as const,
              content: event.chunk,
              timestamp: Date.now(),
              isStreaming: true,
            },
          ];
        })();
        next.set(iid, { ...state, agentRunStatus: "thinking", messages: newMessages });
        return next;
      });

      setGlobalActions((prev) => [
        ...prev.slice(-MAX_GLOBAL_ACTIONS),
        {
          id: `think-${Date.now()}`,
          instanceId: iid,
          timestamp: Date.now(),
          type: "thinking",
          summary: "Thinking...",
        },
      ]);
    }).then((u) => { if (u) unsubs.push(u); });

    api.onAgentToolCall((event) => {
      const iid = event.instance_id;
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        next.set(iid, {
          ...state,
          agentRunStatus: "executing_tool",
          messages: [
            ...state.messages,
            {
              id: `tool-${event.id}`,
              role: "tool" as const,
              content: `${event.name}(${JSON.stringify(event.args).slice(0, 120)})`,
              toolCalls: [
                {
                  id: event.id,
                  name: event.name,
                  arguments: event.args,
                },
              ],
              timestamp: Date.now(),
            },
          ],
        });
        return next;
      });

      setGlobalActions((prev) => [
        ...prev.slice(-MAX_GLOBAL_ACTIONS),
        {
          id: `tc-${event.id}`,
          instanceId: iid,
          timestamp: Date.now(),
          type: "tool_call",
          summary: event.name,
          detail: JSON.stringify(event.args).slice(0, 100),
        },
      ]);
    }).then((u) => { if (u) unsubs.push(u); });

    api.onAgentToolResult((event) => {
      const iid = event.instance_id;
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        next.set(iid, {
          ...state,
          messages: state.messages.map((msg) => {
            const tc = msg.toolCalls?.find((t) => t.id === event.id);
            if (!tc) return msg;
            return {
              ...msg,
              content: `${event.name} ${event.success ? "done" : "failed"} (${event.duration_ms}ms)`,
              toolCalls: msg.toolCalls?.map((t) =>
                t.id === event.id
                  ? {
                      ...t,
                      result: {
                        success: event.success,
                        duration_ms: event.duration_ms,
                      },
                    }
                  : t,
              ),
            };
          }),
        });
        return next;
      });

      setGlobalActions((prev) => [
        ...prev.slice(-MAX_GLOBAL_ACTIONS),
        {
          id: `tr-${event.id}`,
          instanceId: iid,
          timestamp: Date.now(),
          type: "tool_result",
          summary: `${event.name} ${event.success ? "done" : "failed"}`,
          detail: `${event.duration_ms}ms`,
        },
      ]);
    }).then((u) => { if (u) unsubs.push(u); });

    api.onAgentError((event) => {
      const iid = event.instance_id;
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        const newMessages = (() => {
          const last = state.messages[state.messages.length - 1];
          if (last && last.role === "assistant" && last.isStreaming) {
            const updated = [...state.messages];
            updated[updated.length - 1] = {
              ...last,
              content: last.content + `\n\nError: ${event.message}`,
              isStreaming: false,
            };
            return updated;
          }
          return [
            ...state.messages,
            {
              id: `err-${Date.now()}`,
              role: "assistant" as const,
              content: `Error: ${event.message}`,
              timestamp: Date.now(),
            },
          ];
        })();
        next.set(iid, {
          ...state,
          agentRunStatus: "idle",
          messages: newMessages,
        });
        return next;
      });

      setGlobalActions((prev) => [
        ...prev.slice(-MAX_GLOBAL_ACTIONS),
        {
          id: `err-${Date.now()}`,
          instanceId: iid,
          timestamp: Date.now(),
          type: "error",
          summary: "Error",
          detail: event.message,
        },
      ]);
    }).then((u) => { if (u) unsubs.push(u); });

    api.onAgentComplete((event) => {
      const iid = event.instance_id;
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        const newMessages = (() => {
          const last = state.messages[state.messages.length - 1];
          if (last && last.role === "assistant" && last.isStreaming) {
            const updated = [...state.messages];
            updated[updated.length - 1] = { ...last, isStreaming: false };
            return updated;
          }
          return state.messages;
        })();
        next.set(iid, { ...state, agentRunStatus: "idle", messages: newMessages });
        return next;
      });

      // Refresh tree for completed instance
      refreshTreeForInstance(iid);
    }).then((u) => { if (u) unsubs.push(u); });

    api.onAgentHistoryCleared((event) => {
      const iid = event.instance_id;
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        next.set(iid, { ...state, messages: [] });
        return next;
      });
    }).then((u) => { if (u) unsubs.push(u); });

    api.onAgentShutdown((event) => {
      const iid = event.instance_id;
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        next.set(iid, {
          ...state,
          agentConnected: false,
          agentRunStatus: "idle",
        });
        return next;
      });
    }).then((u) => { if (u) unsubs.push(u); });

    return () => {
      unsubs.forEach((u) => u());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // --- Sync instanceStates with instance list ---
  useEffect(() => {
    setInstanceStates((prev) => {
      const next = new Map(prev);
      // Add entries for new instances
      for (const inst of instances) {
        if (!next.has(inst.id)) {
          next.set(inst.id, emptyInstanceState());
        }
      }
      // Remove entries for gone instances
      for (const key of next.keys()) {
        if (!instances.some((i) => i.id === key)) {
          next.delete(key);
        }
      }
      return next;
    });
  }, [instances]);

  // --- Auto-switch to grid when multiple instances, detail when first spawned ---
  useEffect(() => {
    if (instances.length > 1 && viewMode === "detail" && !selectedId) {
      setViewMode("grid");
    }
  }, [instances.length, viewMode, selectedId]);

  // --- Take-over flow ---
  const startTakeOver = useCallback(async (instanceId: string) => {
    try {
      await api.stopAgent(instanceId);
      await api.setAgentStatus(instanceId, "paused");
      // Open browser window if not already open
      const inst = instances.find((i) => i.id === instanceId);
      if (inst && !inst.browser_window_open) {
        await api.openBrowserWindow(instanceId, inst.current_url ?? undefined);
      }
      setTakeOver({ instanceId, active: true, pausedAt: Date.now() });
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(instanceId);
        if (state) {
          next.set(instanceId, { ...state, agentRunStatus: "idle" });
        }
        return next;
      });
    } catch (e) {
      console.error("Failed to start take-over:", e);
    }
  }, [instances]);

  const endTakeOver = useCallback(async (instanceId: string) => {
    try {
      await api.resumeAgent(instanceId, "Resume from where you left off.");
      await api.setAgentStatus(instanceId, "running");
      setTakeOver(null);
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(instanceId);
        if (state) {
          next.set(instanceId, { ...state, agentRunStatus: "thinking", agentConnected: true });
        }
        return next;
      });
    } catch (e) {
      console.error("Failed to end take-over:", e);
    }
  }, []);

  // --- Agent actions ---
  const sendMessage = useCallback(
    async (text: string) => {
      if (!selectedId || !agentConnected) return;
      const sending = sendingRefs.current.get(selectedId) ?? false;
      if (sending) return;

      sendingRefs.current.set(selectedId, true);
      const iid = selectedId;

      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        next.set(iid, {
          ...state,
          messages: [
            ...state.messages,
            {
              id: `user-${Date.now()}`,
              role: "user" as const,
              content: text,
              timestamp: Date.now(),
            },
          ],
        });
        return next;
      });

      try {
        await api.sendAgentMessage(iid, text);
      } catch (e) {
        setInstanceStates((prev) => {
          const next = new Map(prev);
          const state = next.get(iid);
          if (!state) return prev;
          next.set(iid, {
            ...state,
            messages: [
              ...state.messages,
              {
                id: `err-${Date.now()}`,
                role: "assistant" as const,
                content: `Failed to send: ${String(e)}`,
                timestamp: Date.now(),
              },
            ],
            agentRunStatus: "idle",
          });
          return next;
        });
      } finally {
        sendingRefs.current.set(iid, false);
      }
    },
    [selectedId, agentConnected],
  );

  const startAgentHandler = useCallback(
    async (config: AgentConfig) => {
      if (!selectedId) return;
      const iid = selectedId;
      try {
        await api.startAgent(iid, config);
        setInstanceStates((prev) => {
          const next = new Map(prev);
          const state = next.get(iid);
          if (!state) return prev;
          next.set(iid, {
            ...state,
            agentConnected: true,
            agentRunStatus: "idle",
            messages: [],
          });
          return next;
        });
      } catch (e) {
        console.error("Failed to start agent:", e);
        setError(String(e));
      }
    },
    [selectedId],
  );

  const stopAgentHandler = useCallback(async () => {
    if (!selectedId) return;
    const iid = selectedId;
    try {
      await api.stopAgent(iid);
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        next.set(iid, { ...state, agentRunStatus: "idle" });
        return next;
      });
    } catch (e) {
      console.error("Failed to stop agent:", e);
    }
  }, [selectedId]);

  const shutdownAgentHandler = useCallback(async () => {
    if (!selectedId) return;
    const iid = selectedId;
    try {
      await api.shutdownAgent(iid);
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        next.set(iid, {
          ...state,
          agentConnected: false,
          agentRunStatus: "idle",
          messages: [],
        });
        return next;
      });
    } catch (e) {
      console.error("Failed to shutdown agent:", e);
    }
  }, [selectedId]);

  const clearHistoryHandler = useCallback(async () => {
    if (!selectedId) return;
    const iid = selectedId;
    try {
      await api.clearAgentHistory(iid);
      setInstanceStates((prev) => {
        const next = new Map(prev);
        const state = next.get(iid);
        if (!state) return prev;
        next.set(iid, { ...state, messages: [] });
        return next;
      });
    } catch (e) {
      console.error("Failed to clear history:", e);
    }
  }, [selectedId]);

  const setAgentConfig = useCallback((config: AgentConfig) => {
    setAgentConfigState(config);
    saveAgentConfig(config);
  }, []);

  const select = useCallback(
    (id: string | null) => {
      setSelectedId(id);
      if (id) {
        setViewMode("detail");
      }
    },
    [],
  );

  // --- getInstanceState helper ---
  const getInstanceState = useCallback((id: string) => {
    return instanceStates.get(id);
  }, [instanceStates]);

  // --- Context value ---
  return (
    <AgentContext.Provider
      value={{
        instances,
        selectedId,
        select,
        tree,
        stats,
        events,
        refreshInstances,
        refreshTree,
        loading,
        error,
        messages,
        agentRunStatus,
        agentConfig,
        setAgentConfig,
        sendMessage,
        startAgent: startAgentHandler,
        stopAgent: stopAgentHandler,
        shutdownAgent: shutdownAgentHandler,
        clearHistory: clearHistoryHandler,
        agentConnected,

        // Phase 2
        viewMode,
        setViewMode,
        getInstanceState,
        globalActions,
        takeOver,
        startTakeOver,
        endTakeOver,
      }}
    >
      {children}
    </AgentContext.Provider>
  );
}
