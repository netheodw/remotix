import { Fragment, useCallback, useEffect, useState } from "react";
import { sessionLogClear, sessionLogCommands, sessionLogDelete, sessionLogList } from "../api/commands";
import { errorMessage } from "../api/errors";
import type { SessionCommand, TerminalSessionSummary } from "../api/types";
import { ConfirmDialog } from "../components/Modal";
import { useData } from "../state/DataContext";
import { formatSessionDuration, formatTimestamp, sessionStatusLabel, sessionStatusTone } from "./sessionHistory";

const SEARCH_DEBOUNCE_MS = 250;

/**
 * Every terminal session (including failed connects) and the commands run in
 * it, newest first. Read-only: it is for looking back, not reopening.
 */
export function HistoryScreen() {
  const { profiles } = useData();
  const [sessions, setSessions] = useState<TerminalSessionSummary[]>([]);
  const [search, setSearch] = useState("");
  const [profileId, setProfileId] = useState<string>("");
  const [expanded, setExpanded] = useState<string | null>(null);
  const [commands, setCommands] = useState<Record<string, SessionCommand[]>>({});
  const [error, setError] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  const load = useCallback(() => {
    sessionLogList({ profileId: profileId || null, search: search.trim() || null })
      .then((list) => {
        setSessions(list);
        setError(null);
      })
      .catch((e) => setError(errorMessage(e)));
  }, [profileId, search]);

  useEffect(() => {
    const t = setTimeout(load, SEARCH_DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [load]);

  const toggle = (id: string) => {
    if (expanded === id) {
      setExpanded(null);
      return;
    }
    setExpanded(id);
    // Always re-read: a live session keeps adding commands.
    sessionLogCommands(id)
      .then((list) => setCommands((prev) => ({ ...prev, [id]: list })))
      .catch((e) => setError(errorMessage(e)));
  };

  const remove = async (id: string) => {
    try {
      await sessionLogDelete(id);
      if (expanded === id) setExpanded(null);
      load();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const needle = search.trim().toLowerCase();

  return (
    <div className="history-screen">
      <div className="list-pane-header">
        <input
          className="search-input"
          placeholder="Search host, profile or command..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        <select value={profileId} onChange={(e) => setProfileId(e.target.value)} aria-label="Filter by profile">
          <option value="">All profiles</option>
          {profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <div className="history-actions">
          <button onClick={load}>Refresh</button>
          <button className="danger" disabled={sessions.length === 0} onClick={() => setConfirmClear(true)}>
            Clear history
          </button>
        </div>
      </div>

      {error && <div className="field-error list-error">{error}</div>}

      {sessions.length === 0 ? (
        <div className="empty-state">
          <h3>{needle || profileId ? "No sessions match this filter" : "No sessions yet"}</h3>
          <p>Every terminal you open is listed here with the commands you ran in it.</p>
        </div>
      ) : (
        <table className="data-table history-table">
          <thead>
            <tr>
              <th className="caret-col"></th>
              <th>Started</th>
              <th>Profile</th>
              <th>Host</th>
              <th>Duration</th>
              <th>Status</th>
              <th>Commands</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {sessions.map((s) => {
              const live = s.ended_at === null && s.status !== "interrupted";
              const isOpen = expanded === s.id;
              const list = commands[s.id];
              return (
                <Fragment key={s.id}>
                  <tr className={`history-row${isOpen ? " selected" : ""}`} onClick={() => toggle(s.id)}>
                    <td className="caret-col">{isOpen ? "▾" : "▸"}</td>
                    <td>{formatTimestamp(s.started_at)}</td>
                    <td>
                      {s.profile_name}
                      {s.ssh_profile_id === null && <span className="hint"> (deleted)</span>}
                    </td>
                    <td>
                      {s.username}@{s.hostname}
                      {s.port !== 22 && `:${s.port}`}
                    </td>
                    <td>{live ? "-" : (formatSessionDuration(s.connected_at ?? s.started_at, s.ended_at) ?? "unknown")}</td>
                    <td title={s.end_reason ?? undefined}>
                      <span className={`status-dot ${sessionStatusTone(s)}`} />
                      {sessionStatusLabel(s, live)}
                    </td>
                    <td>{s.command_count}</td>
                    <td className="row-actions">
                      <button
                        className="ghost"
                        title="Delete this session from the history"
                        disabled={live}
                        onClick={(e) => {
                          e.stopPropagation();
                          void remove(s.id);
                        }}
                      >
                        Delete
                      </button>
                    </td>
                  </tr>
                  {isOpen && (
                    <tr className="history-detail">
                      <td></td>
                      <td colSpan={7}>
                        {s.end_reason && <div className="hint">Ended: {s.end_reason}</div>}
                        {list === undefined ? (
                          <div className="hint">Loading...</div>
                        ) : list.length === 0 ? (
                          <div className="hint">No commands were recorded in this session.</div>
                        ) : (
                          <ol className="history-commands">
                            {list.map((c) => (
                              <li key={c.id} className={needle && c.command.toLowerCase().includes(needle) ? "match" : undefined}>
                                <span className="history-time">{formatTimestamp(c.executed_at, true)}</span>
                                <code>{c.command}</code>
                              </li>
                            ))}
                          </ol>
                        )}
                      </td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
          </tbody>
        </table>
      )}

      {confirmClear && (
        <ConfirmDialog
          title="Clear the session history?"
          message="Deletes every finished session and the commands recorded in it. Sessions that are still open stay. The Up/Down command history is not affected."
          confirmLabel="Clear"
          danger
          onCancel={() => setConfirmClear(false)}
          onConfirm={async () => {
            setConfirmClear(false);
            try {
              await sessionLogClear();
              setExpanded(null);
              load();
            } catch (e) {
              setError(errorMessage(e));
            }
          }}
        />
      )}
    </div>
  );
}
