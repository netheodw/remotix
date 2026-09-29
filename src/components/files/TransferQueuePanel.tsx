import { formatBytes, formatEta, formatSpeed } from "../../api/format";
import type { ConflictPolicy } from "../../api/types";
import { useFiles } from "../../state/FilesContext";
import type { TransferItem } from "../../state/transferQueue";

const CONFLICTS: { value: ConflictPolicy; label: string }[] = [
  { value: "overwrite", label: "Overwrite" },
  { value: "skip", label: "Skip" },
  { value: "keep_both", label: "Keep both" },
];

function percent(item: TransferItem): number {
  if (item.totalBytes > 0) return Math.min(100, (item.transferredBytes / item.totalBytes) * 100);
  // No bytes planned means nothing actually moved - never show a full bar.
  return 0;
}

/** Finished, but not cleanly: some files failed or there was nothing to move. */
function isPartial(item: TransferItem): boolean {
  return item.status === "done" && (item.failed > 0 || item.fileCount === 0);
}

function detail(item: TransferItem): string {
  if (item.status === "planning") return "Scanning…";
  if (item.status === "canceling") return "Cancelling…";
  if (item.status === "canceled") return "Cancelled";
  if (item.status === "queued") return "Queued";
  const lastError = item.errors[item.errors.length - 1]?.message;
  if (item.status === "failed") return lastError ?? "Failed";
  if (item.status === "done" && item.fileCount === 0) return "Nothing to transfer";
  if (item.status === "done" && item.failed > 0) {
    return `${item.failed} of ${item.fileCount} failed${lastError ? ` — ${lastError}` : ""}`;
  }

  const parts: string[] = [];
  if (item.totalBytes > 0) {
    parts.push(`${formatBytes(item.transferredBytes)} / ${formatBytes(item.totalBytes)}`);
  }
  if (item.fileCount > 1) parts.push(`file ${item.fileIndex}/${item.fileCount}`);

  if (item.status === "running" && item.bytesPerSec > 0) {
    parts.push(formatSpeed(item.bytesPerSec));
    const remaining = item.totalBytes - item.transferredBytes;
    if (remaining > 0) parts.push(`${formatEta(remaining / item.bytesPerSec)} left`);
  }
  if (item.status === "done" && item.skipped > 0) parts.push(`${item.skipped} skipped`);
  return parts.join(" · ");
}

export function TransferQueuePanel() {
  const { queue, activeTransfers, conflict, setConflict, cancel, cancelAll, clearCompleted } = useFiles();

  if (queue.length === 0) return null;

  return (
    <div className="transfer-queue">
      <div className="transfer-queue-header">
        <strong>
          Transfers{activeTransfers > 0 ? ` — ${activeTransfers} active` : ""}
        </strong>
        <div className="transfer-queue-tools">
          <label className="file-toggle">
            On conflict
            <select value={conflict} onChange={(e) => setConflict(e.target.value as ConflictPolicy)}>
              {CONFLICTS.map((c) => (
                <option key={c.value} value={c.value}>
                  {c.label}
                </option>
              ))}
            </select>
          </label>
          <button className="ghost" onClick={clearCompleted}>
            Clear finished
          </button>
          <button className="ghost" disabled={activeTransfers === 0} onClick={cancelAll}>
            Cancel all
          </button>
        </div>
      </div>

      <div className="transfer-list">
        {queue.map((item) => (
          <div key={item.itemId} className={`transfer-row ${isPartial(item) ? "partial" : item.status}`}>
            <span className="transfer-arrow" title={item.direction}>
              {item.direction === "upload" ? "→" : "←"}
            </span>
            <div className="transfer-main">
              <div className="transfer-name" title={`${item.source} → ${item.destination}`}>
                {item.name}
                {item.currentName && item.fileCount > 1 && (
                  <span className="transfer-current"> · {item.currentName}</span>
                )}
              </div>
              <div className="progress-track">
                <div className="progress-fill" style={{ width: `${percent(item)}%` }} />
              </div>
            </div>
            <span className="transfer-meta">{detail(item)}</span>
            <button
              className="ghost"
              disabled={["done", "failed", "canceled"].includes(item.status)}
              onClick={() => cancel(item.itemId)}
            >
              ✕
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
