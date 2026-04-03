import { useState } from "react";
import type { SemanticNode } from "../api/client";

interface Props {
  tree: SemanticNode | null;
  stats: { landmarks: number; links: number; headings: number; actions: number; total_nodes: number } | null;
}

export function TreeViewer({ tree, stats }: Props) {
  const [filter, setFilter] = useState<"all" | "interactive">("all");

  if (!tree) {
    return (
      <div className="tree-viewer">
        <h3>Semantic Tree</h3>
        <p className="empty">Navigate to a page to see the semantic tree</p>
      </div>
    );
  }

  return (
    <div className="tree-viewer">
      <div className="tree-header">
        <h3>Semantic Tree</h3>
        <div className="tree-filter">
          <button className={filter === "all" ? "active" : ""} onClick={() => setFilter("all")}>All</button>
          <button className={filter === "interactive" ? "active" : ""} onClick={() => setFilter("interactive")}>Interactive</button>
        </div>
      </div>
      {stats && (
        <div className="tree-stats">
          <span title="Landmarks">{stats.landmarks} landmarks</span>
          <span title="Links">{stats.links} links</span>
          <span title="Headings">{stats.headings} headings</span>
          <span title="Interactive">{stats.actions} actions</span>
        </div>
      )}
      <div className="tree-content">
        <TreeNode node={tree} depth={0} filter={filter} />
      </div>
    </div>
  );
}

function TreeNode({ node, depth, filter }: { node: SemanticNode; depth: number; filter: "all" | "interactive" }) {
  const [expanded, setExpanded] = useState(depth < 2);
  const isRelevant = filter === "all" || node.interactive;

  if (!isRelevant && !node.children.some((c) => isNodeRelevant(c, filter))) {
    return null;
  }

  const hasChildren = node.children.length > 0;
  const roleDisplay = formatRole(node.role);
  const idBadge = node.element_id != null ? <span className="element-id">#{node.element_id}</span> : null;
  const actionBadge = node.action ? <span className="action-badge">{node.action}</span> : null;

  return (
    <div className="tree-node" style={{ paddingLeft: depth * 16 }}>
      <div className="tree-node-row">
        {hasChildren && (
          <button className="tree-toggle" onClick={() => setExpanded(!expanded)}>
            {expanded ? "▾" : "▸"}
          </button>
        )}
        {!hasChildren && <span className="tree-toggle-spacer" />}
        <span className={`node-role role-${node.role.split("{")[0].toLowerCase()}`}>
          {roleDisplay}
        </span>
        {idBadge}
        {node.name && <span className="node-name">{node.name}</span>}
        {node.tag && <span className="node-tag">&lt;{node.tag}&gt;</span>}
        {actionBadge}
        {node.href && <span className="node-href" title={node.href}>link</span>}
      </div>
      {expanded &&
        node.children.map((child, i) => (
          <TreeNode key={i} node={child} depth={depth + 1} filter={filter} />
        ))}
    </div>
  );
}

function isNodeRelevant(node: SemanticNode, filter: "all" | "interactive"): boolean {
  if (filter === "all") return true;
  return node.interactive || node.children.some((c) => isNodeRelevant(c, filter));
}

function formatRole(role: string): string {
  if (role.startsWith("Heading")) return role;
  return role.charAt(0).toUpperCase() + role.slice(1);
}
