import { useCallback, useEffect, useState } from "react";
import { getServerStatuses } from "../../data/api";
import type { ServerStatus } from "../../data/types";

/** Live status for a list of servers, asked for when the list changes; `refresh` asks again. */
export function useServerStatuses(addresses: string[]) {
  const key = addresses.join("\n");
  const [statuses, setStatuses] = useState<Record<string, ServerStatus>>({});
  const [loading, setLoading] = useState(false);

  const load = useCallback(
    (force: boolean) => {
      const list = key ? key.split("\n") : [];
      if (list.length === 0) return;
      setLoading(true);
      getServerStatuses(list, force)
        .then((found) => setStatuses((current) => ({ ...current, ...Object.fromEntries(found.map((status) => [status.address, status])) })), () => {})
        .finally(() => setLoading(false));
    },
    [key],
  );
  useEffect(() => load(false), [load]);

  return { statuses, loading, refresh: useCallback(() => load(true), [load]) };
}

/** "15,996 playing", "Offline", or nothing while it's being asked. */
export function playersLabel(status: ServerStatus | undefined): string | null {
  if (!status) return null;
  if (!status.online) return "Offline";
  if (status.players === null) return "Online";
  return `${status.players.toLocaleString()} playing`;
}
