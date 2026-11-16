import { ArrowDownUp, Globe2, Info, Play, Plus, RefreshCw, SearchX } from "lucide-react";
import { m } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { getPublicServers } from "../../data/api";
import type { PublicServer } from "../../data/types";
import { cn } from "../../lib/cn";
import { fadeUp } from "../../lib/motion";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { Select, type SelectOption } from "../ui/Select";
import { Loading, Skeleton } from "../ui/Skeleton";
import { AddServerDialog } from "./AddServerDialog";
import { sameServer } from "./address";
import { JoinServerDialog, type JoinTarget } from "./JoinServerDialog";
import { RowButton, ServerRow } from "./ServerRow";
import { useServerStatuses } from "./useServerStatuses";

type Sort = "players" | "name" | "ping";
const SORTS: SelectOption<Sort>[] = [
  { value: "players", label: "Most players" },
  { value: "ping", label: "Lowest ping" },
  { value: "name", label: "Name" },
];
const LANGUAGES: Record<string, string> = { en: "English", de: "German", es: "Spanish" };
const ALL = "all";

/** Discover's Servers tab: public servers with live status, filters on the right. */
export function ServerBrowser({ query, onClearQuery }: { query: string; onClearQuery: () => void }) {
  const { home } = useApp();
  const [servers, setServers] = useState<PublicServer[] | null>(null);
  const [tag, setTag] = useState(ALL);
  const [language, setLanguage] = useState(ALL);
  const [sort, setSort] = useState<Sort>("players");
  const [joining, setJoining] = useState<JoinTarget | null>(null);
  const [adding, setAdding] = useState<{ name: string; address: string } | null>(null);

  useEffect(() => {
    getPublicServers().then(setServers, () => setServers([]));
  }, []);
  const { statuses, loading, refresh } = useServerStatuses(servers?.map((server) => server.address) ?? []);

  const tags = useMemo(() => {
    const counts = new Map<string, number>();
    for (const server of servers ?? []) for (const name of server.tags) counts.set(name, (counts.get(name) ?? 0) + 1);
    return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).map(([name]) => name);
  }, [servers]);
  const languages = useMemo(() => [...new Set((servers ?? []).flatMap((server) => server.languages))], [servers]);

  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const list = (servers ?? []).filter(
      (server) =>
        (tag === ALL || server.tags.includes(tag)) &&
        (language === ALL || server.languages.includes(language)) &&
        (!needle || [server.name, server.address, server.description, ...server.tags].some((text) => text.toLowerCase().includes(needle))),
    );
    const players = (server: PublicServer) => statuses[server.address]?.players ?? -1;
    const ping = (server: PublicServer) => (statuses[server.address]?.online ? (statuses[server.address]?.pingMs ?? 9999) : 99_999);
    if (sort === "name") list.sort((a, b) => a.name.localeCompare(b.name));
    else if (sort === "ping") list.sort((a, b) => ping(a) - ping(b));
    else list.sort((a, b) => players(b) - players(a));
    return list;
  }, [servers, tag, language, query, sort, statuses]);

  const listedIn = (address: string) => home?.servers.find((saved) => sameServer(saved.address, address))?.instanceIds ?? [];
  const onlineCount = Object.values(statuses).filter((status) => status.online).length;

  return (
    <m.div variants={fadeUp} className="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_288px]">
      <section aria-label="Servers" aria-busy={loading} className="flex min-w-0 flex-col gap-4">
        <div className="flex min-h-10 flex-wrap items-center gap-x-3 gap-y-2">
          <div className="flex min-w-0 items-baseline gap-2" aria-live="polite">
            <h2 className="flex items-center gap-2 font-display text-[22px] font-bold tracking-[-0.01em]">
              <Globe2 size={18} aria-hidden="true" />
              Servers
            </h2>
            <p className="truncate text-sm text-mute">
              {servers === null ? (
                "Loading…"
              ) : (
                <>
                  <span className="font-bold tabular-nums text-cream-300">{shown.length}</span> {shown.length === 1 ? "server" : "servers"}
                  {onlineCount > 0 && <span className="text-mute-2"> · {onlineCount} answering</span>}
                </>
              )}
            </p>
          </div>
          <div className="ml-auto flex items-center gap-2.5">
            <span className="flex items-center gap-1.5 text-[13px] font-semibold text-mute">
              <ArrowDownUp size={14} aria-hidden="true" />
              Sort by
            </span>
            <Select label="Sort by" value={sort} options={SORTS} onChange={setSort} className="w-[164px]" />
            <button
              type="button"
              aria-label="Check again"
              title="Check again"
              disabled={loading}
              onClick={refresh}
              className="grid size-10 place-items-center rounded-full border border-cream/12 text-cream-300 transition-colors hover:border-cream/30 hover:text-cream disabled:opacity-50"
            >
              <RefreshCw size={15} className={cn(loading && "animate-spin")} aria-hidden="true" />
            </button>
          </div>
        </div>

        {servers === null ? (
          <Loading label="Loading servers" className="grid gap-3 xl:grid-cols-2">
            {[0, 1, 2, 3, 4, 5].map((index) => (
              <Skeleton key={index} className="h-[150px] rounded-[22px]" />
            ))}
          </Loading>
        ) : shown.length === 0 ? (
          <EmptyState
            art={<SearchX size={38} className="text-mute" aria-hidden="true" />}
            title="No servers match"
            body="Try another kind, or clear the search."
            actions={
              <Button
                variant="secondary"
                onClick={() => {
                  setTag(ALL);
                  setLanguage(ALL);
                  onClearQuery();
                }}
              >
                Show all servers
              </Button>
            }
          />
        ) : (
          <ul className="grid gap-3 xl:grid-cols-2">
            {shown.map((server) => {
              const status = statuses[server.address];
              const listed = listedIn(server.address);
              return (
                <li key={server.address} className="flex">
                  <ServerRow
                    className="w-full"
                    name={server.name}
                    address={server.address}
                    status={status}
                    description={server.description}
                    footer={
                      <>
                        {server.tags.slice(0, 3).map((name) => (
                          <span key={name} className="rounded-full bg-cream/6 px-2.5 py-1 text-[11px] font-bold text-cream-300">
                            {name}
                          </span>
                        ))}
                        {status?.version && <span className="max-w-[160px] truncate text-[11px] text-mute-2">{status.version}</span>}
                      </>
                    }
                    actions={
                      <>
                        {listed.length === 0 && (
                          <RowButton onClick={() => setAdding({ name: server.name, address: server.address })} label={`Add ${server.name} to a server list`}>
                            <Plus size={12} aria-hidden="true" />
                            Add
                          </RowButton>
                        )}
                        <RowButton primary onClick={() => setJoining({ name: server.name, address: server.address, instanceIds: listed })} label={`Join ${server.name}`}>
                          <Play size={11} fill="currentColor" strokeWidth={0} aria-hidden="true" />
                          Join
                        </RowButton>
                      </>
                    }
                  />
                </li>
              );
            })}
          </ul>
        )}
        <p className="flex items-start gap-2 text-xs leading-relaxed text-mute-2">
          <Info size={13} className="mt-px shrink-0" aria-hidden="true" />
          These servers are run by their own teams, with their own rules and stores. Wisp only lists them and asks each one how it's doing, the same way the game's server list does.
        </p>
      </section>

      <aside aria-label="Filter servers" className="flex flex-col gap-5 rounded-[22px] border border-cream/8 bg-ink-800/60 p-4 lg:sticky lg:top-24">
        <FilterGroup title="Kind">
          <Pill active={tag === ALL} onClick={() => setTag(ALL)}>
            All
          </Pill>
          {tags.map((name) => (
            <Pill key={name} active={tag === name} onClick={() => setTag(tag === name ? ALL : name)}>
              {name}
            </Pill>
          ))}
        </FilterGroup>
        {languages.length > 1 && (
          <FilterGroup title="Language">
            <Pill active={language === ALL} onClick={() => setLanguage(ALL)}>
              Any
            </Pill>
            {languages.map((code) => (
              <Pill key={code} active={language === code} onClick={() => setLanguage(language === code ? ALL : code)}>
                {LANGUAGES[code] ?? code}
              </Pill>
            ))}
          </FilterGroup>
        )}
      </aside>

      <JoinServerDialog target={joining} status={joining ? statuses[joining.address] : undefined} onClose={() => setJoining(null)} />
      <AddServerDialog open={adding !== null} initial={adding} onClose={() => setAdding(null)} />
    </m.div>
  );
}

function FilterGroup({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-2.5">
      <h3 className="text-[11px] font-bold uppercase tracking-[0.16em] text-mute-2">{title}</h3>
      <div role="radiogroup" aria-label={title} className="flex flex-wrap gap-1.5">
        {children}
      </div>
    </section>
  );
}

function Pill({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      onClick={onClick}
      className={cn(
        "rounded-full border px-3 py-1.5 text-[13px] font-bold transition-colors duration-150",
        active ? "border-cream/60 bg-cream text-ink-900" : "border-cream/12 text-cream-300 hover:border-cream/30 hover:text-cream",
      )}
    >
      {children}
    </button>
  );
}
