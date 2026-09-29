import { describe, expect, it } from "vitest";
import { appendCommand, HistoryNavigator, normalizeCommand, replaceLineKeystrokes } from "./terminalHistory";

describe("normalizeCommand", () => {
  it("keeps an ordinary command and drops trailing whitespace", () => {
    expect(normalizeCommand("ls -la")).toBe("ls -la");
    expect(normalizeCommand("ls -la   \r")).toBe("ls -la");
  });

  it("drops blank lines", () => {
    expect(normalizeCommand("")).toBeNull();
    expect(normalizeCommand("   ")).toBeNull();
  });

  it("honours the leading-space opt-out so secrets stay unrecorded", () => {
    expect(normalizeCommand(" curl -H 'Authorization: Bearer abc'")).toBeNull();
    expect(normalizeCommand("\techo hi")).toBeNull();
  });

  it("drops multi-line and oversized input", () => {
    expect(normalizeCommand("a\nb")).toBeNull();
    expect(normalizeCommand("x".repeat(4097))).toBeNull();
    expect(normalizeCommand("x".repeat(4096))).toHaveLength(4096);
  });
});

describe("appendCommand", () => {
  it("collapses a repeat of the last command but keeps later repeats", () => {
    let history: string[] = [];
    history = appendCommand(history, "ls");
    history = appendCommand(history, "ls");
    history = appendCommand(history, "pwd");
    history = appendCommand(history, "ls");
    expect(history).toEqual(["ls", "pwd", "ls"]);
  });

  it("leaves the list untouched for lines that are not recorded", () => {
    const history = ["ls"];
    expect(appendCommand(history, "  ")).toEqual(["ls"]);
    expect(appendCommand(history, " hidden")).toEqual(["ls"]);
  });
});

describe("HistoryNavigator", () => {
  it("walks backwards from the newest entry", () => {
    const nav = new HistoryNavigator(["one", "two", "three"]);
    expect(nav.previous("")).toBe("three");
    expect(nav.previous("")).toBe("two");
    expect(nav.previous("")).toBe("one");
  });

  it("stops at the oldest entry instead of wrapping", () => {
    const nav = new HistoryNavigator(["one"]);
    expect(nav.previous("")).toBe("one");
    expect(nav.previous("")).toBeNull();
  });

  it("restores the half-typed draft when coming back down", () => {
    const nav = new HistoryNavigator(["one", "two"]);
    expect(nav.previous("git comm")).toBe("two");
    expect(nav.previous("git comm")).toBe("one");
    expect(nav.next()).toBe("two");
    expect(nav.next()).toBe("git comm");
    expect(nav.isNavigating).toBe(false);
  });

  it("does nothing on Down while already on the live line", () => {
    const nav = new HistoryNavigator(["one"]);
    expect(nav.next()).toBeNull();
  });

  it("returns null on Up with no history at all", () => {
    expect(new HistoryNavigator().previous("")).toBeNull();
  });

  it("captures the draft only on the first step back", () => {
    const nav = new HistoryNavigator(["one", "two"]);
    nav.previous("draft");
    nav.previous("two");
    expect(nav.next()).toBe("two");
    expect(nav.next()).toBe("draft");
  });

  it("appends an executed command and returns to the live line", () => {
    const nav = new HistoryNavigator(["one"]);
    nav.previous("");
    expect(nav.record("two")).toBe("two");
    expect(nav.isNavigating).toBe(false);
    expect(nav.entries).toEqual(["one", "two"]);
    expect(nav.previous("")).toBe("two");
  });

  it("reports nothing stored for lines that are not recorded", () => {
    const nav = new HistoryNavigator(["one"]);
    expect(nav.record(" hidden")).toBeNull();
    expect(nav.record("one")).toBeNull();
    expect(nav.entries).toEqual(["one"]);
  });

  it("drops the draft when the line is abandoned", () => {
    const nav = new HistoryNavigator(["one"]);
    nav.previous("draft");
    nav.reset();
    expect(nav.isNavigating).toBe(false);
    expect(nav.next()).toBeNull();
  });

  it("re-seeds from a reloaded list", () => {
    const nav = new HistoryNavigator(["one"]);
    nav.previous("");
    nav.replaceAll([]);
    expect(nav.entries).toEqual([]);
    expect(nav.previous("")).toBeNull();
  });
});

describe("replaceLineKeystrokes", () => {
  it("clears the remote line before typing the entry", () => {
    expect(replaceLineKeystrokes("echo hi")).toBe("\x05\x15echo hi");
  });

  it("clears the line even when replaying an empty draft", () => {
    expect(replaceLineKeystrokes("")).toBe("\x05\x15");
  });

  it("never submits on the user's behalf", () => {
    const keys = replaceLineKeystrokes("a\nb\r\nc");
    expect(keys).not.toMatch(/[\r\n]/);
    expect(keys).toBe("\x05\x15a b c");
  });
});
