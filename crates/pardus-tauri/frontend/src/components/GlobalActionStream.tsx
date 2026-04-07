import { useRef, useEffect, useState } from "react";
import { useAgent } from "../context/AgentContext";
import {
  formatTime,
  getInstanceColor,
} from "../utils/classifyEvent";

const TYPE_ICONS: Record<string, string> = {
  navigate: "\u2192",
  action_start: "\u25B6",
  action_complete: "\u2713",
  action_fail: "\u2717",
  thinking: "\u{1F4AD}",
  tool_call: "\u{1F527}",
  tool_result: "\u2713",
  error: "\u2717",
};

export function GlobalActionStream() {
  const { globalActions, instances, select, setViewMode } = useAgent();
  const scrollRef = useRef<HTMLDivElement>(null);
  const [autoScroll, setAutoScroll] = useState(true);

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [globalActions.length, autoScroll]);

  // Build instance index lookup for color assignment
  const instanceIndexMap = new Map(
    instances.map((inst, i) => [inst.id, i]),
  );

  const handleClick = (instanceId: string) => {
    select(instanceId);
    setViewMode("detail");
  };

  return (
    <div className="global-action-stream">
      <div className="global-action-stream-header">
        <span className="panel-title">Action Stream</span>
        <span className="log-count">{globalActions.length}</span>
        <label className="auto-scroll-label">
          <input
            type="checkbox"
            checked={autoScroll}
            onChange={(e) => setAutoScroll(e.target.checked)}
          />
          auto-scroll
        </label>
      </div>
      <div className="global-action-stream-entries" ref={scrollRef}>
        {globalActions.length === 0 && (
          <div className="log-empty">No actions yet.</div>
        )}
        {globalActions.map((action) => {
          const color = getInstanceColor(instanceIndexMap.get(action.instanceId) ?? 0);
          return (
            <div
              key={action.id}
              className={`global-action-entry gae-${action.type}`}
              onClick={() => handleClick(action.instanceId)}
              title={action.detail}
            >
              <span
                className="gae-instance-dot"
                style={{ backgroundColor: color }}
              />
              <span className="gae-instance-id">
                {action.instanceId.replace("instance-", "").slice(0, 4)}
              </span>
              <span className="gae-time">{formatTime(action.timestamp)}</span>
              <span className="gae-icon">{TYPE_ICONS[action.type] ?? ""}</span>
              <span className="gae-summary">{action.summary}</span>
              {action.detail && (
                <span className="gae-detail">{action.detail.slice(0, 40)}</span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
