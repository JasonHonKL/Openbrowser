import { useState, useCallback } from "react";
import { useAgent } from "../context/AgentContext";
import { AgentCard } from "./AgentCard";
import { GlobalActionStream } from "./GlobalActionStream";
import * as api from "../api/tauri";

export function AgentGrid() {
  const { instances, refreshInstances, select } = useAgent();
  const [spawning, setSpawning] = useState(false);

  const handleSpawn = useCallback(async () => {
    setSpawning(true);
    try {
      const inst = await api.spawnInstance();
      await api.connectInstance(inst.id);
      await refreshInstances();
      select(inst.id); // goes to detail view
    } catch (e) {
      console.error("Spawn failed:", e);
    } finally {
      setSpawning(false);
    }
  }, [refreshInstances, select]);

  return (
    <div className="agent-grid">
      {/* Header */}
      <div className="agent-grid-header">
        <div className="agent-grid-title-row">
          <h2 className="agent-grid-title">
            Mission Control
          </h2>
          <span className="agent-grid-count">
            {instances.length} agent{instances.length !== 1 ? "s" : ""}
          </span>
        </div>
        <button
          className="btn btn-primary btn-sm"
          onClick={handleSpawn}
          disabled={spawning}
        >
          {spawning ? "Spawning..." : "+ Spawn Agent"}
        </button>
      </div>

      {/* Grid of cards */}
      {instances.length === 0 ? (
        <div className="agent-grid-empty">
          <div className="agent-grid-empty-icon">{"\u{1F578}"}</div>
          <div className="agent-grid-empty-title">No agents running</div>
          <div className="agent-grid-empty-subtitle">
            Spawn a browser instance to get started.
          </div>
          <button
            className="btn btn-primary"
            style={{ marginTop: 16 }}
            onClick={handleSpawn}
            disabled={spawning}
          >
            {spawning ? "Spawning..." : "Spawn First Agent"}
          </button>
        </div>
      ) : (
        <div className="agent-grid-cards">
          {instances.map((inst, i) => (
            <AgentCard key={inst.id} instance={inst} index={i} />
          ))}
        </div>
      )}

      {/* Global action stream */}
      {instances.length > 0 && <GlobalActionStream />}
    </div>
  );
}
