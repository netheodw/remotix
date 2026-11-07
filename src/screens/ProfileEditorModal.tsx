import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import {
  profilesCreate,
  profilesImportPrivateKeyFile,
  profilesRevealPassword,
  profilesSetNotes,
  profilesUpdate,
} from "../api/commands";
import { errorMessage } from "../api/errors";
import type { AuthKind, AuthSecretInput, SshProfile } from "../api/types";
import { Modal } from "../components/Modal";
import { useData } from "../state/DataContext";

function EyeIcon({ open }: { open: boolean }) {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12z" />
      <circle cx="12" cy="12" r="3" />
      {!open && <line x1="3" y1="3" x2="21" y2="21" />}
    </svg>
  );
}

function PasswordInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [visible, setVisible] = useState(false);
  return (
    <div className="password-input">
      <input type={visible ? "text" : "password"} value={value} onChange={(e) => onChange(e.target.value)} />
      <button
        type="button"
        className="icon-toggle"
        title={visible ? "Hide" : "Show"}
        aria-label={visible ? "Hide" : "Show"}
        onClick={() => setVisible((v) => !v)}
      >
        <EyeIcon open={!visible} />
      </button>
    </div>
  );
}

export function ProfileEditorModal({
  profile,
  defaultGroupId,
  onClose,
  onSaved,
}: {
  profile: SshProfile | null;
  defaultGroupId: string | null;
  onClose: () => void;
  onSaved: (p: SshProfile) => void;
}) {
  const { terminalGroups, refreshProfiles } = useData();
  const [name, setName] = useState(profile?.name ?? "");
  const [groupId, setGroupId] = useState<string | null>(profile?.terminal_group_id ?? defaultGroupId);
  const [hostname, setHostname] = useState(profile?.hostname ?? "");
  const [port, setPort] = useState(profile?.port ?? 22);
  const [username, setUsername] = useState(profile?.username ?? "");
  const [terminalType, setTerminalType] = useState(profile?.terminal_type ?? "xterm-256color");
  const [authKind, setAuthKind] = useState<AuthKind>(profile?.authentication_kind ?? "password");
  const [changeSecret, setChangeSecret] = useState(!profile);
  const [password, setPassword] = useState("");
  const [privateKey, setPrivateKey] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [notes, setNotes] = useState(profile?.notes ?? "");
  const [revealedPassword, setRevealedPassword] = useState<string | null>(null);
  const [revealError, setRevealError] = useState<string | null>(null);

  const canRevealSaved = !!profile && profile.has_secret && profile.authentication_kind === "password";

  const toggleRevealSaved = async () => {
    if (revealedPassword !== null) {
      setRevealedPassword(null);
      return;
    }
    if (!profile) return;
    setRevealError(null);
    try {
      setRevealedPassword(await profilesRevealPassword(profile.id));
    } catch (e) {
      setRevealError(errorMessage(e));
    }
  };

  const validate = (): boolean => {
    const e: Record<string, string> = {};
    if (!name.trim()) e.name = "Display name is required.";
    if (!hostname.trim()) e.hostname = "Hostname is required.";
    if (!username.trim()) e.username = "Username is required.";
    if (port < 1 || port > 65535) e.port = "Port must be between 1 and 65535.";
    if (changeSecret) {
      if (authKind === "password" && !password) e.password = "Password is required.";
      if ((authKind === "private_key" || authKind === "private_key_passphrase") && !privateKey.trim()) {
        e.privateKey = "Private key is required.";
      }
      if (authKind === "private_key_passphrase" && !passphrase) e.passphrase = "Passphrase is required.";
    }
    setErrors(e);
    return Object.keys(e).length === 0;
  };

  const buildSecret = (): AuthSecretInput => {
    if (!changeSecret) return { kind: "unchanged" };
    if (authKind === "password") return { kind: "password", password };
    if (authKind === "private_key") return { kind: "private_key", private_key: privateKey };
    return { kind: "private_key_passphrase", private_key: privateKey, passphrase };
  };

  const importKeyFile = async () => {
    const path = await openFileDialog({ multiple: false, title: "Select private key file" });
    if (typeof path === "string") {
      const contents = await profilesImportPrivateKeyFile(path);
      setPrivateKey(contents);
      setChangeSecret(true);
    }
  };

  const submit = async () => {
    if (!validate()) return;
    setBusy(true);
    setFormError(null);
    try {
      const base = {
        terminal_group_id: groupId,
        name: name.trim(),
        hostname: hostname.trim(),
        port,
        username: username.trim(),
        terminal_type: terminalType,
        secret: buildSecret(),
      };
      let saved = profile
        ? await profilesUpdate({ id: profile.id, ...base })
        : await profilesCreate(base);
      if (notes.trimEnd() !== saved.notes) saved = await profilesSetNotes(saved.id, notes);
      await refreshProfiles();
      onSaved(saved);
    } catch (e) {
      setFormError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Modal title={profile ? "Edit SSH Profile" : "New SSH Profile"} onClose={onClose} width={560}>
      <div className="form-section">
        <h3>Identity</h3>
        <label className="field">
          <span>Display name</span>
          <input value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </label>
        {errors.name && <div className="field-error">{errors.name}</div>}
        <label className="field">
          <span>Group</span>
          <select value={groupId ?? ""} onChange={(e) => setGroupId(e.target.value || null)}>
            <option value="">Ungrouped</option>
            {terminalGroups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="form-section">
        <h3>Connection</h3>
        <div className="field-row">
          <label className="field" style={{ flex: 2 }}>
            <span>Hostname</span>
            <input value={hostname} onChange={(e) => setHostname(e.target.value)} />
          </label>
          <label className="field" style={{ flex: 1 }}>
            <span>Port</span>
            <input type="number" value={port} onChange={(e) => setPort(Number(e.target.value))} />
          </label>
        </div>
        {errors.hostname && <div className="field-error">{errors.hostname}</div>}
        {errors.port && <div className="field-error">{errors.port}</div>}
        <label className="field">
          <span>Username</span>
          <input value={username} onChange={(e) => setUsername(e.target.value)} />
        </label>
        {errors.username && <div className="field-error">{errors.username}</div>}
      </div>

      <div className="form-section">
        <h3>Authentication</h3>
        <div className="segmented">
          {(["password", "private_key", "private_key_passphrase"] as AuthKind[]).map((k) => (
            <button
              key={k}
              type="button"
              className={authKind === k ? "active" : ""}
              onClick={() => {
                setAuthKind(k);
                setChangeSecret(true);
              }}
            >
              {k === "password" ? "Password" : k === "private_key" ? "Private Key" : "Private Key + Passphrase"}
            </button>
          ))}
        </div>

        {!changeSecret && profile && (
          <div className="secret-saved-row">
            {revealedPassword !== null ? (
              <span className="secret-value">{revealedPassword}</span>
            ) : (
              <span>Saved securely</span>
            )}
            <div className="secret-saved-actions">
              {canRevealSaved && (
                <button
                  type="button"
                  className="icon-toggle"
                  title={revealedPassword !== null ? "Hide password" : "Show password"}
                  aria-label={revealedPassword !== null ? "Hide password" : "Show password"}
                  onClick={toggleRevealSaved}
                >
                  <EyeIcon open={revealedPassword === null} />
                </button>
              )}
              <button
                className="ghost"
                onClick={() => {
                  setRevealedPassword(null);
                  setChangeSecret(true);
                }}
              >
                Replace
              </button>
            </div>
          </div>
        )}
        {revealError && <div className="field-error">{revealError}</div>}

        {changeSecret && authKind === "password" && (
          <label className="field">
            <span>Password</span>
            <PasswordInput value={password} onChange={setPassword} />
          </label>
        )}
        {errors.password && <div className="field-error">{errors.password}</div>}

        {changeSecret && (authKind === "private_key" || authKind === "private_key_passphrase") && (
          <>
            <label className="field">
              <span>Private key (PEM)</span>
              <textarea rows={5} value={privateKey} onChange={(e) => setPrivateKey(e.target.value)} />
            </label>
            <button type="button" onClick={importKeyFile}>
              Import from file...
            </button>
            {errors.privateKey && <div className="field-error">{errors.privateKey}</div>}
          </>
        )}
        {changeSecret && authKind === "private_key_passphrase" && (
          <label className="field">
            <span>Passphrase</span>
            <PasswordInput value={passphrase} onChange={setPassphrase} />
          </label>
        )}
        {errors.passphrase && <div className="field-error">{errors.passphrase}</div>}
      </div>

      <div className="form-section">
        <h3>Terminal Options</h3>
        <label className="field">
          <span>Terminal type</span>
          <input value={terminalType} onChange={(e) => setTerminalType(e.target.value)} />
        </label>
      </div>

      <div className="form-section">
        <h3>Notes</h3>
      <label className="field">
        <textarea
          rows={4}
          value={notes}
          placeholder="What it is for, who to ask, how to use it... (not for passwords)"
          onChange={(e) => setNotes(e.target.value)}
        />
      </label>

      </div>

      {formError && <div className="field-error">{formError}</div>}
      <div className="modal-actions">
        <button onClick={onClose} disabled={busy}>
          Cancel
        </button>
        <button className="primary" onClick={submit} disabled={busy}>
          {busy ? "Saving..." : "Save"}
        </button>
      </div>
    </Modal>
  );
}
