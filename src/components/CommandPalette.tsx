import { useEffect, useMemo, useState } from "react";
import { forwardsStart } from "../api/commands";
import { useData } from "../state/DataContext";
import { useTerminalTabs } from "../state/TerminalTabsContext";
import type { Section } from "../App";

interface Command {
  id: string;
  label: string;
  hint: string;
  run: () => void;
}

export function CommandPalette({ onClose, onNavigate }: { onClose: () => void; onNavigate: (s: Section) => void }) {
  const { profiles, forwards } = useData();
  const { openTab } = useTerminalTabs();
  const [query, setQuery] = useState("");

  const commands = useMemo<Command[]>(() => {
    const list: Command[] = [
      { id: "nav-dashboard", label: "Go to Dashboard", hint: "navigation", run: () => onNavigate("dashboard") },
      { id: "nav-terminals", label: "Go to Terminals", hint: "navigation", run: () => onNavigate("terminals") },
      { id: "nav-forwards", label: "Go to Port Forwards", hint: "navigation", run: () => onNavigate("forwards") },
      { id: "nav-active", label: "Go to Active", hint: "navigation", run: () => onNavigate("active") },
      { id: "nav-settings", label: "Go to Settings", hint: "navigation", run: () => onNavigate("settings") },
    ];
    profiles.forEach((p) =>
      list.push({
        id: `connect-${p.id}`,
        label: `Connect to ${p.name}`,
        hint: `${p.username}@${p.hostname}`,
        run: () => {
          openTab(p.id, p.name, `${p.username}@${p.hostname}`);
          onNavigate("active");
        },
      }),
    );
    forwards.forEach((f) =>
      list.push({
        id: `start-${f.id}`,
        label: `Start ${f.name}`,
        hint: "port forward",
        run: () => {
          if (f.ssh_profile_id) forwardsStart(f.id);
          onNavigate("active");
        },
      }),
    );
    return list;
  }, [profiles, forwards, onNavigate, openTab]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands.slice(0, 8);
    return commands.filter((c) => c.label.toLowerCase().includes(q) || c.hint.toLowerCase().includes(q)).slice(0, 12);
  }, [commands, query]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  return (
    <div className="modal-overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="command-palette">
        <input
          autoFocus
          placeholder="Search profiles, forwards, or actions..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && filtered[0]) {
              filtered[0].run();
              onClose();
            }
          }}
        />
        <ul>
          {filtered.map((c) => (
            <li
              key={c.id}
              onClick={() => {
                c.run();
                onClose();
              }}
            >
              <span>{c.label}</span>
              <span className="command-hint">{c.hint}</span>
            </li>
          ))}
          {filtered.length === 0 && <li className="command-empty">No matches</li>}
        </ul>
      </div>
    </div>
  );
}
