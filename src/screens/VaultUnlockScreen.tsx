import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useEffect, useRef, useState } from "react";
import { vaultHelloStatus, vaultUnlock, vaultUnlockHello } from "../api/commands";
import { errorMessage } from "../api/errors";
import { isAppError, type HelloInfo } from "../api/types";
import { useVault } from "../state/VaultContext";

export function VaultUnlockScreen() {
  const { refresh } = useVault();
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hello, setHello] = useState<HelloInfo | null>(null);
  const helloBusy = useRef(false);
  const autoPrompted = useRef(false);

  const helloReady = !!hello && hello.supported && hello.enabled && !hello.password_required;

  const unlockWithHello = useCallback(async () => {
    if (helloBusy.current) return;
    helloBusy.current = true;
    setBusy(true);
    setError(null);
    try {
      await vaultUnlockHello();
      await refresh();
    } catch (e) {
      // Cancelling the prompt just leaves the password field to use.
      if (!(isAppError(e) && e.kind === "biometric_cancelled")) setError(errorMessage(e));
      if (isAppError(e) && e.kind === "master_password_required") {
        setHello(await vaultHelloStatus().catch(() => null));
      }
    } finally {
      helloBusy.current = false;
      setBusy(false);
    }
  }, [refresh]);

  useEffect(() => {
    vaultHelloStatus()
      .then(setHello)
      .catch(() => setHello(null));
  }, []);

  // Ask for Windows Hello straight away, so opening Remotix (or one of its
  // desktop shortcuts) needs only the Hello PIN, fingerprint or face. A
  // start at login keeps the window hidden in the tray, so wait until it is
  // shown rather than prompting where nobody is looking.
  useEffect(() => {
    if (!helloReady || autoPrompted.current) return;
    autoPrompted.current = true;
    getCurrentWindow()
      .isVisible()
      .then((visible) => {
        if (visible) void unlockWithHello();
      })
      .catch(() => void unlockWithHello());
  }, [helloReady, unlockWithHello]);

  // Window brought up from the tray, or a desktop shortcut clicked, while
  // Remotix sits locked: prompt again.
  useEffect(() => {
    if (!helloReady) return;
    const unlistens = ["main-window-shown", "launch-open-profile"].map((event) =>
      listen(event, () => void unlockWithHello()),
    );
    return () => {
      unlistens.forEach((u) => u.then((f) => f()));
    };
  }, [helloReady, unlockWithHello]);

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
        {hello?.enabled && hello.password_required && (
          <p className="hint">
            Windows Hello is paused until you enter the master password. Remotix asks for it once every 6 months so it
            is not forgotten.
          </p>
        )}
        <label className="field">
          <span>Master password</span>
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
            autoFocus={!helloReady}
          />
        </label>
        {error && <div className="field-error">{error}</div>}
        <button className="primary full-width" disabled={busy || !password} onClick={submit}>
          {busy ? "Unlocking..." : "Unlock"}
        </button>
        {helloReady && (
          <button className="full-width" disabled={busy} onClick={() => void unlockWithHello()}>
            Unlock with Windows Hello
          </button>
        )}
      </div>
    </div>
  );
}
