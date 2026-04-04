import { useRef, useEffect, useState } from "react";
import type { CdpEvent } from "../types";

interface ActionEntry {
  id: string;
  timestamp: number;
  type: "navigate" | "action_start" | "action_complete" | "action_fail";
  summary: string;
  detail?: string;
}

function classifyEvent(event: CdpEvent): ActionEntry | null {
  const { method, params, timestamp } = event;

  if (method === "Page.frameNavigated") {
    const frame = (params as { frame?: { url?: string } })?.frame;
    const url = frame?.url ?? "unknown";
    return {
      id: `nav-${timestamp}`,
      timestamp,
      type: "navigate",
      summary: "Navigate",
      detail: url,
    };
  }

  if (method === "Pardus.actionStarted") {
    const p = params as { action?: string; target?: { selector?: string } };
    const action = p?.action ?? "unknown";
    const selector = p?.target?.selector ?? "";
    return {
      id: `act-s-${timestamp}`,
      timestamp,
      type: "action_start",
      summary: action.charAt(0).toUpperCase() + action.slice(1),
      detail: selector,
    };
  }

  if (method === "Pardus.actionCompleted") {
    const p = params as { action?: string; result?: { note?: string } };
    const action = p?.action ?? "unknown";
    const note = p?.result?.note ?? "";
    return {
      id: `act-c-${timestamp}`,
      timestamp,
      type: "action_complete",
      summary: `${action} done`,
      detail: note || undefined,
    };
  }

  if (method === "Pardus.actionFailed") {
    const p = params as { action?: string; result?: { error?: string } };
    const action = p?.action ?? "unknown";
    const error = p?.result?.error ?? "unknown error";
    return {
      id: `act-f-${timestamp}`,
      timestamp,
      type: "action_fail",
      summary: `${action} failed`,
      detail: error,
    };
  }

  return null;
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString("en-US", { hour12: false, hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

const TYPE_ICONS: Record<ActionEntry["type"], string> = {
  navigate: "\u2192",
  action_start: "\u25B6",
  action_complete: "\u2713",
  action_fail: "\u2717",
};

const TYPE_COLORS: Record<ActionEntry["type"], string> = {
  navigate: "var(--accent)",
  action_start: "var(--cyan)",
  action_complete: "var(--green)",
  action_fail: "var(--red)",
};

export function ActionLog({ events }: { events: CdpEvent[] }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  const entries: ActionEntry[] = events
    .map(classifyEvent)
    .filter((e): e is ActionEntry => e !== null);

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries.length, autoScroll]);

  return (
    <div className="action-log">
      <div className="action-log-toolbar">
        <span className="panel-title">Action Log</span>
        <span className="log-count">{entries.length}</span>
        <label className="auto-scroll-label">
          <input
            type="checkbox"
            checked={autoScroll}
            onChange={(e) => setAutoScroll(e.target.checked)}
          />
          auto-scroll
        </label>
      </div>
      <div className="action-log-entries" ref={scrollRef}>
        {entries.length === 0 && (
          <div className="log-empty">No actions recorded yet.</div>
        )}
        {entries.map((entry) => (
          <div key={entry.id} className={`log-entry log-${entry.type}`}>
            <span className="log-time">{formatTime(entry.timestamp)}</span>
            <span className="log-icon" style={{ color: TYPE_COLORS[entry.type] }}>
              {TYPE_ICONS[entry.type]}
            </span>
            <span className="log-summary">{entry.summary}</span>
            {entry.detail && (
              <span className="log-detail" title={entry.detail}>
                {entry.detail}
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
