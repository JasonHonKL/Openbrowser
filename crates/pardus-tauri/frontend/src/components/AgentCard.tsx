import { useCallback } from "react";
import type { InstanceInfo, InstanceState } from "../types";
import { useAgent } from "../context/AgentContext";
import * as api from "../api/tauri";
import { classifyEvent, formatTime, getInstanceColor } from "../utils/classifyEvent";

const STATUS_MAP: Record<string, [string, string]> = {
  idle:               ["Idle",     "var(--text-muted)"],
  connected:          ["Ready",    "var(--accent)"],
  running:            ["Running",  "var(--green)"],
  paused:             ["Paused",   "var(--yellow)"],
  "waiting-challenge":["CAPTCHA",  "var(--orange)"],
  error:              ["Error",    "var(--red)"],
  thinking:           ["Thinking", "var(--purple)"],
  executing_tool:     ["Working",  "var(--cyan)"],
};

interface AgentCardProps {
  instance: InstanceInfo;
  index: number;
}

export function AgentCard({ instance, index }: AgentCardProps) {
  const { select, getInstanceState, startTakeOver, instances } = useAgent();
  const state: InstanceState | undefined = getInstanceState(instance.id);

  const status = state?.agentRunStatus ?? instance.agent_status;
  const [label, color] = STATUS_MAP[status] ?? [status, "var(--text-muted)"];
  const accentColor = getInstanceColor(index);

  // Derive last action from events
  const lastAction = (() => {
    if (!state?.events.length) return null;
    for (let i = state.events.length - 1; i >= 0; i--) {
      const entry = classifyEvent(state.events[i]);
      if (entry) return entry;
    }
    return null;
  })();

  const isRunning = state?.agentConnected && state?.agentRunStatus !== "idle";

  const handleTakeOver = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      await startTakeOver(instance.id);
    },
    [instance.id, startTakeOver],
  );

  const handleOpenBrowser = useCallback(
    async (e: React.MouseEvent) => {
      e.stopPropagation();
      await api.openBrowserWindow(instance.id, instance.current_url ?? undefined);
    },
    [instance.id, instance.current_url],
  );

  return (
    <div
      className="agent-card-grid"
      style={{ "--card-accent": accentColor } as React.CSSProperties}
      onClick={() => select(instance.id)}
    >
      {/* Status bar */}
      <div className="agent-card-grid-header">
        <div className="agent-card-grid-status">
          <span
            className={`status-dot ${isRunning ? "pulse" : ""}`}
            style={{ backgroundColor: color }}
          />
          <span className="agent-card-grid-label">{label}</span>
        </div>
        <span className="agent-card-grid-id">{instance.id}</span>
      </div>

      {/* URL */}
      <div className="agent-card-grid-url">
        {instance.current_url ? (
          <span title={instance.current_url}>
            {instance.current_url.replace(/^https?:\/\//, "").slice(0, 40)}
          </span>
        ) : (
          <span className="text-muted">No URL</span>
        )}
      </div>

      {/* Last action */}
      {lastAction && (
        <div className="agent-card-grid-last-action">
          <span className="agent-card-grid-last-time">
            {formatTime(lastAction.timestamp)}
          </span>
          <span className="agent-card-grid-last-summary">
            {lastAction.summary}
          </span>
          {lastAction.detail && (
            <span className="agent-card-grid-last-detail" title={lastAction.detail}>
              {lastAction.detail.slice(0, 30)}
            </span>
          )}
        </div>
      )}

      {/* Actions */}
      <div className="agent-card-grid-actions">
        {isRunning && (
          <button
            className="btn btn-sm btn-warning"
            onClick={handleTakeOver}
            title="Pause agent and take manual control"
          >
            Take Over
          </button>
        )}
        <button
          className="btn btn-sm"
          onClick={handleOpenBrowser}
          title="Open browser window"
        >
          Browser
        </button>
      </div>
    </div>
  );
}
