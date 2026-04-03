import type { InstanceInfo, LogEntry } from "./types";
import * as api from "./api";

export class InstanceManager {
  private instances: InstanceInfo[] = [];
  private tableBody: HTMLElement;
  private noInstances: HTMLElement;
  private onLog: (entry: LogEntry) => void;
  private challengeListenerCleanup: (() => void) | null = null;

  constructor(
    tableBody: HTMLElement,
    noInstances: HTMLElement,
    onLog: (entry: LogEntry) => void
  ) {
    this.tableBody = tableBody;
    this.noInstances = noInstances;
    this.onLog = onLog;
  }

  async refresh(): Promise<void> {
    try {
      this.instances = await api.listInstances();
    } catch {
      this.instances = [];
    }
    this.render();
  }

  async spawn(): Promise<void> {
    try {
      const inst = await api.spawnInstance();
      this.instances.push(inst);
      this.render();
      this.onLog({ level: "info", message: `Instance ${inst.id} spawned on port ${inst.port}`, timestamp: ts() });
    } catch (e) {
      this.onLog({ level: "error", message: `Spawn failed: ${e}`, timestamp: ts() });
    }
  }

  async kill(id: string): Promise<void> {
    try {
      await api.killInstance(id);
      this.instances = this.instances.filter((i) => i.id !== id);
      this.render();
      this.onLog({ level: "info", message: `Instance ${id} killed`, timestamp: ts() });
    } catch (e) {
      this.onLog({ level: "error", message: `Kill failed: ${e}`, timestamp: ts() });
    }
  }

  async killAll(): Promise<void> {
    try {
      await api.killAllInstances();
      this.instances = [];
      this.render();
      this.onLog({ level: "info", message: "All instances killed", timestamp: ts() });
    } catch (e) {
      this.onLog({ level: "error", message: `Kill all failed: ${e}`, timestamp: ts() });
    }
  }

  private render(): void {
    this.tableBody.innerHTML = "";

    if (this.instances.length === 0) {
      this.noInstances.style.display = "block";
      return;
    }
    this.noInstances.style.display = "none";

    for (const inst of this.instances) {
      const tr = document.createElement("tr");
      tr.innerHTML = `
        <td>${inst.id}</td>
        <td>${inst.port}</td>
        <td class="mono">${inst.ws_url}</td>
        <td><span class="status running">running</span></td>
        <td>
          <button class="btn-danger btn-sm" data-kill="${inst.id}">Kill</button>
          <button class="btn-sm" data-copy="${inst.ws_url}">Copy WS</button>
        </td>
      `;
      tr.querySelector(`[data-kill="${inst.id}"]`)?.addEventListener("click", () => this.kill(inst.id));
      tr.querySelector(`[data-copy="${inst.ws_url}"]`)?.addEventListener("click", () => {
        navigator.clipboard.writeText(inst.ws_url);
      });
      this.tableBody.appendChild(tr);
    }
  }

  destroy(): void {
    this.challengeListenerCleanup?.();
  }
}

function ts(): string {
  return new Date().toISOString().slice(11, 19);
}
