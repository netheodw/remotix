import {
  localDelete,
  localHomeDir,
  localListDir,
  localMkdir,
  localRoots,
  sftpDelete,
  sftpList,
} from "../../api/commands";
import type { FileEntryKind, LocalEntry, RemoteEntry } from "../../api/types";
import * as posixPath from "./posixPath";
import * as winPath from "./winPath";

export type FileKind = FileEntryKind;

export interface FileEntry {
  name: string;
  path: string;
  kind: FileKind;
  resolvedKind: FileKind | null;
  size: number;
  modified: number | null;
  hidden: boolean;
}

export interface FileListing {
  path: string;
  parent: string | null;
  entries: FileEntry[];
  truncated: boolean;
}

/**
 * Everything `FilePane` needs to browse one side of the transfer. The local and
 * remote implementations differ in path separator, transport and which
 * mutations they support, so the pane itself never branches on which side it is
 * rendering - and never does path string arithmetic of its own.
 */
export interface FileSource {
  kind: "local" | "remote";
  label: string;
  list(path: string): Promise<FileListing>;
  home(): Promise<string>;
  roots?(): Promise<{ label: string; path: string }[]>;
  join(dir: string, name: string): string;
  parentOf(path: string): string | null;
  segments(path: string): { label: string; path: string }[];
  canMutate: boolean;
  mkdir?(parent: string, name: string): Promise<string>;
  /** Removes a file, or a directory and its contents. */
  remove?(entry: FileEntry): Promise<void>;
}

function toEntry(entry: LocalEntry): FileEntry {
  return {
    name: entry.name,
    path: entry.path,
    kind: entry.kind,
    resolvedKind: null,
    size: entry.size,
    modified: entry.modified,
    hidden: entry.hidden,
  };
}

function toRemote(entry: RemoteEntry): FileEntry {
  return {
    name: entry.name,
    path: entry.path,
    kind: entry.kind,
    resolvedKind: entry.resolved_kind,
    size: entry.size,
    modified: entry.modified,
    hidden: entry.name.startsWith("."),
  };
}

export function createLocalSource(): FileSource {
  return {
    kind: "local",
    label: "This PC",
    async list(path) {
      const listing = await localListDir(path);
      return { ...listing, entries: listing.entries.map(toEntry) };
    },
    home: localHomeDir,
    async roots() {
      return await localRoots();
    },
    join: winPath.join,
    parentOf: winPath.parentOf,
    segments: winPath.segments,
    canMutate: true,
    mkdir: localMkdir,
    remove: (entry) => localDelete(entry.path),
  };
}

/** `homePath` comes back from `sftp_connect`, so `home()` costs no round trip. */
export function createRemoteSource(sessionId: string, label: string, homePath: string): FileSource {
  return {
    kind: "remote",
    label,
    async list(path) {
      const listing = await sftpList(sessionId, path);
      return { ...listing, entries: listing.entries.map(toRemote) };
    },
    async home() {
      return homePath;
    },
    join: posixPath.join,
    parentOf: posixPath.parentOf,
    segments: posixPath.segments,
    canMutate: false,
    remove: (entry) => sftpDelete(sessionId, entry.path),
  };
}
