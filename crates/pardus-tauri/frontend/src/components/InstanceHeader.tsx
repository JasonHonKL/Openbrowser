import { useState, useCallback, useEffect } from "react";
import { useAgent } from "../context/AgentContext";
import * as api from "../api/tauri";

export function InstanceHeader() {
  const { instances, selectedId, refreshTree } = useAgent();
  const [url, setUrl] = useState("");
  const [navigating, setNavigating] = useState(false);

  const instance = instances.find((i) => i.id === selectedId);

  useEffect(() => {
    if (instance?.current_url) {
      setUrl(instance.current_url);
    } else {
      setUrl("");
    }
  }, [selectedId, instance?.current_url]);

  const handleNavigate = useCallback(async () => {
    if (!selectedId || !url.trim()) return;
    let target = url.trim();
    if (!/^https?:\/\//i.test(target)) {
      target = "https://" + target;
    }
    setNavigating(true);
    try {
      await api.executeCdp(selectedId, "Page.navigate", { url: target });
      await refreshTree();
    } catch (e) {
      console.error("Navigate failed:", e);
    } finally {
      setNavigating(false);
    }
  }, [selectedId, url, refreshTree]);

  const handleReload = useCallback(async () => {
    if (!selectedId) return;
    try {
      await api.executeCdp(selectedId, "Page.reload", {});
      await refreshTree();
    } catch (e) {
      console.error("Reload failed:", e);
    }
  }, [selectedId, refreshTree]);

  if (!instance) {
    return (
      <header className="instance-header">
        <span className="brand">Pardus Mission Control</span>
        <span className="header-hint">Spawn an agent to begin</span>
      </header>
    );
  }

  return (
    <header className="instance-header">
      <span className="brand">Pardus</span>
      <div className="nav-bar">
        <input
          className="nav-input"
          type="text"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleNavigate()}
          placeholder={instance.current_url ?? "Enter URL..."}
        />
        <button
          className="btn btn-sm"
          onClick={handleNavigate}
          disabled={navigating}
        >
          Go
        </button>
        <button className="btn-icon btn-sm" onClick={handleReload} title="Reload">
          {"\u21BB"}
        </button>
      </div>
      <div className="header-meta">
        <button
          className="btn btn-sm"
          onClick={async () => {
            try {
              await api.openBrowserWindow(instance.id, url || undefined);
            } catch (e) {
              console.error("Failed to open browser:", e);
            }
          }}
          title="Open visual browser window"
        >
          Browser
        </button>
        <span className="status-dot small" style={{
          backgroundColor:
            instance.agent_status === "running"
              ? "var(--green)"
              : instance.agent_status === "waiting-challenge"
                ? "var(--orange)"
                : instance.agent_status === "error"
                  ? "var(--red)"
                  : "var(--text-muted)",
        }} />
        <span className="meta-text">{instance.agent_status}</span>
        <span className="meta-sep">|</span>
        <span className="meta-text">:{instance.port}</span>
      </div>
    </header>
  );
}
