import { useMemo, useState, type DragEvent } from "react";
import type { Group } from "../api/types";

export interface GroupTreeNode extends Group {
  children: GroupTreeNode[];
  itemCount: number;
}

export function buildTree(groups: Group[], itemCounts: Record<string, number>): GroupTreeNode[] {
  const nodes = new Map<string, GroupTreeNode>();
  groups.forEach((g) => nodes.set(g.id, { ...g, children: [], itemCount: itemCounts[g.id] ?? 0 }));
  const roots: GroupTreeNode[] = [];
  nodes.forEach((node) => {
    if (node.parent_id && nodes.has(node.parent_id)) {
      nodes.get(node.parent_id)!.children.push(node);
    } else {
      roots.push(node);
    }
  });
  const sortFn = (a: GroupTreeNode, b: GroupTreeNode) => a.sort_order - b.sort_order || a.name.localeCompare(b.name);
  const sortRec = (list: GroupTreeNode[]) => {
    list.sort(sortFn);
    list.forEach((n) => sortRec(n.children));
  };
  sortRec(roots);
  return roots;
}

/** Every id in the subtree rooted at `id` (including `id` itself) - used to stop a group being moved under its own descendant. */
export function descendantIds(groups: Group[], id: string): Set<string> {
  const byParent = new Map<string | null, Group[]>();
  groups.forEach((g) => {
    const list = byParent.get(g.parent_id) ?? [];
    list.push(g);
    byParent.set(g.parent_id, list);
  });
  const result = new Set<string>([id]);
  const stack = [id];
  while (stack.length) {
    const current = stack.pop()!;
    for (const child of byParent.get(current) ?? []) {
      if (!result.has(child.id)) {
        result.add(child.id);
        stack.push(child.id);
      }
    }
  }
  return result;
}

export interface GroupOption {
  id: string;
  name: string;
  depth: number;
}

/** Groups flattened in tree order with their nesting depth - for a `<select>` that still shows the hierarchy. */
export function groupOptions(groups: Group[]): GroupOption[] {
  const options: GroupOption[] = [];
  const walk = (nodes: GroupTreeNode[], depth: number) => {
    nodes.forEach((n) => {
      options.push({ id: n.id, name: n.name, depth });
      walk(n.children, depth + 1);
    });
  };
  walk(buildTree(groups, {}), 0);
  return options;
}

/** MIME type for list rows dragged onto the group tree; the payload is a JSON array of item ids. */
const DRAG_ITEMS_MIME = "application/x-remotix-items";

export function startItemDrag(e: DragEvent, ids: string[]) {
  e.dataTransfer.setData(DRAG_ITEMS_MIME, JSON.stringify(ids));
  e.dataTransfer.effectAllowed = "move";
}

function readItemDrag(e: DragEvent): string[] {
  try {
    const ids: unknown = JSON.parse(e.dataTransfer.getData(DRAG_ITEMS_MIME));
    return Array.isArray(ids) ? ids.filter((id): id is string => typeof id === "string") : [];
  } catch {
    return [];
  }
}

type DropItemsHandler = (ids: string[], groupId: string) => void;

export function GroupTreeView({
  nodes,
  selectedId,
  onSelect,
  onRename,
  onDelete,
  onCreateChild,
  onDropItems,
}: {
  nodes: GroupTreeNode[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onRename: (node: GroupTreeNode) => void;
  onDelete: (node: GroupTreeNode) => void;
  onCreateChild: (parentId: string) => void;
  /** When set, list rows dragged with `startItemDrag` can be dropped onto a group to move them into it. */
  onDropItems?: DropItemsHandler;
}) {
  return (
    <ul className="group-tree">
      {nodes.map((node) => (
        <GroupTreeItem
          key={node.id}
          node={node}
          depth={0}
          selectedId={selectedId}
          onSelect={onSelect}
          onRename={onRename}
          onDelete={onDelete}
          onCreateChild={onCreateChild}
          onDropItems={onDropItems}
        />
      ))}
    </ul>
  );
}

function GroupTreeItem({
  node,
  depth,
  selectedId,
  onSelect,
  onRename,
  onDelete,
  onCreateChild,
  onDropItems,
}: {
  node: GroupTreeNode;
  depth: number;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onRename: (node: GroupTreeNode) => void;
  onDelete: (node: GroupTreeNode) => void;
  onCreateChild: (parentId: string) => void;
  onDropItems?: DropItemsHandler;
}) {
  const [expanded, setExpanded] = useState(true);
  const [dropHover, setDropHover] = useState(false);
  const hasChildren = node.children.length > 0;

  const dropProps = onDropItems
    ? {
        onDragOver: (e: DragEvent) => {
          if (!e.dataTransfer.types.includes(DRAG_ITEMS_MIME)) return;
          e.preventDefault();
          e.dataTransfer.dropEffect = "move";
          setDropHover(true);
        },
        onDragLeave: () => setDropHover(false),
        onDrop: (e: DragEvent) => {
          setDropHover(false);
          const ids = readItemDrag(e);
          if (ids.length === 0) return;
          e.preventDefault();
          onDropItems(ids, node.id);
        },
      }
    : {};

  return (
    <li>
      <div
        className={`group-row ${selectedId === node.id ? "selected" : ""} ${dropHover ? "drop-target" : ""}`}
        style={{ paddingLeft: 8 + depth * 14 }}
        onClick={() => onSelect(node.id)}
        {...dropProps}
      >
        <button
          className="ghost tree-caret"
          onClick={(e) => {
            e.stopPropagation();
            setExpanded((v) => !v);
          }}
          aria-label={expanded ? "Collapse" : "Expand"}
        >
          {hasChildren ? (expanded ? "▾" : "▸") : ""}
        </button>
        <span className="group-name">{node.name}</span>
        <span className="group-count">{node.itemCount}</span>
        <span className="group-actions">
          <button className="ghost" title="New subgroup" onClick={(e) => { e.stopPropagation(); onCreateChild(node.id); }}>
            +
          </button>
          <button className="ghost" title="Rename" onClick={(e) => { e.stopPropagation(); onRename(node); }}>
            ✎
          </button>
          <button className="ghost" title="Delete" onClick={(e) => { e.stopPropagation(); onDelete(node); }}>
            🗑
          </button>
        </span>
      </div>
      {expanded && hasChildren && (
        <ul>
          {node.children.map((child) => (
            <GroupTreeItem
              key={child.id}
              node={child}
              depth={depth + 1}
              selectedId={selectedId}
              onSelect={onSelect}
              onRename={onRename}
              onDelete={onDelete}
              onCreateChild={onCreateChild}
              onDropItems={onDropItems}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

export function useGroupTree(groups: Group[], itemCounts: Record<string, number>) {
  return useMemo(() => buildTree(groups, itemCounts), [groups, itemCounts]);
}
