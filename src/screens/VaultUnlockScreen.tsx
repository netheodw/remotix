import { useState } from "react";
import { vaultUnlock } from "../api/commands";
import { errorMessage } from "../api/errors";
import { useVault } from "../state/VaultContext";

export function VaultUnlockScreen() {
  const { refresh } = useVault();
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    if (!password) return;
    setBusy(true);
    setError(null);
    try {
      await vaultUnlock(password);
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
        <h1>Unlock Remotix</h1>
        <p className="hint">This vault lives only on this computer. There is no password reset - if it's lost, the vault cannot be recovered.</p>
        <label className="field">
          <span>Master password</span>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
            autoFocus
          />
        </label>
        {error && <div className="field-error">{error}</div>}
        <button className="primary full-width" disabled={busy || !password} onClick={submit}>
          {busy ? "Unlocking..." : "Unlock"}
        </button>
      </div>
    </div>
  );
}
