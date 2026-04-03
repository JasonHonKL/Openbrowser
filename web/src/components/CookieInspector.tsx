import { useEffect, useState } from "react";
import type { CookieEntry } from "../api/client";
import { api } from "../api/client";

export function CookieInspector() {
  const [cookies, setCookies] = useState<CookieEntry[]>([]);

  const refresh = async () => {
    try {
      const data = await api.listCookies();
      setCookies(data);
    } catch {
      // ignore
    }
  };

  useEffect(() => {
    refresh();
    const interval = setInterval(refresh, 5000);
    return () => clearInterval(interval);
  }, []);

  const handleDelete = async (name: string) => {
    await api.deleteCookie(name);
    refresh();
  };

  const handleClear = async () => {
    await api.clearCookies();
    setCookies([]);
  };

  return (
    <div className="cookie-inspector">
      <div className="panel-header">
        <h3>Cookies ({cookies.length})</h3>
        <div className="panel-actions">
          <button onClick={refresh} title="Refresh">R</button>
          <button onClick={handleClear} title="Clear all">C</button>
        </div>
      </div>
      <div className="cookie-list">
        {cookies.map((c, i) => (
          <div key={i} className="cookie-row">
            <span className="cookie-name">{c.name}</span>
            <span className="cookie-value" title={c.value}>{truncate(c.value, 20)}</span>
            <span className="cookie-domain">{c.domain}</span>
            <button className="cookie-delete" onClick={() => handleDelete(c.name)}>&times;</button>
          </div>
        ))}
        {cookies.length === 0 && <p className="empty">No cookies</p>}
      </div>
    </div>
  );
}

function truncate(s: string, max: number): string {
  return s.length > max ? s.slice(0, max) + "..." : s;
}
