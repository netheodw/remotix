import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import {
  appDataDir,
  autostartIsEnabled,
  autostartSet,
  backupExport,
  settingsGetAll,
  settingsSet,
  vaultHelloDisable,
  vaultHelloEnable,
  vaultHelloStatus,
} from "../api/commands";
import { errorMessage } from "../api/errors";
import type { HelloInfo } from "../api/types";
import { BackupImportDialog } from "../components/BackupImportDialog";
import { useVault } from "../state/VaultContext";

const DEFAULTS = {
  terminal_font_family: "Cascadia Code, JetBrains Mono, Consolas, monospace",
  terminal_font_size: "13",
  terminal_scrollback: "5000",
  terminal_history_navigation: "true",
  theme: "dark",
  confirm_on_close_with_active_sessions: "true",
};

export function SettingsScreen() {
  const { lock } = useVault();
  const [values, setValues] = useState<Record<string, string>>(DEFAULTS);
  const [dataDir, setDataDir] = useState("");
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    settingsGetAll().then((stored) => setValues({ ...DEFAULTS, ...stored }));
    appDataDir().then(setDataDir);
  }, []);

  const set = (key: string, value: string) => {
    setValues((v) => ({ ...v, [key]: value }));
  };

  const save = async () => {
    await Promise.all(Object.entries(values).map(([k, v]) => settingsSet(k, v)));
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  };

  return (
    <div className="settings-screen">
      <h2>Settings</h2>

      <section className="form-section">
        <h3>Terminal</h3>
        <label className="field">
          <span>Font family</span>
          <input value={values.terminal_font_family} onChange={(e) => set("terminal_font_family", e.target.value)} />
        </label>
        <label className="field">
          <span>Font size</span>
          <input
            type="number"
            value={values.terminal_font_size}
            onChange={(e) => set("terminal_font_size", e.target.value)}
          />
        </label>
        <label className="field">
          <span>Scrollback lines</span>
          <input
            type="number"
            value={values.terminal_scrollback}
            onChange={(e) => set("terminal_scrollback", e.target.value)}
          />
        </label>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={values.terminal_history_navigation === "true"}
            onChange={(e) => set("terminal_history_navigation", String(e.target.checked))}
          />
          Use Up/Down for Remotix command history (per profile, kept across reconnects)
        </label>
        <p className="hint">
          When off, Up/Down go straight to the remote shell's own history. Either way the arrows are left alone inside
          full-screen programs like vim, less and top, but not inside REPLs that run on the normal screen such as
          python. Commands typed with a leading space are never recorded, and a profile's stored history can be
          cleared from its terminal tab.
        </p>
        <label className="field">
          <span>Theme</span>
          <select value={values.theme} onChange={(e) => set("theme", e.target.value)}>
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </label>
      </section>

      <section className="form-section">
        <h3>Application</h3>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={values.confirm_on_close_with_active_sessions === "true"}
            onChange={(e) => set("confirm_on_close_with_active_sessions", String(e.target.checked))}
          />
          Confirm before closing the app while sessions or forwards are active
        </label>
        <AutostartRow />
        <p className="hint">
          Closing the window keeps Remotix running in the system tray, still unlocked, with your terminals and forwards
          open. Click the tray icon to bring it back, or use Quit Remotix in the tray menu to exit.
        </p>
      </section>

      <WindowsHelloSection />

      <section className="form-section">
        <h3>Local data</h3>
        <p className="hint">Everything is stored only on this computer, encrypted at rest.</p>
        <div className="data-dir-row">
          <code>{dataDir}</code>
        </div>
      </section>

      <BackupSection />

      <div className="modal-actions">
        <button onClick={lock}>Lock Vault</button>
        <button className="primary" onClick={save}>
          {saved ? "Saved" : "Save Settings"}
        </button>
      </div>
    </div>
  );
}

const BACKUP_FILTERS = [{ name: "Remotix backup", extensions: ["remotix"] }];

/** Pick a `.remotix` file to import. Shared with the vault setup screen. */
export async function pickBackupFile(): Promise<string | null> {
  const path = await openFileDialog({ multiple: false, title: "Import Remotix backup", filters: BACKUP_FILTERS });
  return typeof path === "string" ? path : null;
}

function BackupSection() {
  const { refresh } = useVault();
  const [importPath, setImportPath] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const exportBackup = async () => {
    setMessage(null);
    const date = new Date().toISOString().slice(0, 10);
    const path = await saveFileDialog({
      title: "Export Remotix backup",
      defaultPath: `remotix-backup-${date}.remotix`,
      filters: BACKUP_FILTERS,
    });
    if (!path) return;
    setBusy(true);
    try {
      await backupExport(path);
      setMessage({ ok: true, text: `Backup saved: ${path}` });
    } catch (e) {
      setMessage({ ok: false, text: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="form-section">
      <h3>Backup</h3>
      <p className="hint">
        One file with everything: profiles, passwords and keys, sudo passwords, forwards, notes, snippets and
        history. The whole file is encrypted with your master password. To move Remotix to another computer, export
        here and choose Import there (or Restore from backup on its first screen).
      </p>
      <div className="backup-actions">
        <button className="primary" onClick={exportBackup} disabled={busy}>
          {busy ? "Exporting..." : "Export backup..."}
        </button>
        <button
          onClick={async () => {
            setMessage(null);
            setImportPath(await pickBackupFile());
          }}
          disabled={busy}
        >
          Import backup...
        </button>
      </div>
      {message && <div className={message.ok ? "hint" : "field-error"}>{message.text}</div>}
      {importPath && (
        <BackupImportDialog
          path={importPath}
          replacesExisting
          onClose={() => setImportPath(null)}
          onImported={() => void refresh()}
        />
      )}
    </section>
  );
}

/** Applied immediately: the OS owns this setting, not `app_settings`. */
function AutostartRow() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    autostartIsEnabled()
      .then(setEnabled)
      .catch((e) => setError(errorMessage(e)));
  }, []);

  const toggle = async (next: boolean) => {
    setError(null);
    try {
      await autostartSet(next);
      setEnabled(next);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <>
      <label className="checkbox-row">
        <input
          type="checkbox"
          disabled={enabled === null}
          checked={enabled ?? false}
          onChange={(e) => toggle(e.target.checked)}
        />
        Start Remotix minimized to the tray when I sign in
      </label>
      {error && <div className="field-error">{error}</div>}
    </>
  );
}

function WindowsHelloSection() {
  const [hello, setHello] = useState<HelloInfo | null>(null);
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = () =>
    vaultHelloStatus()
      .then(setHello)
      .catch((e) => setError(errorMessage(e)));

  useEffect(() => {
    void reload();
  }, []);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      setPassword("");
      await reload();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  if (!hello) return null;

  return (
    <section className="form-section">
      <h3>Windows Hello</h3>
      {!hello.supported && !hello.enabled ? (
        <p className="hint">
          Windows Hello is not set up for this Windows account. Add a PIN, fingerprint or face in Windows Settings &gt;
          Accounts &gt; Sign-in options, then come back here.
        </p>
      ) : hello.enabled ? (
        <>
          <p className="hint">
            Remotix unlocks with Windows Hello. The master password is still asked once every 6 months
            {hello.password_due_at && !hello.password_required
              ? ` - next time on ${new Date(hello.password_due_at).toLocaleDateString()}`
              : ""}
            .
          </p>
          <div className="modal-actions">
            <button disabled={busy} onClick={() => run(vaultHelloDisable)}>
              Turn off Windows Hello
            </button>
          </div>
        </>
      ) : (
        <>
          <p className="hint">
            Unlock Remotix and its desktop shortcuts with your Windows Hello PIN, fingerprint or face. The master
            password is still asked once every 6 months. Confirm your master password to turn it on.
          </p>
          <label className="field">
            <span>Master password</span>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && password && run(() => vaultHelloEnable(password))}
            />
          </label>
          <div className="modal-actions">
            <button className="primary" disabled={busy || !password} onClick={() => run(() => vaultHelloEnable(password))}>
              {busy ? "Waiting for Windows Hello..." : "Turn on Windows Hello"}
            </button>
          </div>
        </>
      )}
      {error && <div className="field-error">{error}</div>}
    </section>
  );
}
