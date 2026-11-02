import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useEffect, useRef } from "react";
import {
  commandHistoryList,
  commandHistoryRecord,
  settingsGetAll,
  terminalClose,
  terminalInput,
  terminalOpen,
  terminalResize,
} from "../api/commands";
import { base64ToBytes, bytesToBase64 } from "../api/base64";
import type { TerminalEvent } from "../api/types";
import { useTerminalTabs, type TerminalTab } from "../state/TerminalTabsContext";
import { parseHostKeyError } from "../api/errors";
import { readLogicalLine, readPromptPrefix, stripPrompt } from "./terminalBuffer";
import { HistoryNavigator, replaceLineKeystrokes } from "./terminalHistory";

const HISTORY_LOAD_LIMIT = 1000;

/** Types `command` at the prompt; with `run`, also presses Enter. */
export type TerminalSender = (command: string, run: boolean) => void;

export function TerminalPane({
  tab,
  visible,
  historyEpoch = 0,
  onHostKeyDecision,
  registerSender,
}: {
  tab: TerminalTab;
  visible: boolean;
  /** Bumped by the workspace to make open panes drop a cleared history. */
  historyEpoch?: number;
  onHostKeyDecision: (decision: ReturnType<typeof parseHostKeyError>, retry: () => void) => void;
  /** Hands the workspace a way to type a toolbox command into this tab; `null` on unmount. */
  registerSender?: (tabId: string, sender: TerminalSender | null) => void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const backendIdRef = useRef<string | null>(null);
  const historyRef = useRef(new HistoryNavigator());
  const historyEnabledRef = useRef(true);
  // The prompt the shell drew for the line currently being typed, captured
  // on the first keystroke so the command can be split off it at Enter.
  // `null` means nothing has been typed on this line yet.
  const promptRef = useRef<string | null>(null);
  const { updateTab } = useTerminalTabs();

  useEffect(() => {
    let canceled = false;
    commandHistoryList(tab.profileId, HISTORY_LOAD_LIMIT)
      .then((entries) => {
        if (!canceled) historyRef.current.replaceAll(entries.map((e) => e.command));
      })
      .catch(() => {});
    return () => {
      canceled = true;
    };
  }, [tab.profileId, historyEpoch]);

  useEffect(() => {
    settingsGetAll()
      .then((s) => {
        historyEnabledRef.current = s.terminal_history_navigation !== "false";
      })
      .catch(() => {});
  }, [visible]);

  useEffect(() => {
    if (!containerRef.current) return;
    const term = new Terminal({
      fontFamily: "Cascadia Code, JetBrains Mono, Consolas, monospace",
      fontSize: 13,
      theme: { background: "#0f1115", foreground: "#e8ebf0" },
      cursorBlink: true,
      scrollback: 5000,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new SearchAddon());
    term.loadAddon(new WebLinksAddon());
    term.open(containerRef.current);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;

    const send = (data: string) => {
      if (backendIdRef.current) {
        terminalInput(backendIdRef.current, bytesToBase64(new TextEncoder().encode(data)));
      }
    };

    // Command history only makes sense while the remote shell owns the
    // screen. On the alternate buffer the keys belong to whatever full-screen
    // program is running (vim, less, top, htop) and what is on screen is not
    // a command line. Application cursor-key mode is deliberately *not* used
    // as a second signal: zsh's line editor turns it on at every prompt, so
    // gating on it would disable history for zsh users entirely.
    const shellOwnsTheLine = () => historyEnabledRef.current && term.buffer.active.type === "normal";

    /** Captures the prompt once per line, before any input is echoed back. */
    const markLineStarted = () => {
      if (promptRef.current === null) promptRef.current = readPromptPrefix(term.buffer.active);
    };

    /** What the user has typed on the current line, prompt removed. */
    const currentInput = () => stripPrompt(readLogicalLine(term.buffer.active), promptRef.current);

    const endOfLine = () => {
      historyRef.current.reset();
      promptRef.current = null;
    };

    term.onData((data) => {
      if (shellOwnsTheLine()) {
        if (data === "\r" || data === "\n") {
          const command = promptRef.current === null ? null : currentInput();
          const stored = command === null ? null : historyRef.current.record(command);
          if (stored) commandHistoryRecord(tab.profileId, stored).catch(() => {});
          endOfLine();
        } else if (data === "\x03") {
          // Ctrl+C abandons the line; the shell draws a fresh prompt.
          endOfLine();
        } else {
          markLineStarted();
        }
      }
      send(data);
    });

    /**
     * Replays a stored command onto the input line. Returns false when there
     * is nothing to replay, so the key falls through to the remote shell and
     * its own history instead.
     */
    const navigateHistory = (direction: "previous" | "next") => {
      if (!shellOwnsTheLine() || historyRef.current.entries.length === 0) return false;
      markLineStarted();
      const replacement =
        direction === "previous" ? historyRef.current.previous(currentInput() ?? "") : historyRef.current.next();
      if (replacement !== null) send(replaceLineKeystrokes(replacement));
      return true;
    };

    const pasteFromClipboard = () => {
      readText()
        .then((text) => {
          if (text) term.paste(text);
        })
        .catch(() => {});
    };

    // Ctrl+C copies the current selection instead of sending SIGINT (only
    // when there is a selection - with nothing selected it still interrupts
    // the remote process as usual). Ctrl+V always pastes. Up/Down walk the
    // per-profile command history Remotix keeps for this host.
    term.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;

      if ((event.key === "ArrowUp" || event.key === "ArrowDown") && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) {
        if (!navigateHistory(event.key === "ArrowUp" ? "previous" : "next")) return true;
        event.preventDefault();
        return false;
      }

      if (!(event.ctrlKey || event.metaKey) || event.altKey) return true;
      const key = event.key.toLowerCase();
      if (key === "c" && term.hasSelection()) {
        event.preventDefault();
        writeText(term.getSelection()).catch(() => {});
        return false;
      }
      if (key === "v") {
        event.preventDefault();
        pasteFromClipboard();
        return false;
      }
      return true;
    });

    // Toolbox snippets arrive as a paste, so a multi-line snippet stays one
    // edit under bracketed paste and the user can still change it before
    // Enter. Running records the snippet itself in history: the echo of the
    // paste has not come back yet, so the rendered line cannot be read.
    registerSender?.(tab.tabId, (command, run) => {
      if (!backendIdRef.current) return;
      term.paste(command);
      if (run) {
        if (shellOwnsTheLine()) {
          const stored = historyRef.current.record(command);
          if (stored) commandHistoryRecord(tab.profileId, stored).catch(() => {});
        }
        endOfLine();
        send("\r");
      }
      term.focus();
    });

    // Right-click pastes immediately, matching the convention most SSH/terminal
    // tools on Windows use (PuTTY, mintty) instead of a native context menu.
    const handleContextMenu = (e: MouseEvent) => {
      e.preventDefault();
      pasteFromClipboard();
    };
    containerRef.current.addEventListener("contextmenu", handleContextMenu);

    const connect = () => {
      term.reset();
      endOfLine();
      term.writeln("Connecting...");
      updateTab(tab.tabId, { status: "connecting", statusMessage: null });
      terminalOpen(tab.profileId, term.cols, term.rows, (event: TerminalEvent) => {
        if (event.type === "status") {
          updateTab(tab.tabId, { status: event.state as TerminalTab["status"], statusMessage: event.message });
          if (event.state === "connected") {
            term.writeln(`Connected.\r\n`);
          }
        } else if (event.type === "output") {
          term.write(base64ToBytes(event.data_b64));
        } else if (event.type === "closed") {
          updateTab(tab.tabId, { status: "disconnected", statusMessage: event.reason });
          term.writeln(`\r\n[disconnected: ${event.reason}]`);
        }
      })
        .then((backendId) => {
          backendIdRef.current = backendId;
        })
        .catch((e) => {
          const decision = parseHostKeyError(e);
          if (decision) {
            onHostKeyDecision(decision, connect);
            updateTab(tab.tabId, { status: "disconnected", statusMessage: "awaiting host key decision" });
          } else {
            updateTab(tab.tabId, { status: "failed", statusMessage: String(e) });
            term.writeln(`\r\n[connection failed: ${e}]`);
          }
        });
    };
    connect();

    const resizeObserver = new ResizeObserver(() => {
      // Hidden (another tab or another section): there is no size to fit to,
      // and resizing the remote PTY to it would garble the shell.
      if (!containerRef.current || containerRef.current.clientWidth === 0) return;
      fit.fit();
      if (backendIdRef.current) {
        terminalResize(backendIdRef.current, term.cols, term.rows);
      }
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      resizeObserver.disconnect();
      registerSender?.(tab.tabId, null);
      containerRef.current?.removeEventListener("contextmenu", handleContextMenu);
      if (backendIdRef.current) terminalClose(backendIdRef.current);
      term.dispose();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.tabId]);

  useEffect(() => {
    if (visible) {
      fitRef.current?.fit();
      termRef.current?.focus();
    }
  }, [visible]);

  return <div ref={containerRef} className="terminal-pane" style={{ display: visible ? "block" : "none" }} />;
}
