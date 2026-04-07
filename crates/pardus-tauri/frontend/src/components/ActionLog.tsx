import { useRef, useEffect, useState } from "react";
import type { CdpEvent } from "../types";
import { classifyEvent, formatTime, TYPE_COLORS, type ActionEntry } from "../utils/classifyEvent";

const TYPE_ICONS: Record<ActionEntry["type"], string> = {
  navigate: "\u2192",
  action_start: "\u25B6",
  action_complete: "\u2713",
  action_fail: "\u2717",
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
