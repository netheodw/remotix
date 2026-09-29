import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import {
  forwardsList,
  knownHostsList,
  portForwardGroupsList,
  profilesList,
  terminalGroupsList,
} from "../api/commands";
import type { Group, KnownHost, PortForward, SshProfile } from "../api/types";

interface DataContextValue {
  terminalGroups: Group[];
  portForwardGroups: Group[];
  profiles: SshProfile[];
  forwards: PortForward[];
  knownHosts: KnownHost[];
  loading: boolean;
  refreshTerminalGroups: () => Promise<void>;
  refreshPortForwardGroups: () => Promise<void>;
  refreshProfiles: () => Promise<void>;
  refreshForwards: () => Promise<void>;
  refreshKnownHosts: () => Promise<void>;
  refreshAll: () => Promise<void>;
}

const DataContext = createContext<DataContextValue | null>(null);

export function DataProvider({ children }: { children: ReactNode }) {
  const [terminalGroups, setTerminalGroups] = useState<Group[]>([]);
  const [portForwardGroups, setPortForwardGroups] = useState<Group[]>([]);
  const [profiles, setProfiles] = useState<SshProfile[]>([]);
  const [forwards, setForwards] = useState<PortForward[]>([]);
  const [knownHosts, setKnownHosts] = useState<KnownHost[]>([]);
  const [loading, setLoading] = useState(true);

  const refreshTerminalGroups = useCallback(async () => setTerminalGroups(await terminalGroupsList()), []);
  const refreshPortForwardGroups = useCallback(async () => setPortForwardGroups(await portForwardGroupsList()), []);
  const refreshProfiles = useCallback(async () => setProfiles(await profilesList()), []);
  const refreshForwards = useCallback(async () => setForwards(await forwardsList()), []);
  const refreshKnownHosts = useCallback(async () => setKnownHosts(await knownHostsList()), []);

  const refreshAll = useCallback(async () => {
    await Promise.all([
      refreshTerminalGroups(),
      refreshPortForwardGroups(),
      refreshProfiles(),
      refreshForwards(),
      refreshKnownHosts(),
    ]);
  }, [refreshTerminalGroups, refreshPortForwardGroups, refreshProfiles, refreshForwards, refreshKnownHosts]);

  useEffect(() => {
    refreshAll().finally(() => setLoading(false));
  }, [refreshAll]);

  return (
    <DataContext.Provider
      value={{
        terminalGroups,
        portForwardGroups,
        profiles,
        forwards,
        knownHosts,
        loading,
        refreshTerminalGroups,
        refreshPortForwardGroups,
        refreshProfiles,
        refreshForwards,
        refreshKnownHosts,
        refreshAll,
      }}
    >
      {children}
    </DataContext.Provider>
  );
}

export function useData() {
  const ctx = useContext(DataContext);
  if (!ctx) throw new Error("useData must be used within DataProvider");
  return ctx;
}
