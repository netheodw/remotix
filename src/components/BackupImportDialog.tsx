import { useState } from "react";
import { backupImport } from "../api/commands";
import { errorMessage } from "../api/errors";
import { Modal } from "./Modal";

/**
 * Asks for the backup's master password and imports it. On success every
 * session is closed and the vault is locked; `onImported` should refresh the
 * vault status so the unlock screen shows (unlock with the backup's password).
 */
export function BackupImportDialog({
  path,
  replacesExisting,
  onClose,
  onImported,
}: {
  path: string;
  /** true when this install already has a vault whose data will be replaced. */
  replacesExisting: boolean;
  onClose: () => void;
  onImported: () => void;
}) {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await backupImport(path, password);
      onImported();
    } catch (e) {
      setError(errorMessage(e));
      setBusy(false);
    }
  };

  return (
    <Modal title="Import backup" onClose={busy ? () => {} : onClose} width={480}>
      <p className="hint">
        <code>{path}</code>
      </p>
      {replacesExisting && (
        <div className="warning-box">
          <strong>This replaces everything in Remotix on this computer</strong> with the backup: profiles, passwords,
          forwards, notes and history. Open terminals and forwards are closed. A copy of the current data is kept in
          the data folder as <code>remotix-before-import-...db</code>.
        </div>
      )}
      <label className="field">
        <span>Master password of the backup</span>
        <input
          type="password"
          value={password}
          autoFocus
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && password && !busy && void submit()}
        />
      </label>
      <p className="hint">
        The password the vault had when the backup was made. You unlock with it afterwards. Windows Hello has to be
        turned on again on this computer.
      </p>
      {error && <div className="field-error">{error}</div>}
      <div className="modal-actions">
        <button onClick={onClose} disabled={busy}>
          Cancel
        </button>
        <button className={replacesExisting ? "danger" : "primary"} onClick={submit} disabled={!password || busy}>
          {busy ? "Importing..." : replacesExisting ? "Replace and import" : "Import"}
        </button>
      </div>
    </Modal>
  );
}
