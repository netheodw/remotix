import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  CommandHistoryEntry,
  Group,
  EditHandle,
  EnqueueInput,
  KnownHost,
  LocalListing,
  LocalRoot,
  NewPortForwardInput,
  NewSshProfileInput,
  PortForward,
  RemoteListing,
  SftpConnected,
  SshProfile,
  TerminalEvent,
  TransferEvent,
  TermiusImportReport,
  TermiusPreview,
  TestConnectionStage,
  UpdatePortForwardInput,
  UpdateSshProfileInput,
  VaultStatus,
  GroupDeleteStrategy,
} from "./types";

// --- Vault ---
export const vaultStatus = () => invoke<VaultStatus>("vault_status");
export const vaultSetup = (password: string) => invoke<void>("vault_setup", { password });
export const vaultUnlock = (password: string) => invoke<void>("vault_unlock", { password });
export const vaultLock = () => invoke<void>("vault_lock");

// --- Terminal groups ---
export const terminalGroupsList = () => invoke<Group[]>("terminal_groups_list");
export const terminalGroupsCreate = (input: { parent_id: string | null; name: string }) =>
  invoke<Group>("terminal_groups_create", { input });
export const terminalGroupsUpdate = (input: { id: string; parent_id: string | null; name: string; sort_order: number | null }) =>
  invoke<Group>("terminal_groups_update", { input });
export const terminalGroupsDelete = (id: string, strategy: GroupDeleteStrategy) =>
  invoke<void>("terminal_groups_delete", { id, strategy });

// --- Port forward groups ---
export const portForwardGroupsList = () => invoke<Group[]>("port_forward_groups_list");
export const portForwardGroupsCreate = (input: { parent_id: string | null; name: string }) =>
  invoke<Group>("port_forward_groups_create", { input });
export const portForwardGroupsUpdate = (input: { id: string; parent_id: string | null; name: string; sort_order: number | null }) =>
  invoke<Group>("port_forward_groups_update", { input });
export const portForwardGroupsDelete = (id: string, strategy: GroupDeleteStrategy) =>
  invoke<void>("port_forward_groups_delete", { id, strategy });

// --- Profiles ---
export const profilesList = () => invoke<SshProfile[]>("profiles_list");
export const profilesGet = (id: string) => invoke<SshProfile>("profiles_get", { id });
export const profilesCreate = (input: NewSshProfileInput) => invoke<SshProfile>("profiles_create", { input });
export const profilesUpdate = (input: UpdateSshProfileInput) => invoke<SshProfile>("profiles_update", { input });
export const profilesDelete = (id: string) => invoke<void>("profiles_delete", { id });
export const profilesDuplicate = (id: string, newName: string) =>
  invoke<SshProfile>("profiles_duplicate", { id, newName });
export const profilesMoveToGroup = (id: string, groupId: string | null) =>
  invoke<SshProfile>("profiles_move_to_group", { id, groupId });
export const profilesImportPrivateKeyFile = (path: string) =>
  invoke<string>("profiles_import_private_key_file", { path });

export function profilesTestConnection(id: string, onStage: (s: TestConnectionStage) => void): Promise<void> {
  const channel = new Channel<TestConnectionStage>();
  channel.onmessage = onStage;
  return invoke<void>("profiles_test_connection", { id, channel });
}

// --- Import ---
export const importTermiusPreview = (path: string) => invoke<TermiusPreview>("import_termius_preview", { path });
export const importTermiusApply = (path: string, hostIndices: number[]) =>
  invoke<TermiusImportReport>("import_termius_apply", { path, hostIndices });

// --- Known hosts ---
export const knownHostsList = () => invoke<KnownHost[]>("known_hosts_list");
export const knownHostsTrust = (args: {
  host: string;
  port: number;
  keyAlgorithm: string;
  hostKeyB64: string;
  fingerprintSha256: string;
}) => invoke<void>("known_hosts_trust", args);

// --- Port forwards ---
export const forwardsList = () => invoke<PortForward[]>("forwards_list");
export const forwardsGet = (id: string) => invoke<PortForward>("forwards_get", { id });
export const forwardsCreate = (input: NewPortForwardInput) => invoke<PortForward>("forwards_create", { input });
export const forwardsUpdate = (input: UpdatePortForwardInput) => invoke<PortForward>("forwards_update", { input });
export const forwardsDelete = (id: string) => invoke<void>("forwards_delete", { id });
export const forwardsDuplicate = (id: string, newName: string) =>
  invoke<PortForward>("forwards_duplicate", { id, newName });
export const forwardsMoveToGroup = (id: string, groupId: string | null) =>
  invoke<PortForward>("forwards_move_to_group", { id, groupId });
export const forwardsStart = (id: string) => invoke<void>("forwards_start", { id });
export const forwardsStop = (id: string) => invoke<void>("forwards_stop", { id });

// --- Terminal ---
export function terminalOpen(
  profileId: string,
  cols: number,
  rows: number,
  onEvent: (e: TerminalEvent) => void,
): Promise<string> {
  const channel = new Channel<TerminalEvent>();
  channel.onmessage = onEvent;
  return invoke<string>("terminal_open", { profileId, cols, rows, channel });
}
export const terminalInput = (id: string, dataB64: string) => invoke<void>("terminal_input", { id, dataB64 });
export const terminalResize = (id: string, cols: number, rows: number) =>
  invoke<void>("terminal_resize", { id, cols, rows });
export const terminalClose = (id: string) => invoke<void>("terminal_close", { id });

// --- Command history (per SSH profile, replayed by Up/Down in the terminal) ---
export const commandHistoryList = (profileId: string, limit?: number) =>
  invoke<CommandHistoryEntry[]>("command_history_list", { profileId, limit });
export const commandHistoryRecord = (profileId: string, command: string) =>
  invoke<CommandHistoryEntry | null>("command_history_record", { profileId, command });
export const commandHistoryClear = (profileId: string) =>
  invoke<void>("command_history_clear", { profileId });

// --- SFTP ---
export function sftpConnect(profileId: string, onTransferEvent: (e: TransferEvent) => void): Promise<SftpConnected> {
  const transfers = new Channel<TransferEvent>();
  transfers.onmessage = onTransferEvent;
  return invoke<SftpConnected>("sftp_connect", { profileId, transfers });
}
export const sftpDisconnect = (sessionId: string) => invoke<void>("sftp_disconnect", { sessionId });
export const sftpList = (sessionId: string, path: string) =>
  invoke<RemoteListing>("sftp_list", { sessionId, path });
export const sftpEnqueue = (sessionId: string, input: EnqueueInput) =>
  invoke<string[]>("sftp_enqueue", { sessionId, input });
export const sftpCancel = (sessionId: string, itemId: string) =>
  invoke<void>("sftp_cancel", { sessionId, itemId });
export const sftpCancelAll = (sessionId: string) => invoke<void>("sftp_cancel_all", { sessionId });
export const sftpDelete = (sessionId: string, path: string) =>
  invoke<void>("sftp_delete", { sessionId, path });
export const sftpEditOpen = (sessionId: string, remotePath: string) =>
  invoke<EditHandle>("sftp_edit_open", { sessionId, remotePath });

// --- Local filesystem ---
export const localListDir = (path: string) => invoke<LocalListing>("local_list_dir", { path });
export const localHomeDir = () => invoke<string>("local_home_dir");
export const localRoots = () => invoke<LocalRoot[]>("local_roots");
export const localMkdir = (parent: string, name: string) => invoke<string>("local_mkdir", { parent, name });
export const localDelete = (path: string) => invoke<void>("local_delete", { path });
export const localFileMtime = (path: string) => invoke<number | null>("local_file_mtime", { path });
export const localDiscardScratch = (path: string) => invoke<void>("local_discard_scratch", { path });

// --- Settings ---
export const settingsGetAll = () => invoke<Record<string, string>>("settings_get_all");
export const settingsSet = (key: string, value: string) => invoke<void>("settings_set", { key, value });
export const appDataDir = () => invoke<string>("app_data_dir");
