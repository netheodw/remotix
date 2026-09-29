import { useData } from "../state/DataContext";
import { useForwardStatuses } from "../state/ForwardStatusContext";
import { useTerminalTabs } from "../state/TerminalTabsContext";
import type { Section } from "../App";

export function DashboardScreen({ onNavigate }: { onNavigate: (s: Section) => void }) {
  const { profiles, forwards } = useData();
  const { tabs } = useTerminalTabs();
  const statuses = useForwardStatuses();

  const activeTerminals = tabs.filter((t) => t.status === "connected").length;
  const activeForwards = Object.values(statuses).filter((s) => s.status === "active").length;

  if (profiles.length === 0 && forwards.length === 0) {
    return (
      <div className="empty-state dashboard-empty">
        <h2>Welcome to Remotix</h2>
        <p>Get started by creating your first SSH profile, then add a port forward if you need one.</p>
        <div className="dashboard-empty-actions">
          <button className="primary" onClick={() => onNavigate("terminals")}>
            Create SSH Profile
          </button>
          <button onClick={() => onNavigate("forwards")}>Create Port Forward</button>
        </div>
      </div>
    );
  }

  return (
    <div className="dashboard">
      <h2>Dashboard</h2>
      <div className="dashboard-cards">
        <div className="dashboard-card">
          <span className="card-value">{profiles.length}</span>
          <span className="card-label">SSH Profiles</span>
        </div>
        <div className="dashboard-card">
          <span className="card-value">{forwards.length}</span>
          <span className="card-label">Saved Forwards</span>
        </div>
        <div className="dashboard-card">
          <span className="card-value">{activeTerminals}</span>
          <span className="card-label">Active Terminals</span>
        </div>
        <div className="dashboard-card">
          <span className="card-value">{activeForwards}</span>
          <span className="card-label">Active Forwards</span>
        </div>
      </div>
      <div className="dashboard-empty-actions">
        <button className="primary" onClick={() => onNavigate("terminals")}>
          Create SSH Profile
        </button>
        <button onClick={() => onNavigate("forwards")}>Create Port Forward</button>
      </div>
    </div>
  );
}
