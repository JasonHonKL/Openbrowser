import { useState, useEffect, useRef, useCallback } from "react";
import type { ChallengeInfo } from "../types";
import * as api from "../api/tauri";

interface ActiveChallenge {
  url: string;
  kinds: string[];
  riskScore: number;
  resolvedAt: number | null;
}

const RESOLVED_TTL = 10_000;
const MAX_CHALLENGES = 50;

export function ChallengePanel() {
  const [challenges, setChallenges] = useState<ActiveChallenge[]>([]);
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
        setChallenges((prev) => {
          const filtered = prev.filter((c) => c.url !== info.url);
          if (filtered.length >= MAX_CHALLENGES) filtered.shift();
          return [
            ...filtered,
            { url: info.url, kinds: info.kinds, riskScore: info.risk_score, resolvedAt: null },
          ];
        });
      }),
      api.onChallengeSolved((info) => {
        if (!mountedRef.current) return;
        setChallenges((prev) =>
          prev.map((c) => (c.url === info.url ? { ...c, resolvedAt: Date.now() } : c)),
        );
        scheduleRemoval(info.url);
      }),
      api.onChallengeFailed((info) => {
        if (!mountedRef.current) return;
        setChallenges((prev) => prev.filter((c) => c.url !== info.challenge_url));
      }),
    ]);

    return () => {
      mountedRef.current = false;
      removeTimersRef.current.forEach((t) => clearTimeout(t));
      removeTimersRef.current.clear();
      unsubPromises.then((unsubs) => unsubs.forEach((u) => u()));
    };
  }, [scheduleRemoval]);

  const active = challenges.filter((c) => !c.resolvedAt);

  return (
    <div className="challenge-panel">
      <div className="panel-header">
        <span className="panel-title">Challenges</span>
        {active.length > 0 && (
          <span className="badge badge-warning">{active.length} active</span>
        )}
      </div>
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
              onClick={() => api.openChallengeWindow(ch.url)}
            >
              Open to Solve
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
