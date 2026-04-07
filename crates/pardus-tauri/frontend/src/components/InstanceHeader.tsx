import { useState, useCallback, useEffect } from "react";
import { useAgent } from "../context/AgentContext";
import * as api from "../api/tauri";

const STATUS_COLORS: Record<string, string> = {
  idle: "var(--text-muted)",
  connected: "var(--accent)",
  running: "var(--green)",
  paused: "var(--yellow)",
  "waiting-challenge": "var(--orange)",
  error: "var(--red)",
};

export function InstanceHeader({
  onToggleChallenges,
}: {
  onToggleChallenges?: () => void;
}) {
  const { instances, selectedId, refreshTree, viewMode, setViewMode, takeOver, startTakeOver, endTakeOver, agentConnected } = useAgent();
  const [url, setUrl] = useState("");
  const [navigating, setNavigating] = useState(false);
  const [theme, setTheme] = useState<"dark" | "light">(() =>
    (localStorage.getItem("pardus-theme") as "dark" | "light") ??
    (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark"),
  );

  const instance = instances.find((i) => i.id === selectedId);

  useEffect(() => {
    setUrl(instance?.current_url ?? "");
  }, [selectedId, instance?.current_url]);

  const toggleTheme = useCallback(() => {
    setTheme((t) => {
      const next = t === "dark" ? "light" : "dark";
      document.documentElement.setAttribute("data-theme", next);
      localStorage.setItem("pardus-theme", next);
      return next;
    });
  }, []);

  const nav = useCallback(
    async (method: string, params: Record<string, unknown>) => {
      if (!selectedId) return;
      try {
        await api.executeCdp(selectedId, method, params);
        await refreshTree();
      } catch (e) {
        console.error(`${method} failed:`, e);
      }
    },
    [selectedId, refreshTree],
  );

  const handleNavigate = useCallback(async () => {
    if (!selectedId || !url.trim()) return;
    const target = /^https?:\/\//i.test(url.trim()) ? url.trim() : `https://${url.trim()}`;
    setNavigating(true);
    try { await nav("Page.navigate", { url: target }); } finally { setNavigating(false); }
  }, [url, nav]);

  if (!instance) {
    return (
      <header className="instance-header">
        <span className="brand">
          <span className="brand-highlight">Pardus</span>
          <span className="brand-full">Mission Control</span>
        </span>
        <button className="btn-icon theme-toggle" onClick={toggleTheme} title="Toggle theme">
          {theme === "dark" ? "\u263E" : "\u2600"}
        </button>
        <span className="header-hint">Spawn an agent to begin</span>
      </header>
    );
  }

  return (
    <header className="instance-header">
      <span className="brand">
        <span className="brand-highlight">Pardus</span>
      </span>
      <div className="nav-bar">
        <button className="nav-btn" onClick={() => nav("Page.navigate", { url: "back" })} title="Back">
          {"\u2190"}
        </button>
        <button className="nav-btn" onClick={() => nav("Page.navigate", { url: "forward" })} title="Forward">
          {"\u2192"}
        </button>
        <button className="nav-btn" onClick={() => nav("Page.reload", {})} title="Reload">
          {"\u21BB"}
        </button>
        <input
          className="nav-input"
          type="text"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleNavigate()}
          placeholder={instance.current_url ?? "Enter URL..."}
        />
        <button className="btn btn-sm" onClick={handleNavigate} disabled={navigating}>
          Go
        </button>
      </div>
      <div className="header-meta">
        <button className="btn-icon theme-toggle" onClick={toggleTheme} title="Toggle theme">
          {theme === "dark" ? "\u263E" : "\u2600"}
        </button>
        <button
          className="btn btn-sm"
          onClick={async () => {
            try { await api.openBrowserWindow(instance.id, url || undefined); }
            catch (e) { console.error("Failed to open browser:", e); }
          }}
          title="Open visual browser window"
        >
          Browser
        </button>
        {onToggleChallenges && (
          <button className="btn-icon challenge-badge" onClick={onToggleChallenges} title="Challenges">
            {"\u26A0"}
          </button>
        )}
        <span className="status-dot small pulse" style={{ backgroundColor: STATUS_COLORS[instance.agent_status] ?? "var(--text-muted)" }} />
        <span className="meta-text">{instance.agent_status}</span>
        <span className="meta-sep">{"\u00B7"}</span>
        <span className="meta-text">:{instance.port}</span>
      </div>
    </header>
  );
}
