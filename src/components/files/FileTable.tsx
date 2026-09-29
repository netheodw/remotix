import { useEffect, useRef } from "react";
import { formatBytes, formatModified } from "../../api/format";
import type { FileEntry } from "./fileSource";
import type { SortKey, SortState } from "./sort";

const COLUMNS: { key: SortKey; label: string; className: string }[] = [
  { key: "name", label: "Name", className: "file-name" },
  { key: "size", label: "Size", className: "file-size" },
  { key: "modified", label: "Modified", className: "file-modified" },
];

export function FileTable({
  entries,
  hasParent,
  selection,
  focusedIndex,
  sort,
  onSortChange,
  onRowClick,
  onRowContextMenu,
  onRowOpen,
  onParentOpen,
}: {
  entries: FileEntry[];
  hasParent: boolean;
  selection: Set<string>;
  focusedIndex: number;
  sort: SortState;
  onSortChange: (key: SortKey) => void;
  onRowClick: (index: number, event: React.MouseEvent) => void;
  onRowContextMenu: (index: number, event: React.MouseEvent) => void;
  onRowOpen: (entry: FileEntry) => void;
  onParentOpen: () => void;
}) {
  const rows = useRef(new Map<number, HTMLTableRowElement>());

  useEffect(() => {
    rows.current.get(focusedIndex)?.scrollIntoView({ block: "nearest" });
  }, [focusedIndex, entries]);

  return (
    <table className="data-table file-table">
      <thead>
        <tr>
          {COLUMNS.map((column) => (
            <th
              key={column.key}
              className={`sortable ${column.className} ${sort.key === column.key ? `sorted-${sort.dir}` : ""}`}
              onClick={() => onSortChange(column.key)}
            >
              {column.label}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {hasParent && (
          <tr className="file-row dir parent-row" onDoubleClick={onParentOpen}>
            <td className="file-name" colSpan={3}>
              ..
            </td>
          </tr>
        )}
        {entries.map((entry, index) => {
          const isDir = entry.kind === "directory" || entry.resolvedKind === "directory";
          return (
            <tr
              key={entry.name}
              ref={(element) => {
                if (element) rows.current.set(index, element);
                else rows.current.delete(index);
              }}
              className={[
                "file-row",
                isDir ? "dir" : "",
                entry.hidden ? "hidden-entry" : "",
                selection.has(entry.name) ? "selected" : "",
                index === focusedIndex ? "focused" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              onClick={(event) => onRowClick(index, event)}
              onContextMenu={(event) => onRowContextMenu(index, event)}
              onDoubleClick={() => onRowOpen(entry)}
            >
              <td className="file-name">
                {entry.name}
                {entry.kind === "symlink" && <span className="file-link-marker"> →</span>}
              </td>
              <td className="file-size">{isDir ? "—" : formatBytes(entry.size)}</td>
              <td className="file-modified">{formatModified(entry.modified)}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
