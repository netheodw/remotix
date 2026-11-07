import { useData } from "../state/DataContext";
import { useForwardStatuses } from "../state/ForwardStatusContext";
import { useTerminalTabs } from "../state/TerminalTabsContext";
import { useVault } from "../state/VaultContext";
import type { Section } from "../App";

export function Sidebar({ section, onSelect }: { section: Section; onSelect: (s: Section) => void }) {
  const { profiles, forwards } = useData();
  const { tabs } = useTerminalTabs();
  const statuses = useForwardStatuses();
  const { lock } = useVault();

  const activeTerminals = tabs.filter((t) => t.status === "connected" || t.status === "connecting").length;
  const activeForwards = Object.values(statuses).filter((s) => s.status === "active" || s.status === "reconnecting").length;

  const item = (key: Section, label: string, badge?: number) => (
    <button className={`sidebar-item ${section === key ? "active" : ""}`} onClick={() => onSelect(key)}>
      <span>{label}</span>
      {badge !== undefined && badge > 0 && <span className="badge">{badge}</span>}
    </button>
  );

  return (
    <div className="sidebar">
      <div className="sidebar-brand">Remotix</div>
      <nav className="sidebar-nav">
        {item("dashboard", "Dashboard")}
        {item("terminals", "Terminals", profiles.length)}
        {item("files", "Files")}
        {item("forwards", "Port Forwards", forwards.length)}
        {item("active", "Active", activeTerminals + activeForwards)}
        {item("history", "History")}
        {item("settings", "Settings")}
      </nav>
      <div className="sidebar-footer">
        <button onClick={lock}>Lock Vault</button>
      </div>
    </div>
  );
}
