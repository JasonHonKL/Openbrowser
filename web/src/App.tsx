import { useState, useEffect, useCallback } from "react";
import { api } from "./api/client";
import type { TabInfo, SemanticNode, TreeStats } from "./api/client";
import { useEvents } from "./hooks/useEvents";
import { NavBar } from "./components/NavBar";
import { TabBar } from "./components/TabBar";
import { TreeViewer } from "./components/TreeViewer";
import { NetworkLog } from "./components/NetworkLog";
import { CookieInspector } from "./components/CookieInspector";
import { InteractionConsole } from "./components/InteractionConsole";
import "./index.css";

export default function App() {
  const [tabs, setTabs] = useState<TabInfo[]>([]);
  const [activeTab, setActiveTab] = useState<TabInfo | null>(null);
  const [tree, setTree] = useState<SemanticNode | null>(null);
  const [stats, setStats] = useState<TreeStats | null>(null);
  const [loading, setLoading] = useState(false);
  const { events, connected } = useEvents();

  const refresh = useCallback(async () => {
    try {
      const tabList = await api.listTabs();
      setTabs(tabList);
      const active = tabList.length > 0 ? tabList[0] : null;
      setActiveTab(active);

      if (active) {
        try {
          const treeData = await api.semanticTree();
          setTree(treeData.root);
          setStats(treeData.stats);
        } catch {
          setTree(null);
          setStats(null);
        }
      } else {
        setTree(null);
        setStats(null);
      }
    } catch {
      // server not reachable
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // React to WebSocket events
  useEffect(() => {
    const latest = events[events.length - 1];
    if (!latest) return;

    if (
      latest.type === "navigation.completed" ||
      latest.type === "tab.created" ||
      latest.type === "tab.closed" ||
      latest.type === "tab.activated" ||
      latest.type === "semantic.updated"
    ) {
      refresh();
    }
    if (latest.type === "navigation.started") {
      setLoading(true);
    }
    if (latest.type === "navigation.completed" || latest.type === "navigation.failed") {
      setLoading(false);
    }
  }, [events, refresh]);

  const activeTabId = tabs.length > 0 ? tabs.find((t) => t.state === "Ready")?.id ?? tabs[0]?.id : null;

  return (
    <div className="app">
      <header className="app-header">
        <NavBar onNavigate={refresh} loading={loading} />
        <TabBar tabs={tabs} activeId={activeTabId ?? null} onChange={refresh} />
      </header>
      <main className="app-main">
        <aside className="sidebar sidebar-left">
          <TreeViewer tree={tree} stats={stats} />
        </aside>
        <section className="center">
          <div className="page-info">
            {activeTab ? (
              <>
                <h2>{activeTab.title ?? "Untitled"}</h2>
                <p className="page-url">{activeTab.url}</p>
                <span className={`page-state state-${activeTab.state.toLowerCase()}`}>
                  {activeTab.state}
                </span>
              </>
            ) : (
              <div className="welcome">
                <h1>Pardus Browser</h1>
                <p>Enter a URL to start browsing</p>
              </div>
            )}
          </div>
          <InteractionConsole onAction={refresh} />
        </section>
        <aside className="sidebar sidebar-right">
          <NetworkLog />
          <CookieInspector />
          <div className="ws-status">
            WS: <span className={connected ? "ws-on" : "ws-off"}>{connected ? "Connected" : "Disconnected"}</span>
          </div>
        </aside>
      </main>
    </div>
  );
}
