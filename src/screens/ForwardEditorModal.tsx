import { useState } from "react";
import { forwardsCreate, forwardsSetLocalHostname, forwardsSetNotes, forwardsStart, forwardsUpdate } from "../api/commands";
import { errorMessage } from "../api/errors";
import type { ForwardType, PortForward } from "../api/types";
import { LocalHostnameGuide } from "../components/LocalHostnameGuide";
import { Modal } from "../components/Modal";
import { useData } from "../state/DataContext";

function isLoopback(addr: string): boolean {
  return addr === "127.0.0.1" || addr === "::1" || addr === "localhost";
}

export function ForwardEditorModal({
  forward,
  defaultGroupId,
  onClose,
  onSaved,
}: {
  forward: PortForward | null;
  defaultGroupId: string | null;
  onClose: () => void;
  onSaved: (f: PortForward) => void;
}) {
  const { portForwardGroups, profiles, refreshForwards } = useData();
  const [name, setName] = useState(forward?.name ?? "");
  const [groupId, setGroupId] = useState<string | null>(forward?.port_forward_group_id ?? defaultGroupId);
  const [profileId, setProfileId] = useState(forward?.ssh_profile_id ?? profiles[0]?.id ?? "");
  const [type, setType] = useState<ForwardType>(forward?.forward_type ?? "local");
  const [bindAddress, setBindAddress] = useState(forward?.bind_address ?? "127.0.0.1");
  const [listenPort, setListenPort] = useState(forward?.listen_port ?? 8080);
  const [targetHostname, setTargetHostname] = useState(forward?.target_hostname ?? "");
  const [targetPort, setTargetPort] = useState(forward?.target_port ?? 0);
  const [targetIsSocket, setTargetIsSocket] = useState(
    forward?.forward_type === "local" && (forward.target_hostname ?? "").startsWith("/"),
  );
  const [startOnLaunch, setStartOnLaunch] = useState(forward?.start_on_launch ?? false);
  const [nonLoopbackAck, setNonLoopbackAck] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [notes, setNotes] = useState(forward?.notes ?? "");
  const [localHostname, setLocalHostname] = useState(forward?.local_hostname ?? "");

  const needsTarget = type !== "dynamic";
  const socketTarget = type === "local" && targetIsSocket;
  const nonLoopback = !isLoopback(bindAddress);

  const validate = () => {
    const e: Record<string, string> = {};
    if (!name.trim()) e.name = "Name is required.";
    if (!profileId) e.profileId = "Select an SSH profile.";
    if (!bindAddress.trim()) e.bindAddress = "Bind address is required.";
    if (listenPort < 1 || listenPort > 65535) e.listenPort = "Listen port must be between 1 and 65535.";
    if (socketTarget) {
      if (!targetHostname.trim().startsWith("/")) e.targetHostname = "Socket path must be absolute, e.g. /var/run/docker.sock.";
    } else if (needsTarget) {
      if (!targetHostname.trim()) e.targetHostname = "Target hostname is required.";
      else if (targetHostname.trim().startsWith("/")) e.targetHostname = "Choose Unix socket as the target to forward to a socket path.";
      if (targetPort < 1 || targetPort > 65535) e.targetPort = "Target port must be between 1 and 65535.";
    }
    if (nonLoopback && !nonLoopbackAck) e.bindAddress = "Acknowledge the non-loopback exposure warning below.";
    setErrors(e);
    return Object.keys(e).length === 0;
  };

  const buildInput = () => ({
    port_forward_group_id: groupId,
    ssh_profile_id: profileId,
    name: name.trim(),
    forward_type: type,
    bind_address: bindAddress.trim(),
    listen_port: listenPort,
    target_hostname: needsTarget ? targetHostname.trim() : null,
    target_port: needsTarget && !socketTarget ? targetPort : null,
    start_on_launch: startOnLaunch,
  });

  const save = async (): Promise<PortForward | null> => {
    if (!validate()) return null;
    setBusy(true);
    setFormError(null);
    try {
      let saved = forward
        ? await forwardsUpdate({ id: forward.id, ...buildInput() })
        : await forwardsCreate(buildInput());
      if (notes.trimEnd() !== saved.notes) saved = await forwardsSetNotes(saved.id, notes);
      // Also re-sent when the bind address changed, so the hosts entry follows it.
      const wantedHost = type === "remote" ? null : localHostname.trim().toLowerCase() || null;
      if (wantedHost !== saved.local_hostname || (wantedHost && forward?.bind_address !== saved.bind_address)) {
        saved = await forwardsSetLocalHostname(saved.id, wantedHost);
      }
      await refreshForwards();
      return saved;
    } catch (e) {
      setFormError(errorMessage(e));
      return null;
    } finally {
      setBusy(false);
    }
  };

  const helperText = () => {
    const profileName = profiles.find((p) => p.id === profileId)?.name ?? "a profile";
    if (socketTarget) return `${bindAddress}:${listenPort || "?"} → remote socket ${targetHostname || "/path.sock"} through ${profileName}`;
    const listenHost = localHostname.trim() ? localHostname.trim().toLowerCase() : bindAddress;
    if (type === "local") return `${listenHost}:${listenPort || "?"} → ${targetHostname || "target"}:${targetPort || "?"} through ${profileName}`;
    if (type === "remote") return `remote-server:${listenPort || "?"} → 127.0.0.1:${targetPort || "?"} through ${profileName}`;
    return `${bindAddress}:${listenPort || "?"} SOCKS5 through ${profileName}`;
  };

  return (
    <Modal title={forward ? "Edit Port Forward" : "New Port Forward"} onClose={onClose} width={560}>
      <label className="field">
        <span>Name</span>
        <input value={name} onChange={(e) => setName(e.target.value)} autoFocus />
      </label>
      {errors.name && <div className="field-error">{errors.name}</div>}

      <label className="field">
        <span>Group</span>
        <select value={groupId ?? ""} onChange={(e) => setGroupId(e.target.value || null)}>
          <option value="">Ungrouped</option>
          {portForwardGroups.map((g) => (
            <option key={g.id} value={g.id}>
              {g.name}
            </option>
          ))}
        </select>
      </label>

      <label className="field">
        <span>SSH profile</span>
        {profiles.length === 0 ? (
          <div className="field-error">Create an SSH profile first before adding a forward.</div>
        ) : (
          <select value={profileId} onChange={(e) => setProfileId(e.target.value)}>
            {profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        )}
      </label>
      {errors.profileId && <div className="field-error">{errors.profileId}</div>}

      <div className="segmented">
        {(["local", "remote", "dynamic"] as ForwardType[]).map((t) => (
          <button key={t} type="button" className={type === t ? "active" : ""} onClick={() => setType(t)}>
            {t === "local" ? "Local" : t === "remote" ? "Remote" : "Dynamic (SOCKS5)"}
          </button>
        ))}
      </div>

      <div className="field-row">
        <label className="field" style={{ flex: 2 }}>
          <span>Bind address</span>
          <input value={bindAddress} onChange={(e) => setBindAddress(e.target.value)} />
        </label>
        <label className="field" style={{ flex: 1 }}>
          <span>Listen port</span>
          <input type="number" value={listenPort} onChange={(e) => setListenPort(Number(e.target.value))} />
        </label>
      </div>
      {errors.listenPort && <div className="field-error">{errors.listenPort}</div>}

      {nonLoopback && (
        <div className="warning-box">
          <strong>Warning:</strong> {bindAddress} is not a loopback address. This forward will be reachable from other
          devices on the network, not just this computer.
          <label className="checkbox-row">
            <input type="checkbox" checked={nonLoopbackAck} onChange={(e) => setNonLoopbackAck(e.target.checked)} />
            I understand and want to expose this forward beyond loopback.
          </label>
        </div>
      )}
      {errors.bindAddress && <div className="field-error">{errors.bindAddress}</div>}

      {type !== "remote" && (
        <label className="field">
          <span>Local hostname (optional)</span>
          <input
            value={localHostname}
            placeholder="ifg-portainer.localhost"
            onChange={(e) => setLocalHostname(e.target.value)}
          />
        </label>
      )}
      {type !== "remote" && (
        <LocalHostnameGuide hostname={localHostname} bindAddress={bindAddress} port={listenPort} />
      )}

      {type === "local" && (
        <div className="segmented">
          <button type="button" className={!targetIsSocket ? "active" : ""} onClick={() => setTargetIsSocket(false)}>
            Host &amp; port
          </button>
          <button type="button" className={targetIsSocket ? "active" : ""} onClick={() => setTargetIsSocket(true)}>
            Unix socket
          </button>
        </div>
      )}

      {socketTarget && (
        <label className="field">
          <span>Remote socket path</span>
          <input
            value={targetHostname}
            placeholder="/var/run/docker.sock"
            onChange={(e) => setTargetHostname(e.target.value)}
          />
        </label>
      )}

      {needsTarget && !socketTarget && (
        <div className="field-row">
          <label className="field" style={{ flex: 2 }}>
            <span>Target hostname</span>
            <input value={targetHostname} onChange={(e) => setTargetHostname(e.target.value)} />
          </label>
          <label className="field" style={{ flex: 1 }}>
            <span>Target port</span>
            <input type="number" value={targetPort} onChange={(e) => setTargetPort(Number(e.target.value))} />
          </label>
        </div>
      )}
      {errors.targetHostname && <div className="field-error">{errors.targetHostname}</div>}
      {errors.targetPort && <div className="field-error">{errors.targetPort}</div>}

      <div className="forward-helper">{helperText()}</div>

      <label className="checkbox-row">
        <input type="checkbox" checked={startOnLaunch} onChange={(e) => setStartOnLaunch(e.target.checked)} />
        Start automatically when Remotix launches
      </label>

      <label className="field">
        <span>Notes</span>
        <textarea
          rows={4}
          value={notes}
          placeholder="What it is for, who to ask, how to use it... (not for passwords)"
          onChange={(e) => setNotes(e.target.value)}
        />
      </label>

      {formError && <div className="field-error">{formError}</div>}
      <div className="modal-actions">
        <button onClick={onClose} disabled={busy}>
          Cancel
        </button>
        <button
          disabled={busy || profiles.length === 0}
          onClick={async () => {
            const saved = await save();
            if (saved) onSaved(saved);
          }}
        >
          Save
        </button>
        <button
          className="primary"
          disabled={busy || profiles.length === 0}
          onClick={async () => {
            const saved = await save();
            if (saved) {
              await forwardsStart(saved.id);
              onSaved(saved);
            }
          }}
        >
          Save and Start
        </button>
      </div>
    </Modal>
  );
}
