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
} from "../types";
import * as api from "../api/tauri";

interface AgentContextValue {
  instances: InstanceInfo[];
  selectedId: string | null;
  select: (id: string | null) => void;
  tree: SemanticNode | null;
  stats: TreeStats | null;
  events: CdpEvent[];
  refreshInstances: () => Promise<void>;
  refreshTree: () => Promise<void>;
  loading: boolean;
}

const AgentContext = createContext<AgentContextValue | null>(null);

export function useAgent(): AgentContextValue {
  const ctx = useContext(AgentContext);
  if (!ctx) throw new Error("useAgent must be used within AgentProvider");
  return ctx;
}

const MAX_EVENTS = 1000;

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

export function AgentProvider({ children }: { children: ReactNode }) {
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [tree, setTree] = useState<SemanticNode | null>(null);
  const [stats, setStats] = useState<TreeStats | null>(null);
  const [events, setEvents] = useState<CdpEvent[]>([]);
  const [loading, setLoading] = useState(false);
  const eventsRef = useRef<CdpEvent[]>([]);
  const mountedRef = useRef(true);

  const refreshInstances = useCallback(async () => {
    try {
      const list = await api.listInstances();
      setInstances(list);
    } catch {
      console.error("Failed to refresh instances");
    }
  }, []);

  const refreshTree = useCallback(async () => {
    if (!selectedId) return;
    try {
      setLoading(true);
      const result = await api.getSemanticTree(selectedId);
      setTree(result.semanticTree.root);
      setStats(result.semanticTree.stats);
    } catch {
      setTree(null);
      setStats(null);
    } finally {
      setLoading(false);
    }
  }, [selectedId]);

  const debouncedRefreshTree = useRef(
    debounce(() => {
      refreshTree();
    }, 300),
  );

  useEffect(() => {
    refreshInstances();
    const interval = setInterval(refreshInstances, 3000);
    return () => clearInterval(interval);
  }, [refreshInstances]);

  useEffect(() => {
    if (!selectedId) {
      setTree(null);
      setStats(null);
      setEvents([]);
      eventsRef.current = [];
      return;
    }

    mountedRef.current = true;
    refreshTree();

    const unsubPromises = Promise.all([
      api.onCdpEvent((event) => {
        if (!mountedRef.current) return;
        if (event.instance_id !== selectedId) return;

        const updated = [...eventsRef.current, event];
        if (updated.length > MAX_EVENTS) {
          updated.splice(0, updated.length - MAX_EVENTS);
        }
        eventsRef.current = updated;
        setEvents(updated);

        if (event.method === "Page.frameNavigated" || event.method.startsWith("Pardus.action")) {
          debouncedRefreshTree.current();
        }
      }),
      api.onAgentStatusChanged((change) => {
        if (!mountedRef.current) return;
        if (change.instance_id === selectedId) {
          refreshInstances();
        }
      }),
    ]);

    return () => {
      mountedRef.current = false;
      unsubPromises.then((unsubs) => {
        if (unsubs.length > 0) {
          unsubs.forEach((u) => u());
        }
      });
    };
  }, [selectedId, refreshInstances, refreshTree]);

  const select = useCallback(
    (id: string | null) => {
      setSelectedId(id);
      setEvents([]);
      eventsRef.current = [];
    },
    [],
  );

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
      }}
    >
      {children}
    </AgentContext.Provider>
  );
}
