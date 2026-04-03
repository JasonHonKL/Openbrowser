import { useState, type FormEvent } from "react";
import type { TabInfo } from "../api/client";
import { api } from "../api/client";

interface Props {
  tabs: TabInfo[];
  activeId: number | null;
  onChange: () => void;
}

export function TabBar({ tabs, activeId, onChange }: Props) {
  const [newUrl, setNewUrl] = useState("");
  const [showNew, setShowNew] = useState(false);

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault();
    if (!newUrl.trim()) return;
    try {
      await api.createTab(newUrl.trim());
      setNewUrl("");
      setShowNew(false);
      onChange();
    } catch (err) {
      console.error("Create tab failed:", err);
    }
  };

  const handleClose = async (id: number) => {
    try {
      await api.closeTab(id);
      onChange();
    } catch (err) {
      console.error("Close tab failed:", err);
    }
  };

  const handleActivate = async (id: number) => {
    try {
      await api.activateTab(id);
      onChange();
    } catch (err) {
      console.error("Activate tab failed:", err);
    }
  };

  return (
    <div className="tabbar">
      {tabs.map((tab) => (
        <div
          key={tab.id}
          className={`tab ${tab.id === activeId ? "active" : ""}`}
          onClick={() => handleActivate(tab.id)}
        >
          <span className="tab-title">{tab.title ?? tab.url}</span>
          <span className={`tab-state state-${tab.state.toLowerCase()}`} />
          <button
            className="tab-close"
            onClick={(e) => {
              e.stopPropagation();
              handleClose(tab.id);
            }}
          >
            &times;
          </button>
        </div>
      ))}
      {showNew ? (
        <form className="tab-new-form" onSubmit={handleCreate}>
          <input
            autoFocus
            className="tab-new-input"
            placeholder="URL..."
            value={newUrl}
            onChange={(e) => setNewUrl(e.target.value)}
            onBlur={() => setShowNew(false)}
          />
        </form>
      ) : (
        <button className="tab-new" onClick={() => setShowNew(true)}>+</button>
      )}
    </div>
  );
}
