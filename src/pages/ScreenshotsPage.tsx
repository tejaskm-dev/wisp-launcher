import { ChevronLeft, Earth, Images } from "lucide-react";
import { m } from "motion/react";
import { useEffect, useMemo, useRef, useState } from "react";
import frostpeak from "../assets/art/world-frostpeak.jpg";
import sakura from "../assets/art/world-sakura.jpg";
import { Sparkle } from "../components/brand/Sparkle";
import { Lightbox } from "../components/screenshots/Lightbox";
import { Button } from "../components/ui/Button";
import { EmptyState } from "../components/ui/EmptyState";
import { ErrorState } from "../components/ui/ErrorState";
import { Page } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { PageTabs } from "../components/ui/PageTabs";
import { Segmented } from "../components/ui/Segmented";
import { Select, type SelectOption } from "../components/ui/Select";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { getScreenshots, thumbUrl } from "../data/api";
import type { Screenshot } from "../data/types";
import { cn } from "../lib/cn";
import { timeAgo } from "../lib/format";
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

// The last list read, shown at once when you come back (from the Timeline tab, say) while it's read again.
let lastShots: Screenshot[] | null = null;

export function ScreenshotsPage() {
  const { settings, updateSettings, home, now, journalVersion, openWorld } = useApp();
  const syncing = settings?.sync.screenshots ?? false;
  const [shots, setShots] = useState<Screenshot[] | null>(lastShots);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    getScreenshots().then(
      (found) => {
        if (!active) return;
        lastShots = found;
        setShots(found);
        setFailed(false);
      },
      () => active && setFailed(true),
    );
    return () => {
      active = false;
    };
  }, [journalVersion, attempt, home?.instances.length]);

  // Albums (one per world) or every shot, and the album open, kept for when you come back.
  const [view, setViewState] = useState<View>(remembered.view);
  const [albumKey, setAlbumKey] = useState<string | null>(remembered.album);
  const [instanceId, setInstanceId] = useState<string | null>(remembered.instanceId);
  useEffect(() => {
    remembered = { view, album: albumKey, instanceId };
  }, [view, albumKey, instanceId]);
  const setView = (next: View) => {
    setViewState(next);
    setAlbumKey(null);
  };

  const filtered = useMemo(() => (shots ?? []).filter((shot) => !instanceId || shot.instanceId === instanceId), [shots, instanceId]);
  const albums = useMemo(() => albumsOf(filtered), [filtered]);
  const album = albumKey ? (albums.find((candidate) => candidate.key === albumKey) ?? null) : null;
  const shown = album ? album.shots : filtered;

  const instanceCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const shot of shots ?? []) counts.set(shot.instanceId, (counts.get(shot.instanceId) ?? 0) + 1);
    return counts;
  }, [shots]);
  const instances = home?.instances.filter((instance) => instanceCounts.has(instance.id)) ?? [];
  const instanceOptions: SelectOption<string>[] = [
    { value: ALL, label: "Every instance", note: String(shots?.length ?? 0) },
    ...instances.map((instance) => ({ value: instance.id, label: instance.name, note: String(instanceCounts.get(instance.id) ?? 0) })),
  ];

  const [open, setOpen] = useState<number | null>(null);
  const worldCount = albums.filter((candidate) => candidate.worldFolder).length;

  return (
    <Page>
      <PageHeader
        title="Screenshots"
        subtitle="Every screenshot from every instance, in an album for each world you took them in. Press F2 in game to take one."
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
        <Loading label="Loading screenshots" className={ALBUM_GRID}>
          {Array.from({ length: 6 }, (_, index) => (
            <Skeleton key={index} className="aspect-[4/3] w-full rounded-[22px]" />
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
        <m.div variants={fadeUp} className="flex flex-col gap-6">
          <div className="flex flex-wrap items-center gap-3">
            <Segmented<View>
              label="Show"
              value={view}
              onChange={setView}
              options={[
                { value: "albums", label: "Albums" },
                { value: "all", label: "All shots" },
              ]}
            />
            <p className="text-sm text-mute">
              <span className="font-bold tabular-nums text-cream-300">{filtered.length}</span> {filtered.length === 1 ? "shot" : "shots"}
              {worldCount > 0 && (
                <>
                  {" "}from <span className="font-bold tabular-nums text-cream-300">{worldCount}</span> {worldCount === 1 ? "world" : "worlds"}
                </>
              )}
            </p>
            {instances.length > 1 && (
              <Select
                label="Instance"
                value={instanceId ?? ALL}
                options={instanceOptions}
                onChange={(next) => {
                  setInstanceId(next === ALL ? null : next);
                  setAlbumKey(null);
                }}
                className="ml-auto w-[220px]"
              />
            )}
          </div>

          {view === "albums" && !album ? (
            <ul className={ALBUM_GRID}>
              {albums.map((candidate, index) => (
                <li key={candidate.key} className="animate-card-in" style={{ animationDelay: `${Math.min(index, 12) * 30}ms` }}>
                  <AlbumCard album={candidate} now={now} onOpen={() => setAlbumKey(candidate.key)} />
                </li>
              ))}
            </ul>
          ) : (
            <>
              {album && (
                <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
                  <button
                    type="button"
                    onClick={() => setAlbumKey(null)}
                    className="flex h-10 items-center gap-1.5 rounded-full border border-cream/12 pl-2.5 pr-4 text-sm font-bold text-cream-300 transition-colors hover:border-cream/30 hover:text-cream"
                  >
                    <ChevronLeft size={16} aria-hidden="true" />
                    Albums
                  </button>
                  <div className="min-w-0">
                    <h2 className="truncate font-display text-2xl font-bold tracking-[-0.01em]">{album.title}</h2>
                    <p className="flex items-center gap-1.5 text-xs text-mute">
                      {album.instanceName} · {album.shots.length} {album.shots.length === 1 ? "shot" : "shots"} · {spanOf(album)}
                    </p>
                  </div>
                  {album.worldFolder && (
                    <Button variant="secondary" icon={Earth} onClick={() => openWorld(`${album.instanceId}/${album.worldFolder}`)} className="ml-auto">
                      Open world
                    </Button>
                  )}
                </div>
              )}
              <ShotGrid key={album?.key ?? `all-${instanceId ?? ""}`} shots={shown} now={now} showWorld={!album} onOpen={setOpen} />
            </>
          )}
        </m.div>
      )}

      <Lightbox
        shots={shown}
        index={open}
        onIndex={setOpen}
        onDeleted={(gone) => setShots((current) => current?.filter((shot) => shot.id !== gone.id) ?? null)}
        onOpenWorld={openWorld}
      />
    </Page>
  );
}

type View = "albums" | "all";
const ALL = "all";
const ALBUM_GRID = "grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-x-5 gap-y-7";

// What you were looking at, kept between visits.
let remembered: { view: View; album: string | null; instanceId: string | null } = { view: "albums", album: null, instanceId: null };

/** The shots of one world (or, per instance, those taken outside any world), newest first. */
interface Album {
  key: string;
  title: string;
  instanceId: string;
  instanceName: string;
  worldFolder: string | null;
  shots: Screenshot[];
}

function albumsOf(shots: Screenshot[]): Album[] {
  const albums = new Map<string, Album>();
  for (const shot of shots) {
    const key = `${shot.instanceId}/${shot.worldFolder ?? ""}`;
    let album = albums.get(key);
    if (!album) {
      album = { key, title: shot.world ?? "Outside a world", instanceId: shot.instanceId, instanceName: shot.instanceName, worldFolder: shot.worldFolder, shots: [] };
      albums.set(key, album);
    }
    album.shots.push(shot);
  }
  for (const album of albums.values()) album.shots.sort((a, b) => b.takenAt - a.takenAt);
  // The album added to most lately first; ones outside a world after the worlds.
  return [...albums.values()].sort((a, b) => Number(a.worldFolder === null) - Number(b.worldFolder === null) || b.shots[0].takenAt - a.shots[0].takenAt);
}

/** "March 3 to 9", "Today", or one day. */
function spanOf(album: Album) {
  const newest = album.shots[0].takenAt;
  const oldest = album.shots.at(-1)?.takenAt ?? newest;
  const format = (time: number) => new Date(time).toLocaleDateString(undefined, { month: "long", day: "numeric" });
  return dayKey(newest) === dayKey(oldest) ? format(newest) : `${format(oldest)} to ${format(newest)}`;
}

/** An album: its newest shot on top of a little pile of the ones before it. */
function AlbumCard({ album, now, onOpen }: { album: Album; now: number; onOpen: () => void }) {
  const [cover, second, third] = album.shots;
  const pile = "absolute inset-x-3 top-0 aspect-video rounded-2xl bg-ink-700 bg-cover bg-center ring-1 ring-cream/8 transition-transform duration-500 ease-out-expo";
  return (
    <button type="button" onClick={onOpen} aria-label={`${album.title}, ${album.instanceName}: ${album.shots.length} screenshots`} className="group relative block w-full pt-3 text-left">
      {third && <span aria-hidden="true" className={cn(pile, "rotate-[-4deg] opacity-60 group-hover:-translate-x-2 group-hover:rotate-[-7deg]")} style={{ backgroundImage: `url("${thumbUrl(third.path, 320, third.takenAt)}")` }} />}
      {second && <span aria-hidden="true" className={cn(pile, "rotate-[3deg] opacity-80 group-hover:translate-x-2 group-hover:rotate-[6deg]")} style={{ backgroundImage: `url("${thumbUrl(second.path, 320, second.takenAt)}")` }} />}
      <span className="wisp-card wag lit relative block overflow-visible [--tail:30px]">
        <span className="block overflow-hidden rounded-t-[22px]">
          <img src={thumbUrl(cover.path, 640, cover.takenAt)} alt="" loading="lazy" decoding="async" className="aspect-video w-full object-cover transition-transform duration-700 ease-out-expo group-hover:scale-[1.04]" />
        </span>
        <span className="flex items-center gap-3 px-4 pb-4 pt-3">
          <span className="min-w-0 flex-1">
            <span className="flex items-center gap-1.5 truncate font-display text-[17px] font-bold">
              {album.worldFolder ? <Earth size={14} className="shrink-0 text-mute" aria-hidden="true" /> : <Images size={14} className="shrink-0 text-mute" aria-hidden="true" />}
              <span className="truncate">{album.title}</span>
            </span>
            <span className="mt-0.5 block truncate text-xs text-mute">
              {album.instanceName} · {timeAgo(cover.takenAt, now)}
            </span>
          </span>
          <span className="grid h-8 min-w-8 place-items-center rounded-full bg-cream/8 px-2.5 text-[13px] font-bold tabular-nums text-cream-300">{album.shots.length}</span>
        </span>
      </span>
    </button>
  );
}

/** Shots by the day they were taken, rendered in steps as the end comes into view. */
function ShotGrid({ shots, now, showWorld, onOpen }: { shots: Screenshot[]; now: number; showWorld: boolean; onOpen: (index: number) => void }) {
  const [limit, setLimit] = useState(STEP);
  const sentinel = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = sentinel.current;
    if (!element) return;
    const observer = new IntersectionObserver((entries) => entries.some((entry) => entry.isIntersecting) && setLimit((current) => current + STEP), { rootMargin: "600px" });
    observer.observe(element);
    return () => observer.disconnect();
  }, [shots.length, limit]);

  const groups = useMemo(() => {
    const byDay: { day: number; shots: { shot: Screenshot; index: number }[] }[] = [];
    shots.slice(0, limit).forEach((shot, index) => {
      const day = dayKey(shot.takenAt);
      const last = byDay.at(-1);
      if (last && last.day === day) last.shots.push({ shot, index });
      else byDay.push({ day, shots: [{ shot, index }] });
    });
    return byDay;
  }, [shots, limit]);

  return (
    <div className="flex flex-col gap-6">
      {groups.map((group) => (
        <section key={group.day} aria-label={dayHeading(group.day, now)} className="flex flex-col gap-3">
          <h3 className="flex items-baseline gap-2 text-[13px] font-bold text-cream-300">
            {dayHeading(group.day, now)}
            <span className="text-xs font-semibold text-mute-2">{group.shots.length}</span>
          </h3>
          <ul className={GRID}>
            {group.shots.map(({ shot, index }) => (
              <li key={shot.id} className="animate-card-in" style={{ animationDelay: `${Math.min(index % STEP, 16) * 18}ms` }}>
                <button
                  type="button"
                  onClick={() => onOpen(index)}
                  aria-label={`Screenshot from ${new Date(shot.takenAt).toLocaleString()}${shot.world ? ` in ${shot.world}` : ""}`}
                  className="group relative block w-full overflow-hidden rounded-2xl bg-ink-800 ring-1 ring-cream/7 transition-[box-shadow,transform] duration-300 ease-out-expo hover:-translate-y-0.5 hover:ring-cream/25"
                >
                  <img src={thumbUrl(shot.path, 480, shot.takenAt)} alt="" loading="lazy" decoding="async" className="aspect-video w-full object-cover transition-transform duration-700 ease-out-expo group-hover:scale-[1.04]" />
                  <span className="absolute inset-x-0 bottom-0 flex items-end justify-between gap-2 bg-linear-to-t from-ink-950/85 to-transparent px-3 pb-2 pt-8 text-left opacity-0 transition-opacity duration-200 group-hover:opacity-100 group-focus-visible:opacity-100">
                    <span className="min-w-0">
                      {showWorld && <span className="block truncate text-xs font-bold text-cream">{shot.world ?? shot.instanceName}</span>}
                      <span className="block text-[11px] text-cream-400">{new Date(shot.takenAt).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}</span>
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      ))}
      {limit < shots.length && <div ref={sentinel} className="h-10" aria-hidden="true" />}
    </div>
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
