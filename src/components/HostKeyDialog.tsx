import { useState } from "react";
import { knownHostsTrust } from "../api/commands";
import { Modal } from "./Modal";

export interface HostKeyDecision {
  kind: "untrusted" | "changed";
  host: string;
  port: number;
  keyAlgorithm: string;
  fingerprint: string;
  previousFingerprint?: string;
  hostKeyB64: string;
}

export function HostKeyDialog({
  decision,
  onResolved,
  onCancel,
}: {
  decision: HostKeyDecision;
  onResolved: () => void;
  onCancel: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [confirmReplace, setConfirmReplace] = useState(false);
  const isChanged = decision.kind === "changed";

  const trust = async () => {
    setBusy(true);
    try {
      await knownHostsTrust({
        host: decision.host,
        port: decision.port,
        keyAlgorithm: decision.keyAlgorithm,
        hostKeyB64: decision.hostKeyB64,
        fingerprintSha256: decision.fingerprint,
      });
      onResolved();
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={isChanged ? "Host key has changed" : "Verify host key"} onClose={onCancel} width={520}>
      {isChanged ? (
        <div className="host-key-warning">
          <p>
            <strong>Warning:</strong> the SSH key presented by <code>{decision.host}:{decision.port}</code> does not
            match the key this computer previously trusted. This can mean the server was reinstalled - or that
            something is intercepting the connection.
          </p>
          <div className="fingerprint-row">
            <span>Previously trusted</span>
            <code>{decision.previousFingerprint}</code>
          </div>
          <div className="fingerprint-row">
            <span>Presented now</span>
            <code>{decision.fingerprint}</code>
          </div>
          <label className="checkbox-row">
            <input type="checkbox" checked={confirmReplace} onChange={(e) => setConfirmReplace(e.target.checked)} />
            I understand the risk and want to replace the trusted key on this computer only.
          </label>
        </div>
      ) : (
        <div>
          <p>
            This is the first time connecting to <code>{decision.host}:{decision.port}</code>. Verify the fingerprint
            out-of-band if possible before trusting it.
          </p>
          <div className="fingerprint-row">
            <span>Algorithm</span>
            <code>{decision.keyAlgorithm}</code>
          </div>
          <div className="fingerprint-row">
            <span>SHA256 fingerprint</span>
            <code>{decision.fingerprint}</code>
          </div>
          <p className="hint">Trust is saved only on this computer.</p>
        </div>
      )}
      <div className="modal-actions">
        <button onClick={onCancel} disabled={busy}>
          {isChanged ? "Cancel" : "Reject"}
        </button>
        <button
          className={isChanged ? "danger" : "primary"}
          disabled={busy || (isChanged && !confirmReplace)}
          onClick={trust}
        >
          {isChanged ? "Replace trusted key" : "Trust and Continue"}
        </button>
      </div>
    </Modal>
  );
}
