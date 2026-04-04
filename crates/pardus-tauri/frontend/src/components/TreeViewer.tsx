import { useState, useCallback } from "react";
import { useAgent } from "../context/AgentContext";
import type { SemanticNode } from "../types";
import * as api from "../api/tauri";

type Filter = "all" | "interactive";

function hasInteractiveDescendant(node: SemanticNode): boolean {
  if (node.interactive) return true;
  return node.children.some(hasInteractiveDescendant);
}

function formatRole(role: string): string {
  if (role.startsWith("heading")) return role;
  return role.charAt(0).toUpperCase() + role.slice(1);
}

function TreeNode({
  node,
  depth,
  filter,
  onAction,
}: {
  node: SemanticNode;
  depth: number;
  filter: Filter;
  onAction: (node: SemanticNode) => void;
}) {
  const [expanded, setExpanded] = useState(depth < 2);

  if (filter === "interactive" && !node.interactive && !hasInteractiveDescendant(node)) {
    return null;
  }

  const hasChildren = node.children.length > 0;
  const actionLabel = node.action ? node.action : null;

  return (
    <div className="tree-node">
      <div
        className={`tree-node-row ${node.interactive ? "tree-node-interactive" : ""}`}
        style={{ paddingLeft: depth * 16 }}
      >
        {hasChildren ? (
          <button
            className="tree-toggle"
            onClick={() => setExpanded(!expanded)}
          >
            {expanded ? "\u25BE" : "\u25B8"}
          </button>
        ) : (
          <span className="tree-toggle-spacer" />
        )}
        <span className="tree-role" onClick={() => node.interactive && onAction(node)}>
          {formatRole(node.role)}
        </span>
        {node.element_id != null && (
          <span className="tree-eid" onClick={() => onAction(node)} title={`Element #${node.element_id}`}>
            #{node.element_id}
          </span>
        )}
        {node.name && <span className="tree-name">"{node.name}"</span>}
        <span className="tree-tag">{node.tag}</span>
        {actionLabel && (
          <span className="tree-action" title={`Action: ${actionLabel}`}>
            {actionLabel}
          </span>
        )}
        {node.href && (
          <span className="tree-href" title={node.href}>
            {"\u2192"} {node.href.length > 40 ? node.href.slice(0, 40) + "..." : node.href}
          </span>
        )}
      </div>
      {expanded && hasChildren && (
        <div className="tree-children">
          {node.children.map((child, i) => (
            <TreeNode
              key={`${child.tag}-${child.element_id ?? i}-${depth + 1}`}
              node={child}
              depth={depth + 1}
              filter={filter}
              onAction={onAction}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function TreeViewer() {
  const { tree, stats, selectedId, loading } = useAgent();
  const [filter, setFilter] = useState<Filter>("interactive");

  const handleAction = useCallback(
    async (node: SemanticNode) => {
      if (!selectedId || !node.interactive) return;

      const action = node.action;
      const selector = node.selector ?? (node.element_id != null ? `#${node.element_id}` : undefined);

      if (!action || !selector) return;

      if (action === "navigate" && node.href) {
        try {
          await api.executeCdp(selectedId, "Page.navigate", { url: node.href });
        } catch (e) {
          console.error("Navigate failed:", e);
        }
      } else {
        try {
          await api.executeCdp(selectedId, "Pardus.interact", {
            action: action === "fill" ? "type" : action,
            selector,
          });
        } catch (e) {
          console.error("Action failed:", e);
        }
      }
    },
    [selectedId],
  );

  if (!tree) {
    return (
      <div className="tree-panel">
        <div className="tree-toolbar">
          <button className={`btn btn-sm ${filter === "all" ? "btn-active" : ""}`} onClick={() => setFilter("all")}>
            All
          </button>
          <button className={`btn btn-sm ${filter === "interactive" ? "btn-active" : ""}`} onClick={() => setFilter("interactive")}>
            Interactive
          </button>
        </div>
        <div className="tree-empty">
          {loading ? "Loading..." : "Select an agent and navigate to see the semantic tree"}
        </div>
      </div>
    );
  }

  return (
    <div className="tree-panel">
      <div className="tree-toolbar">
        <button className={`btn btn-sm ${filter === "all" ? "btn-active" : ""}`} onClick={() => setFilter("all")}>
          All
        </button>
        <button className={`btn btn-sm ${filter === "interactive" ? "btn-active" : ""}`} onClick={() => setFilter("interactive")}>
          Interactive
        </button>
        {stats && (
          <span className="tree-stats">
            {stats.landmarks}L {stats.links}lnk {stats.headings}H {stats.actions}act
          </span>
        )}
      </div>
      <div className="tree-content">
        <TreeNode node={tree} depth={0} filter={filter} onAction={handleAction} />
      </div>
    </div>
  );
}
