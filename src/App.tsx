import { useEffect, useState } from "react";
import { CommandPalette } from "./components/CommandPalette";
import { Sidebar } from "./components/Sidebar";
import { ActiveScreen } from "./screens/ActiveScreen";
import { DashboardScreen } from "./screens/DashboardScreen";
import { FilesScreen } from "./screens/FilesScreen";
import { ForwardsScreen } from "./screens/ForwardsScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { TerminalsScreen } from "./screens/TerminalsScreen";
import { VaultSetupScreen } from "./screens/VaultSetupScreen";
import { VaultUnlockScreen } from "./screens/VaultUnlockScreen";
import { DataProvider } from "./state/DataContext";
import { FilesProvider } from "./state/FilesContext";
import { ForwardStatusProvider } from "./state/ForwardStatusContext";
import { TerminalTabsProvider, useTerminalTabs } from "./state/TerminalTabsContext";
import { useVault, VaultProvider } from "./state/VaultContext";

export type Section = "dashboard" | "terminals" | "files" | "forwards" | "active" | "settings";

function AppShell() {
  const [section, setSection] = useState<Section>("dashboard");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [createSignal, setCreateSignal] = useState(0);
  const { activeTabId, closeTab } = useTerminalTabs();

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      if (e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen(true);
      } else if (e.key === ",") {
        e.preventDefault();
        setSection("settings");
      } else if (e.key.toLowerCase() === "n") {
        if (section === "terminals" || section === "forwards") {
          e.preventDefault();
          setCreateSignal((n) => n + 1);
        }
      } else if (e.key.toLowerCase() === "w") {
        if (section === "active" && activeTabId) {
          e.preventDefault();
          closeTab(activeTabId);
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [section, activeTabId, closeTab]);

  return (
    <div className="app-shell">
      <Sidebar section={section} onSelect={setSection} />
      <main className="app-main">
        {section === "dashboard" && <DashboardScreen onNavigate={setSection} />}
        {section === "terminals" && <TerminalsScreen onNavigate={setSection} createSignal={createSignal} />}
        {section === "files" && <FilesScreen />}
        {section === "forwards" && <ForwardsScreen createSignal={createSignal} />}
        {section === "active" && <ActiveScreen />}
        {section === "settings" && <SettingsScreen />}
      </main>
      {paletteOpen && <CommandPalette onClose={() => setPaletteOpen(false)} onNavigate={setSection} />}
    </div>
  );
}

function Gate() {
  const { status, loading, error, refresh } = useVault();

  if (loading) {
    return <div className="auth-screen" />;
  }
  if (error || !status) {
    return (
      <div className="auth-screen">
        <div className="auth-card">
          <h1>Could not reach the local vault</h1>
          <p className="hint">{error ?? "The application backend did not respond."}</p>
          <button className="primary full-width" onClick={refresh}>
            Retry
          </button>
        </div>
      </div>
    );
  }
  if (!status.initialized) {
    return <VaultSetupScreen />;
  }
  if (!status.unlocked) {
    return <VaultUnlockScreen />;
  }
  return (
    <DataProvider>
      <ForwardStatusProvider>
        <TerminalTabsProvider>
          <FilesProvider>
            <AppShell />
          </FilesProvider>
        </TerminalTabsProvider>
      </ForwardStatusProvider>
    </DataProvider>
  );
}

export default function App() {
  return (
    <VaultProvider>
      <Gate />
    </VaultProvider>
  );
}
