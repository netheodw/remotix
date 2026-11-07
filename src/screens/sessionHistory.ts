import type { TerminalSessionSummary } from "../api/types";

/** "1h 05m", "4m 12s", "38s"; null when the end is unknown. */
export function formatSessionDuration(start: string, end: string | null): string | null {
  if (!end) return null;
  const seconds = Math.max(0, Math.round((Date.parse(end) - Date.parse(start)) / 1000));
  if (Number.isNaN(seconds)) return null;
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  if (h > 0) return `${h}h ${String(m).padStart(2, "0")}m`;
  if (m > 0) return `${m}m ${String(s).padStart(2, "0")}s`;
  return `${s}s`;
}

/** Local date and time, to the minute or (`withSeconds`) the second. */
export function formatTimestamp(iso: string, withSeconds = false): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    ...(withSeconds ? { second: "2-digit" } : {}),
  });
}

export function sessionStatusLabel(s: TerminalSessionSummary, live: boolean): string {
  if (live && s.status === "connected") return "connected now";
  switch (s.status) {
    case "connecting":
      return live ? "connecting" : "never finished";
    case "connected":
      return "connected";
    case "failed":
      return "failed";
    case "closed":
      return "closed";
    case "interrupted":
      return "interrupted";
  }
}

export function sessionStatusTone(s: TerminalSessionSummary): "ok" | "err" | "idle" | "warn" {
  if (s.status === "failed") return "err";
  if (s.status === "interrupted") return "warn";
  if (s.status === "connected" || s.status === "connecting") return "ok";
  return "idle";
}
