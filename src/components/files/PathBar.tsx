import { useState } from "react";
import type { FileSource } from "./fileSource";

export function PathBar({
  source,
  cwd,
  roots,
  busy,
  onNavigate,
  onRefresh,
  onNewFolder,
}: {
  source: FileSource;
  cwd: string;
  roots: { label: string; path: string }[];
  busy: boolean;
  onNavigate: (path: string) => void;
  onRefresh: () => void;
  onNewFolder: () => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  const parent = source.parentOf(cwd);

  const commit = () => {
    const next = draft?.trim();
    setDraft(null);
    if (next) onNavigate(next);
  };

  return (
    <div className="file-pathbar">
      <button className="ghost" title="Parent directory" disabled={!parent} onClick={() => parent && onNavigate(parent)}>
        ↑
      </button>
      <button className="ghost" title="Refresh" disabled={busy} onClick={onRefresh}>
        ⟳
      </button>

      {draft === null ? (
        <div className="file-crumbs" onDoubleClick={() => setDraft(cwd)} title="Double-click to edit">
          {source.segments(cwd).map((crumb, index, all) => (
            <button
              key={crumb.path}
              className={`ghost file-crumb ${index === all.length - 1 ? "current" : ""}`}
              onClick={() => onNavigate(crumb.path)}
            >
              {crumb.label}
            </button>
          ))}
        </div>
      ) : (
        <input
          className="file-path-input"
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") commit();
            if (e.key === "Escape") setDraft(null);
          }}
        />
      )}

      {roots.length > 0 && (
        <select
          className="file-roots"
          value=""
          onChange={(e) => {
            if (e.target.value) onNavigate(e.target.value);
          }}
        >
          <option value="">Go to…</option>
          {roots.map((root) => (
            <option key={root.path} value={root.path}>
              {root.label}
            </option>
          ))}
        </select>
      )}

      {source.canMutate && (
        <button className="ghost" title="New folder (F7)" onClick={onNewFolder}>
          + Folder
        </button>
      )}
    </div>
  );
}
