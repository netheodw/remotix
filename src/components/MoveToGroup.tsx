import { useMemo, useState } from "react";
import type { Group } from "../api/types";
import { groupOptions } from "./GroupTree";

const UNGROUPED = "__ungrouped__";

/** Ids a row drag should carry: the whole selection when the dragged row is part of it, otherwise just that row. */
export function dragPayload(rowId: string, selected: ReadonlySet<string>): string[] {
  return selected.has(rowId) ? [...selected] : [rowId];
}

/**
 * Moves each item into `groupId`, skipping items already there. Sequential on purpose: if one move
 * fails the error propagates and the remaining items stay where they were.
 */
export async function moveItems(
  ids: string[],
  groupId: string | null,
  currentGroup: (id: string) => string | null | undefined,
  move: (id: string, groupId: string | null) => Promise<unknown>,
) {
  for (const id of ids) {
    if (currentGroup(id) !== groupId) await move(id, groupId);
  }
}

/** Checkbox selection over a list, narrowed to the rows currently visible so a filter never hides selected rows. */
export function useRowSelection(visibleIds: string[]) {
  const [picked, setPicked] = useState<Set<string>>(() => new Set());
  const selected = useMemo(() => new Set(visibleIds.filter((id) => picked.has(id))), [visibleIds, picked]);
  const allSelected = visibleIds.length > 0 && selected.size === visibleIds.length;

  const toggle = (id: string) =>
    setPicked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const toggleAll = () => setPicked(allSelected ? new Set() : new Set(visibleIds));
  const clear = () => setPicked(new Set());

  return { selected, allSelected, toggle, toggleAll, clear };
}

export function MoveToGroupSelect({
  groups,
  count,
  onMove,
}: {
  groups: Group[];
  count: number;
  onMove: (groupId: string | null) => void;
}) {
  const options = useMemo(() => groupOptions(groups), [groups]);

  return (
    <select
      className="move-to-group"
      value=""
      disabled={count === 0}
      title={count === 0 ? "Tick rows to move them, or drag a row onto a group" : undefined}
      onChange={(e) => {
        if (e.target.value) onMove(e.target.value === UNGROUPED ? null : e.target.value);
      }}
    >
      <option value="">{count === 0 ? "Move to group..." : `Move ${count} selected to...`}</option>
      <option value={UNGROUPED}>Ungrouped</option>
      {options.map((o) => (
        <option key={o.id} value={o.id}>
          {"   ".repeat(o.depth) + o.name}
        </option>
      ))}
    </select>
  );
}
