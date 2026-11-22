import { useCallback, useEffect, useRef, useState } from "react";
import { getHostedServer, getHostedServers, getServerConsole, onHostingChange } from "../../data/api";
import type { ConsoleLine, HostedView } from "../../data/types";

const CONSOLE_KEPT = 1500;

/** Every server hosted on this computer, kept current as they start, stop and fill up. */
export function useHostedServers(onFailed?: (serverId: string, message: string) => void) {
  const [servers, setServers] = useState<HostedView[] | null>(null);
  const failed = useRef(onFailed);
  failed.current = onFailed;
  const reload = useCallback(() => {
    getHostedServers().then(setServers, () => setServers([]));
  }, []);
  useEffect(() => {
    reload();
    return onHostingChange((change) => {
      switch (change.kind) {
        case "lines":
          return;
        case "progress":
          setServers((list) => list?.map((server) => (server.id === change.serverId ? { ...server, state: "preparing", progress: change.update } : server)) ?? list);
          return;
        case "players":
          setServers((list) => list?.map((server) => (server.id === change.serverId ? { ...server, players: change.players } : server)) ?? list);
          return;
        case "failed":
          failed.current?.(change.serverId, change.message);
          reload();
          return;
        default:
          reload();
      }
    });
  }, [reload]);
  return { servers, reload };
}

/** One hosted server, with its console as it comes. `onFixed` hears what Wisp fixed by itself. */
export function useHostedServer(id: string, onFailed?: (message: string) => void, onFixed?: (message: string) => void) {
  const [server, setServer] = useState<HostedView | null | undefined>(undefined);
  const [console, setConsole] = useState<ConsoleLine[]>([]);
  const failed = useRef(onFailed);
  failed.current = onFailed;
  const fixed = useRef(onFixed);
  fixed.current = onFixed;
  const reload = useCallback(() => {
    getHostedServer(id).then(setServer, () => setServer(null));
  }, [id]);
  useEffect(() => {
    reload();
    getServerConsole(id).then(setConsole, () => {});
    return onHostingChange((change) => {
      if (change.serverId !== id) return;
      switch (change.kind) {
        case "lines":
          setConsole((lines) => {
            const next = lines.concat(change.lines);
            return next.length > CONSOLE_KEPT ? next.slice(next.length - CONSOLE_KEPT) : next;
          });
          return;
        case "progress":
          setServer((current) => (current ? { ...current, state: "preparing", progress: change.update } : current));
          return;
        case "players":
          setServer((current) => (current ? { ...current, players: change.players } : current));
          return;
        case "failed":
          failed.current?.(change.message);
          reload();
          return;
        case "fixed":
          fixed.current?.(change.message);
          reload();
          return;
        default:
          reload();
      }
    });
  }, [id, reload]);
  // A fresh start begins with an empty console.
  const startedAt = server?.startedAt ?? null;
  useEffect(() => {
    if (startedAt === null) return;
    getServerConsole(id).then(setConsole, () => {});
  }, [id, startedAt]);
  return { server, setServer, console, reload };
}
