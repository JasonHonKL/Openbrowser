import { useState, useCallback, useRef, useEffect } from "react";
import { useAgent } from "../context/AgentContext";
import * as api from "../api/tauri";

interface LogEntry {
  id: string;
  text: string;
  type: "command" | "result" | "error";
}

/**
 * Split a line into tokens by whitespace, respecting double-quoted strings.
 * Mirrors the REPL's split_tokens function (does NOT treat # as comment).
 */
function splitTokens(input: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let inQuotes = false;

  for (const ch of input) {
    if (ch === '"') {
      inQuotes = !inQuotes;
    } else if (/\s/.test(ch) && !inQuotes) {
      if (current.length > 0) {
        tokens.push(current);
        current = "";
      }
    } else {
      current += ch;
    }
  }
  if (current.length > 0) tokens.push(current);
  return tokens;
}

export function InteractionBar() {
  const { selectedId, refreshTree, refreshInstances } = useAgent();
  const [input, setInput] = useState("");
  const [log, setLog] = useState<LogEntry[]>([]);
  const [history, setHistory] = useState<string[]>([]);
  const [historyIdx, setHistoryIdx] = useState(-1);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [log.length]);

  const addLog = useCallback((text: string, type: LogEntry["type"]) => {
    setLog((prev) => [...prev.slice(-99), { id: `${Date.now()}-${Math.random()}`, text, type }]);
  }, []);

  const handleExecute = useCallback(
    async (cmd: string) => {
      if (!selectedId || !cmd.trim()) return;

      addLog(`pardus> ${cmd}`, "command");
      setHistory((prev) => [...prev, cmd]);
      setHistoryIdx(-1);

      const tokens = splitTokens(cmd.trim());
      if (tokens.length === 0) return;

      try {
        switch (tokens[0]) {
          // Navigation
          case "visit":
          case "open": {
            if (tokens.length < 2) { addLog("Usage: visit <url>", "error"); return; }
            await api.executeCdp(selectedId, "Page.navigate", { url: tokens[1] });
            addLog("Navigated", "result");
            refreshTree();
            break;
          }
          case "reload": {
            await api.executeCdp(selectedId, "Page.reload", {});
            addLog("Reloaded", "result");
            refreshTree();
            break;
          }
          case "back": {
            await api.executeCdp(selectedId, "Page.navigate", { url: "back" });
            addLog("Back", "result");
            refreshTree();
            break;
          }
          case "forward": {
            await api.executeCdp(selectedId, "Page.navigate", { url: "forward" });
            addLog("Forward", "result");
            refreshTree();
            break;
          }

          // Interactions
          case "click": {
            if (tokens.length < 2) { addLog("Usage: click <selector|#id>", "error"); return; }
            const sel = tokens[1];
            const selector = sel.startsWith("#") && /^\d+$/.test(sel.slice(1))
              ? `#${sel.slice(1)}` : sel;
            await api.executeCdp(selectedId, "Pardus.interact", { action: "click", selector });
            addLog(`Clicked ${sel}`, "result");
            refreshTree();
            break;
          }
          case "type": {
            if (tokens.length < 3) { addLog("Usage: type <selector|#id> <value>", "error"); return; }
            const sel = tokens[1];
            const value = tokens.slice(2).join(" ");
            await api.executeCdp(selectedId, "Pardus.interact", {
              action: "type", selector: sel, value,
            });
            addLog(`Typed '${value}' into ${sel}`, "result");
            break;
          }
          case "submit": {
            if (tokens.length < 2) { addLog("Usage: submit <selector> [name=value ...]", "error"); return; }
            const fields: Record<string, string> = {};
            for (const f of tokens.slice(2)) {
              const [k, ...v] = f.split("=");
              if (v.length > 0) fields[k] = v.join("=");
              else addLog(`Invalid field '${f}', expected name=value`, "error");
            }
            await api.executeCdp(selectedId, "Pardus.interact", {
              action: "submit", selector: tokens[1], fields,
            });
            addLog(`Submitted ${tokens[1]}`, "result");
            refreshTree();
            break;
          }
          case "scroll": {
            const dir = tokens[1] ?? "down";
            const px = dir === "up" ? -400 : dir === "to-top" ? -99999 : dir === "to-bottom" ? 99999 : 400;
            await api.executeCdp(selectedId, "Runtime.evaluate", {
              expression: `window.scrollBy(0, ${px})`,
            });
            addLog(`Scrolled ${dir}`, "result");
            refreshTree();
            break;
          }
          case "wait": {
            if (tokens.length < 2) { addLog("Usage: wait <selector> [timeout_ms]", "error"); return; }
            const timeout = tokens[2] ? parseInt(tokens[2]) : 5000;
            await api.executeCdp(selectedId, "Pardus.wait", {
              condition: "selector", selector: tokens[1], timeoutMs: timeout,
            });
            addLog(`Wait satisfied: ${tokens[1]}`, "result");
            break;
          }
          case "event": {
            if (tokens.length < 3) { addLog("Usage: event <selector|#id> <event_type> [init_json]", "error"); return; }
            await api.executeCdp(selectedId, "Pardus.interact", {
              action: "event", selector: tokens[1], eventType: tokens[2], init: tokens[3],
            });
            addLog(`Dispatched '${tokens[2]}' on ${tokens[1]}`, "result");
            break;
          }

          // Tree / inspect
          case "tree":
          case "dom": {
            refreshTree();
            addLog("Tree refreshed", "result");
            break;
          }

          // Settings
          case "js": {
            const val = tokens[1];
            if (val === "on" || val === "true" || val === "1") {
              addLog("JS enabled (applied on next navigation)", "result");
            } else if (val === "off" || val === "false" || val === "0") {
              addLog("JS disabled", "result");
            } else {
              addLog("JS is on by default", "result");
            }
            break;
          }

          // Help
          case "help":
          case "?": {
            addLog([
              "Navigation:  visit <url> | reload | back | forward",
              "Interact:    click <#id|sel> | type <#id|sel> <text> | submit <sel> [k=v..]",
              "             scroll [down|up|to-top|to-bottom] | wait <sel> [ms] | event <sel> <type>",
              "Inspect:     tree",
              "Settings:    js [on|off] | help",
              "Exit:        exit",
            ].join("\n"), "result");
            break;
          }

          case "exit":
          case "quit": {
            addLog("Use the sidebar kill button to stop the agent", "result");
            break;
          }

          default:
            addLog(`Unknown command: ${tokens[0]}. Type "help" for available commands.`, "error");
        }
      } catch (e) {
        addLog(String(e), "error");
      }
    },
    [selectedId, addLog, refreshTree, refreshInstances],
  );

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleExecute(input);
      setInput("");
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (history.length > 0) {
        const newIdx = historyIdx < 0 ? history.length - 1 : Math.max(0, historyIdx - 1);
        setHistoryIdx(newIdx);
        setInput(history[newIdx]);
      }
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (historyIdx >= 0) {
        const newIdx = historyIdx + 1;
        if (newIdx >= history.length) {
          setHistoryIdx(-1);
          setInput("");
        } else {
          setHistoryIdx(newIdx);
          setInput(history[newIdx]);
        }
      }
    }
  };

  if (!selectedId) {
    return <div className="interaction-bar"><div className="log-empty">Spawn an agent to start</div></div>;
  }

  return (
    <div className="interaction-bar">
      <div className="interaction-log" ref={scrollRef}>
        {log.length === 0 && (
          <div className="log-empty">
            pardus-browser repl — type "help" for commands
          </div>
        )}
        {log.map((entry) => (
          <div key={entry.id} className={`log-entry log-${entry.type}`}>
            {entry.type === "command" ? (
              <span className="log-cmd">{entry.text}</span>
            ) : entry.type === "error" ? (
              <span className="log-err">{entry.text}</span>
            ) : (
              <span className="log-res" style={{ whiteSpace: "pre-wrap" }}>{entry.text}</span>
            )}
          </div>
        ))}
      </div>
      <div className="interaction-input-row">
        <span className="prompt">pardus&gt;</span>
        <input
          className="interaction-input"
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="visit https://example.com"
          spellCheck={false}
        />
      </div>
    </div>
  );
}
