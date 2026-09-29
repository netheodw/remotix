import type { TransferDirection, TransferEvent } from "../api/types";

export type TransferStatus =
  | "queued"
  | "planning"
  | "running"
  | "canceling"
  | "done"
  | "failed"
  | "canceled";

export interface TransferItem {
  itemId: string;
  direction: TransferDirection;
  name: string;
  source: string;
  destination: string;
  status: TransferStatus;
  fileIndex: number;
  fileCount: number;
  transferredBytes: number;
  totalBytes: number;
  bytesPerSec: number;
  currentName: string | null;
  skipped: number;
  failed: number;
  errors: { path: string; message: string }[];
}

const TERMINAL: TransferStatus[] = ["done", "failed", "canceled"];

function isTerminal(item: TransferItem): boolean {
  return TERMINAL.includes(item.status);
}

/**
 * Pure reducer over the backend's event stream. Events can arrive late or out
 * of order relative to a cancel, so the invariants matter more than the
 * transitions: a finished row is never resurrected and a progress bar never
 * goes backwards.
 */
export function transferQueueReducer(state: TransferItem[], event: TransferEvent): TransferItem[] {
  if (event.type === "idle") return state;

  if (event.type === "queued") {
    if (state.some((item) => item.itemId === event.item_id)) return state;
    return [
      ...state,
      {
        itemId: event.item_id,
        direction: event.direction,
        name: event.name,
        source: event.source,
        destination: event.destination,
        status: "queued",
        fileIndex: 0,
        fileCount: 0,
        transferredBytes: 0,
        totalBytes: 0,
        bytesPerSec: 0,
        currentName: null,
        skipped: 0,
        failed: 0,
        errors: [],
      },
    ];
  }

  const index = state.findIndex((item) => item.itemId === event.item_id);
  if (index < 0) return state;

  const current = state[index];
  const patch = patchFor(current, event);
  if (!patch) return state;

  const next = [...state];
  next[index] = { ...current, ...patch };
  return next;
}

function patchFor(item: TransferItem, event: TransferEvent): Partial<TransferItem> | null {
  // Anything that has already finished stays finished.
  if (isTerminal(item) && event.type !== "queued") {
    return null;
  }

  switch (event.type) {
    case "planning":
      return { status: "planning" };
    case "planned":
      return { status: "running", fileCount: event.file_count, totalBytes: event.total_bytes };
    case "started":
      return {
        status: item.status === "canceling" ? "canceling" : "running",
        fileIndex: event.file_index,
        fileCount: event.file_count,
        currentName: event.current_name,
      };
    case "progress":
      return {
        // A late event must not rewind the bar.
        transferredBytes: Math.max(item.transferredBytes, event.transferred_bytes),
        totalBytes: event.total_bytes,
        fileIndex: event.file_index,
        fileCount: event.file_count,
        bytesPerSec: event.bytes_per_sec,
      };
    case "file_done":
      return { fileIndex: Math.max(item.fileIndex, event.file_index) };
    case "file_failed":
      return {
        failed: item.failed + 1,
        errors: [...item.errors, { path: event.path, message: event.message }],
      };
    case "skipped":
      return { skipped: item.skipped + 1 };
    case "canceled":
      return { status: "canceled", bytesPerSec: 0, currentName: null };
    case "failed":
      return {
        status: "failed",
        bytesPerSec: 0,
        currentName: null,
        errors: [...item.errors, { path: item.name, message: event.message }],
      };
    case "completed": {
      // Land exactly on 100%, whatever the last throttled progress event said.
      const total = Math.max(item.totalBytes, event.transferred_bytes);
      // Every file failing is a failed transfer, not a finished one - reporting
      // it as done is how a broken upload ends up looking like a green bar.
      const allFailed = event.file_count > 0 && event.failed >= event.file_count;
      return {
        status: allFailed ? "failed" : "done",
        transferredBytes: total,
        totalBytes: total,
        fileCount: event.file_count,
        fileIndex: event.file_count,
        skipped: event.skipped,
        failed: event.failed,
        bytesPerSec: 0,
        currentName: null,
      };
    }
    default:
      return null;
  }
}

/** Optimistic local transition so a cancel click feels immediate; the row
 *  clears for real when the worker's `canceled` event lands. */
export function markCanceling(state: TransferItem[], itemId: string): TransferItem[] {
  return state.map((item) =>
    item.itemId === itemId && !isTerminal(item) ? { ...item, status: "canceling" } : item,
  );
}

export function markAllCanceling(state: TransferItem[]): TransferItem[] {
  return state.map((item) => (isTerminal(item) ? item : { ...item, status: "canceling" }));
}

export function clearFinished(state: TransferItem[]): TransferItem[] {
  return state.filter((item) => !isTerminal(item));
}

export function activeCount(state: TransferItem[]): number {
  return state.filter((item) => !isTerminal(item)).length;
}
