import { useEffect, useMemo, useRef, useState } from "react";
import { localDiscardScratch, localFileMtime, sftpEditOpen } from "../api/commands";
import { errorMessage, parseHostKeyError } from "../api/errors";
import type { SshProfile } from "../api/types";
import { FilePane, type FileAction } from "../components/files/FilePane";
import { createLocalSource, createRemoteSource, type FileEntry } from "../components/files/fileSource";
import { TransferQueuePanel } from "../components/files/TransferQueuePanel";
import type { HostKeyDecision } from "../components/HostKeyDialog";
import { HostKeyDialog } from "../components/HostKeyDialog";
import { useData } from "../state/DataContext";
import { useFiles } from "../state/FilesContext";
import { useTerminalTabs } from "../state/TerminalTabsContext";

const EDIT_POLL_MS = 2000;

interface EditSession {
  name: string;
  localPath: string;
  localDir: string;
  remoteDir: string;
  modifiedAt: number | null;
  savedCount: number;
}

export function FilesScreen({
  variant = "screen",
  preferredProfileId,
  onOpenTerminal,
}: {
  /** "panel" is the narrow browser docked beside the terminal on the Active screen. */
  variant?: "screen" | "panel";
  /** The host of the terminal next to the panel: connected to when the panel opens. */
  preferredProfileId?: string;
  onOpenTerminal?: (profile: SshProfile) => void;
} = {}) {
  const { profiles } = useData();
  const { tabs } = useTerminalTabs();
  const {
    session,
    status,
    connect,
    disconnect,
    localCwd,
    setLocalCwd,
    remoteCwd,
    setRemoteCwd,
    localSelection,
    setLocalSelection,
    remoteSelection,
    setRemoteSelection,
    enqueue,
    activeTransfers,
    completionToken,
  } = useFiles();
  const panel = variant === "panel";

  const [picked, setPicked] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [hostKey, setHostKey] = useState<{ decision: HostKeyDecision; profile: SshProfile } | null>(null);
  const [edits, setEdits] = useState<EditSession[]>([]);

  const localSource = useMemo(() => createLocalSource(), []);
  const remoteSource = useMemo(
    () => (session ? createRemoteSource(session.sessionId, session.profileName, session.homePath) : null),
    [session],
  );

  // A host you already have a shell open on is the one you most likely want to
  // move files to, so those profiles lead the picker.
  const liveProfileIds = new Set(tabs.filter((t) => t.status === "connected").map((t) => t.profileId));
  const activeProfiles = profiles.filter((p) => liveProfileIds.has(p.id));
  const otherProfiles = profiles.filter((p) => !liveProfileIds.has(p.id));
  const preferredProfile = profiles.find((p) => p.id === preferredProfileId) ?? null;
  const selectedId = picked || preferredProfile?.id || activeProfiles[0]?.id || profiles[0]?.id || "";
  const sessionProfile = session ? (profiles.find((p) => p.id === session.profileId) ?? null) : null;

  // Polls each open scratch copy; a changed mtime means the editor saved, so
  // the file goes back up through the normal transfer queue.
  useEffect(() => {
    if (edits.length === 0 || !session) return;
    const timer = setInterval(async () => {
      for (const edit of edits) {
        const mtime = await localFileMtime(edit.localPath).catch(() => null);
        if (mtime === null || mtime === edit.modifiedAt) continue;
        setEdits((current) =>
          current.map((e) =>
            e.localPath === edit.localPath ? { ...e, modifiedAt: mtime, savedCount: e.savedCount + 1 } : e,
          ),
        );
        enqueue({
          direction: "upload",
          items: [edit.name],
          localDir: edit.localDir,
          remoteDir: edit.remoteDir,
        }).catch((e) => setError(errorMessage(e)));
      }
    }, EDIT_POLL_MS);
    return () => clearInterval(timer);
  }, [edits, session, enqueue]);

  const runConnect = async (profile: SshProfile) => {
    setError(null);
    try {
      await connect(profile);
    } catch (e) {
      const decision = parseHostKeyError(e);
      if (decision) setHostKey({ decision, profile });
      else setError(errorMessage(e));
    }
  };

  // Opening the panel beside a terminal browses that terminal's host. Only
  // once per mount, so a deliberate Disconnect is not undone.
  const autoConnected = useRef(false);
  useEffect(() => {
    if (!panel || autoConnected.current || session || status !== "idle" || !preferredProfile) return;
    autoConnected.current = true;
    runConnect(preferredProfile);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [panel, session, status, preferredProfile]);

  const switchToPreferred = () => {
    if (!preferredProfile) return;
    disconnect();
    runConnect(preferredProfile);
  };

  const openForEditing = async (entry: FileEntry) => {
    if (!session) return;
    setError(null);
    try {
      const handle = await sftpEditOpen(session.sessionId, entry.path);
      setEdits((current) => [
        ...current,
        {
          name: handle.name,
          localPath: handle.local_path,
          localDir: handle.local_dir,
          remoteDir: handle.remote_dir,
          modifiedAt: handle.modified_at,
          savedCount: 0,
        },
      ]);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const stopEditing = (edit: EditSession) => {
    setEdits((current) => current.filter((e) => e.localPath !== edit.localPath));
    localDiscardScratch(edit.localPath).catch(() => {});
  };

  const transfer = (direction: "upload" | "download") => {
    setError(null);
    const names = direction === "upload" ? localSelection : remoteSelection;
    enqueue({ direction, items: [...names] }).catch((e) => setError(errorMessage(e)));
  };

  const localActions: FileAction[] = [
    {
      label: "Upload to remote  →",
      enabled: () => Boolean(session),
      run: () => transfer("upload"),
    },
  ];

  const remoteActions: FileAction[] = [
    {
      label: "←  Download to this PC",
      run: () => transfer("download"),
    },
    {
      label: "Edit in default app",
      enabled: (entry) => entry.kind !== "directory" && entry.resolvedKind !== "directory",
      run: (entry) => openForEditing(entry),
    },
  ];

  const option = (p: SshProfile) => (
    <option key={p.id} value={p.id}>
      {p.name} — {p.username}@{p.hostname}
    </option>
  );

  return (
    <div className={`files-screen${panel ? " panel" : ""}`}>
      <div className="files-toolbar">
        <h1>Files</h1>
        {session ? (
          <>
            <span className="files-session">
              <span className="status-dot ok" />
              {session.profileName} — {session.hostLabel}
            </span>
            <button onClick={disconnect}>Disconnect</button>
            {panel && preferredProfile && preferredProfile.id !== session.profileId && (
              <button
                disabled={activeTransfers > 0}
                title={
                  activeTransfers > 0
                    ? "Wait for the running transfers to finish first"
                    : `Browse the files of ${preferredProfile.name}, the host of the current terminal`
                }
                onClick={switchToPreferred}
              >
                Switch to {preferredProfile.name}
              </button>
            )}
            {!panel && onOpenTerminal && sessionProfile && (
              <button
                title="Open an SSH terminal for this host beside the file browser"
                onClick={() => onOpenTerminal(sessionProfile)}
              >
                Open terminal
              </button>
            )}
          </>
        ) : (
          <>
            <select
              className="files-host-picker"
              value={selectedId}
              disabled={status === "connecting" || profiles.length === 0}
              onChange={(e) => setPicked(e.target.value)}
            >
              {activeProfiles.length > 0 && (
                <optgroup label="Active terminals">{activeProfiles.map(option)}</optgroup>
              )}
              {otherProfiles.length > 0 && (
                <optgroup label={activeProfiles.length > 0 ? "Other profiles" : "Profiles"}>
                  {otherProfiles.map(option)}
                </optgroup>
              )}
            </select>
            <button
              className="primary"
              disabled={status === "connecting" || !selectedId}
              onClick={() => {
                const profile = profiles.find((p) => p.id === selectedId);
                if (profile) runConnect(profile);
              }}
            >
              {status === "connecting" ? "Connecting…" : "Connect"}
            </button>
          </>
        )}
        {error && <span className="field-error files-connect-error">{error}</span>}
      </div>

      {edits.length > 0 && (
        <div className="edit-sessions">
          {edits.map((edit) => (
            <div key={edit.localPath} className="edit-session">
              <span className="status-dot warn" />
              <strong>{edit.name}</strong>
              <span className="hint">
                {edit.savedCount === 0
                  ? "open in your editor — saving uploads it back"
                  : `uploaded ${edit.savedCount} time${edit.savedCount === 1 ? "" : "s"}`}
              </span>
              <button className="ghost" onClick={() => stopEditing(edit)}>
                Stop watching
              </button>
            </div>
          ))}
        </div>
      )}

      <div className={`files-panes${panel ? " stacked" : ""}`}>
        <FilePane
          source={localSource}
          cwd={localCwd}
          onCwdChange={setLocalCwd}
          selection={localSelection}
          onSelectionChange={setLocalSelection}
          actions={localActions}
          refreshToken={completionToken}
        />

        <div className="file-transfer-bar">
          <button
            className="ghost"
            title="Upload selection to the remote host"
            disabled={!session || localSelection.size === 0}
            onClick={() => transfer("upload")}
          >
            {panel ? "↓" : "→"}
          </button>
          <button
            className="ghost"
            title="Download selection to this PC"
            disabled={!session || remoteSelection.size === 0}
            onClick={() => transfer("download")}
          >
            {panel ? "↑" : "←"}
          </button>
        </div>

        {remoteSource ? (
          <FilePane
            source={remoteSource}
            cwd={remoteCwd}
            onCwdChange={setRemoteCwd}
            selection={remoteSelection}
            onSelectionChange={setRemoteSelection}
            onOpenFile={openForEditing}
            actions={remoteActions}
            refreshToken={completionToken}
          />
        ) : (
          <div className="file-pane file-pane-disconnected">
            <div className="file-pane-empty">
              <p>Not connected.</p>
              <p className="hint">
                {profiles.length === 0
                  ? "Add an SSH profile on the Terminals screen first."
                  : "Pick a host above and connect to browse its files."}
              </p>
            </div>
          </div>
        )}
      </div>

      <TransferQueuePanel />

      {hostKey && (
        <HostKeyDialog
          decision={hostKey.decision}
          onCancel={() => setHostKey(null)}
          onResolved={() => {
            const { profile } = hostKey;
            setHostKey(null);
            runConnect(profile);
          }}
        />
      )}
    </div>
  );
}
