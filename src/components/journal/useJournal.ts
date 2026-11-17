import { useCallback, useEffect, useState } from "react";
import { getJournal } from "../../data/api";
import type { JournalEntry, JournalQuery } from "../../data/types";
import { useApp } from "../../state/AppContext";

const PAGE = 80;

/** Journal entries matching a query, newest first, reloaded whenever the journal changes. */
// The first page of each query last read, shown at once when you come back (from the Screenshots
// tab, say) while it's read again.
const firstPages = new Map<string, { entries: JournalEntry[]; more: boolean }>();

export function useJournal(query: Omit<JournalQuery, "before" | "limit">) {
  const { journalVersion } = useApp();
  const key = JSON.stringify(query);
  const [entries, setEntries] = useState<JournalEntry[] | null>(() => firstPages.get(key)?.entries ?? null);
  const [failed, setFailed] = useState(false);
  const [more, setMore] = useState(() => firstPages.get(key)?.more ?? false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    setFailed(false);
    getJournal({ ...(JSON.parse(key) as JournalQuery), limit: PAGE }).then(
      (found) => {
        if (!active) return;
        firstPages.set(key, { entries: found, more: found.length === PAGE });
        setEntries(found);
        setMore(found.length === PAGE);
      },
      () => active && setFailed(true),
    );
    return () => {
      active = false;
    };
  }, [key, journalVersion, attempt]);

  const loadMore = useCallback(() => {
    const oldest = entries?.at(-1);
    if (!oldest || loadingMore) return;
    setLoadingMore(true);
    getJournal({ ...(JSON.parse(key) as JournalQuery), before: oldest.at, limit: PAGE })
      .then(
        (found) => {
          setEntries((current) => [...(current ?? []), ...found.filter((entry) => !current?.some((known) => known.id === entry.id))]);
          setMore(found.length === PAGE);
        },
        () => setFailed(true),
      )
      .finally(() => setLoadingMore(false));
  }, [entries, key, loadingMore]);

  const retry = useCallback(() => setAttempt((count) => count + 1), []);
  return { entries, failed, more, loadingMore, loadMore, retry };
}
