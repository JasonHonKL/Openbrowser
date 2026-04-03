import { useState, type FormEvent } from "react";
import { api } from "../api/client";

interface Props {
  onAction: () => void;
}

interface LogEntry {
  type: "cmd" | "result" | "error";
  text: string;
}

export function InteractionConsole({ onAction }: Props) {
  const [cmd, setCmd] = useState("");
  const [log, setLog] = useState<LogEntry[]>([]);

  const addLog = (type: LogEntry["type"], text: string) => {
    setLog((prev) => [...prev.slice(-99), { type, text }]);
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const input = cmd.trim();
    if (!input) return;

    setCmd("");
    addLog("cmd", input);

    try {
      const parts = input.split(/\s+/);
      const action = parts[0].toLowerCase();

      switch (action) {
        case "click": {
          const id = parseInt(parts[1]?.replace("#", ""));
          if (isNaN(id)) {
            addLog("error", "Usage: click #<id>");
            return;
          }
          addLog("result", `Clicking element #${id}...`);
          await api.click(id);
          addLog("result", "Clicked");
          onAction();
          break;
        }
        case "type": {
          const id = parseInt(parts[1]?.replace("#", ""));
          const value = parts.slice(2).join(" ");
          if (isNaN(id) || !value) {
            addLog("error", "Usage: type #<id> <value>");
            return;
          }
          await api.typeText(value, id);
          addLog("result", `Typed "${value}" into #${id}`);
          onAction();
          break;
        }
        case "submit": {
          const selector = parts[1];
          if (!selector) {
            addLog("error", "Usage: submit <form-selector> [field=value ...]");
            return;
          }
          const fields: Record<string, string> = {};
          for (const part of parts.slice(2)) {
            const [k, ...v] = part.split("=");
            fields[k] = v.join("=");
          }
          await api.submit(selector, fields);
          addLog("result", "Form submitted");
          onAction();
          break;
        }
        case "scroll": {
          const dir = parts[1] === "up" ? "up" : "down";
          await api.scroll(dir as "up" | "down");
          addLog("result", `Scrolled ${dir}`);
          onAction();
          break;
        }
        case "goto": {
          const url = parts.slice(1).join(" ");
          if (!url) {
            addLog("error", "Usage: goto <url>");
            return;
          }
          addLog("result", `Navigating to ${url}...`);
          await api.navigate(url);
          addLog("result", "Done");
          onAction();
          break;
        }
        default:
          addLog("error", `Unknown command: ${action}. Try: click, type, submit, scroll, goto`);
      }
    } catch (err) {
      addLog("error", String(err));
    }
  };

  return (
    <div className="console">
      <div className="console-log">
        {log.map((entry, i) => (
          <div key={i} className={`console-line console-${entry.type}`}>
            {entry.text}
          </div>
        ))}
        {log.length === 0 && (
          <div className="console-hint">
            Commands: click #id, type #id value, submit selector, scroll up/down, goto url
          </div>
        )}
      </div>
      <form className="console-input-form" onSubmit={handleSubmit}>
        <span className="console-prompt">&gt;</span>
        <input
          className="console-input"
          value={cmd}
          onChange={(e) => setCmd(e.target.value)}
          placeholder="Enter command..."
          autoFocus
        />
      </form>
    </div>
  );
}
