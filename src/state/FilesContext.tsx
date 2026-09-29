import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { sftpCancel, sftpCancelAll, sftpConnect, sftpDisconnect, sftpEnqueue } from "../api/commands";
import type { ConflictPolicy, SshProfile, TransferDirection } from "../api/types";
import {
  activeCount,
  clearFinished,
  markAllCanceling,
  markCanceling,
  transferQueueReducer,
  type TransferItem,
} from "./transferQueue";

export interface FilesSession {
  sessionId: string;
  profileId: string;
  profileName: string;
  hostLabel: string;
  homePath: string;
}

export type FilesStatus = "idle" | "connecting" | "connected";

export interface EnqueueRequest {
  direction: TransferDirection;
  items: string[];
  /** Default to the two panes' directories; an edit save overrides them,
   *  since its scratch copy lives in a temp directory. */
  localDir?: string;
  remoteDir?: string;
}

interface FilesContextValue {
  session: FilesSession | null;
  status: FilesStatus;
  connect: (profile: SshProfile) => Promise<void>;
  disconnect: () => void;
  localCwd: string;
  setLocalCwd: (path: string) => void;
  remoteCwd: string;
  setRemoteCwd: (path: string) => void;
  localSelection: Set<string>;
  setLocalSelection: (selection: Set<string>) => void;
  remoteSelection: Set<string>;
  setRemoteSelection: (selection: Set<string>) => void;
  queue: TransferItem[];
  activeTransfers: number;
  conflict: ConflictPolicy;
  setConflict: (policy: ConflictPolicy) => void;
  enqueue: (request: EnqueueRequest) => Promise<void>;
  cancel: (itemId: string) => void;
  cancelAll: () => void;
  clearCompleted: () => void;
  /** Bumped when a transfer finishes, so the receiving pane re-lists. */
  completionToken: number;
}

const FilesContext = createContext<FilesContextValue | null>(null);

/**
 * Mounted above `AppShell` rather than inside the Files screen. Screens are
 * conditionally rendered, so anything holding a backend session id or an event
 * channel inside one is torn down the moment the user clicks another sidebar
 * entry - which is exactly how terminals lose their session today.
 */
export function FilesProvider({ children }: { children: ReactNode }) {
  const [session, setSession] = useState<FilesSession | null>(null);
  const [status, setStatus] = useState<FilesStatus>("idle");
  const [localCwd, setLocalCwd] = useState("");
  const [remoteCwd, setRemoteCwd] = useState("");
  const [localSelection, setLocalSelection] = useState<Set<string>>(new Set());
  const [remoteSelection, setRemoteSelection] = useState<Set<string>>(new Set());
  const [queue, setQueue] = useState<TransferItem[]>([]);
  const [conflict, setConflict] = useState<ConflictPolicy>("overwrite");
  const [completionToken, setCompletionToken] = useState(0);

  // Read inside the enqueue callback without making it depend on them.
  const dirs = useRef({ localCwd, remoteCwd });
  dirs.current = { localCwd, remoteCwd };

  const connect = useCallback(async (profile: SshProfile) => {
    setStatus("connecting");
    try {
      const result = await sftpConnect(profile.id, (event) => {
        setQueue((current) => transferQueueReducer(current, event));
        if (event.type === "completed" || event.type === "file_done") {
          setCompletionToken((n) => n + 1);
        }
      });
      setSession({
        sessionId: result.session_id,
        profileId: profile.id,
        profileName: profile.name,
        hostLabel: `${profile.username}@${profile.hostname}`,
        homePath: result.home_path,
      });
      setRemoteCwd(result.home_path);
      setRemoteSelection(new Set());
      setStatus("connected");
    } catch (e) {
      setStatus("idle");
      // Rethrown so the screen can tell an untrusted host key (which needs the
      // trust dialog and a retry) apart from a plain failure.
      throw e;
    }
  }, []);

  const disconnect = useCallback(() => {
    setSession((current) => {
      if (current) sftpDisconnect(current.sessionId).catch(() => {});
      return null;
    });
    setStatus("idle");
    setRemoteCwd("");
    setRemoteSelection(new Set());
    setQueue([]);
  }, []);

  const enqueue = useCallback(
    async (request: EnqueueRequest) => {
      if (!session || request.items.length === 0) return;
      await sftpEnqueue(session.sessionId, {
        direction: request.direction,
        localDir: request.localDir ?? dirs.current.localCwd,
        remoteDir: request.remoteDir ?? dirs.current.remoteCwd,
        items: request.items,
        conflict,
      });
    },
    [session, conflict],
  );

  const cancel = useCallback(
    (itemId: string) => {
      setQueue((current) => markCanceling(current, itemId));
      if (session) sftpCancel(session.sessionId, itemId).catch(() => {});
    },
    [session],
  );

  const cancelAll = useCallback(() => {
    setQueue(markAllCanceling);
    if (session) sftpCancelAll(session.sessionId).catch(() => {});
  }, [session]);

  const clearCompleted = useCallback(() => setQueue(clearFinished), []);

  return (
    <FilesContext.Provider
      value={{
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
        queue,
        activeTransfers: activeCount(queue),
        conflict,
        setConflict,
        enqueue,
        cancel,
        cancelAll,
        clearCompleted,
        completionToken,
      }}
    >
      {children}
    </FilesContext.Provider>
  );
}

export function useFiles() {
  const ctx = useContext(FilesContext);
  if (!ctx) throw new Error("useFiles must be used within FilesProvider");
  return ctx;
}
