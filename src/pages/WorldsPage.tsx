import { Download, FolderInput, Play, Plus, Search, X } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect, useMemo, useState } from "react";
import { InstanceAvatar } from "../components/instances/InstanceAvatar";
import { WorldCard, WorldCollage } from "../components/WorldsSection";
import { worldLaunch } from "../components/worlds/launch";
import { Button } from "../components/ui/Button";
import { EmptyState } from "../components/ui/EmptyState";
import { Page } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { PageTabs } from "../components/ui/PageTabs";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { onFileDrop } from "../data/api";
import type { WorldSummary } from "../data/types";
import { cn } from "../lib/cn";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

const GRID = "grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4";
type Sort = "recent" | "name" | "time";
const SORTS: { value: Sort; label: string }[] = [
  { value: "recent", label: "Last played" },
  { value: "name", label: "Name" },
  { value: "time", label: "Most played" },
];

// Remembered while the app is open.
let remembered: { instanceId: string | null; sort: Sort } = { instanceId: null, sort: "recent" };

export function WorldsPage() {
  const { home, now, openNewInstance, launches, launchWorlds, play, stop, openWorld, importWorlds } = useApp();
  const worlds = home?.worlds ?? null;
  const instances = home?.instances ?? [];
  const [instanceId, setInstanceId] = useState<string | null>(remembered.instanceId);
  const [sort, setSort] = useState<Sort>(remembered.sort);
  const [query, setQuery] = useState("");
  useEffect(() => {
    remembered = { instanceId, sort };
  }, [instanceId, sort]);

  // Worlds dropped on the window import straight away.
  const [dragging, setDragging] = useState(false);
  useEffect(
    () =>
      onFileDrop((drop) => {
        if (drop.type === "over") setDragging(true);
        else if (drop.type === "leave") setDragging(false);
        else {
          setDragging(false);
          if (drop.paths.length > 0) importWorlds(drop.paths);
        }
      }),
    [importWorlds],
  );

  const counts = useMemo(() => {
    const byInstance = new Map<string, number>();
    for (const world of worlds ?? []) byInstance.set(world.instanceId, (byInstance.get(world.instanceId) ?? 0) + 1);
    return byInstance;
  }, [worlds]);
  const shown = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = (worlds ?? []).filter(
      (world) => (!instanceId || world.instanceId === instanceId) && (!needle || world.name.toLowerCase().includes(needle) || world.folder.toLowerCase().includes(needle)),
    );
    const sorted = [...filtered];
    if (sort === "name") sorted.sort((a, b) => a.name.localeCompare(b.name));
    else if (sort === "time") sorted.sort((a, b) => (b.playSeconds ?? 0) - (a.playSeconds ?? 0));
    else sorted.sort((a, b) => b.lastPlayed - a.lastPlayed);
    return sorted;
  }, [worlds, instanceId, query, sort]);
  const mostRecentId = worlds && worlds.length > 0 ? worlds.reduce((latest, world) => (world.lastPlayed > latest.lastPlayed ? world : latest)).id : null;
  const withWorlds = instances.filter((instance) => counts.has(instance.id));

  return (
    <Page>
      <PageHeader
        title="Worlds"
        subtitle="Every world in every instance. Press Play to go straight into one, or drop a world's .zip or folder anywhere here to bring it in."
        actions={
          worlds && worlds.length > 0 ? (
            <Button variant="secondary" icon={Download} onClick={() => importWorlds()}>
              Import
            </Button>
          ) : undefined
        }
      />
      <PageTabs
        label="Worlds and servers"
        current="worlds"
        tabs={[
          { id: "worlds", label: "Worlds", count: worlds?.length },
          { id: "servers", label: "Servers", count: home?.servers.length },
        ]}
      />

      {worlds === null ? (
        <Loading label="Loading worlds" className={GRID}>
          {[0, 1, 2, 3].map((index) => (
            <div key={index} className="overflow-hidden rounded-[20px] border border-cream/7 bg-ink-800">
              <Skeleton className="h-32 w-full" />
              <div className="flex flex-col gap-2 p-3.5">
                <Skeleton className="h-4 w-3/4 rounded-full" />
                <Skeleton className="h-3 w-1/2 rounded-full" />
              </div>
            </div>
          ))}
        </Loading>
      ) : worlds.length === 0 ? (
        <EmptyState
          art={<WorldCollage />}
          title="No worlds yet"
          body={
            instances.length > 0
              ? "Play an instance and make a world in Minecraft, or bring one in: a .zip from the web, or a world folder from another launcher (drop it right here)."
              : "Worlds live in instances. Make an instance, then play it to start a world, or import one you already have."
          }
          actions={
            instances.length > 0 ? (
              <>
                <Button icon={Download} onClick={() => importWorlds()}>
                  Import a world
                </Button>
                <Button variant="secondary" icon={Play} onClick={() => play(instances[0].id)}>
                  Play {instances[0].name}
                </Button>
              </>
            ) : (
              <Button icon={Plus} onClick={openNewInstance}>
                New instance
              </Button>
            )
          }
        />
      ) : (
        <m.div variants={fadeUp} className="flex flex-col gap-4">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
            <label className="flex h-10 w-[240px] items-center gap-2 rounded-full border border-cream/12 bg-ink-800 px-3.5 text-mute focus-within:border-cream/30">
              <Search size={15} aria-hidden="true" />
              <span className="sr-only">Find a world</span>
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Find a world…"
                spellCheck={false}
                className="min-w-0 flex-1 bg-transparent text-sm text-cream outline-none placeholder:text-mute"
              />
              {query && (
                <button type="button" aria-label="Clear" onClick={() => setQuery("")} className="rounded-full p-0.5 hover:text-cream">
                  <X size={14} aria-hidden="true" />
                </button>
              )}
            </label>
            {withWorlds.length > 1 && (
              <div role="radiogroup" aria-label="Instance" className="flex flex-wrap items-center gap-1.5">
                <Chip active={instanceId === null} onClick={() => setInstanceId(null)}>
                  All <span className="tabular-nums opacity-60">{worlds.length}</span>
                </Chip>
                {withWorlds.map((instance) => (
                  <Chip key={instance.id} active={instanceId === instance.id} onClick={() => setInstanceId(instance.id)}>
                    <InstanceAvatar instance={instance} className="size-5 rounded-md text-[10px]" />
                    {instance.name} <span className="tabular-nums opacity-60">{counts.get(instance.id)}</span>
                  </Chip>
                ))}
              </div>
            )}
            <div role="radiogroup" aria-label="Sort by" className="ml-auto flex items-center gap-1.5">
              <span className="mr-1 text-[13px] font-semibold text-mute">Sort by</span>
              {SORTS.map((option) => (
                <Chip key={option.value} active={sort === option.value} onClick={() => setSort(option.value)}>
                  {option.label}
                </Chip>
              ))}
            </div>
          </div>

          {shown.length === 0 ? (
            <p className="rounded-2xl border border-dashed border-cream/12 px-5 py-10 text-center text-sm text-mute">No worlds match “{query.trim()}”.</p>
          ) : (
            <ul className={GRID}>
              {shown.map((world, index) => (
                <li key={world.id} className="min-w-0 animate-card-in" style={{ animationDelay: `${Math.min(index, 12) * 25}ms` }}>
                  <WorldCard
                    world={world}
                    now={now}
                    lastPlayed={world.id === mostRecentId}
                    launch={worldLaunch(world, launches, launchWorlds, now)}
                    openLabel={`Open ${world.name}`}
                    onOpen={() => openWorld(world.id)}
                    onPlay={() => play(world.instanceId, world.folder)}
                    onStop={() => stop(world.instanceId)}
                    detail={instanceDetail(world)}
                  />
                </li>
              ))}
            </ul>
          )}
        </m.div>
      )}

      <AnimatePresence>
        {dragging && (
          <m.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="pointer-events-none fixed inset-0 z-[70] grid place-items-center bg-ink-950/70 p-10 backdrop-blur-sm"
          >
            <m.div
              initial={{ scale: 0.94, y: 10 }}
              animate={{ scale: 1, y: 0 }}
              className="flex flex-col items-center gap-3 rounded-[28px] border-2 border-dashed border-cream/40 bg-ink-800/90 px-14 py-12 text-center"
            >
              <FolderInput size={40} className="text-cream" aria-hidden="true" />
              <p className="font-display text-2xl font-bold">Drop to import</p>
              <p className="text-sm text-mute">World .zip files and world folders</p>
            </m.div>
          </m.div>
        )}
      </AnimatePresence>
    </Page>
  );
}

function instanceDetail(world: WorldSummary) {
  return world.instanceName;
}

function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      onClick={onClick}
      className={cn(
        "flex h-8 items-center gap-1.5 rounded-full border px-3 text-[13px] font-semibold transition-colors duration-150",
        active ? "border-cream/60 bg-cream text-ink-900" : "border-cream/10 bg-cream/[0.04] text-cream-300 hover:border-cream/25 hover:text-cream",
      )}
    >
      {children}
    </button>
  );
}
