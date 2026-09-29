import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useMemo, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { FilePane } from "./FilePane";
import type { FileEntry, FileSource } from "./fileSource";
import * as posixPath from "./posixPath";

// jsdom has no layout, so it does not implement scrollIntoView.
Element.prototype.scrollIntoView = vi.fn();

function entry(name: string, kind: FileEntry["kind"]): FileEntry {
  return {
    name,
    path: `/home/${name}`,
    kind,
    resolvedKind: null,
    size: 10,
    modified: 1700000000,
    hidden: false,
  };
}

const TREE: Record<string, FileEntry[]> = {
  "/home": [entry("notes.txt", "file"), entry("projects", "directory"), entry("archive.zip", "file")],
  "/home/projects": [entry("readme.md", "file")],
  "/": [entry("home", "directory")],
};

/** The whole point of `FileSource`: the pane under test needs no `invoke` mock. */
function memorySource(): FileSource {
  return {
    kind: "remote",
    label: "test-host",
    async list(path) {
      return { path, parent: posixPath.parentOf(path), entries: TREE[path] ?? [], truncated: false };
    },
    async home() {
      return "/home";
    },
    join: posixPath.join,
    parentOf: posixPath.parentOf,
    segments: posixPath.segments,
    canMutate: false,
  };
}

function Harness({ onCwd, source }: { onCwd?: (path: string) => void; source?: FileSource }) {
  const [cwd, setCwd] = useState("/home");
  const [selection, setSelection] = useState<Set<string>>(new Set());
  const fallback = useMemo(memorySource, []);
  return (
    <FilePane
      source={source ?? fallback}
      cwd={cwd}
      onCwdChange={(path) => {
        onCwd?.(path);
        setCwd(path);
      }}
      selection={selection}
      onSelectionChange={setSelection}
    />
  );
}

/** Mirrors how FilesScreen drives the pane: a token bumped when a transfer
 *  lands, which must cause the destination pane to re-list. */
function RefreshHarness({ source, token }: { source: FileSource; token: number }) {
  const [selection, setSelection] = useState<Set<string>>(new Set());
  return (
    <FilePane
      source={source}
      cwd="/home"
      onCwdChange={() => {}}
      selection={selection}
      onSelectionChange={setSelection}
      refreshToken={token}
    />
  );
}

async function rowNames(): Promise<string[]> {
  await screen.findByText("notes.txt");
  return Array.from(document.querySelectorAll(".file-row:not(.parent-row) .file-name")).map((cell) =>
    (cell.textContent ?? "").trim(),
  );
}

describe("FilePane", () => {
  it("lists directories before files", async () => {
    render(<Harness />);
    expect(await rowNames()).toEqual(["projects", "archive.zip", "notes.txt"]);
  });

  it("navigates into a directory on double-click", async () => {
    const onCwd = vi.fn();
    render(<Harness onCwd={onCwd} />);
    fireEvent.doubleClick(await screen.findByText("projects"));
    expect(onCwd).toHaveBeenCalledWith("/home/projects");
    expect(await screen.findByText("readme.md")).toBeDefined();
  });

  it("navigates to the parent on Backspace", async () => {
    const onCwd = vi.fn();
    render(<Harness onCwd={onCwd} />);
    await screen.findByText("notes.txt");
    fireEvent.keyDown(document.querySelector(".file-table-wrap")!, { key: "Backspace" });
    expect(onCwd).toHaveBeenCalledWith("/");
  });

  it("filters the visible rows", async () => {
    render(<Harness />);
    await screen.findByText("notes.txt");
    fireEvent.change(screen.getByPlaceholderText("Filter"), { target: { value: "zip" } });
    expect(screen.queryByText("notes.txt")).toBeNull();
    expect(screen.getByText("archive.zip")).toBeDefined();
  });

  it("re-lists when the refresh token changes, so a finished transfer shows up", async () => {
    const source = memorySource();
    const list = vi.spyOn(source, "list");
    const { rerender } = render(<RefreshHarness source={source} token={0} />);
    await screen.findByText("notes.txt");
    expect(list).toHaveBeenCalledTimes(1);

    rerender(<RefreshHarness source={source} token={1} />);
    await waitFor(() => expect(list).toHaveBeenCalledTimes(2));
    expect(list).toHaveBeenLastCalledWith("/home");
  });

  it("does not re-list on an unrelated re-render", async () => {
    const source = memorySource();
    const list = vi.spyOn(source, "list");
    const { rerender } = render(<RefreshHarness source={source} token={0} />);
    await screen.findByText("notes.txt");

    rerender(<RefreshHarness source={source} token={0} />);
    rerender(<RefreshHarness source={source} token={0} />);
    expect(list).toHaveBeenCalledTimes(1);
  });

  it("shows a file that appeared only after the refresh", async () => {
    const source = memorySource();
    const { rerender } = render(<RefreshHarness source={source} token={0} />);
    await screen.findByText("notes.txt");
    expect(screen.queryByText("uploaded.txt")).toBeNull();

    TREE["/home"] = [...TREE["/home"], entry("uploaded.txt", "file")];
    rerender(<RefreshHarness source={source} token={1} />);
    expect(await screen.findByText("uploaded.txt")).toBeDefined();
    TREE["/home"] = TREE["/home"].filter((e) => e.name !== "uploaded.txt");
  });

  it("reports hidden entries in the footer so an arrived dotfile is not invisible", async () => {
    TREE["/home"] = [...TREE["/home"], { ...entry(".env", "file"), hidden: true }];
    render(<Harness />);
    await screen.findByText("notes.txt");

    expect(screen.queryByText(".env")).toBeNull();
    const hint = screen.getByText("1 hidden");
    fireEvent.click(hint);
    expect(await screen.findByText(".env")).toBeDefined();

    TREE["/home"] = TREE["/home"].filter((e) => e.name !== ".env");
  });

  it("asks before deleting and only removes on confirmation", async () => {
    const source = memorySource();
    const remove = vi.fn().mockResolvedValue(undefined);
    source.remove = remove;
    render(<Harness source={source} />);
    await screen.findByText("notes.txt");

    fireEvent.click(screen.getByText("notes.txt"));
    fireEvent.keyDown(document.querySelector(".file-table-wrap")!, { key: "Delete" });

    expect(await screen.findByText(/Permanently delete/)).toBeDefined();
    expect(remove).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText("Cancel"));
    expect(remove).not.toHaveBeenCalled();
  });

  it("deletes every selected entry once confirmed", async () => {
    const source = memorySource();
    const remove = vi.fn().mockResolvedValue(undefined);
    source.remove = remove;
    render(<Harness source={source} />);
    await screen.findByText("notes.txt");

    fireEvent.click(screen.getByText("projects"));
    fireEvent.click(screen.getByText("notes.txt"), { shiftKey: true });
    fireEvent.keyDown(document.querySelector(".file-table-wrap")!, { key: "Delete" });

    fireEvent.click(await screen.findByText("Delete 3 items"));
    await waitFor(() => expect(remove).toHaveBeenCalledTimes(3));
  });

  it("does not offer delete when the source cannot remove", async () => {
    const source = memorySource();
    render(<Harness source={source} />);
    await screen.findByText("notes.txt");

    fireEvent.click(screen.getByText("notes.txt"));
    fireEvent.keyDown(document.querySelector(".file-table-wrap")!, { key: "Delete" });
    expect(screen.queryByText(/Permanently delete/)).toBeNull();
  });

  it("selects a range with shift-click", async () => {
    render(<Harness />);
    await screen.findByText("notes.txt");
    fireEvent.click(screen.getByText("projects"));
    fireEvent.click(screen.getByText("notes.txt"), { shiftKey: true });
    expect(document.querySelectorAll(".file-row.selected").length).toBe(3);
  });
});
