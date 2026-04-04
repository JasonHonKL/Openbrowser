import { AgentProvider, useAgent } from "./context/AgentContext";
import { AgentSidebar } from "./components/AgentSidebar";
import { InstanceHeader } from "./components/InstanceHeader";
import { TreeViewer } from "./components/TreeViewer";
import { ActionLog } from "./components/ActionLog";
import { InteractionBar } from "./components/InteractionBar";
import { ChallengePanel } from "./components/ChallengePanel";

function Dashboard() {
  const { events } = useAgent();

  return (
    <div className="app">
      <InstanceHeader />
      <main className="app-main">
        <AgentSidebar />
        <div className="center">
          <div className="panel-split">
            <div className="panel panel-tree">
              <TreeViewer />
            </div>
            <div className="panel panel-log">
              <ActionLog events={events} />
            </div>
          </div>
          <InteractionBar />
        </div>
        <aside className="sidebar sidebar-right">
          <ChallengePanel />
        </aside>
      </main>
    </div>
  );
}

export function App() {
  return (
    <AgentProvider>
      <Dashboard />
    </AgentProvider>
  );
}
