import { useEffect, useState } from "react";
import { sudoForget, sudoSetRemember, sudoStatus } from "../api/commands";
import { errorMessage } from "../api/errors";
import type { SudoStatus } from "../api/types";
import { Modal } from "./Modal";

/**
 * Shows whether a sudo password is saved for a profile and lets the user
 * forget it or stop remembering it. The password itself never reaches the UI:
 * terminals save it after the first successful `[sudo] password for` prompt
 * and answer later prompts themselves (src-tauri/src/ssh/sudo.rs).
 */
export function SudoPasswordDialog({
  profileId,
  profileName,
  onClose,
}: {
  profileId: string;
  profileName: string;
  onClose: () => void;
}) {
  const [status, setStatus] = useState<SudoStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    sudoStatus(profileId).then(setStatus, (e) => setError(errorMessage(e)));
  }, [profileId]);

  const run = (action: () => Promise<SudoStatus>) => {
    setError(null);
    action().then(setStatus, (e) => setError(errorMessage(e)));
  };

  return (
    <Modal title={`Sudo password: ${profileName}`} onClose={onClose} width={460}>
      {status === null ? (
        <p className="hint">{error ?? "Loading..."}</p>
      ) : (
        <>
          <p className="confirm-message">
            {!status.remember
              ? "Not remembered. sudo asks for the password every time."
              : status.saved
                ? "Saved in the vault. Terminals answer sudo prompts on this host for you."
                : "Nothing saved yet. The next password you type at a sudo prompt is saved once sudo accepts it."}
          </p>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={status.remember}
              onChange={(e) => run(() => sudoSetRemember(profileId, e.target.checked))}
            />
            Remember the sudo password for this host
          </label>
          {error && <div className="field-error">{error}</div>}
          <div className="modal-actions">
            <button className="danger" disabled={!status.saved} onClick={() => run(() => sudoForget(profileId))}>
              Forget saved password
            </button>
            <button className="primary" onClick={onClose}>
              Done
            </button>
          </div>
        </>
      )}
    </Modal>
  );
}
