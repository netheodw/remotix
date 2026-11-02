import { useCallback, useEffect, useMemo, useState } from "react";
import {
  snippetCategoriesCreate,
  snippetCategoriesDelete,
  snippetCategoriesList,
  snippetCategoriesRename,
  snippetsCreate,
  snippetsDelete,
  snippetsList,
  snippetsUpdate,
} from "../api/commands";
import { errorMessage } from "../api/errors";
import type { Snippet, SnippetCategory, SnippetInput } from "../api/types";
import { ConfirmDialog, Modal } from "./Modal";
import { PromptDialog } from "./PromptDialog";
import { fillPlaceholders, filterSnippets, snippetPlaceholders } from "./toolbox";

const COLLAPSED_KEY = "remotix.toolboxCollapsed";

function storedCollapsed(): Set<string> {
  try {
    const raw = localStorage.getItem(COLLAPSED_KEY);
    if (raw) return new Set(JSON.parse(raw) as string[]);
  } catch {
    // Storage unavailable or corrupt: start with everything expanded.
  }
  return new Set();
}

/**
 * Categorized command snippets docked beside the terminal. A snippet is
 * typed into the active tab (Insert) or typed and executed (Run); one with
 * `{{placeholders}}` asks for their values first.
 */
export function ToolboxPanel({
  canSend,
  onSend,
}: {
  /** False while the active tab has no connected session. */
  canSend: boolean;
  onSend: (command: string, run: boolean) => void;
}) {
  const [categories, setCategories] = useState<SnippetCategory[]>([]);
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [collapsed, setCollapsed] = useState(storedCollapsed);
  const [filling, setFilling] = useState<{ snippet: Snippet; run: boolean } | null>(null);
  const [editing, setEditing] = useState<Snippet | { categoryId: string | null } | null>(null);
  const [deletingSnippet, setDeletingSnippet] = useState<Snippet | null>(null);
  const [categoryPrompt, setCategoryPrompt] = useState<SnippetCategory | "new" | null>(null);
  const [deletingCategory, setDeletingCategory] = useState<SnippetCategory | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [cats, items] = await Promise.all([snippetCategoriesList(), snippetsList()]);
      setCategories(cats);
      setSnippets(items);
      setLoadError(null);
    } catch (e) {
      setLoadError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    try {
      localStorage.setItem(COLLAPSED_KEY, JSON.stringify([...collapsed]));
    } catch {
      // Only a convenience.
    }
  }, [collapsed]);

  const searching = search.trim().length > 0;
  const byCategory = useMemo(() => {
    const visible = filterSnippets(snippets, search);
    const map = new Map<string, Snippet[]>();
    visible.forEach((s) => map.set(s.category_id, [...(map.get(s.category_id) ?? []), s]));
    return map;
  }, [snippets, search]);

  const toggleCategory = (id: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const use = (snippet: Snippet, run: boolean) => {
    if (!canSend) return;
    if (snippetPlaceholders(snippet.command).length > 0) setFilling({ snippet, run });
    else onSend(snippet.command, run);
  };

  return (
    <div className="toolbox-panel">
      <div className="toolbox-header">
        <h3>Toolbox</h3>
        <div className="toolbox-header-actions">
          <button className="ghost" title="New category" onClick={() => setCategoryPrompt("new")}>
            + Category
          </button>
          <button className="primary" title="New command" onClick={() => setEditing({ categoryId: null })}>
            + Command
          </button>
        </div>
      </div>
      <input
        className="search-input toolbox-search"
        placeholder="Search commands..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
      />
      {!canSend && <div className="hint toolbox-notice">Connect a terminal to insert or run commands.</div>}
      {loadError && <div className="field-error toolbox-notice">{loadError}</div>}

      <div className="toolbox-list">
        {categories.map((cat) => {
          const items = byCategory.get(cat.id) ?? [];
          if (searching && items.length === 0) return null;
          const open = searching || !collapsed.has(cat.id);
          return (
            <div key={cat.id} className="toolbox-category">
              <div className="toolbox-category-row" onClick={() => toggleCategory(cat.id)}>
                <span className="toolbox-caret">{open ? "▾" : "▸"}</span>
                <span className="toolbox-category-name">{cat.name}</span>
                <span className="group-count">{items.length}</span>
                <span className="toolbox-row-actions" onClick={(e) => e.stopPropagation()}>
                  <button className="ghost" title="Add a command to this category" onClick={() => setEditing({ categoryId: cat.id })}>
                    +
                  </button>
                  <button className="ghost" title="Rename category" onClick={() => setCategoryPrompt(cat)}>
                    ✎
                  </button>
                  <button className="ghost" title="Delete category" onClick={() => setDeletingCategory(cat)}>
                    ✕
                  </button>
                </span>
              </div>
              {open &&
                items.map((s) => (
                  <div
                    key={s.id}
                    className={`toolbox-snippet${canSend ? "" : " disabled"}`}
                    title={s.description ? `${s.description}\n\n${s.command}` : s.command}
                    onClick={() => use(s, false)}
                  >
                    <div className="toolbox-snippet-text">
                      <span className="toolbox-snippet-name">{s.name}</span>
                      <code className="toolbox-snippet-command">{s.command}</code>
                    </div>
                    <span className="toolbox-row-actions" onClick={(e) => e.stopPropagation()}>
                      <button className="ghost" title="Insert without running" disabled={!canSend} onClick={() => use(s, false)}>
                        ⤓
                      </button>
                      <button className="ghost" title="Run now" disabled={!canSend} onClick={() => use(s, true)}>
                        ▶
                      </button>
                      <button className="ghost" title="Edit" onClick={() => setEditing(s)}>
                        ✎
                      </button>
                      <button className="ghost" title="Delete" onClick={() => setDeletingSnippet(s)}>
                        ✕
                      </button>
                    </span>
                  </div>
                ))}
              {open && !searching && items.length === 0 && <div className="hint toolbox-empty">No commands yet.</div>}
            </div>
          );
        })}
        {searching && byCategory.size === 0 && <div className="hint toolbox-empty">No commands match.</div>}
      </div>

      {filling && (
        <PlaceholderDialog
          snippet={filling.snippet}
          initialRun={filling.run}
          onCancel={() => setFilling(null)}
          onSubmit={(command, run) => {
            setFilling(null);
            onSend(command, run);
          }}
        />
      )}

      {editing && (
        <SnippetEditorModal
          snippet={"id" in editing ? editing : null}
          defaultCategoryId={"id" in editing ? editing.category_id : editing.categoryId ?? categories[0]?.id ?? ""}
          categories={categories}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            setEditing(null);
            await refresh();
          }}
        />
      )}

      {deletingSnippet && (
        <ConfirmDialog
          title={`Delete "${deletingSnippet.name}"?`}
          message={<code>{deletingSnippet.command}</code>}
          confirmLabel="Delete"
          danger
          onCancel={() => setDeletingSnippet(null)}
          onConfirm={async () => {
            await snippetsDelete(deletingSnippet.id);
            setDeletingSnippet(null);
            await refresh();
          }}
        />
      )}

      {categoryPrompt && (
        <PromptDialog
          title={categoryPrompt === "new" ? "New Category" : "Rename Category"}
          label="Category name"
          initialValue={categoryPrompt === "new" ? "" : categoryPrompt.name}
          onCancel={() => setCategoryPrompt(null)}
          onConfirm={async (name) => {
            try {
              if (categoryPrompt === "new") await snippetCategoriesCreate(name);
              else await snippetCategoriesRename(categoryPrompt.id, name);
            } catch (e) {
              throw errorMessage(e);
            }
            setCategoryPrompt(null);
            await refresh();
          }}
        />
      )}

      {deletingCategory && (
        <ConfirmDialog
          title={`Delete category "${deletingCategory.name}"?`}
          message={`This also deletes the ${snippets.filter((s) => s.category_id === deletingCategory.id).length} command(s) in it.`}
          confirmLabel="Delete"
          danger
          onCancel={() => setDeletingCategory(null)}
          onConfirm={async () => {
            await snippetCategoriesDelete(deletingCategory.id);
            setDeletingCategory(null);
            await refresh();
          }}
        />
      )}
    </div>
  );
}

function PlaceholderDialog({
  snippet,
  initialRun,
  onCancel,
  onSubmit,
}: {
  snippet: Snippet;
  initialRun: boolean;
  onCancel: () => void;
  onSubmit: (command: string, run: boolean) => void;
}) {
  const names = useMemo(() => snippetPlaceholders(snippet.command), [snippet.command]);
  const [values, setValues] = useState<Record<string, string>>({});
  const preview = fillPlaceholders(snippet.command, values);
  const complete = names.every((n) => values[n]?.trim());

  const submit = (run: boolean) => {
    if (run && !complete) return;
    onSubmit(preview, run);
  };

  return (
    <Modal title={snippet.name} onClose={onCancel} width={460}>
      {snippet.description && <p className="hint">{snippet.description}</p>}
      {names.map((name, i) => (
        <label key={name} className="field">
          <span>{name}</span>
          <input
            autoFocus={i === 0}
            value={values[name] ?? ""}
            onChange={(e) => setValues((v) => ({ ...v, [name]: e.target.value }))}
            onKeyDown={(e) => e.key === "Enter" && submit(initialRun)}
          />
        </label>
      ))}
      <pre className="toolbox-preview">{preview}</pre>
      <div className="modal-actions">
        <button onClick={onCancel}>Cancel</button>
        <button className={initialRun ? undefined : "primary"} onClick={() => submit(false)}>
          Insert
        </button>
        <button className={initialRun ? "primary" : undefined} disabled={!complete} onClick={() => submit(true)}>
          Run
        </button>
      </div>
    </Modal>
  );
}

function SnippetEditorModal({
  snippet,
  defaultCategoryId,
  categories,
  onClose,
  onSaved,
}: {
  snippet: Snippet | null;
  defaultCategoryId: string;
  categories: SnippetCategory[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const [form, setForm] = useState<SnippetInput>({
    category_id: defaultCategoryId,
    name: snippet?.name ?? "",
    command: snippet?.command ?? "",
    description: snippet?.description ?? "",
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<SnippetInput>) => setForm((f) => ({ ...f, ...patch }));

  const save = async () => {
    if (!form.category_id) {
      setError("Create a category first.");
      return;
    }
    if (!form.name.trim() || !form.command.trim()) {
      setError("Name and command are required.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      if (snippet) await snippetsUpdate(snippet.id, form);
      else await snippetsCreate(form);
      onSaved();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={snippet ? "Edit Command" : "New Command"} onClose={onClose} width={520}>
      <div className="field-row">
        <label className="field" style={{ flex: 1 }}>
          <span>Name</span>
          <input autoFocus value={form.name} onChange={(e) => set({ name: e.target.value })} />
        </label>
        <label className="field" style={{ flex: 1 }}>
          <span>Category</span>
          <select value={form.category_id} onChange={(e) => set({ category_id: e.target.value })}>
            {categories.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
      </div>
      <label className="field">
        <span>Command</span>
        <textarea
          rows={4}
          className="toolbox-command-input"
          spellCheck={false}
          value={form.command}
          onChange={(e) => set({ command: e.target.value })}
        />
      </label>
      <p className="hint toolbox-editor-hint">
        Use <code>{"{{name}}"}</code> for values to fill in each time, e.g. <code>{"tail -f {{file}}"}</code>.
      </p>
      <label className="field">
        <span>Description (optional)</span>
        <input value={form.description} onChange={(e) => set({ description: e.target.value })} />
      </label>
      {error && <div className="field-error">{error}</div>}
      <div className="modal-actions">
        <button onClick={onClose} disabled={busy}>
          Cancel
        </button>
        <button className="primary" onClick={save} disabled={busy}>
          Save
        </button>
      </div>
    </Modal>
  );
}
