import { useState, type FormEvent } from "react";
import { api } from "../api/client";

interface Props {
  onNavigate: () => void;
  loading: boolean;
}

export function NavBar({ onNavigate, loading }: Props) {
  const [url, setUrl] = useState("");

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!url.trim()) return;
    try {
      await api.navigate(url.trim());
      onNavigate();
    } catch (err) {
      console.error("Navigation failed:", err);
    }
  };

  const handleReload = async () => {
    try {
      await api.reload();
      onNavigate();
    } catch (err) {
      console.error("Reload failed:", err);
    }
  };

  return (
    <nav className="navbar">
      <div className="navbar-brand">Pardus</div>
      <form className="navbar-form" onSubmit={handleSubmit}>
        <button type="button" className="btn-icon" onClick={handleReload} title="Reload" disabled={loading}>
          &#x21bb;
        </button>
        <input
          type="text"
          className="navbar-input"
          placeholder="Enter URL..."
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          disabled={loading}
        />
        <button type="submit" className="btn-primary" disabled={loading}>
          Go
        </button>
      </form>
      <div className="navbar-status">
        {loading && <span className="spinner" />}
      </div>
    </nav>
  );
}
