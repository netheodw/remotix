import { useEffect, useMemo, useState } from "react";
import { importTermiusApply, importTermiusPreview } from "../api/commands";
import { errorMessage } from "../api/errors";
import { authKindLabel, type TermiusImportReport, type TermiusPreview } from "../api/types";
import { Modal } from "../components/Modal";
import { useData } from "../state/DataContext";
import { defaultSelection, describeForward, forwardKey, listenerConflicts } from "./termiusImport";

export function TermiusImportModal({ path, onClose }: { path: string; onClose: () => void }) {
  const { forwards, refreshProfiles, refreshForwards } = useData();
  const [preview, setPreview] = useState<TermiusPreview | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [loadError, setLoadError] = useState<string | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<TermiusImportReport | null>(null);

  useEffect(() => {
    let cancelled = false;
    importTermiusPreview(path)
      .then((p) => {
        if (cancelled) return;
        setPreview(p);
        setSelected(new Set(defaultSelection(p.hosts)));
      })
      .catch((e) => !cancelled && setLoadError(errorMessage(e)));
    return () => {
      cancelled = true;
    };
  }, [path]);

  const hosts = preview?.hosts ?? [];
  const importable = hosts.filter((h) => !h.problem);
  const conflicts = useMemo(() => listenerConflicts(hosts, selected, forwards), [hosts, selected, forwards]);
  const forwardCount = hosts
    .filter((h) => selected.has(h.index))
    .reduce((n, h) => n + h.forwards.filter((f, i) => !f.problem && !conflicts.has(forwardKey(h.index, i))).length, 0);

  const toggle = (index: number, on: boolean) =>
    setSelected((prev) => {
      const next = new Set(prev);
      if (on) next.add(index);
      else next.delete(index);
      return next;
    });

  const runImport = async () => {
    setBusy(true);
    setImportError(null);
    try {
      const result = await importTermiusApply(path, [...selected]);
      await Promise.all([refreshProfiles(), refreshForwards()]);
      setReport(result);
    } catch (e) {
      setImportError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const close = busy ? () => {} : onClose;

  if (report) {
    return (
      <Modal title="Import from Termius" onClose={close} width={560}>
        <div className="field-success">
          Imported {report.profiles_created} SSH {report.profiles_created === 1 ? "profile" : "profiles"} and{" "}
          {report.forwards_created} port {report.forwards_created === 1 ? "forward" : "forwards"}.
        </div>
        {report.skipped.length > 0 && (
          <div className="warning-box">
            <strong>Skipped</strong>
            <ul className="import-skipped">
              {report.skipped.map((s, i) => (
                <li key={i}>
                  {s.name}: {s.reason}
                </li>
              ))}
            </ul>
          </div>
        )}
        <p className="hint">
          The export file still contains your passwords in plain text - delete it once you've confirmed the import.
        </p>
        <div className="modal-actions">
          <button className="primary" onClick={onClose}>
            Done
          </button>
        </div>
      </Modal>
    );
  }

  return (
    <Modal title="Import from Termius" onClose={close} width={960}>
      {loadError ? (
        <>
          <div className="field-error">{loadError}</div>
          <div className="modal-actions">
            <button onClick={onClose}>Close</button>
          </div>
        </>
      ) : !preview ? (
        <p className="hint">Reading export...</p>
      ) : hosts.length === 0 ? (
        <>
          <p className="hint">This export contains no hosts.</p>
          <div className="modal-actions">
            <button onClick={onClose}>Close</button>
          </div>
        </>
      ) : (
        <>
          <p className="hint">
            Imports each host's name, address, port, username, credentials, and port forwards as ungrouped profiles.
            Credentials are encrypted into the vault and never shown here. Forwards are imported stopped.
          </p>
          <table className="data-table import-table">
            <thead>
              <tr>
                <th>
                  <input
                    type="checkbox"
                    aria-label="Select all"
                    checked={importable.length > 0 && importable.every((h) => selected.has(h.index))}
                    onChange={(e) => setSelected(new Set(e.target.checked ? importable.map((h) => h.index) : []))}
                  />
                </th>
                <th>Name</th>
                <th>Host</th>
                <th>User</th>
                <th>Auth</th>
                <th>Port forwards</th>
              </tr>
            </thead>
            <tbody>
              {hosts.map((h) => (
                <tr key={h.index} className={h.problem ? "disabled" : ""}>
                  <td>
                    <input
                      type="checkbox"
                      aria-label={`Import ${h.name}`}
                      disabled={!!h.problem}
                      checked={selected.has(h.index)}
                      onChange={(e) => toggle(h.index, e.target.checked)}
                    />
                  </td>
                  <td>
                    {h.name}
                    {h.problem && <div className="import-note error">{h.problem}</div>}
                    {h.existing_profile_name && (
                      <div className="import-note warn">Already exists as "{h.existing_profile_name}"</div>
                    )}
                  </td>
                  <td>
                    {h.hostname}:{h.port}
                  </td>
                  <td>{h.username}</td>
                  <td>{h.authentication_kind ? authKindLabel(h.authentication_kind) : "-"}</td>
                  <td>
                    {h.forwards.length === 0 ? (
                      <span className="hint">None</span>
                    ) : (
                      <ul className="import-forwards">
                        {h.forwards.map((f, i) => {
                          const note = f.problem ?? (selected.has(h.index) ? conflicts.get(forwardKey(h.index, i)) : undefined);
                          return (
                            <li key={i}>
                              <span>{f.name}</span> <code>{describeForward(f)}</code>
                              {note && <div className="import-note warn">Will be skipped: {note}</div>}
                            </li>
                          );
                        })}
                      </ul>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          {importError && <div className="field-error import-error">{importError}</div>}
          <div className="modal-actions import-actions">
            <span className="hint import-summary">
              {selected.size} {selected.size === 1 ? "profile" : "profiles"} and {forwardCount} port{" "}
              {forwardCount === 1 ? "forward" : "forwards"} will be imported
            </span>
            <button onClick={onClose} disabled={busy}>
              Cancel
            </button>
            <button className="primary" onClick={runImport} disabled={busy || selected.size === 0}>
              {busy ? "Importing..." : "Import"}
            </button>
          </div>
        </>
      )}
    </Modal>
  );
}
