/**
 * Per-profile command history for terminal tabs.
 *
 * The remote shell keeps its own history, but it starts empty on every new
 * connection. This layer is Remotix-owned: it remembers what was run against
 * a profile and lets Up/Down replay it, so history survives reconnects and
 * app restarts. The terminal drives it through `HistoryNavigator`; the
 * capture side is in `terminalBuffer.ts`.
 */

/**
 * Decides whether a captured terminal line should be remembered, and in what
 * form. Mirrors `normalize` in `src-tauri/src/repositories/command_history_repo.rs`
 * so the in-memory list the terminal navigates matches what the database keeps.
 *
 * A leading space or tab means "do not remember this one", matching the
 * `HISTCONTROL=ignorespace` convention shells already use.
 */
export function normalizeCommand(raw: string): string | null {
  if (raw.startsWith(" ") || raw.startsWith("\t")) return null;
  const command = raw.replace(/[ \t\r\n]+$/, "");
  if (!command || command.includes("\n") || command.length > 4096) return null;
  return command;
}

/** Appends to an oldest-first list, collapsing a repeat of the last entry. */
export function appendCommand(history: readonly string[], raw: string): string[] {
  const command = normalizeCommand(raw);
  if (command === null) return history as string[];
  if (history.length > 0 && history[history.length - 1] === command) return history as string[];
  return [...history, command];
}

/**
 * Cursor over an oldest-first history list, with the same shape as a shell's:
 * position `history.length` means "on the live line", and the text the user
 * had typed before navigating away is held as the draft so Down restores it.
 */
export class HistoryNavigator {
  private history: string[];
  private index: number;
  private draft = "";

  constructor(history: readonly string[] = []) {
    this.history = [...history];
    this.index = this.history.length;
  }

  get entries(): readonly string[] {
    return this.history;
  }

  /** True while Up has moved off the live line. */
  get isNavigating(): boolean {
    return this.index < this.history.length;
  }

  replaceAll(history: readonly string[]): void {
    this.history = [...history];
    this.reset();
  }

  /** Records an executed command and returns it if it was actually stored. */
  record(raw: string): string | null {
    const next = appendCommand(this.history, raw);
    const stored = next.length > this.history.length ? next[next.length - 1] : null;
    this.history = next;
    this.reset();
    return stored;
  }

  /** Back to the live line, discarding any saved draft. */
  reset(): void {
    this.index = this.history.length;
    this.draft = "";
  }

  /**
   * Moves to the previous entry. `currentLine` is what is on the input line
   * right now, kept as the draft on the first step back. Returns the text the
   * line should be replaced with, or `null` at the oldest entry.
   */
  previous(currentLine: string): string | null {
    if (this.index === 0) return null;
    if (!this.isNavigating) this.draft = currentLine;
    this.index -= 1;
    return this.history[this.index];
  }

  /**
   * Moves to the next entry, returning the draft once past the newest one and
   * `null` when already on the live line.
   */
  next(): string | null {
    if (!this.isNavigating) return null;
    this.index += 1;
    return this.index === this.history.length ? this.draft : this.history[this.index];
  }
}

/**
 * Keystrokes that replace the whole input line with `text`: jump to the end
 * (Ctrl+E), kill back to the start (Ctrl+U), then type the entry. This works
 * through the remote shell's own line editor rather than guessing at the
 * terminal's state, so it stays correct no matter what the user typed before.
 */
export function replaceLineKeystrokes(text: string): string {
  return `\x05\x15${text.replace(/[\r\n]+/g, " ")}`;
}
