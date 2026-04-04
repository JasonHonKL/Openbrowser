import { useAgent } from "../context/AgentContext";
import * as api from "../api/tauri";
import { useState, useCallback } from "react";

const STATUS_COLORS: Record<string, string> = {
  idle: "var(--text-muted)",
  connected: "var(--accent)",
  running: "var(--green)",
  paused: "var(--yellow)",
  "waiting-challenge": "var(--orange)",
  error: "var(--red)",
};

const STATUS_LABELS: Record<string, string> = {
  idle: "Idle",
  connected: "Ready",
  running: "Running",
  paused: "Paused",
  "waiting-challenge": "CAPTCHA",
  error: "Error",
};

export function AgentSidebar() {
  const { instances, selectedId, select, refreshInstances } = useAgent();
  const [spawning, setSpawning] = useState(false);

  const handleSpawn = useCallback(async () => {
    setSpawning(true);
    try {
      const inst = await api.spawnInstance();
      await api.connectInstance(inst.id);
      await refreshInstances();
      select(inst.id);
    } catch (e) {
      console.error("Failed to spawn:", e);
    } finally {
      setSpawning(false);
    }
  }, [refreshInstances, select]);

  const handleKill = useCallback(
    async (id: string) => {
      try {
        await api.disconnectInstance(id);
        await api.killInstance(id);
        await refreshInstances();
        if (selectedId === id) select(null);
      } catch (e) {
        console.error("Failed to kill:", e);
      }
    },
    [selectedId, select, refreshInstances],
  );

  const handleOpenBrowser = useCallback(
    async (id: string) => {
      try {
        await api.openBrowserWindow(id);
        await refreshInstances();
      } catch (e) {
        console.error("Failed to open browser:", e);
      }
    },
    [refreshInstances],
  );

  return (
    <aside className="sidebar sidebar-left">
      <div className="panel-header">
        <span className="panel-title">Agents</span>
        <button
          className="btn btn-sm btn-primary"
          onClick={handleSpawn}
          disabled={spawning}
        >
          {spawning ? "..." : "+ Spawn"}
        </button>
      </div>
      <div className="agent-list">
        {instances.length === 0 && (
          <div className="agent-empty">
            No agents running. Click <strong>+ Spawn</strong> to start.
          </div>
        )}
        {instances.map((inst) => (
          <div
            key={inst.id}
            className={`agent-card ${selectedId === inst.id ? "agent-card-selected" : ""}`}
            onClick={() => select(inst.id)}
          >
            <div className="agent-card-header">
              <span
                className="status-dot"
                style={{
                  backgroundColor: STATUS_COLORS[inst.agent_status] ?? "var(--text-muted)",
                }}
              />
              <span className="agent-card-id">{inst.id}</span>
              <span className="agent-card-status">
                {STATUS_LABELS[inst.agent_status] ?? inst.agent_status}
              </span>
              <button
                className="btn-icon btn-icon-sm"
                title="Open browser window"
                onClick={(e) => {
                  e.stopPropagation();
                  handleOpenBrowser(inst.id);
                }}
              >
                &#x1F578;
              </button>
              <button
                className="btn-icon btn-icon-sm"
                title="Kill instance"
                onClick={(e) => {
                  e.stopPropagation();
                  handleKill(inst.id);
                }}
              >
            x
              </button>
            </div>
            {inst.current_url && (
              <div className="agent-card-url" title={inst.current_url}>
                {inst.current_url}
              </div>
            )}
          </div>
        ))}
      </div>
    </aside>
  );
}
