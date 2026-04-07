import { useEffect, useState } from "react";
import type { NetworkRecord } from "../api/client";
import { api } from "../api/client";

export function NetworkLog() {
  const [records, setRecords] = useState<NetworkRecord[]>([]);

  const refresh = async () => {
    try {
      const data = await api.networkRequests();
      setRecords(data);
    } catch (e) {
      console.error('Failed to fetch network requests:', e);
    }
  };

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 3000);
    return () => clearInterval(interval);
  }, []);

  const handleClear = async () => {
    await api.clearNetwork();
    setRecords([]);
  };

  return (
    <div className="network-log">
      <div className="panel-header">
        <h3>Network</h3>
        <div className="panel-actions">
          <button onClick={refresh} title="Refresh">R</button>
          <button onClick={handleClear} title="Clear">C</button>
        </div>
      </div>
      <div className="network-table-wrapper">
        <table className="network-table">
          <thead>
            <tr>
              <th>Method</th>
              <th>Status</th>
              <th>Type</th>
              <th>URL</th>
              <th>Size</th>
              <th>Time</th>
            </tr>
          </thead>
          <tbody>
            {records.map((r) => (
              <tr key={r.id}>
                <td className="method">{r.method}</td>
                <td className={r.status != null && r.status >= 400 ? "status-error" : "status"}>
                  {r.status ?? "-"}
                </td>
                <td className="type">{r.type}</td>
                <td className="url" title={r.url}>
                  {truncateUrl(r.url)}
                </td>
                <td className="size">{r.body_size ? formatBytes(r.body_size) : "-"}</td>
                <td className="time">{r.timing_ms != null ? `${r.timing_ms}ms` : "-"}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {records.length === 0 && <p className="empty">No network requests</p>}
      </div>
    </div>
  );
}

function truncateUrl(url: string): string {
  try {
    const u = new URL(url);
    const path = u.pathname + u.search;
    return path.length > 50 ? path.slice(0, 50) + "..." : path;
  } catch {
    return url.length > 50 ? url.slice(0, 50) + "..." : url;
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)}MB`;
}
