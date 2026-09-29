import { listen } from "@tauri-apps/api/event";
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import type { ForwardRuntimeStatus } from "../api/types";

interface ForwardStatusEvent {
  id: string;
  status: ForwardRuntimeStatus;
}

const ForwardStatusContext = createContext<Record<string, ForwardRuntimeStatus>>({});

export function ForwardStatusProvider({ children }: { children: ReactNode }) {
  const [statuses, setStatuses] = useState<Record<string, ForwardRuntimeStatus>>({});

  useEffect(() => {
    const unlisten = listen<ForwardStatusEvent>("forward-status", (event) => {
      setStatuses((prev) => ({ ...prev, [event.payload.id]: event.payload.status }));
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  return <ForwardStatusContext.Provider value={statuses}>{children}</ForwardStatusContext.Provider>;
}

export function useForwardStatuses() {
  return useContext(ForwardStatusContext);
}

export function useForwardStatus(id: string): ForwardRuntimeStatus {
  const statuses = useForwardStatuses();
  return statuses[id] ?? { status: "stopped" };
}
