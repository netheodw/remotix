import type { HostKeyDecision } from "../components/HostKeyDialog";
import type { AppError } from "./types";

function isAppErrorShape(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "kind" in (e as Record<string, unknown>);
}

export function parseHostKeyError(e: unknown): HostKeyDecision | null {
  if (!isAppErrorShape(e)) return null;
  if (e.kind === "host_key_untrusted" && typeof e.message === "object" && e.message) {
    const m = e.message;
    return {
      kind: "untrusted",
      host: m.host ?? "",
      port: m.port ?? 0,
      keyAlgorithm: m.key_algorithm ?? "",
      fingerprint: m.fingerprint_sha256 ?? "",
      hostKeyB64: m.host_key_b64 ?? "",
    };
  }
  if (e.kind === "host_key_mismatch" && typeof e.message === "object" && e.message) {
    const m = e.message;
    return {
      kind: "changed",
      host: m.host ?? "",
      port: m.port ?? 0,
      keyAlgorithm: m.key_algorithm ?? "",
      fingerprint: m.presented_fingerprint_sha256 ?? "",
      previousFingerprint: m.previous_fingerprint_sha256,
      hostKeyB64: m.host_key_b64 ?? "",
    };
  }
  return null;
}

const KIND_LABELS: Partial<Record<AppError["kind"], string>> = {
  vault_locked: "The vault is locked.",
  incorrect_password: "Incorrect master password.",
  authentication_failed: "Authentication failed. Check the username, password, or key.",
  vault_not_initialized: "No vault has been created yet.",
};

export function errorMessage(e: unknown): string {
  if (isAppErrorShape(e)) {
    if (typeof e.message === "string") return e.message;
    if (e.kind === "rate_limited" && typeof e.message === "object") {
      return `Too many attempts. Try again in ${e.message.retry_after_secs ?? "a few"}s.`;
    }
    if (KIND_LABELS[e.kind]) return KIND_LABELS[e.kind]!;
    return e.kind.replace(/_/g, " ");
  }
  if (e instanceof Error) return e.message;
  return String(e);
}
