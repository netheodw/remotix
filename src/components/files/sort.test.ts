import { describe, expect, it } from "vitest";
import type { FileEntry } from "./fileSource";
import { sortEntries, type SortDir, type SortKey } from "./sort";

function entry(name: string, overrides: Partial<FileEntry> = {}): FileEntry {
  return {
    name,
    path: `/${name}`,
    kind: "file",
    resolvedKind: null,
    size: 0,
    modified: 0,
    hidden: false,
    ...overrides,
  };
}

const dir = (name: string, overrides: Partial<FileEntry> = {}) =>
  entry(name, { kind: "directory", ...overrides });

const KEYS: SortKey[] = ["name", "size", "modified"];
const DIRS: SortDir[] = ["asc", "desc"];

describe("sortEntries", () => {
  it("puts directories first under every key and direction", () => {
    const entries = [entry("a.txt", { size: 9 }), dir("zzz", { size: 1 }), entry("b.txt", { size: 2 })];
    for (const key of KEYS) {
      for (const sortDir of DIRS) {
        const sorted = sortEntries(entries, { key, dir: sortDir });
        expect(sorted[0].name, `${key}/${sortDir}`).toBe("zzz");
      }
    }
  });

  it("sorts names naturally and honours direction", () => {
    const entries = [entry("file10"), entry("file2"), entry("file1")];
    expect(sortEntries(entries, { key: "name", dir: "asc" }).map((e) => e.name)).toEqual([
      "file1",
      "file2",
      "file10",
    ]);
    expect(sortEntries(entries, { key: "name", dir: "desc" }).map((e) => e.name)).toEqual([
      "file10",
      "file2",
      "file1",
    ]);
  });

  it("sorts an unknown modified time last in both directions", () => {
    const entries = [entry("unknown", { modified: null }), entry("older", { modified: 10 }), entry("newer", { modified: 20 })];
    for (const sortDir of DIRS) {
      const sorted = sortEntries(entries, { key: "modified", dir: sortDir });
      expect(sorted[sorted.length - 1].name, sortDir).toBe("unknown");
    }
  });

  it("is stable for equal keys", () => {
    const entries = [entry("b", { size: 1 }), entry("a", { size: 1 }), entry("c", { size: 1 })];
    expect(sortEntries(entries, { key: "size", dir: "asc" }).map((e) => e.name)).toEqual(["b", "a", "c"]);
  });

  it("groups a symlink that resolves to a directory with the directories", () => {
    const entries = [entry("a.txt"), entry("link", { kind: "symlink", resolvedKind: "directory" })];
    expect(sortEntries(entries, { key: "name", dir: "asc" })[0].name).toBe("link");
  });

  it("does not mutate the input", () => {
    const entries = [entry("b"), entry("a")];
    sortEntries(entries, { key: "name", dir: "asc" });
    expect(entries.map((e) => e.name)).toEqual(["b", "a"]);
  });
});
