import { useState } from "react";
import { vaultSetup } from "../api/commands";
import { errorMessage } from "../api/errors";
import { useVault } from "../state/VaultContext";

function passwordStrength(pw: string): { score: number; label: string } {
  let score = 0;
  if (pw.length >= 8) score++;
  if (pw.length >= 14) score++;
  if (/[A-Z]/.test(pw) && /[a-z]/.test(pw)) score++;
  if (/[0-9]/.test(pw)) score++;
  if (/[^A-Za-z0-9]/.test(pw)) score++;
  const labels = ["Very weak", "Weak", "Fair", "Good", "Strong", "Very strong"];
  return { score, label: labels[score] };
}

export function VaultSetupScreen() {
  const { refresh } = useVault();
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const strength = passwordStrength(password);
  const mismatch = confirm.length > 0 && password !== confirm;
  const canSubmit = password.length >= 8 && password === confirm && acknowledged && !busy;

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await vaultSetup(password);
      await refresh();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="auth-screen">
      <div className="auth-card">
        <h1>Create your vault</h1>
        <p className="hint">
          Remotix keeps all connection profiles, credentials, and settings on this computer only. Your master
          password encrypts everything locally - there is no account, no cloud backup, and no way for Remotix to
          recover a lost password.
        </p>
        <label className="field">
          <span>Master password</span>
          <input type="password" value={password} onChange={(e) => setPassword(e.target.value)} autoFocus />
        </label>
        {password.length > 0 && (
          <div className={`strength-meter s${strength.score}`}>
            <div className="strength-bar" />
            <span>{strength.label}</span>
          </div>
        )}
        <label className="field">
          <span>Confirm master password</span>
          <input type="password" value={confirm} onChange={(e) => setConfirm(e.target.value)} />
        </label>
        {mismatch && <div className="field-error">Passwords do not match.</div>}
        <label className="checkbox-row">
          <input type="checkbox" checked={acknowledged} onChange={(e) => setAcknowledged(e.target.checked)} />
          I understand that losing this password means permanently losing access to everything in the vault.
        </label>
        {error && <div className="field-error">{error}</div>}
        <button className="primary full-width" disabled={!canSubmit} onClick={submit}>
          {busy ? "Creating vault..." : "Create Vault"}
        </button>
      </div>
    </div>
  );
}
