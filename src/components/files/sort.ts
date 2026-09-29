import type { FileEntry } from "./fileSource";

export type SortKey = "name" | "size" | "modified";
export type SortDir = "asc" | "desc";
export interface SortState {
  key: SortKey;
  dir: SortDir;
}

function isDirLike(entry: FileEntry): boolean {
  return entry.kind === "directory" || entry.resolvedKind === "directory";
}

/** Only `modified` can be unknown; an unknown always sorts last. */
function unknownRank(entry: FileEntry, key: SortKey): number {
  return key === "modified" && entry.modified === null ? 1 : 0;
}

function compare(a: FileEntry, b: FileEntry, key: SortKey): number {
  switch (key) {
    case "size":
      return a.size - b.size;
    case "modified":
      return (a.modified ?? 0) - (b.modified ?? 0);
    case "name":
      return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" });
  }
}

/**
 * Directories always come first, whatever the sort key or direction - the
 * convention every file manager follows. `Array.sort` is stable, so equal keys
 * keep their listing order.
 */
export function sortEntries(entries: FileEntry[], sort: SortState): FileEntry[] {
  const factor = sort.dir === "asc" ? 1 : -1;
  return [...entries].sort((a, b) => {
    const grouping = Number(!isDirLike(a)) - Number(!isDirLike(b));
    if (grouping !== 0) return grouping;
    const unknowns = unknownRank(a, sort.key) - unknownRank(b, sort.key);
    if (unknowns !== 0) return unknowns;
    return factor * compare(a, b, sort.key);
  });
}
