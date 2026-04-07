import { useCallback, useState } from "react";
import { useAgent } from "../context/AgentContext";
import * as api from "../api/tauri";

export function TakeOverBar() {
  const { takeOver, endTakeOver, instances } = useAgent();
  const [resuming, setResuming] = useState(false);

  const inst = instances.find((i) => i.id === takeOver?.instanceId);

  const handleResume = useCallback(async () => {
    if (!takeOver?.instanceId) return;
    setResuming(true);
    try {
      await endTakeOver(takeOver.instanceId);
    } finally {
      setResuming(false);
    }
  }, [takeOver?.instanceId, endTakeOver]);

  const handleOpenBrowser = useCallback(async () => {
    if (!takeOver?.instanceId || !inst) return;
    await api.openBrowserWindow(
      takeOver.instanceId,
      inst.current_url ?? undefined,
    );
  }, [takeOver?.instanceId, inst]);

  if (!takeOver?.active) return null;

  return (
    <div className="take-over-bar">
      <div className="take-over-bar-info">
        <span className="take-over-bar-icon">{"\u26A0"}</span>
        <span className="take-over-bar-text">
          Manual Control Active &mdash; Agent Paused
        </span>
        {takeOver.pausedAt && (
          <span className="take-over-bar-duration">
            for {Math.round((Date.now() - takeOver.pausedAt) / 1000)}s
          </span>
        )}
      </div>
      <div className="take-over-bar-actions">
        <button
          className="btn btn-sm"
          onClick={handleOpenBrowser}
          title="Open browser window for manual interaction"
        >
          Open Browser
        </button>
        <button
          className="btn btn-sm btn-primary"
          onClick={handleResume}
          disabled={resuming}
        >
          {resuming ? "Resuming..." : "Resume Agent"}
        </button>
      </div>
    </div>
  );
}
