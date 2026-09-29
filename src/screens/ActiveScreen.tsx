import { useState } from "react";
import { commandHistoryClear, forwardsStop } from "../api/commands";
import type { HostKeyDecision } from "../components/HostKeyDialog";
import { HostKeyDialog } from "../components/HostKeyDialog";
import { ConfirmDialog } from "../components/Modal";
import { ForwardStatusBadge } from "../components/StatusBadge";
import { TerminalPane } from "../components/TerminalPane";
import { useData } from "../state/DataContext";
import { useForwardStatuses } from "../state/ForwardStatusContext";
import { useTerminalTabs } from "../state/TerminalTabsContext";

function tabStatusTone(status: string): "ok" | "warn" | "err" | "idle" {
  if (status === "connected") return "ok";
  if (status === "failed" || status === "disconnected") return "err";
  return "warn";
}

export function ActiveScreen() {
  const { tabs, activeTabId, setActiveTabId, closeTab } = useTerminalTabs();
  const { forwards } = useData();
  const statuses = useForwardStatuses();
  const [pendingHostKey, setPendingHostKey] = useState<{ decision: HostKeyDecision; retry: () => void } | null>(null);
  const [clearingHistoryFor, setClearingHistoryFor] = useState<{ profileId: string; profileName: string } | null>(null);
  // Bumped after a clear so every open pane drops the list it is navigating.
  const [historyEpoch, setHistoryEpoch] = useState(0);
  const activeTab = tabs.find((t) => t.tabId === activeTabId) ?? null;

  const activeForwards = forwards.filter((f) => {
    const s = statuses[f.id]?.status;
    return s && s !== "stopped";
  });

  return (
    <div className="active-screen">
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
          {activeTab && (
            <button
              className="ghost tab-strip-action"
              title={`Clear the command history Remotix stores for ${activeTab.profileName}`}
              onClick={() => setClearingHistoryFor({ profileId: activeTab.profileId, profileName: activeTab.profileName })}
            >
              Clear history
            </button>
          )}
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
              visible={t.tabId === activeTabId}
              historyEpoch={historyEpoch}
              onHostKeyDecision={(decision, retry) => decision && setPendingHostKey({ decision, retry })}
            />
          ))}
        </div>
      </div>

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
