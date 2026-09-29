import { describe, expect, it } from "vitest";
import { readLogicalLine, readPromptPrefix, stripPrompt, type BufferLike } from "./terminalBuffer";

const COLS = 20;

/**
 * Builds a buffer the way xterm exposes one: fixed-width rows padded with
 * spaces, with `isWrapped` marking the continuation rows of a soft-wrapped
 * logical line.
 */
function fakeBuffer(rows: { text: string; wrapped?: boolean }[], cursorRow: number, cursorX: number): BufferLike {
  const lines = rows.map((r) => ({
    isWrapped: r.wrapped ?? false,
    translateToString(trimRight = false, start = 0, end = COLS) {
      const padded = r.text.padEnd(COLS, " ").slice(start, end);
      return trimRight ? padded.replace(/\s+$/, "") : padded;
    },
  }));
  return {
    baseY: 0,
    cursorX,
    cursorY: cursorRow,
    length: lines.length,
    getLine: (y) => lines[y],
  };
}

describe("readLogicalLine", () => {
  it("reads the cursor's row without its padding", () => {
    const buf = fakeBuffer([{ text: "$ ls -la" }], 0, 8);
    expect(readLogicalLine(buf)).toBe("$ ls -la");
  });

  it("joins the continuation rows of a soft-wrapped command", () => {
    const buf = fakeBuffer(
      [
        { text: "$ echo aaaaaaaaaaaaa" },
        { text: "bbbb", wrapped: true },
      ],
      1,
      4,
    );
    expect(readLogicalLine(buf)).toBe("$ echo aaaaaaaaaaaaabbbb");
  });

  it("reads to the end of the line, not just to the cursor", () => {
    // Cursor parked mid-line after fixing a typo, then Enter pressed.
    const buf = fakeBuffer([{ text: "$ echo hello" }], 0, 8);
    expect(readLogicalLine(buf)).toBe("$ echo hello");
  });

  it("follows a wrap forward when the cursor is on the first row", () => {
    const buf = fakeBuffer(
      [
        { text: "$ aaaaaaaaaaaaaaaaaa" },
        { text: "bb", wrapped: true },
      ],
      0,
      5,
    );
    expect(readLogicalLine(buf)).toBe("$ aaaaaaaaaaaaaaaaaabb");
  });

  it("ignores earlier output above the prompt", () => {
    const buf = fakeBuffer([{ text: "some output" }, { text: "$ pwd" }], 1, 5);
    expect(readLogicalLine(buf)).toBe("$ pwd");
  });
});

describe("readPromptPrefix", () => {
  it("captures the prompt including its trailing space", () => {
    const buf = fakeBuffer([{ text: "user@host:~$ " }], 0, 13);
    expect(readPromptPrefix(buf)).toBe("user@host:~$ ");
  });

  it("captures only the last row of a multi-line prompt", () => {
    // The decorative first row is not wrapped, so it is a line of its own.
    const buf = fakeBuffer([{ text: "[user@host ~]" }, { text: "$ " }], 1, 2);
    expect(readPromptPrefix(buf)).toBe("$ ");
  });

  it("spans the wrap when a long prompt pushed the cursor to the next row", () => {
    const buf = fakeBuffer(
      [
        { text: "a-very-long-prompt::" },
        { text: "$ ", wrapped: true },
      ],
      1,
      2,
    );
    expect(readPromptPrefix(buf)).toBe("a-very-long-prompt::$ ");
  });

  it("is empty at the start of a bare line", () => {
    expect(readPromptPrefix(fakeBuffer([{ text: "" }], 0, 0))).toBe("");
  });
});

describe("stripPrompt", () => {
  it("returns what the user typed after the prompt", () => {
    expect(stripPrompt("user@host:~$ ls -la", "user@host:~$ ")).toBe("ls -la");
  });

  it("keeps a leading space the user typed deliberately", () => {
    expect(stripPrompt("$  secret", "$ ")).toBe(" secret");
  });

  it("returns an empty command when nothing was typed", () => {
    // The trailing blanks of the row are already gone by this point.
    expect(stripPrompt("user@host:~$", "user@host:~$ ")).toBe("");
    expect(stripPrompt("user@host:~$ ", "user@host:~$ ")).toBe("");
  });

  it("refuses to guess when the prompt was redrawn differently", () => {
    expect(stripPrompt("other@box:/tmp$ ls", "user@host:~$ ")).toBeNull();
  });

  it("refuses to guess when no prompt was captured", () => {
    expect(stripPrompt("user@host:~$ ls", null)).toBeNull();
  });
});
