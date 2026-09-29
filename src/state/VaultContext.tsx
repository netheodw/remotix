import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { errorMessage } from "../api/errors";
import { vaultLock, vaultStatus } from "../api/commands";
import type { VaultStatus } from "../api/types";

interface VaultContextValue {
  status: VaultStatus | null;
  loading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  lock: () => Promise<void>;
}

const VaultContext = createContext<VaultContextValue | null>(null);

export function VaultProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<VaultStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = await vaultStatus();
      setStatus(s);
      setError(null);
    } catch (e) {
      setError(errorMessage(e));
    }
  }, []);

  useEffect(() => {
    refresh().finally(() => setLoading(false));
  }, [refresh]);

  const lock = useCallback(async () => {
    await vaultLock();
    await refresh();
  }, [refresh]);

  return <VaultContext.Provider value={{ status, loading, error, refresh, lock }}>{children}</VaultContext.Provider>;
}

export function useVault() {
  const ctx = useContext(VaultContext);
  if (!ctx) throw new Error("useVault must be used within VaultProvider");
  return ctx;
}
