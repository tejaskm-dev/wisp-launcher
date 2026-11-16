import { useCallback, useEffect, useState } from "react";
import { getJournal } from "../../data/api";
import type { JournalEntry, JournalQuery } from "../../data/types";
import { useApp } from "../../state/AppContext";

const PAGE = 80;

/** Journal entries matching a query, newest first, reloaded whenever the journal changes. */
export function useJournal(query: Omit<JournalQuery, "before" | "limit">) {
  const { journalVersion } = useApp();
  const [entries, setEntries] = useState<JournalEntry[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [more, setMore] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const key = JSON.stringify(query);

  useEffect(() => {
    let active = true;
    setFailed(false);
    getJournal({ ...(JSON.parse(key) as JournalQuery), limit: PAGE }).then(
      (found) => {
        if (!active) return;
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
