import { useState, useCallback, useMemo } from "react";
import { useAgent } from "../context/AgentContext";
import type { SemanticNode } from "../types";
import * as api from "../api/tauri";

type Filter = "all" | "interactive";

const hasInteractive = (n: SemanticNode): boolean => n.interactive || n.children.some(hasInteractive);
const matchesSearch = (n: SemanticNode, q: string): boolean => {
  const lq = q.toLowerCase();
  return [n.role, n.name, n.tag, n.action, n.href, n.selector]
    .some((v) => v?.toLowerCase().includes(lq))
    || (n.element_id != null && `#${n.element_id}`.includes(lq))
    || n.children.some((c) => matchesSearch(c, q));
};
const countNodes = (n: SemanticNode): number => 1 + n.children.reduce((s, c) => s + countNodes(c), 0);
const fmtRole = (r: string) => r.startsWith("heading") ? r : r[0].toUpperCase() + r.slice(1);

function TreeNode({ node, depth, filter, q, onAction }: {
  node: SemanticNode; depth: number; filter: Filter; q: string;
  onAction: (n: SemanticNode) => void;
}) {
  const [open, setOpen] = useState(depth < 2);
  if (filter === "interactive" && !node.interactive && !hasInteractive(node)) return null;
  if (q && !matchesSearch(node, q)) return null;

  const kids = node.children;
  const hl = q && matchesSearch(node, q);

  return (
    <div className="tree-node">
      <div className={`tree-node-row ${node.interactive ? "tree-node-interactive" : ""} ${hl && q ? "tree-node-highlight" : ""}`}
        style={{ paddingLeft: depth * 16 }}>
        {kids.length
          ? <button className="tree-toggle" onClick={() => setOpen(!open)}>{open ? "\u25BE" : "\u25B8"}</button>
          : <span className="tree-toggle-spacer" />}
        <span className="tree-role" onClick={() => node.interactive && onAction(node)}>{fmtRole(node.role)}</span>
        {node.element_id != null && <span className="tree-eid onClick" onClick={() => onAction(node)}>#{node.element_id}</span>}
        {node.name && <span className="tree-name" title={node.name}>"{node.name}"</span>}
        {node.action && <span className="tree-action">{node.action}</span>}
        {node.href && <span className="tree-href">{"\u2192"} {node.href.length > 40 ? node.href.slice(0, 40) + "..." : node.href}</span>}
        <span className="tree-tag">{node.tag}</span>
        {node.input_type && <span className="tree-tag">[{node.input_type}]</span>}
      </div>
      {open && kids.map((c, i) => (
        <TreeNode key={`${c.tag}-${c.element_id ?? i}-${depth + 1}`}
          node={c} depth={depth + 1} filter={filter} q={q} onAction={onAction} />
      ))}
    </div>
  );
}

export function TreeViewer() {
  const { tree, stats, selectedId, loading } = useAgent();
  const [filter, setFilter] = useState<Filter>("interactive");
  const [q, setQ] = useState("");

  const handleAction = useCallback(async (node: SemanticNode) => {
    if (!selectedId || !node.interactive) return;
    const selector = node.selector ?? (node.element_id != null ? `#${node.element_id}` : undefined);
    if (!node.action || !selector) return;
    try {
      if (node.action === "navigate" && node.href) await api.executeCdp(selectedId, "Page.navigate", { url: node.href });
      else await api.executeCdp(selectedId, "Pardus.interact", { action: node.action === "fill" ? "type" : node.action, selector });
    } catch (e) { console.error("Action failed:", e); }
  }, [selectedId]);

  const total = useMemo(() => tree ? countNodes(tree) : 0, [tree]);

  const toolbar = (
    <div className="tree-toolbar">
      <div className="tree-filter-group">
        {(["all", "interactive"] as const).map((f) => (
          <button key={f} className={`tree-filter-btn ${filter === f ? "active" : ""}`}
            onClick={() => setFilter(f)}>{f[0].toUpperCase() + f.slice(1)}</button>
        ))}
      </div>
      <input className="tree-search" value={q} onChange={(e) => setQ(e.target.value)}
        placeholder="Search nodes..." spellCheck={false} />
      <div className="tree-stats">
        {stats && <>{[
          [stats.landmarks, "L"], [stats.links, "lnk"], [stats.headings, "H"], [stats.actions, "act"],
        ].map(([v, l]) => <span key={l as string} className="tree-stat"><span className="tree-stat-val">{v}</span>{l}</span>)}</>}
        <span className="tree-stat"><span className="tree-stat-val">{total}</span>nodes</span>
      </div>
    </div>
  );

  if (!tree) return (
    <div className="tree-panel">
      {toolbar}
      <div className="tree-empty">
        {loading ? <div className="tree-loading"><span className="spinner" /> Loading tree...</div>
          : "Select an agent and navigate to see the semantic tree"}
      </div>
    </div>
  );

  return (
    <div className="tree-panel">
      {toolbar}
      <div className="tree-content">
        <TreeNode node={tree} depth={0} filter={filter} q={q} onAction={handleAction} />
      </div>
    </div>
  );
}
