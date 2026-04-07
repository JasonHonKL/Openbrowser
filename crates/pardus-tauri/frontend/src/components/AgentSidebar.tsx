import { useState, useCallback } from "react";
import { useAgent } from "../context/AgentContext";
import * as api from "../api/tauri";

const STATUS_MAP: Record<string, [string, string]> = {
  idle:              ["Idle",    "var(--text-muted)"],
  connected:         ["Ready",   "var(--accent)"],
  running:           ["Running", "var(--green)"],
  paused:            ["Paused",  "var(--yellow)"],
  "waiting-challenge":["CAPTCHA","var(--orange)"],
  error:             ["Error",   "var(--red)"],
  thinking:          ["Thinking","var(--purple)"],
  executing_tool:    ["Working", "var(--cyan)"],
};

export function AgentSidebar() {
  const { instances, selectedId, select, refreshInstances, agentConnected, agentRunStatus, viewMode, setViewMode } = useAgent();
  const [spawning, setSpawning] = useState(false);
  const [confirmKillId, setConfirmKillId] = useState<string | null>(null);

  const spawn = useCallback(async () => {
    setSpawning(true);
    try {
      const inst = await api.spawnInstance();
      await api.connectInstance(inst.id);
      await refreshInstances();
      select(inst.id);
    } catch (e) { console.error("Spawn failed:", e); }
    finally { setSpawning(false); }
  }, [refreshInstances, select]);

  const kill = useCallback(async (id: string) => {
    if (confirmKillId !== id) { setConfirmKillId(id); setTimeout(() => setConfirmKillId(null), 3000); return; }
    setConfirmKillId(null);
    try {
      await api.disconnectInstance(id);
      await api.killInstance(id);
      await refreshInstances();
      if (selectedId === id) select(null);
    } catch (e) { console.error("Kill failed:", e); }
  }, [selectedId, select, refreshInstances, confirmKillId]);

  const killAll = useCallback(async () => {
    try { await api.killAllInstances(); await refreshInstances(); select(null); }
    catch (e) { console.error("Kill all failed:", e); }
  }, [refreshInstances, select]);

  const openBrowser = useCallback(async (id: string) => {
    try { await api.openBrowserWindow(id); await refreshInstances(); }
    catch (e) { console.error("Open browser failed:", e); }
  }, [refreshInstances]);

  return (
    <aside className="sidebar">
      <div className="panel-header">
        <span className="panel-title">Agents</span>
        <div className="view-mode-toggle">
          <button
            className={`view-mode-btn ${viewMode === "grid" ? "active" : ""}`}
            onClick={() => setViewMode("grid")}
            title="Grid view"
          >
            {"\u25A6"}
          </button>
          <button
            className={`view-mode-btn ${viewMode === "detail" ? "active" : ""}`}
            onClick={() => setViewMode("detail")}
            title="Detail view"
          >
            {"\u2503"}
          </button>
        </div>
        {instances.length > 0 && (
          <button className="btn-icon btn-icon-sm btn-danger" title="Kill all" onClick={killAll}>
            {"\u{1F5D1}"}
          </button>
        )}
        <button className="btn btn-sm btn-primary" onClick={spawn} disabled={spawning}>
          {spawning ? "..." : "+ Spawn"}
        </button>
      </div>
      <div className="agent-list">
        {instances.length === 0 && (
          <div className="agent-empty">
            No agents running.<br />
            Click <strong>+ Spawn</strong> to start.
          </div>
        )}
        {instances.map((inst) => {
          const isActive = selectedId === inst.id && agentConnected && agentRunStatus !== "idle";
          const status = isActive ? agentRunStatus : inst.agent_status;
          const [label, color] = STATUS_MAP[status] ?? [status, "var(--text-muted)"];
          const killing = confirmKillId === inst.id;

          return (
            <div
              key={inst.id}
              className={`agent-card ${selectedId === inst.id ? "agent-card-selected" : ""}`}
              onClick={() => select(inst.id)}
            >
              <div className="agent-card-header">
                <span className={`status-dot ${isActive ? "pulse" : ""}`} style={{ backgroundColor: color }} />
                <span className="agent-card-id">{inst.id}</span>
                <span className="agent-card-status">{label}</span>
                <div className="agent-card-actions">
                  <button className="btn-icon btn-icon-sm" title="Open browser"
                    onClick={(e) => { e.stopPropagation(); openBrowser(inst.id); }}>
                    {"\u{1F578}"}
                  </button>
                  <button className={`btn-icon btn-icon-sm ${killing ? "btn-danger" : ""}`}
                    title={killing ? "Click again to confirm" : "Kill"}
                    onClick={(e) => { e.stopPropagation(); kill(inst.id); }}>
                    {killing ? "!" : "\u00D7"}
                  </button>
                </div>
              </div>
              {inst.current_url && (
                <div className="agent-card-url" title={inst.current_url}>{inst.current_url}</div>
              )}
            </div>
          );
        })}
      </div>
    </aside>
  );
}
