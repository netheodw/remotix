/**
 * Reads the terminal's input line straight off the screen buffer.
 *
 * Mirroring keystrokes to reconstruct what the user typed breaks the moment
 * anything the app cannot model happens - tab completion, Ctrl+R, a mid-line
 * edit. Reading the rendered line instead is always accurate; the only thing
 * that has to be worked out is where the shell's prompt ends, which is
 * captured once per line (see `readPromptPrefix`) at the moment the user
 * starts typing, before any input has been echoed.
 */

export interface BufferLineLike {
  isWrapped: boolean;
  translateToString(trimRight?: boolean, startColumn?: number, endColumn?: number): string;
}

export interface BufferLike {
  baseY: number;
  cursorX: number;
  cursorY: number;
  length: number;
  getLine(y: number): BufferLineLike | undefined;
}

/** Absolute row bounds of the logical (possibly soft-wrapped) line the cursor sits on. */
function logicalLineRange(buf: BufferLike): { start: number; end: number } {
  const cursorRow = buf.baseY + buf.cursorY;
  let start = cursorRow;
  while (start > 0 && buf.getLine(start)?.isWrapped) start -= 1;
  let end = cursorRow;
  while (end + 1 < buf.length && buf.getLine(end + 1)?.isWrapped) end += 1;
  return { start, end };
}

/**
 * The whole logical line the cursor is on, trailing blanks removed. Taken to
 * the end of the line rather than to the cursor, so pressing Enter after
 * fixing a typo mid-line still yields the complete command.
 */
export function readLogicalLine(buf: BufferLike): string {
  const { start, end } = logicalLineRange(buf);
  let text = "";
  for (let y = start; y <= end; y += 1) {
    text += buf.getLine(y)?.translateToString(false) ?? "";
  }
  return text.replace(/\s+$/, "");
}

/**
 * Everything on the logical line before the cursor. Called on the first
 * keystroke after the shell has finished writing, so this is the prompt -
 * including its trailing space, which must be kept for the later match.
 */
export function readPromptPrefix(buf: BufferLike): string {
  const { start } = logicalLineRange(buf);
  const cursorRow = buf.baseY + buf.cursorY;
  let text = "";
  for (let y = start; y < cursorRow; y += 1) {
    text += buf.getLine(y)?.translateToString(false) ?? "";
  }
  text += buf.getLine(cursorRow)?.translateToString(false, 0, buf.cursorX) ?? "";
  return text;
}

/**
 * Removes the captured prompt from a line. Returns `null` when the line does
 * not start with the prompt the shell was last seen drawing - the prompt was
 * redrawn differently, so anything left would be a guess, and a wrong guess
 * would end up in the user's history forever.
 */
export function stripPrompt(line: string, prompt: string | null): string | null {
  if (prompt === null) return null;
  if (line.startsWith(prompt)) return line.slice(prompt.length);
  // `readLogicalLine` trims the blanks at the end of the row, so a prompt
  // that ends in a space leaves no trace on an empty line.
  if (line === prompt.replace(/\s+$/, "")) return "";
  return null;
}
