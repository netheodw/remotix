import type { ForwardRuntimeStatus } from "../api/types";

const LABELS: Record<ForwardRuntimeStatus["status"], string> = {
  stopped: "Stopped",
  starting: "Starting",
  verifying_host: "Verifying Host",
  authenticating: "Authenticating",
  active: "Active",
  reconnecting: "Reconnecting",
  failed: "Failed",
  needs_profile: "Needs Profile",
};

const TONE: Record<ForwardRuntimeStatus["status"], "ok" | "warn" | "err" | "idle"> = {
  stopped: "idle",
  starting: "warn",
  verifying_host: "warn",
  authenticating: "warn",
  active: "ok",
  reconnecting: "warn",
  failed: "err",
  needs_profile: "err",
};

export function ForwardStatusBadge({ status }: { status: ForwardRuntimeStatus }) {
  const tone = TONE[status.status];
  let extra = "";
  if (status.status === "active") extra = ` (${status.connection_count})`;
  if (status.status === "reconnecting") extra = ` (attempt ${status.attempt}, retry in ${status.next_retry_secs}s)`;
  if (status.status === "failed") extra = `: ${status.message}`;

  return (
    <span title={status.status === "failed" ? status.message : undefined}>
      <span className={`status-dot ${tone}`} />
      {LABELS[status.status]}
      {extra}
    </span>
  );
}

export function TextStatusBadge({ label, tone }: { label: string; tone: "ok" | "warn" | "err" | "idle" }) {
  return (
    <span>
      <span className={`status-dot ${tone}`} />
      {label}
    </span>
  );
}
