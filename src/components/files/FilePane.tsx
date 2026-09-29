import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { errorMessage } from "../../api/errors";
import { ConfirmDialog } from "../Modal";
import { PromptDialog } from "../PromptDialog";
import { FileTable } from "./FileTable";
import type { FileEntry, FileListing, FileSource } from "./fileSource";
import { PathBar } from "./PathBar";
import { sortEntries, type SortKey, type SortState } from "./sort";

function isDirLike(entry: FileEntry): boolean {
  return entry.kind === "directory" || entry.resolvedKind === "directory";
}

/** A context-menu entry contributed by the screen. `run` receives the row that
 *  was right-clicked; actions that work on many files read the selection,
 *  which the screen already owns. */
export interface FileAction {
  label: string;
  run: (entry: FileEntry) => void;
  enabled?: (entry: FileEntry) => boolean;
}

export function FilePane({
  source,
  cwd,
  onCwdChange,
  selection,
  onSelectionChange,
  onOpenFile,
  actions = [],
  refreshToken = 0,
}: {
  source: FileSource;
  cwd: string;
  onCwdChange: (path: string) => void;
  selection: Set<string>;
  onSelectionChange: (selection: Set<string>) => void;
  onOpenFile?: (entry: FileEntry) => void;
  actions?: FileAction[];
  refreshToken?: number;
}) {
  const [listing, setListing] = useState<FileListing | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [roots, setRoots] = useState<{ label: string; path: string }[]>([]);
  const [sort, setSort] = useState<SortState>({ key: "name", dir: "asc" });
  const [filter, setFilter] = useState("");
  const [showHidden, setShowHidden] = useState(false);
  const [focusedIndex, setFocusedIndex] = useState(0);
  const [reloadToken, setReloadToken] = useState(0);
  const [newFolderOpen, setNewFolderOpen] = useState(false);
  const [menu, setMenu] = useState<{ x: number; y: number; entry: FileEntry } | null>(null);
  const [pendingDelete, setPendingDelete] = useState<FileEntry[] | null>(null);
  const [deleting, setDeleting] = useState(false);
  const anchor = useRef(0);
  const request = useRef(0);
  // Held in a ref so the listing effect does not depend on the callback's
  // identity: a caller passing an inline arrow would otherwise re-list on
  // every render, and each listing re-renders.
  const cwdChange = useRef(onCwdChange);
  cwdChange.current = onCwdChange;

  useEffect(() => {
    if (!menu) return;
    const dismiss = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setMenu(null);
    window.addEventListener("mousedown", dismiss);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", dismiss);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  useEffect(() => {
    source.roots?.().then(setRoots).catch(() => setRoots([]));
  }, [source]);

  useEffect(() => {
    if (cwd === "") {
      source
        .home()
        .then((home) => cwdChange.current(home))
        .catch((e) => setError(errorMessage(e)));
      return;
    }
    // A slow listing must not overwrite a newer one the user has navigated to.
    const token = ++request.current;
    setLoading(true);
    source
      .list(cwd)
      .then((result) => {
        if (token !== request.current) return;
        setListing(result);
        setError(null);
      })
      .catch((e) => {
        if (token !== request.current) return;
        setError(errorMessage(e));
      })
      .finally(() => {
        if (token === request.current) setLoading(false);
      });
  }, [source, cwd, reloadToken, refreshToken]);

  useEffect(() => {
    onSelectionChange(new Set());
    setFocusedIndex(0);
    setFilter("");
    anchor.current = 0;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [cwd]);

  const entries = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    const visible = (listing?.entries ?? []).filter(
      (entry) =>
        (showHidden || !entry.hidden) && (needle === "" || entry.name.toLowerCase().includes(needle)),
    );
    return sortEntries(visible, sort);
  }, [listing, filter, showHidden, sort]);

  // Without this the footer counts only what survived the filters, so a file
  // that arrived but is hidden (any dotfile) looks like it never transferred.
  const hiddenCount = useMemo(
    () => (showHidden ? 0 : (listing?.entries ?? []).filter((entry) => entry.hidden).length),
    [listing, showHidden],
  );

  const refresh = useCallback(() => setReloadToken((n) => n + 1), []);

  const goToParent = useCallback(() => {
    const parent = source.parentOf(cwd);
    if (parent) onCwdChange(parent);
  }, [source, cwd, onCwdChange]);

  const open = useCallback(
    (entry: FileEntry) => {
      if (isDirLike(entry)) onCwdChange(source.join(cwd, entry.name));
      else onOpenFile?.(entry);
    },
    [source, cwd, onCwdChange, onOpenFile],
  );

  const selectRange = useCallback(
    (from: number, to: number) => {
      const [start, end] = from < to ? [from, to] : [to, from];
      onSelectionChange(new Set(entries.slice(start, end + 1).map((entry) => entry.name)));
    },
    [entries, onSelectionChange],
  );

  // Right-clicking a row outside the current selection selects just that row,
  // the way Explorer does, so a menu action never acts on something invisible.
  const handleRowContextMenu = (index: number, event: React.MouseEvent) => {
    event.preventDefault();
    const entry = entries[index];
    if (!entry) return;
    setFocusedIndex(index);
    if (!selection.has(entry.name)) {
      anchor.current = index;
      onSelectionChange(new Set([entry.name]));
    }
    setMenu({ x: event.clientX, y: event.clientY, entry });
  };

  const handleRowClick = (index: number, event: React.MouseEvent) => {
    const entry = entries[index];
    setFocusedIndex(index);
    if (event.shiftKey) {
      selectRange(anchor.current, index);
      return;
    }
    anchor.current = index;
    if (event.ctrlKey || event.metaKey) {
      const next = new Set(selection);
      if (next.has(entry.name)) next.delete(entry.name);
      else next.add(entry.name);
      onSelectionChange(next);
      return;
    }
    onSelectionChange(new Set([entry.name]));
  };

  const moveFocus = (index: number, extend: boolean) => {
    const clamped = Math.max(0, Math.min(entries.length - 1, index));
    setFocusedIndex(clamped);
    if (extend) {
      selectRange(anchor.current, clamped);
    } else {
      anchor.current = clamped;
      const entry = entries[clamped];
      if (entry) onSelectionChange(new Set([entry.name]));
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement) return;
    const focused = entries[focusedIndex];

    if (event.key === "ArrowUp" && event.altKey) goToParent();
    else if (event.key === "ArrowDown") moveFocus(focusedIndex + 1, event.shiftKey);
    else if (event.key === "ArrowUp") moveFocus(focusedIndex - 1, event.shiftKey);
    else if (event.key === "Home") moveFocus(0, event.shiftKey);
    else if (event.key === "End") moveFocus(entries.length - 1, event.shiftKey);
    else if (event.key === "Enter" && focused) open(focused);
    else if (event.key === "Backspace") goToParent();
    else if (event.key === "F5") refresh();
    else if (event.key === "F7" && source.canMutate) setNewFolderOpen(true);
    else if (event.key === "Delete") requestDelete();
    else if (event.key === " " && focused) {
      const next = new Set(selection);
      if (next.has(focused.name)) next.delete(focused.name);
      else next.add(focused.name);
      onSelectionChange(next);
    } else if (event.key.toLowerCase() === "a" && (event.ctrlKey || event.metaKey)) {
      onSelectionChange(new Set(entries.map((entry) => entry.name)));
    } else {
      return;
    }
    event.preventDefault();
  };

  const requestDelete = useCallback(() => {
    if (!source.remove) return;
    const targets = entries.filter((entry) => selection.has(entry.name));
    if (targets.length > 0) setPendingDelete(targets);
  }, [source, entries, selection]);

  const confirmDelete = async () => {
    const targets = pendingDelete ?? [];
    setPendingDelete(null);
    setDeleting(true);
    // One failure should not strand the rest of the selection.
    const failures: string[] = [];
    for (const entry of targets) {
      try {
        await source.remove!(entry);
      } catch (e) {
        failures.push(`${entry.name}: ${errorMessage(e)}`);
      }
    }
    setDeleting(false);
    setError(failures.length > 0 ? failures.join("; ") : null);
    onSelectionChange(new Set());
    refresh();
  };

  const createFolder = async (name: string) => {
    try {
      await source.mkdir!(cwd, name);
    } catch (e) {
      throw new Error(errorMessage(e));
    }
    setNewFolderOpen(false);
    refresh();
  };

  return (
    <div className="file-pane">
      <div className="file-pane-header">
        <span className="file-pane-label">{source.label}</span>
        <div className="file-pane-tools">
          <input
            className="file-filter"
            placeholder="Filter"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <label className="file-toggle">
            <input type="checkbox" checked={showHidden} onChange={(e) => setShowHidden(e.target.checked)} />
            Hidden
          </label>
        </div>
      </div>

      <PathBar
        source={source}
        cwd={cwd}
        roots={roots}
        busy={loading || deleting}
        onNavigate={onCwdChange}
        onRefresh={refresh}
        onNewFolder={() => setNewFolderOpen(true)}
      />

      {error && <div className="field-error file-pane-error">{error}</div>}

      <div className="file-table-wrap" tabIndex={0} onKeyDown={handleKeyDown}>
        {!listing || entries.length === 0 ? (
          <div className="file-pane-empty">{loading ? "Loading…" : "Nothing here."}</div>
        ) : (
          <FileTable
            entries={entries}
            hasParent={listing?.parent != null}
            selection={selection}
            focusedIndex={focusedIndex}
            sort={sort}
            onSortChange={(key: SortKey) =>
              setSort((current) =>
                current.key === key
                  ? { key, dir: current.dir === "asc" ? "desc" : "asc" }
                  : { key, dir: "asc" },
              )
            }
            onRowClick={handleRowClick}
            onRowContextMenu={handleRowContextMenu}
            onRowOpen={open}
            onParentOpen={goToParent}
          />
        )}
      </div>

      {menu && (
        <div
          className="context-menu"
          style={{ left: menu.x, top: menu.y }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {actions.map((action) => (
            <button
              key={action.label}
              className="ghost"
              disabled={action.enabled ? !action.enabled(menu.entry) : false}
              onClick={() => {
                setMenu(null);
                action.run(menu.entry);
              }}
            >
              {action.label}
            </button>
          ))}
          {source.remove && (
            <button
              className="ghost danger-text"
              onClick={() => {
                setMenu(null);
                requestDelete();
              }}
            >
              Delete
            </button>
          )}
          <div className="context-menu-separator" />
          <button
            className="ghost"
            onClick={() => {
              setMenu(null);
              refresh();
            }}
          >
            Refresh
          </button>
          {source.canMutate && (
            <button
              className="ghost"
              onClick={() => {
                setMenu(null);
                setNewFolderOpen(true);
              }}
            >
              New folder
            </button>
          )}
        </div>
      )}

      <div className="file-pane-footer">
        {deleting && "Deleting… · "}
        {entries.length} item{entries.length === 1 ? "" : "s"}
        {hiddenCount > 0 && (
          <button className="ghost file-hidden-hint" onClick={() => setShowHidden(true)}>
            {hiddenCount} hidden
          </button>
        )}
        {selection.size > 0 && ` · ${selection.size} selected`}
        {listing?.truncated && ` · showing the first entries only — use the filter`}
      </div>

      {pendingDelete && (
        <ConfirmDialog
          title="Delete"
          danger
          confirmLabel={`Delete ${pendingDelete.length === 1 ? "item" : `${pendingDelete.length} items`}`}
          onCancel={() => setPendingDelete(null)}
          onConfirm={confirmDelete}
          message={
            <>
              <p>
                Permanently delete{" "}
                {pendingDelete.length === 1 ? (
                  <strong>{pendingDelete[0].name}</strong>
                ) : (
                  <strong>{pendingDelete.length} items</strong>
                )}{" "}
                from {source.label}?
              </p>
              {pendingDelete.some(isDirLike) && (
                <p className="hint">
                  The selection includes folders — everything inside them is deleted too.
                </p>
              )}
              <p className="hint">This cannot be undone.</p>
            </>
          }
        />
      )}

      {newFolderOpen && (
        <PromptDialog
          title="New folder"
          label="Folder name"
          confirmLabel="Create"
          onConfirm={createFolder}
          onCancel={() => setNewFolderOpen(false)}
        />
      )}
    </div>
  );
}
