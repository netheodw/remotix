import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";

export interface TerminalTab {
  tabId: string;
  profileId: string;
  profileName: string;
  hostLabel: string;
  status: "connecting" | "verifying_host" | "authenticating" | "connected" | "disconnected" | "failed";
  statusMessage: string | null;
  backendId: string | null;
  /** Set when the terminal saves or drops this profile's sudo password; bumps the sudo button. */
  sudoSaved?: boolean;
}

interface TerminalTabsContextValue {
  tabs: TerminalTab[];
  activeTabId: string | null;
  setActiveTabId: (id: string | null) => void;
  openTab: (profileId: string, profileName: string, hostLabel: string) => string;
  updateTab: (tabId: string, patch: Partial<TerminalTab>) => void;
  closeTab: (tabId: string) => void;
}

const TerminalTabsContext = createContext<TerminalTabsContextValue | null>(null);

let tabCounter = 0;

export function TerminalTabsProvider({ children }: { children: ReactNode }) {
  const [tabs, setTabs] = useState<TerminalTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string | null>(null);
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;

  const openTab = useCallback((profileId: string, profileName: string, hostLabel: string) => {
    const tabId = `tab-${++tabCounter}-${Date.now()}`;
    setTabs((prev) => [
      ...prev,
      { tabId, profileId, profileName, hostLabel, status: "connecting", statusMessage: null, backendId: null },
    ]);
    setActiveTabId(tabId);
    return tabId;
  }, []);

  const updateTab = useCallback((tabId: string, patch: Partial<TerminalTab>) => {
    setTabs((prev) => prev.map((t) => (t.tabId === tabId ? { ...t, ...patch } : t)));
  }, []);

  const closeTab = useCallback(
    (tabId: string) => {
      setTabs((prev) => {
        const next = prev.filter((t) => t.tabId !== tabId);
        if (activeTabId === tabId) {
          setActiveTabId(next.length ? next[next.length - 1].tabId : null);
        }
        return next;
      });
    },
    [activeTabId],
  );

  return (
    <TerminalTabsContext.Provider value={{ tabs, activeTabId, setActiveTabId, openTab, updateTab, closeTab }}>
      {children}
    </TerminalTabsContext.Provider>
  );
}

export function useTerminalTabs() {
  const ctx = useContext(TerminalTabsContext);
  if (!ctx) throw new Error("useTerminalTabs must be used within TerminalTabsProvider");
  return ctx;
}
