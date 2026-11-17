import { useCallback, useEffect, useState } from "react";
import { getServerStatuses } from "../../data/api";
import type { ServerStatus } from "../../data/types";

// The last status seen for each server, shown at once when a list comes back (switching from the
// Worlds tab, say) while it's asked again.
const lastKnown: Record<string, ServerStatus> = {};

/** Live status for a list of servers, asked for when the list changes; `refresh` asks again. */
export function useServerStatuses(addresses: string[]) {
  const key = addresses.join("\n");
  const [statuses, setStatuses] = useState<Record<string, ServerStatus>>(() => Object.fromEntries(addresses.filter((address) => lastKnown[address]).map((address) => [address, lastKnown[address]])));
  const [loading, setLoading] = useState(false);

  const load = useCallback(
    (force: boolean) => {
      const list = key ? key.split("\n") : [];
      if (list.length === 0) return;
      setLoading(true);
      getServerStatuses(list, force)
        .then((found) => {
          for (const status of found) lastKnown[status.address] = status;
          setStatuses((current) => ({ ...current, ...Object.fromEntries(found.map((status) => [status.address, status])) }));
        }, () => {})
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
