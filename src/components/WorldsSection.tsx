import { ArrowRight, ChevronLeft, ChevronRight, Download, LoaderCircle, Play, Plus, Square } from "lucide-react";
import { m } from "motion/react";
import type { ReactNode } from "react";
import frostpeak from "../assets/art/world-frostpeak.jpg";
import sakura from "../assets/art/world-sakura.jpg";
import treehouse from "../assets/art/world-treehouse.jpg";
import type { WorldSummary } from "../data/types";
import { cn } from "../lib/cn";
import { timeAgo } from "../lib/format";
import { useScrollEdges } from "../lib/hooks";
import { fadeUp, rise, stagger } from "../lib/motion";
import { Ghost } from "./brand/Ghost";
import { Sparkle } from "./brand/Sparkle";
import type { LaunchView } from "./launch/describe";
import { Button } from "./ui/Button";
import { ProgressBar } from "./ui/ProgressBar";
import { SectionHeader } from "./ui/SectionHeader";
import { Loading, Skeleton } from "./ui/Skeleton";
import { SyncStatus } from "./ui/SyncStatus";
import { WorldArt, worldMode } from "./worlds/WorldArt";

interface WorldsSectionProps {
  /** Null while the library is loading. */
  worlds: WorldSummary[] | null;
  selectedId: string | null;
  now: number;
  /** How each world's launch looks (idle unless its game is starting or running it). */
  viewOf: (world: WorldSummary) => WorldLaunch;
  onSelect: (world: WorldSummary) => void;
  onPlay: (world: WorldSummary) => void;
  onStop: (world: WorldSummary) => void;
  onImport: () => void;
  onNewWorld: () => void;
  onSeeAll: () => void;
}

/** A world's Play button state: this world's own launch, or another game busy in its instance. */
export interface WorldLaunch {
  view: LaunchView;
  /** The instance is running something else, so this world can't start right now. */
  blocked: boolean;
}

const CARD_STEP = 236; // card width + gap

export function WorldsSection(props: WorldsSectionProps) {
  const { worlds } = props;
  const hasWorlds = worlds !== null && worlds.length > 0;

  return (
    <m.section variants={rise} aria-labelledby="worlds-title" className="flex min-w-0 flex-col gap-3.5">
      {hasWorlds ? (
        <WorldsRow {...props} worlds={worlds} />
      ) : (
        <>
          <SectionHeader id="worlds-title" title="Your worlds" icon={<Sparkle className="size-4 text-ember" />} />
          {worlds === null ? <WorldsLoading /> : <NoWorlds onImport={props.onImport} />}
        </>
      )}
    </m.section>
  );
}

function WorldsLoading() {
  return (
    <Loading label="Loading your worlds" className="flex gap-4 overflow-hidden pb-2 pt-1">
      {[0, 1, 2].map((index) => (
        <div key={index} className="w-[220px] shrink-0 overflow-hidden rounded-[20px] border border-cream/7 bg-ink-800">
          <Skeleton className="h-32 w-full" />
          <div className="flex flex-col gap-2 p-3.5">
            <Skeleton className="h-4 w-3/4 rounded-full" />
            <Skeleton className="h-3 w-1/2 rounded-full" />
            <Skeleton className="h-3 w-2/5 rounded-full" />
          </div>
        </div>
      ))}
    </Loading>
  );
}

function NoWorlds({ onImport }: { onImport: () => void }) {
  return (
    <m.div
      initial={{ opacity: 0, y: 10 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
      className="group flex flex-wrap items-center gap-x-8 gap-y-5 rounded-[22px] border border-dashed border-cream/15 bg-cream/2 p-6"
    >
      <WorldCollage />
      <div className="min-w-[220px] flex-1">
        <h3 className="font-display text-xl font-bold">No worlds yet</h3>
        <p className="mt-1 max-w-md text-sm leading-relaxed text-mute">
          Worlds you make in game, import or drop on this window show up here, along with where each one was last played.
        </p>
        <div className="mt-4">
          <Button variant="secondary" icon={Download} onClick={onImport}>
            Import a world
          </Button>
        </div>
      </div>
    </m.div>
  );
}

/** Decorative: a hint of what the worlds will look like once there are some. Fans out on hover of a `group`. */
export function WorldCollage() {
  return (
    <div aria-hidden="true" className="relative h-[124px] w-[232px] shrink-0">
      <img
        src={treehouse}
        alt=""
        className="absolute left-0 top-4 size-[100px] -rotate-6 rounded-2xl object-cover opacity-70 shadow-xl ring-1 ring-cream/10 transition-transform duration-500 ease-out-expo group-hover:-translate-x-1.5 group-hover:-rotate-9"
      />
      <img
        src={sakura}
        alt=""
        className="absolute left-[132px] top-5 size-[96px] rotate-6 rounded-2xl object-cover opacity-70 shadow-xl ring-1 ring-cream/10 transition-transform duration-500 ease-out-expo group-hover:translate-x-1.5 group-hover:rotate-9"
      />
      <img
        src={frostpeak}
        alt=""
        className="absolute left-[60px] top-0 z-10 size-[112px] rounded-2xl object-cover shadow-2xl ring-1 ring-cream/15 transition-transform duration-500 ease-out-expo group-hover:-translate-y-1"
      />
      <Ghost size={36} floating className="absolute -top-4 right-2 z-20 drop-shadow-[0_0_12px_rgba(244,238,228,0.3)]" />
    </div>
  );
}

function WorldsRow({ worlds, selectedId, now, viewOf, onSelect, onPlay, onStop, onNewWorld, onSeeAll }: WorldsSectionProps & { worlds: WorldSummary[] }) {
  const { ref, edges } = useScrollEdges<HTMLUListElement>();
  const mask =
    edges.start && edges.end
      ? "linear-gradient(to right, transparent, black 40px, black calc(100% - 56px), transparent)"
      : edges.end
        ? "linear-gradient(to right, black calc(100% - 56px), transparent)"
        : edges.start
          ? "linear-gradient(to right, transparent, black 40px)"
          : undefined;
  const scroll = (direction: 1 | -1) => ref.current?.scrollBy({ left: direction * CARD_STEP, behavior: "smooth" });
  const mostRecentId = worlds.reduce((latest, world) => (world.lastPlayed > latest.lastPlayed ? world : latest), worlds[0]).id;

  return (
    <>
      <SectionHeader
        id="worlds-title"
        title="Your worlds"
        count={worlds.length}
        icon={<Sparkle className="size-4 text-ember" />}
        action={
          <>
            <ScrollButton label="Scroll worlds left" disabled={!edges.start} onClick={() => scroll(-1)}>
              <ChevronLeft size={16} aria-hidden="true" />
            </ScrollButton>
            <ScrollButton label="Scroll worlds right" disabled={!edges.end} onClick={() => scroll(1)}>
              <ChevronRight size={16} aria-hidden="true" />
            </ScrollButton>
            <button type="button" onClick={onSeeAll} className="group ml-2 flex items-center gap-1.5 text-sm font-semibold text-cream-200 hover:text-cream">
              See all
              <ArrowRight size={16} className="transition-transform duration-300 group-hover:translate-x-0.5" aria-hidden="true" />
            </button>
          </>
        }
      />
      <m.ul
        ref={ref}
        initial="hidden"
        animate="show"
        variants={stagger}
        style={{ maskImage: mask, WebkitMaskImage: mask }}
        className="no-scrollbar -mx-1 flex snap-x snap-mandatory gap-4 overflow-x-auto px-1 pb-2 pt-1"
      >
        {worlds.map((world) => (
          <m.li key={world.id} variants={fadeUp} className="w-[220px] shrink-0 snap-start">
            <WorldCard
              world={world}
              now={now}
              selected={world.id === selectedId}
              lastPlayed={world.id === mostRecentId}
              launch={viewOf(world)}
              openLabel={`Select ${world.name}`}
              onOpen={() => onSelect(world)}
              onPlay={() => onPlay(world)}
              onStop={() => onStop(world)}
            />
          </m.li>
        ))}
        <m.li variants={fadeUp} className="w-[176px] shrink-0 snap-start">
          <button
            type="button"
            onClick={onNewWorld}
            className="group flex h-full min-h-[218px] w-full flex-col items-center justify-center gap-2.5 rounded-[20px] border-[1.5px] border-dashed border-cream/20 bg-cream/2 text-[15px] font-semibold text-cream-200 transition-colors hover:border-cream/40 hover:bg-cream/5"
          >
            <span className="grid size-12 place-items-center rounded-full bg-cream/8 transition-transform duration-300 ease-out-expo group-hover:rotate-90 group-hover:scale-110">
              <Plus size={22} aria-hidden="true" />
            </span>
            New world
            <span className="px-3 text-center text-xs font-medium text-mute-2">or import one</span>
          </button>
        </m.li>
      </m.ul>
    </>
  );
}

interface WorldCardProps {
  world: WorldSummary;
  now: number;
  selected?: boolean;
  lastPlayed: boolean;
  launch: WorldLaunch;
  /** What the card itself does when clicked, for screen readers. */
  openLabel: string;
  onOpen: () => void;
  onPlay: () => void;
  onStop: () => void;
  /** Extra line under the name (the Worlds page shows the instance). */
  detail?: ReactNode;
}

export function WorldCard({ world, now, selected = false, lastPlayed, launch, openLabel, onOpen, onPlay, onStop, detail }: WorldCardProps) {
  const { view, blocked } = launch;
  const busy = view.kind === "busy";
  const running = view.kind === "running";
  const mode = worldMode(world);

  return (
    <m.article
      whileHover={{ y: -4 }}
      transition={{ type: "spring", stiffness: 320, damping: 24 }}
      className={cn(
        "group relative flex h-full flex-col overflow-hidden rounded-[20px] border bg-ink-800 transition-colors duration-300",
        selected ? "border-cream/45" : running ? "border-moss/40" : "border-cream/7 hover:border-cream/18",
      )}
    >
      {/* The whole card opens (or selects) the world; the play button sits above it. */}
      <button type="button" aria-pressed={selected || undefined} aria-label={openLabel} onClick={onOpen} className="absolute inset-0 z-0 rounded-[20px]" />
      <div className="pointer-events-none relative h-32">
        <WorldArt world={world} width={480} className="absolute inset-0" zoomOnHover />
        <div className="absolute inset-0 bg-linear-to-t from-ink-800/80 via-transparent to-transparent" />
        {running ? (
          <span className="absolute left-2.5 top-2.5 flex items-center gap-1.5 rounded-full bg-moss px-2.5 py-1 text-xs font-bold text-ink-900">
            <span className="size-1.5 rounded-full bg-ink-900" aria-hidden="true" />
            Playing
          </span>
        ) : (
          lastPlayed && <span className="absolute left-2.5 top-2.5 rounded-full bg-cream px-2.5 py-1 text-xs font-bold text-ink-900">Last played</span>
        )}
        {mode && <span className="absolute bottom-2 right-2.5 rounded-full bg-ink-900/70 px-2 py-0.5 text-[11px] font-bold text-cream-300 backdrop-blur-sm">{mode}</span>}
      </div>
      <div className="pointer-events-none relative flex flex-1 items-center gap-2.5 p-3.5 pr-3">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <h3 className="truncate text-[15px] font-bold">{world.name}</h3>
          <p className="truncate text-xs text-mute">
            {detail ?? world.loader} · {timeAgo(world.lastPlayed, now)}
          </p>
          {busy ? <p className="truncate text-xs font-semibold text-cream-300">{view.label}</p> : <SyncStatus world={world} now={now} />}
          {world.sync.kind === "uploading" && <ProgressBar value={world.sync.progress} tone="amber" label={`Uploading ${world.name}`} className="mt-0.5 h-1" />}
        </div>
        <m.button
          type="button"
          aria-label={running ? `Stop ${world.name}` : busy ? `${world.name}: ${view.label}` : blocked ? `${world.instanceName} is already running` : `Play ${world.name}`}
          title={blocked ? `${world.instanceName} is running another world` : undefined}
          onClick={running ? onStop : busy || blocked ? undefined : onPlay}
          aria-disabled={busy || blocked}
          whileHover={busy || blocked ? undefined : { scale: 1.1 }}
          whileTap={busy || blocked ? undefined : { scale: 0.92 }}
          className={cn(
            "pointer-events-auto relative z-10 grid size-10 shrink-0 place-items-center rounded-full transition-colors",
            running
              ? "bg-moss/15 text-moss-soft hover:bg-moss/25"
              : selected
                ? "bg-cream text-ink-900"
                : "border border-cream/20 text-cream hover:bg-cream hover:text-ink-900",
            blocked && "opacity-40",
          )}
        >
          {busy ? (
            <LoaderCircle size={16} className="animate-spin" aria-hidden="true" />
          ) : running ? (
            <Square size={11} fill="currentColor" strokeWidth={0} aria-hidden="true" />
          ) : (
            <Play size={14} fill="currentColor" strokeWidth={0} aria-hidden="true" />
          )}
        </m.button>
      </div>
    </m.article>
  );
}

function ScrollButton({ label, disabled, onClick, children }: { label: string; disabled: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      className="grid size-8 place-items-center rounded-full border border-cream/12 text-cream-200 transition-colors hover:bg-cream/8 disabled:opacity-35 disabled:hover:bg-transparent"
    >
      {children}
    </button>
  );
}
