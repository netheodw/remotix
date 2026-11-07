import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent } from "react";
import { commandHistoryClear, forwardsStop, sudoStatus } from "../api/commands";
import type { HostKeyDecision } from "../components/HostKeyDialog";
import { HostKeyDialog } from "../components/HostKeyDialog";
import { ConfirmDialog } from "../components/Modal";
import { ForwardStatusBadge } from "../components/StatusBadge";
import { TerminalPane, type TerminalSender } from "../components/TerminalPane";
import { SudoPasswordDialog } from "../components/SudoPasswordDialog";
import { ToolboxPanel } from "../components/ToolboxPanel";
import { useData } from "../state/DataContext";
import { useForwardStatuses } from "../state/ForwardStatusContext";
import { useTerminalTabs } from "../state/TerminalTabsContext";
import { FilesScreen } from "./FilesScreen";

const FILES_PANEL_WIDTH_KEY = "remotix.filesPanelWidth";
const FILES_PANEL_MIN = 340;
const TERMINAL_MIN = 360;

function clampPanelWidth(width: number): number {
  return Math.max(FILES_PANEL_MIN, Math.min(width, window.innerWidth - TERMINAL_MIN));
}

function storedPanelWidth(): number {
  try {
    const stored = Number(localStorage.getItem(FILES_PANEL_WIDTH_KEY));
    if (stored > 0) return clampPanelWidth(stored);
  } catch {
    // Storage unavailable: fall through to the default.
  }
  return clampPanelWidth(Math.round(window.innerWidth * 0.42));
}

function tabStatusTone(status: string): "ok" | "warn" | "err" | "idle" {
  if (status === "connected") return "ok";
  if (status === "failed" || status === "disconnected") return "err";
  return "warn";
}

export function ActiveScreen({
  shown,
  filesPanelOpen,
  onFilesPanelChange,
}: {
  shown: boolean;
  /** SFTP browser docked beside the terminal; lifted so the Files screen can open it. */
  filesPanelOpen: boolean;
  onFilesPanelChange: (open: boolean) => void;
}) {
  const { tabs, activeTabId, setActiveTabId, closeTab } = useTerminalTabs();
  const { forwards } = useData();
  const statuses = useForwardStatuses();
  const [pendingHostKey, setPendingHostKey] = useState<{ decision: HostKeyDecision; retry: () => void } | null>(null);
  const [clearingHistoryFor, setClearingHistoryFor] = useState<{ profileId: string; profileName: string } | null>(null);
  // Bumped after a clear so every open pane drops the list it is navigating.
  const [historyEpoch, setHistoryEpoch] = useState(0);
  const activeTab = tabs.find((t) => t.tabId === activeTabId) ?? null;
  const [sudoDialogFor, setSudoDialogFor] = useState<{ profileId: string; profileName: string } | null>(null);
  // Whether the active tab's profile has a saved sudo password; re-read when
  // the tab changes, when a terminal saves or drops one, and after the dialog.
  const [activeSudoSaved, setActiveSudoSaved] = useState(false);
  const activeProfileId = activeTab?.profileId ?? null;
  useEffect(() => {
    if (!activeProfileId) return;
    let canceled = false;
    sudoStatus(activeProfileId)
      .then((s) => !canceled && setActiveSudoSaved(s.saved))
      .catch(() => {});
    return () => {
      canceled = true;
    };
  }, [activeProfileId, activeTab?.sudoSaved, sudoDialogFor]);
  const [panelWidth, setPanelWidth] = useState(storedPanelWidth);
  // The side panel shows Files or the Toolbox, never both. Files is lifted
  // (the Files screen can open it), so it wins whenever it is open.
  const [toolboxOpen, setToolboxOpen] = useState(false);
  const showToolbox = toolboxOpen && !filesPanelOpen;
  const sendersRef = useRef(new Map<string, TerminalSender>());
  const registerSender = (tabId: string, sender: TerminalSender | null) => {
    if (sender) sendersRef.current.set(tabId, sender);
    else sendersRef.current.delete(tabId);
  };

  useEffect(() => {
    try {
      localStorage.setItem(FILES_PANEL_WIDTH_KEY, String(panelWidth));
    } catch {
      // Only a convenience; the default width is fine.
    }
  }, [panelWidth]);

  const startPanelResize = (e: ReactMouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = panelWidth;
    const move = (ev: MouseEvent) => setPanelWidth(clampPanelWidth(startWidth + startX - ev.clientX));
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      document.body.classList.remove("col-resizing");
    };
    document.body.classList.add("col-resizing");
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const activeForwards = forwards.filter((f) => {
    const s = statuses[f.id]?.status;
    return s && s !== "stopped";
  });

  return (
    <div
      className={`active-screen${filesPanelOpen ? " with-files" : ""}`}
      style={shown ? undefined : { display: "none" }}
    >
      <div className="terminal-workspace">
        <div className="tab-strip">
          {tabs.map((t) => (
            <div key={t.tabId} className={`tab ${activeTabId === t.tabId ? "active" : ""}`} onClick={() => setActiveTabId(t.tabId)}>
              <span className={`status-dot ${tabStatusTone(t.status)}`} />
              <span className="tab-label">{t.profileName}</span>
              <span className="tab-host">{t.hostLabel}</span>
              <button
                className="ghost tab-close"
                onClick={(e) => {
                  e.stopPropagation();
                  closeTab(t.tabId);
                }}
              >
                ✕
              </button>
            </div>
          ))}
          <div className="tab-strip-actions">
            {activeTab && (
              <button
                className={`ghost tab-strip-action${activeSudoSaved ? " toggled" : ""}`}
                title={
                  activeSudoSaved
                    ? `A sudo password is saved for ${activeTab.profileName} - click to forget it`
                    : `The sudo password for ${activeTab.profileName} is saved after you type it once`
                }
                onClick={() => setSudoDialogFor({ profileId: activeTab.profileId, profileName: activeTab.profileName })}
              >
                {activeSudoSaved ? "Sudo: saved" : "Sudo"}
              </button>
            )}
            {activeTab && (
              <button
                className="ghost tab-strip-action"
                title={`Clear the command history Remotix stores for ${activeTab.profileName}`}
                onClick={() => setClearingHistoryFor({ profileId: activeTab.profileId, profileName: activeTab.profileName })}
              >
                Clear history
              </button>
            )}
            <button
              className={`ghost tab-strip-action${showToolbox ? " toggled" : ""}`}
              title={showToolbox ? "Hide the toolbox" : "Saved commands to insert or run in the terminal"}
              onClick={() => {
                setToolboxOpen(!showToolbox);
                if (!showToolbox) onFilesPanelChange(false);
              }}
            >
              {showToolbox ? "Hide toolbox" : "Toolbox"}
            </button>
            <button
              className={`ghost tab-strip-action${filesPanelOpen ? " toggled" : ""}`}
              title={filesPanelOpen ? "Hide the file browser" : "Browse and transfer files beside the terminal"}
              onClick={() => {
                onFilesPanelChange(!filesPanelOpen);
                setToolboxOpen(false);
              }}
            >
              {filesPanelOpen ? "Hide files" : "Files"}
            </button>
          </div>
        </div>

        <div className="terminal-panes">
          {tabs.length === 0 && (
            <div className="empty-state">
              <h3>No open terminals</h3>
              <p>Connect to an SSH profile from the Terminals screen to open a session here.</p>
            </div>
          )}
          {tabs.map((t) => (
            <TerminalPane
              key={t.tabId}
              tab={t}
              visible={shown && t.tabId === activeTabId}
              historyEpoch={historyEpoch}
              onHostKeyDecision={(decision, retry) => decision && setPendingHostKey({ decision, retry })}
              registerSender={registerSender}
            />
          ))}
        </div>
      </div>

      {filesPanelOpen ? (
        <>
          <div className="panel-splitter" title="Drag to resize" onMouseDown={startPanelResize} />
          <div className="files-side-panel" style={{ width: panelWidth }}>
            <FilesScreen variant="panel" preferredProfileId={activeTab?.profileId} />
          </div>
        </>
      ) : showToolbox ? (
        <ToolboxPanel
          canSend={activeTab?.status === "connected"}
          onSend={(command, run) => activeTab && sendersRef.current.get(activeTab.tabId)?.(command, run)}
        />
      ) : (
        <div className="active-forwards-panel">
          <h3>Active Forwards</h3>
          {activeForwards.length === 0 ? (
            <p className="hint">No forwards are currently running.</p>
          ) : (
            <ul className="active-forward-list">
              {activeForwards.map((f) => (
                <li key={f.id}>
                  <div className="active-forward-row">
                    <span>{f.name}</span>
                    <ForwardStatusBadge status={statuses[f.id]} />
                    <button className="ghost" onClick={() => forwardsStop(f.id)}>
                      Stop
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {clearingHistoryFor && (
        <ConfirmDialog
          title={`Clear command history for "${clearingHistoryFor.profileName}"?`}
          message="Up/Down in terminals for this profile will start from empty again. This only clears the history Remotix stores - the remote shell's own history file is untouched."
          confirmLabel="Clear"
          danger
          onCancel={() => setClearingHistoryFor(null)}
          onConfirm={async () => {
            await commandHistoryClear(clearingHistoryFor.profileId);
            setClearingHistoryFor(null);
            setHistoryEpoch((e) => e + 1);
          }}
        />
      )}

      {sudoDialogFor && (
        <SudoPasswordDialog
          profileId={sudoDialogFor.profileId}
          profileName={sudoDialogFor.profileName}
          onClose={() => setSudoDialogFor(null)}
        />
      )}

      {pendingHostKey && (
        <HostKeyDialog
          decision={pendingHostKey.decision}
          onCancel={() => setPendingHostKey(null)}
          onResolved={() => {
            const retry = pendingHostKey.retry;
            setPendingHostKey(null);
            retry();
          }}
        />
      )}
    </div>
  );
}
