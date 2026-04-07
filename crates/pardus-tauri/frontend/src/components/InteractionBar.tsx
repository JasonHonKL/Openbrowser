import { useState, useCallback, useRef, useEffect } from "react";
import { useAgent } from "../context/AgentContext";
import * as api from "../api/tauri";

interface LogEntry { id: string; text: string; type: "command" | "result" | "error" }

function splitTokens(input: string): string[] {
  const tokens: string[] = [];
  let current = "", inQ = false;
  for (const ch of input) {
    if (ch === '"') inQ = !inQ;
    else if (/\s/.test(ch) && !inQ) { if (current) { tokens.push(current); current = ""; } }
    else current += ch;
  }
  if (current) tokens.push(current);
  return tokens;
}

export function InteractionBar() {
  const { selectedId, refreshTree } = useAgent();
  const [input, setInput] = useState("");
  const [log, setLog] = useState<LogEntry[]>([]);
  const [hist, setHist] = useState<string[]>([]);
  const [hIdx, setHIdx] = useState(-1);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => { scrollRef.current && (scrollRef.current.scrollTop = scrollRef.current.scrollHeight); }, [log.length]);

  const add = useCallback((text: string, type: LogEntry["type"]) => {
    setLog((p) => [...p.slice(-99), { id: `${Date.now()}-${Math.random()}`, text, type }]);
  }, []);

  const exec = useCallback(async (cmd: string) => {
    if (!selectedId || !cmd.trim()) return;
    add(`pardus> ${cmd}`, "command");
    setHist((p) => [...p, cmd]);
    setHIdx(-1);
    const t = splitTokens(cmd.trim());
    if (!t.length) return;
    const cdp = (m: string, p: Record<string, unknown>) => api.executeCdp(selectedId!, m, p);
    try {
      switch (t[0]) {
        case "visit": case "open":
          if (t.length < 2) { add("Usage: visit <url>", "error"); return; }
          await cdp("Page.navigate", { url: t[1] }); add("Navigated", "result"); refreshTree(); break;
        case "reload":
          await cdp("Page.reload", {}); add("Reloaded", "result"); refreshTree(); break;
        case "back":
          await cdp("Page.navigate", { url: "back" }); add("Back", "result"); refreshTree(); break;
        case "forward":
          await cdp("Page.navigate", { url: "forward" }); add("Forward", "result"); refreshTree(); break;
        case "click":
          if (t.length < 2) { add("Usage: click <selector|#id>", "error"); return; }
          await cdp("Pardus.interact", { action: "click", selector: t[1] }); add(`Clicked ${t[1]}`, "result"); refreshTree(); break;
        case "type":
          if (t.length < 3) { add("Usage: type <selector|#id> <value>", "error"); return; }
          await cdp("Pardus.interact", { action: "type", selector: t[1], value: t.slice(2).join(" ") });
          add(`Typed into ${t[1]}`, "result"); break;
        case "submit":
          if (t.length < 2) { add("Usage: submit <selector> [name=value ...]", "error"); return; }
          { const f: Record<string, string> = {};
            for (const p of t.slice(2)) { const [k, ...v] = p.split("="); if (v.length) f[k] = v.join("="); }
            await cdp("Pardus.interact", { action: "submit", selector: t[1], fields: f });
            add(`Submitted ${t[1]}`, "result"); refreshTree(); }
          break;
        case "scroll": {
          const d = t[1] ?? "down";
          const px = d === "up" ? -400 : d === "to-top" ? -99999 : d === "to-bottom" ? 99999 : 400;
          await cdp("Runtime.evaluate", { expression: `window.scrollBy(0,${px})` });
          add(`Scrolled ${d}`, "result"); refreshTree(); break;
        }
        case "wait":
          if (t.length < 2) { add("Usage: wait <selector> [timeout_ms]", "error"); return; }
          await cdp("Pardus.wait", { condition: "selector", selector: t[1], timeoutMs: t[2] ? parseInt(t[2]) : 5000 });
          add(`Wait satisfied: ${t[1]}`, "result"); break;
        case "event":
          if (t.length < 3) { add("Usage: event <selector> <type>", "error"); return; }
          await cdp("Pardus.interact", { action: "event", selector: t[1], eventType: t[2] });
          add(`Dispatched '${t[2]}' on ${t[1]}`, "result"); break;
        case "tree": case "dom":
          refreshTree(); add("Tree refreshed", "result"); break;
        case "help": case "?":
          add([
            "Navigation:  visit <url> | reload | back | forward",
            "Interact:    click <sel> | type <sel> <text> | submit <sel> [k=v..]",
            "             scroll [down|up|to-top|to-bottom] | wait <sel> [ms] | event <sel> <type>",
            "Inspect:     tree",
          ].join("\n"), "result"); break;
        case "exit": case "quit":
          add("Use sidebar to manage agents", "result"); break;
        default:
          add(`Unknown: ${t[0]}. Type "help" for commands.`, "error");
      }
    } catch (e) { add(String(e), "error"); }
  }, [selectedId, add, refreshTree]);

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") { exec(input); setInput(""); }
    else if (e.key === "ArrowUp") { e.preventDefault(); if (hist.length) { const i = hIdx < 0 ? hist.length - 1 : Math.max(0, hIdx - 1); setHIdx(i); setInput(hist[i]); } }
    else if (e.key === "ArrowDown") { e.preventDefault(); if (hIdx >= 0) { const i = hIdx + 1; if (i >= hist.length) { setHIdx(-1); setInput(""); } else { setHIdx(i); setInput(hist[i]); } } }
  };

  if (!selectedId) return <div className="interaction-bar"><div className="log-empty">Spawn an agent to start</div></div>;

  return (
    <div className="interaction-bar">
      <div className="interaction-log" ref={scrollRef}>
        {log.length === 0 && <div className="log-empty">pardus-browser repl — type "help" for commands</div>}
        {log.map((e) => (
          <div key={e.id} className={`log-entry log-${e.type}`}>
            {e.type === "command" ? <span className="log-cmd">{e.text}</span>
              : e.type === "error" ? <span className="log-err">{e.text}</span>
              : <span className="log-res" style={{ whiteSpace: "pre-wrap" }}>{e.text}</span>}
          </div>
        ))}
      </div>
      <div className="interaction-input-row">
        <span className="prompt">pardus&gt;</span>
        <input className="interaction-input" type="text" value={input}
          onChange={(e) => setInput(e.target.value)} onKeyDown={onKey}
          placeholder="visit https://example.com" spellCheck={false} />
      </div>
    </div>
  );
}
