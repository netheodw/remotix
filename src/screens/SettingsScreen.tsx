import { useEffect, useState } from "react";
import { appDataDir, settingsGetAll, settingsSet } from "../api/commands";
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
      </section>

      <section className="form-section">
        <h3>Local data</h3>
        <p className="hint">Everything is stored only on this computer, encrypted at rest.</p>
        <div className="data-dir-row">
          <code>{dataDir}</code>
        </div>
        <p className="hint">
          To back up: copy <code>remotix.db</code> from this folder while the app is closed. It stays encrypted with
          your master password and can be restored by placing it back and unlocking normally.
        </p>
      </section>

      <div className="modal-actions">
        <button onClick={lock}>Lock Vault</button>
        <button className="primary" onClick={save}>
          {saved ? "Saved" : "Save Settings"}
        </button>
      </div>
    </div>
  );
}
