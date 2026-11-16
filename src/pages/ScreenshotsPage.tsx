import { Globe } from "lucide-react";
import { m } from "motion/react";
import { useEffect, useMemo, useRef, useState } from "react";
import frostpeak from "../assets/art/world-frostpeak.jpg";
import sakura from "../assets/art/world-sakura.jpg";
import { Sparkle } from "../components/brand/Sparkle";
import { InstanceAvatar } from "../components/instances/InstanceAvatar";
import { Lightbox } from "../components/screenshots/Lightbox";
import { EmptyState } from "../components/ui/EmptyState";
import { ErrorState } from "../components/ui/ErrorState";
import { Page } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { PageTabs } from "../components/ui/PageTabs";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { getScreenshots, thumbUrl } from "../data/api";
import type { Screenshot } from "../data/types";
import { cn } from "../lib/cn";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

const GRID = "grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-3";
/** Screenshots render in steps as you scroll, so big libraries stay quick. */
const STEP = 90;

function dayKey(time: number) {
  return new Date(time).setHours(0, 0, 0, 0);
}

function dayHeading(day: number, now: number) {
  const today = dayKey(now);
  if (day === today) return "Today";
  if (day === today - 86_400_000) return "Yesterday";
  const date = new Date(day);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return date.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric", year: sameYear ? undefined : "numeric" });
}

export function ScreenshotsPage() {
  const { settings, updateSettings, home, now, journalVersion, openWorld } = useApp();
  const syncing = settings?.sync.screenshots ?? false;
  const [shots, setShots] = useState<Screenshot[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    getScreenshots().then(
      (found) => {
        if (!active) return;
        setShots(found);
        setFailed(false);
      },
      () => active && setFailed(true),
    );
    return () => {
      active = false;
    };
  }, [journalVersion, attempt, home?.instances.length]);

  const [instanceId, setInstanceId] = useState<string | null>(null);
  const [world, setWorld] = useState<string | null>(null);
  const filtered = useMemo(
    () => (shots ?? []).filter((shot) => (!instanceId || shot.instanceId === instanceId) && (!world || `${shot.instanceId}/${shot.worldFolder}` === world)),
    [shots, instanceId, world],
  );
  const instanceCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const shot of shots ?? []) counts.set(shot.instanceId, (counts.get(shot.instanceId) ?? 0) + 1);
    return counts;
  }, [shots]);
  const worlds = useMemo(() => {
    const found = new Map<string, { label: string; count: number }>();
    for (const shot of shots ?? []) {
      if (!shot.world || !shot.worldFolder || (instanceId && shot.instanceId !== instanceId)) continue;
      const key = `${shot.instanceId}/${shot.worldFolder}`;
      found.set(key, { label: shot.world, count: (found.get(key)?.count ?? 0) + 1 });
    }
    return [...found.entries()];
  }, [shots, instanceId]);

  // Render in steps as the end comes into view.
  const [limit, setLimit] = useState(STEP);
  useEffect(() => setLimit(STEP), [instanceId, world]);
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = sentinel.current;
    if (!element) return;
    const observer = new IntersectionObserver((entries) => entries.some((entry) => entry.isIntersecting) && setLimit((current) => current + STEP), { rootMargin: "600px" });
    observer.observe(element);
    return () => observer.disconnect();
  }, [filtered.length, limit]);

  const visible = filtered.slice(0, limit);
  const groups = useMemo(() => {
    const byDay: { day: number; shots: { shot: Screenshot; index: number }[] }[] = [];
    visible.forEach((shot, index) => {
      const day = dayKey(shot.takenAt);
      const last = byDay.at(-1);
      if (last && last.day === day) last.shots.push({ shot, index });
      else byDay.push({ day, shots: [{ shot, index }] });
    });
    return byDay;
  }, [visible]);

  const [open, setOpen] = useState<number | null>(null);
  const instances = home?.instances.filter((instance) => instanceCounts.has(instance.id)) ?? [];

  return (
    <Page>
      <PageHeader
        title="Screenshots"
        subtitle="Every screenshot from every instance, filed by the world you took it in. Press F2 in game to take one."
        actions={settings && shots && shots.length > 0 ? <SyncSwitch syncing={syncing} onToggle={() => updateSettings((current) => ({ sync: { ...current.sync, screenshots: !current.sync.screenshots } }))} /> : undefined}
      />
      <PageTabs
        label="Journal"
        current="screenshots"
        tabs={[
          { id: "journal", label: "Timeline" },
          { id: "screenshots", label: "Screenshots", count: shots?.length },
        ]}
      />

      {failed ? (
        <ErrorState title="Couldn't read the screenshots" body="Something went wrong reading the instances' screenshot folders." onRetry={() => setAttempt((count) => count + 1)} />
      ) : shots === null ? (
        <Loading label="Loading screenshots" className={GRID}>
          {Array.from({ length: 8 }, (_, index) => (
            <Skeleton key={index} className="aspect-video w-full rounded-2xl" />
          ))}
        </Loading>
      ) : shots.length === 0 ? (
        <EmptyState
          art={<Polaroids />}
          title="No screenshots yet"
          body={
            <>
              Press <kbd className="rounded-md bg-cream/10 px-1.5 py-0.5 font-sans text-[13px] font-bold text-cream">F2</kbd> in game to take one.
              {syncing ? " With sync on, the ones from your other computers land here too." : " Turn on syncing to see the ones from your other computers here too."}
            </>
          }
          actions={settings && <SyncSwitch syncing={syncing} onToggle={() => updateSettings((current) => ({ sync: { ...current.sync, screenshots: !current.sync.screenshots } }))} />}
        />
      ) : (
        <m.div variants={fadeUp} className="flex flex-col gap-5">
          <div className="flex flex-wrap items-center gap-1.5">
            <Chip
              active={instanceId === null && world === null}
              onClick={() => {
                setInstanceId(null);
                setWorld(null);
              }}
            >
              All <span className="tabular-nums opacity-60">{shots.length}</span>
            </Chip>
            {instances.length > 1 &&
              instances.map((instance) => (
                <Chip
                  key={instance.id}
                  active={instanceId === instance.id && world === null}
                  onClick={() => {
                    setInstanceId(instance.id);
                    setWorld(null);
                  }}
                >
                  <InstanceAvatar instance={instance} className="size-5 rounded-md text-[10px]" />
                  {instance.name} <span className="tabular-nums opacity-60">{instanceCounts.get(instance.id)}</span>
                </Chip>
              ))}
            {worlds.map(([key, info]) => (
              <Chip key={key} active={world === key} onClick={() => setWorld(world === key ? null : key)}>
                <Globe size={13} aria-hidden="true" />
                {info.label} <span className="tabular-nums opacity-60">{info.count}</span>
              </Chip>
            ))}
          </div>

          {groups.map((group) => (
            <section key={group.day} aria-label={dayHeading(group.day, now)} className="flex flex-col gap-3">
              <h2 className="flex items-baseline gap-2 text-[13px] font-bold text-cream-300">
                {dayHeading(group.day, now)}
                <span className="text-xs font-semibold text-mute-2">{group.shots.length}</span>
              </h2>
              <ul className={GRID}>
                {group.shots.map(({ shot, index }) => (
                  <li key={shot.id} className="animate-card-in" style={{ animationDelay: `${Math.min(index % STEP, 16) * 18}ms` }}>
                    <button
                      type="button"
                      onClick={() => setOpen(index)}
                      aria-label={`Screenshot from ${new Date(shot.takenAt).toLocaleString()}${shot.world ? ` in ${shot.world}` : ""}`}
                      className="group relative block w-full overflow-hidden rounded-2xl bg-ink-800 ring-1 ring-cream/7 transition-[box-shadow,transform] duration-300 ease-out-expo hover:-translate-y-0.5 hover:ring-cream/25"
                    >
                      <img src={thumbUrl(shot.path, 480, shot.takenAt)} alt="" loading="lazy" decoding="async" className="aspect-video w-full object-cover transition-transform duration-700 ease-out-expo group-hover:scale-[1.04]" />
                      <span className="absolute inset-x-0 bottom-0 flex items-end justify-between gap-2 bg-linear-to-t from-ink-950/85 to-transparent px-3 pb-2 pt-8 text-left opacity-0 transition-opacity duration-200 group-hover:opacity-100 group-focus-visible:opacity-100">
                        <span className="min-w-0">
                          <span className="block truncate text-xs font-bold text-cream">{shot.world ?? shot.instanceName}</span>
                          <span className="block text-[11px] text-cream-400">{new Date(shot.takenAt).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}</span>
                        </span>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
          {limit < filtered.length && <div ref={sentinel} className="h-10" aria-hidden="true" />}
        </m.div>
      )}

      <Lightbox
        shots={filtered}
        index={open}
        onIndex={setOpen}
        onDeleted={(gone) => setShots((current) => current?.filter((shot) => shot.id !== gone.id) ?? null)}
        onOpenWorld={openWorld}
      />
    </Page>
  );
}

function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={active}
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

function SyncSwitch({ syncing, onToggle }: { syncing: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={syncing}
      onClick={onToggle}
      className="flex h-11 items-center gap-3 rounded-full border border-cream/15 pl-4 pr-1.5 text-sm font-bold transition-colors hover:border-cream/35 hover:bg-cream/5"
    >
      Sync screenshots
      <span className={cn("relative h-7 w-12 rounded-full transition-colors duration-200", syncing ? "bg-cream" : "bg-cream/15")}>
        <span className={cn("absolute left-1 top-1 size-5 rounded-full transition-[translate,background-color] duration-200 ease-out-expo", syncing ? "translate-x-5 bg-ink-900" : "bg-cream")} />
      </span>
    </button>
  );
}

/** Decorative: two snapshots pinned at an angle. */
function Polaroids() {
  const frame = "absolute rounded-[10px] bg-cream p-1.5 pb-5 shadow-[0_18px_40px_-14px_rgba(0,0,0,0.8)] transition-transform duration-500 ease-out-expo";
  return (
    <div aria-hidden="true" className="relative mb-2 h-[120px] w-[200px]">
      <div className={cn(frame, "left-2 top-3 -rotate-8 group-hover:-translate-x-1.5 group-hover:-rotate-12")}>
        <img src={frostpeak} alt="" className="size-[92px] rounded-[6px] object-cover" />
      </div>
      <div className={cn(frame, "right-2 top-0 rotate-6 group-hover:translate-x-1.5 group-hover:rotate-10")}>
        <img src={sakura} alt="" className="size-[92px] rounded-[6px] object-cover" />
      </div>
      <Sparkle twinkle className="absolute -top-2 left-[92px] size-5 text-ember" />
    </div>
  );
}
