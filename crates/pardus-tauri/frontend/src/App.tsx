import {
  createContext,
  useContext,
  useState,
  useCallback,
  useEffect,
  type ReactNode,
} from "react";
import { AgentProvider, useAgent } from "./context/AgentContext";
import { AgentSidebar } from "./components/AgentSidebar";
import { InstanceHeader } from "./components/InstanceHeader";
import { TreeViewer } from "./components/TreeViewer";
import { ActionLog } from "./components/ActionLog";
import { ChatPanel } from "./components/ChatPanel";
import { InteractionBar } from "./components/InteractionBar";
import { ChallengePanel } from "./components/ChallengePanel";
import { AgentGrid } from "./components/AgentGrid";
import { TakeOverBar } from "./components/TakeOverBar";
import * as api from "./api/tauri";

// ── Theme ──

type Theme = "dark" | "light";

function useTheme(): [Theme, () => void] {
  const [theme, setTheme] = useState<Theme>(() => {
    const saved = localStorage.getItem("pardus-theme") as Theme | null;
    return saved ?? (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark");
  });

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("pardus-theme", theme);
  }, [theme]);

  return [theme, () => setTheme((t) => (t === "dark" ? "light" : "dark"))];
}

// ── Toast ──

interface Toast { id: string; message: string; type: "error" | "success" | "info"; exiting?: boolean }

const ToastContext = createContext<{ addToast: (m: string, t?: Toast["type"]) => void } | null>(null);
export function useToast() {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToast must be used within ToastProvider");
  return ctx;
}

function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const addToast = useCallback((message: string, type: Toast["type"] = "info") => {
    const id = `${Date.now()}-${Math.random()}`;
    setToasts((prev) => [...prev.slice(-4), { id, message, type }]);
    setTimeout(() => {
      setToasts((p) => p.map((t) => (t.id === id ? { ...t, exiting: true } : t)));
      setTimeout(() => setToasts((p) => p.filter((t) => t.id !== id)), 200);
    }, 3000);
  }, []);

  return (
    <ToastContext.Provider value={{ addToast }}>
      {children}
      <div className="toast-container">
        {toasts.map((t) => (
          <div
            key={t.id}
            className={`toast toast-${t.type} ${t.exiting ? "toast-exit" : ""}`}
            onClick={() => setToasts((p) => p.filter((x) => x.id !== t.id))}
          >
            {t.message}
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

// ── Welcome ──

function WelcomeState() {
  const { instances, select, refreshInstances } = useAgent();
  const { addToast } = useToast();
  const [spawning, setSpawning] = useState(false);

  const handleQuickSpawn = useCallback(async () => {
    setSpawning(true);
    try {
      const inst = await api.spawnInstance();
      await api.connectInstance(inst.id);
      await refreshInstances();
      select(inst.id);
      addToast("Agent spawned and connected", "success");
    } catch (e) {
      addToast(`Failed to spawn: ${e}`, "error");
    } finally {
      setSpawning(false);
    }
  }, [select, refreshInstances, addToast]);

  if (instances.length > 0) {
    return (
      <div className="welcome">
        <div className="welcome-title">Select an agent</div>
        <div className="welcome-subtitle">
          Choose an agent from the sidebar or spawn a new one.
        </div>
      </div>
    );
  }

  return (
    <div className="welcome">
      <div className="welcome-icon">{"\u{1F578}"}</div>
      <div className="welcome-title">
        <span className="brand-highlight">Pardus</span> Mission Control
      </div>
      <div className="welcome-subtitle">
        Headless browser for AI agents. Spawn an instance, navigate to a page,
        and interact through the semantic tree or chat.
      </div>
      <div className="welcome-steps">
        {[["1", "Spawn a browser instance"], ["2", "Navigate to a URL"], ["3", "Interact via tree or chat"]].map(
          ([num, text]) => (
            <div key={num} className="welcome-step">
              <div className="welcome-step-num">{num}</div>
              <div className="welcome-step-text">{text}</div>
            </div>
          ),
        )}
      </div>
      <button
        className="btn btn-primary"
        style={{ marginTop: 16, padding: "10px 28px", fontSize: 14 }}
        onClick={handleQuickSpawn}
        disabled={spawning}
      >
        {spawning ? "Spawning..." : "Spawn First Agent"}
      </button>
    </div>
  );
}

// ── Dashboard ──

function Dashboard() {
  const { selectedId, events, viewMode, takeOver } = useAgent();
  const [showChallenges, setShowChallenges] = useState(false);

  return (
    <div className="app">
      <InstanceHeader
        onToggleChallenges={() => setShowChallenges(!showChallenges)}
      />
      <main className="app-main">
        <AgentSidebar />
        <div className="center">
          {viewMode === "grid" ? (
            <AgentGrid />
          ) : selectedId ? (
            <>
              {takeOver?.active && takeOver.instanceId === selectedId && (
                <TakeOverBar />
              )}
              <div className="panel-tree">
                <TreeViewer />
              </div>
              <div className="panel-repl">
                <InteractionBar />
              </div>
              <div className="panel-log">
                <ActionLog events={events} />
              </div>
              <ChatPanel />
            </>
          ) : (
            <WelcomeState />
          )}
        </div>
      </main>
      {showChallenges && (
        <div className="challenge-overlay">
          <ChallengePanel />
        </div>
      )}
    </div>
  );
}

export function App() {
  useTheme(); // initialises data-theme on <html>
  return (
    <ToastProvider>
      <AgentProvider>
        <Dashboard />
      </AgentProvider>
    </ToastProvider>
  );
}
