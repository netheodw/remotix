export interface Group {
  id: string;
  parent_id: string | null;
  name: string;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export type AuthKind = "password" | "private_key" | "private_key_passphrase";

export function authKindLabel(kind: AuthKind): string {
  if (kind === "password") return "Password";
  if (kind === "private_key") return "Private Key";
  return "Private Key + Passphrase";
}

export interface SshProfile {
  id: string;
  terminal_group_id: string | null;
  name: string;
  hostname: string;
  port: number;
  username: string;
  authentication_kind: AuthKind;
  has_secret: boolean;
  has_passphrase: boolean;
  terminal_type: string;
  sort_order: number;
  last_status: string | null;
  last_connected_at: string | null;
  created_at: string;
  updated_at: string;
}

export type AuthSecretInput =
  | { kind: "password"; password: string }
  | { kind: "private_key"; private_key: string }
  | { kind: "private_key_passphrase"; private_key: string; passphrase: string }
  | { kind: "unchanged" }
  | { kind: "clear" };

export interface NewSshProfileInput {
  terminal_group_id: string | null;
  name: string;
  hostname: string;
  port: number;
  username: string;
  terminal_type: string | null;
  secret: AuthSecretInput;
}

export interface UpdateSshProfileInput extends NewSshProfileInput {
  id: string;
}

export type ForwardType = "local" | "remote" | "dynamic";

export interface PortForward {
  id: string;
  port_forward_group_id: string | null;
  ssh_profile_id: string | null;
  name: string;
  forward_type: ForwardType;
  bind_address: string;
  listen_port: number;
  target_hostname: string | null;
  target_port: number | null;
  start_on_launch: boolean;
  sort_order: number;
  created_at: string;
  updated_at: string;
}

export interface NewPortForwardInput {
  port_forward_group_id: string | null;
  ssh_profile_id: string;
  name: string;
  forward_type: ForwardType;
  bind_address: string;
  listen_port: number;
  target_hostname: string | null;
  target_port: number | null;
  start_on_launch: boolean;
}

export interface UpdatePortForwardInput extends NewPortForwardInput {
  id: string;
}

export interface TermiusForwardPreview {
  name: string;
  forward_type: ForwardType | null;
  bind_address: string;
  listen_port: number;
  target_hostname: string | null;
  target_port: number | null;
  problem: string | null;
}

export interface TermiusHostPreview {
  index: number;
  name: string;
  hostname: string;
  port: number;
  username: string;
  authentication_kind: AuthKind | null;
  problem: string | null;
  existing_profile_name: string | null;
  forwards: TermiusForwardPreview[];
}

export interface TermiusPreview {
  hosts: TermiusHostPreview[];
}

export interface TermiusImportReport {
  profiles_created: number;
  forwards_created: number;
  skipped: { name: string; reason: string }[];
}

export type ForwardRuntimeStatus =
  | { status: "stopped" }
  | { status: "starting" }
  | { status: "verifying_host" }
  | { status: "authenticating" }
  | { status: "active"; connection_count: number }
  | { status: "reconnecting"; attempt: number; next_retry_secs: number }
  | { status: "failed"; message: string }
  | { status: "needs_profile" };

export interface KnownHost {
  id: string;
  hostname: string;
  port: number;
  key_algorithm: string;
  sha256_fingerprint: string;
  first_trusted_at: string;
  last_seen_at: string;
}

export type GroupDeleteStrategy = "reassign_to_parent" | "delete_subtree";

export interface VaultStatus {
  initialized: boolean;
  unlocked: boolean;
}

export type AppErrorKind =
  | "validation"
  | "not_found"
  | "vault_locked"
  | "vault_already_unlocked"
  | "vault_already_initialized"
  | "vault_not_initialized"
  | "incorrect_password"
  | "rate_limited"
  | "crypto"
  | "database"
  | "host_key_untrusted"
  | "host_key_mismatch"
  | "authentication_failed"
  | "connection_failed"
  | "ssh"
  | "io"
  | "internal";

export interface AppError {
  kind: AppErrorKind;
  message?:
    | {
        host?: string;
        port?: number;
        fingerprint_sha256?: string;
        key_algorithm?: string;
        host_key_b64?: string;
        previous_fingerprint_sha256?: string;
        presented_fingerprint_sha256?: string;
        retry_after_secs?: number;
      }
    | string;
}

export function isAppError(e: unknown): e is AppError {
  return typeof e === "object" && e !== null && "kind" in e;
}

export type TestConnectionStage =
  | { stage: "connecting" }
  | { stage: "verifying_host" }
  | { stage: "authenticating" };

export type TerminalEvent =
  | { type: "status"; state: string; message: string | null }
  | { type: "output"; data_b64: string }
  | { type: "closed"; reason: string };

export interface CommandHistoryEntry {
  id: string;
  ssh_profile_id: string;
  command: string;
  executed_at: string;
}

export type FileEntryKind = "file" | "directory" | "symlink" | "other";

export interface LocalEntry {
  name: string;
  path: string;
  kind: FileEntryKind;
  size: number;
  modified: number | null;
  hidden: boolean;
}

export interface LocalListing {
  path: string;
  parent: string | null;
  entries: LocalEntry[];
  truncated: boolean;
}

export interface LocalRoot {
  label: string;
  path: string;
  kind: "drive" | "shortcut";
}

export interface RemoteEntry {
  name: string;
  path: string;
  kind: FileEntryKind;
  resolved_kind: FileEntryKind | null;
  size: number;
  modified: number | null;
  permissions: number | null;
}

export interface RemoteListing {
  path: string;
  parent: string | null;
  entries: RemoteEntry[];
  truncated: boolean;
}

export interface SftpConnected {
  session_id: string;
  home_path: string;
}

export type TransferDirection = "upload" | "download";
export type ConflictPolicy = "overwrite" | "skip" | "keep_both";

export interface EnqueueInput {
  direction: TransferDirection;
  localDir: string;
  remoteDir: string;
  items: string[];
  conflict: ConflictPolicy;
}

export interface EditHandle {
  local_path: string;
  local_dir: string;
  name: string;
  remote_dir: string;
  modified_at: number | null;
}

export type TransferEvent =
  | {
      type: "queued";
      item_id: string;
      direction: TransferDirection;
      name: string;
      source: string;
      destination: string;
    }
  | { type: "planning"; item_id: string }
  | { type: "planned"; item_id: string; file_count: number; total_bytes: number }
  | { type: "started"; item_id: string; file_index: number; file_count: number; current_name: string }
  | {
      type: "progress";
      item_id: string;
      transferred_bytes: number;
      total_bytes: number;
      file_index: number;
      file_count: number;
      bytes_per_sec: number;
    }
  | { type: "file_done"; item_id: string; file_index: number; path: string }
  | { type: "file_failed"; item_id: string; path: string; message: string }
  | { type: "skipped"; item_id: string; path: string; reason: string }
  | { type: "canceled"; item_id: string }
  | { type: "failed"; item_id: string; message: string }
  | {
      type: "completed";
      item_id: string;
      transferred_bytes: number;
      file_count: number;
      skipped: number;
      failed: number;
    }
  | { type: "idle" };
