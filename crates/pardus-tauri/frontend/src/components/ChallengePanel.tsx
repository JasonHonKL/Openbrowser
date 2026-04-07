import { useState, useEffect, useRef, useCallback } from "react";
import { useAgent } from "../context/AgentContext";
import * as api from "../api/tauri";

interface ActiveChallenge {
  url: string;
  kinds: string[];
  riskScore: number;
  instanceId: string | null;
  resolvedAt: number | null;
}

const RESOLVED_TTL = 10_000;
const MAX_CHALLENGES = 50;

export function ChallengePanel() {
  const { instances } = useAgent();
  const [challenges, setChallenges] = useState<ActiveChallenge[]>([]);
  const [collapsed, setCollapsed] = useState(false);
  const [solvingUrl, setSolvingUrl] = useState<string | null>(null);
  const mountedRef = useRef(true);
  const removeTimersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const scheduleRemoval = useCallback((url: string) => {
    const existing = removeTimersRef.current.get(url);
    if (existing) clearTimeout(existing);
    const timer = setTimeout(() => {
      if (mountedRef.current) {
        setChallenges((prev) => prev.filter((c) => c.url !== url));
      }
      removeTimersRef.current.delete(url);
    }, RESOLVED_TTL);
    removeTimersRef.current.set(url, timer);
  }, []);

  useEffect(() => {
    mountedRef.current = true;

    const unsubPromises = Promise.all([
      api.onChallengeDetected((info) => {
        if (!mountedRef.current) return;
        setCollapsed(false);
        const matchingInstance = instances.find(
          (i) => i.current_url === info.url,
        );
        setChallenges((prev) => {
          const filtered = prev.filter((c) => c.url !== info.url);
          if (filtered.length >= MAX_CHALLENGES) filtered.shift();
          return [
            ...filtered,
            {
              url: info.url,
              kinds: info.kinds,
              riskScore: info.risk_score,
              instanceId: matchingInstance?.id ?? null,
              resolvedAt: null,
            },
          ];
        });
      }),
      api.onChallengeSolved((info) => {
        if (!mountedRef.current) return;
        setChallenges((prev) =>
          prev.map((c) => (c.url === info.url ? { ...c, resolvedAt: Date.now() } : c)),
        );
        scheduleRemoval(info.url);
        setSolvingUrl(null);
      }),
      api.onChallengeFailed((info) => {
        if (!mountedRef.current) return;
        setChallenges((prev) => prev.filter((c) => c.url !== info.challenge_url));
        setSolvingUrl((prev) => prev === info.challenge_url ? null : prev);
      }),
    ]);

    return () => {
      mountedRef.current = false;
      removeTimersRef.current.forEach((t) => clearTimeout(t));
      removeTimersRef.current.clear();
      unsubPromises.then((unsubs) => unsubs.forEach((u) => u()));
    };
  }, [scheduleRemoval, instances]);

  const active = challenges.filter((c) => !c.resolvedAt);

  const handleSolve = useCallback(async (ch: ActiveChallenge) => {
    setSolvingUrl(ch.url);
    try {
      if (ch.instanceId) {
        await api.openBrowserWindow(ch.instanceId, ch.url);
      } else {
        await api.openChallengeWindow(ch.url, "Solve Challenge");
      }
    } catch (e) {
      console.error("Failed to open challenge window:", e);
      setSolvingUrl(null);
    }
  }, []);

  return (
    <div className="challenge-panel">
      <div
        className="panel-header"
        onClick={() => setCollapsed(!collapsed)}
        style={{ cursor: "pointer" }}
      >
        <span className="panel-title">Challenges</span>
        {active.length > 0 && (
          <span className="badge badge-warning">{active.length}</span>
        )}
        <button
          className="btn-icon btn-icon-sm"
          onClick={(e) => { e.stopPropagation(); setCollapsed(!collapsed); }}
          title={collapsed ? "Expand" : "Collapse"}
          style={{ marginLeft: "auto" }}
        >
          {collapsed ? "\u25B8" : "\u25BE"}
        </button>
      </div>
      {!collapsed && (
        <div className="challenge-list">
          {active.length === 0 && (
            <div className="challenge-empty">No active challenges</div>
          )}
          {active.map((ch) => (
            <div key={ch.url} className="challenge-card">
              <div className="challenge-card-header">
                <span className="challenge-icon">{"\u26A0"}</span>
                <span className="challenge-types">{ch.kinds.join(", ")}</span>
              </div>
              <div className="challenge-url" title={ch.url}>
                {ch.url}
              </div>
              <div className="challenge-risk">
                Risk: {ch.riskScore}/100
              </div>
              <button
                className="btn btn-sm btn-primary"
                style={{ marginTop: 6, width: "100%" }}
                onClick={() => handleSolve(ch)}
                disabled={solvingUrl === ch.url}
              >
                {solvingUrl === ch.url ? "Opening..." : "Open to Solve"}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
