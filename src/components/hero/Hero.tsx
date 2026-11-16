import { ChevronsUpDown, Download, Plus } from "lucide-react";
import { AnimatePresence, m, useMotionValue, useSpring, useTransform } from "motion/react";
import { useRef, useState, type PointerEvent } from "react";
import type { HomeSnapshot, Instance, WorldSummary } from "../../data/types";
import { greeting, instanceLabel, timeAgo } from "../../lib/format";
import { easeOutExpo, fadeUp, spring, stagger } from "../../lib/motion";
import { Sparkle } from "../brand/Sparkle";
import { InstanceAvatar } from "../instances/InstanceAvatar";
import type { LaunchView } from "../launch/describe";
import type { WorldLaunch } from "../WorldsSection";
import { playTime, worldMode } from "../worlds/WorldArt";
import { Button } from "../ui/Button";
import { MenuItem, Popover } from "../ui/Popover";
import { Loading, Skeleton } from "../ui/Skeleton";
import { SyncStatus } from "../ui/SyncStatus";
import { Embers } from "./Embers";
import { HeroScrims } from "./HeroScrims";
import { PlayButton } from "./PlayButton";
import { WorldPicker } from "./WorldPicker";

interface HeroProps {
  art: string;
  /** Null while the library is loading. */
  home: HomeSnapshot | null;
  /** The world Play launches; null when there are no worlds yet. */
  world: WorldSummary | null;
  /** The selected world's launch (its instance's, while that's busy with something else). */
  worldLaunch: WorldLaunch | null;
  /** With no worlds yet, Play starts this instance instead (the most recent one by default). */
  instance: Instance | null;
  instanceView: LaunchView;
  onSelectInstance: (instanceId: string) => void;
  onPlayInstance: () => void;
  onStopInstance: () => void;
  now: number;
  onSelectWorld: (worldId: string) => void;
  onPlay: () => void;
  onStop: () => void;
  onImport: () => void;
  onNewWorld: () => void;
  /** The dashboard is showing (the startup intro has handed over). */
  revealed: boolean;
  /**
   * The startup intro's floating art lands on this card. "flying": it's still landing, so this card
   * keeps still with its own art hidden; "landed": the art swaps in and the text appears.
   * null: no hand-off, the card makes its own entrance.
   */
  intro: HeroIntro;
}

export type HeroIntro = "flying" | "landed" | null;

/** Art scale at rest. The intro's floating art lands at exactly this, so keep them in step. */
const ART_SCALE = 1.06;

export function Hero(props: HeroProps) {
  const { art, home, world, revealed, intro } = props;
  const handoff = intro !== null;
  const visible = intro === "landed" || (intro === null && revealed);

  // Subtle parallax: the art drifts against the pointer, smoothed by a soft spring.
  const pointerX = useMotionValue(0);
  const pointerY = useMotionValue(0);
  const smoothX = useSpring(pointerX, { stiffness: 50, damping: 18 });
  const smoothY = useSpring(pointerY, { stiffness: 50, damping: 18 });
  const artX = useTransform(smoothX, (value) => value * -18);
  const artY = useTransform(smoothY, (value) => value * -12);

  const handlePointerMove = (event: PointerEvent<HTMLElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    pointerX.set((event.clientX - rect.left) / rect.width - 0.5);
    pointerY.set((event.clientY - rect.top) / rect.height - 0.5);
  };
  const resetPointer = () => {
    pointerX.set(0);
    pointerY.set(0);
  };

  return (
    // data-hero: the startup intro measures this card to land its floating art on it.
    <m.section
      data-hero
      aria-labelledby="hero-title"
      onPointerMove={handlePointerMove}
      onPointerLeave={resetPointer}
      initial={handoff ? false : { opacity: 0, y: 18 }}
      animate={handoff || revealed ? { opacity: 1, y: 0 } : { opacity: 0, y: 18 }}
      transition={handoff ? { duration: 0 } : spring}
      className="relative isolate flex min-h-[360px] flex-col justify-end overflow-hidden rounded-[26px] bg-ink-850 p-9"
    >
      <m.img
        src={art}
        alt=""
        draggable={false}
        decoding="async"
        fetchPriority="high"
        style={{ x: artX, y: artY }}
        initial={{ opacity: 0, scale: handoff ? ART_SCALE : 1.14 }}
        animate={visible ? { opacity: 1, scale: ART_SCALE } : { opacity: 0, scale: handoff ? ART_SCALE : 1.14 }}
        transition={handoff ? { duration: 0 } : { duration: 1.8, ease: easeOutExpo }}
        className="absolute inset-0 -z-20 size-full object-cover object-[62%_42%]"
      />
      <div aria-hidden="true" className="absolute inset-0 -z-10">
        <HeroScrims />
      </div>
      <Embers />

      <m.div variants={stagger} initial="hidden" animate={visible ? "show" : "hidden"} className="relative flex flex-col gap-6">
        {home === null ? (
          <HeroLoading />
        ) : world ? (
          <WorldHero {...props} home={home} world={world} />
        ) : props.instance ? (
          <InstanceHero {...props} home={home} instance={props.instance} />
        ) : (
          <WelcomeHero {...props} home={home} />
        )}
      </m.div>
    </m.section>
  );
}

function Greeting({ home }: { home: HomeSnapshot }) {
  return (
    <>
      <m.p variants={fadeUp} className="font-serif text-[34px] leading-none text-cream-200">
        {home.account ? `${greeting()},` : "Welcome to"}
      </m.p>
      <m.h1
        variants={fadeUp}
        id="hero-title"
        className="mt-1.5 flex items-start gap-2.5 font-display text-[clamp(44px,4.6vw,66px)] font-extrabold leading-none tracking-[-0.04em]"
      >
        {home.account ? home.account.name : "Wisp"}
        <Sparkle twinkle className="mt-1 size-[22px] text-ember" />
      </m.h1>
    </>
  );
}

/** First run: nothing to play yet, so the hero points at the two ways to get a world. */
function WelcomeHero({ home, onImport, onNewWorld }: HeroProps & { home: HomeSnapshot }) {
  return (
    <>
      {/* Kept narrow so the text never covers the ghost and shrine in the art. */}
      <div className="relative max-w-[360px]">
        <Greeting home={home} />
        <m.p variants={fadeUp} className="mt-4 text-[15px] leading-relaxed text-cream-300">
          Bring your worlds over from another launcher or start a new one. Once cloud sync is on, they'll be waiting on
          every computer you play on.
        </m.p>
      </div>
      <m.div variants={fadeUp} className="relative flex flex-wrap items-center gap-3">
        <Button size="lg" icon={Download} onClick={onImport}>
          Import worlds
        </Button>
        <Button size="lg" variant="secondary" icon={Plus} onClick={onNewWorld}>
          New world
        </Button>
      </m.div>
    </>
  );
}

function WorldHero({ home, world, worldLaunch, now, onSelectWorld, onPlay, onStop }: HeroProps & { home: HomeSnapshot; world: WorldSummary }) {
  const noteSource = world.lastDeviceIsThis ? "on this computer" : `on ${world.lastDevice ?? "another computer"}`;
  const view = worldLaunch?.view ?? { kind: "idle" as const, label: "Play", detail: "", progress: null };
  const locked = view.kind === "busy" || view.kind === "running";
  const mode = worldMode(world);
  return (
    <>
      <div className="relative max-w-[340px]">
        <Greeting home={home} />
        <m.div variants={fadeUp} className="mt-4 min-h-[62px]">
          <AnimatePresence mode="wait" initial={false}>
            {world.note ? (
              <m.div
                key={world.id}
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                transition={{ duration: 0.28, ease: easeOutExpo }}
              >
                <p className="text-sm font-semibold text-cream-400">
                  Your note from {timeAgo(world.noteAt ?? world.lastPlayed, now)}, {noteSource}
                </p>
                <p className="mt-0.5 line-clamp-3 font-serif text-[22px] italic leading-tight text-cream-200">“{world.note}”</p>
              </m.div>
            ) : (
              <m.p
                key={world.id}
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                transition={{ duration: 0.28, ease: easeOutExpo }}
                className="text-[15px] leading-relaxed text-cream-300"
              >
                {view.kind === "running"
                  ? `${world.name} is open in Minecraft. Have fun!`
                  : `Jump back into ${world.name}${mode ? ` (${mode.toLowerCase()})` : ""}. You last played ${timeAgo(world.lastPlayed, now)}${world.playSeconds ? `, ${playTime(world.playSeconds)} in all` : ""}.`}
              </m.p>
            )}
          </AnimatePresence>
        </m.div>
      </div>
      <m.div variants={fadeUp} className="relative flex flex-wrap items-center gap-x-4 gap-y-3">
        <WorldPicker worlds={home.worlds} selected={world} onSelect={onSelectWorld} disabled={locked} now={now} />
        <PlayButton view={view} name={worldLaunch?.blocked ? world.instanceName : world.name} onPlay={onPlay} onStop={onStop} />
        <SyncStatus world={world} now={now} size="md" />
      </m.div>
    </>
  );
}

/** No worlds yet, but there's an instance: Play goes straight into it. */
function InstanceHero({ home, instance, instanceView, onSelectInstance, onPlayInstance, onStopInstance, now }: HeroProps & { home: HomeSnapshot; instance: Instance }) {
  const status =
    instanceView.kind === "running"
      ? `${instance.name} is running. Have fun!`
      : instance.lastPlayed !== null
        ? `Jump back into ${instance.name}. You last played ${timeAgo(instance.lastPlayed, now)}.`
        : `${instance.name} is ready for its first game. The first launch downloads Minecraft.`;
  return (
    <>
      <div className="relative max-w-[360px]">
        <Greeting home={home} />
        <m.p variants={fadeUp} className="mt-4 text-[15px] leading-relaxed text-cream-300">
          {status}
        </m.p>
      </div>
      <m.div variants={fadeUp} className="relative flex flex-wrap items-center gap-x-4 gap-y-3">
        <InstancePicker
          instances={home.instances}
          selected={instance}
          onSelect={onSelectInstance}
          locked={instanceView.kind === "busy" || instanceView.kind === "running"}
        />
        <PlayButton view={instanceView} name={instance.name} onPlay={onPlayInstance} onStop={onStopInstance} />
      </m.div>
    </>
  );
}

/** Shows which instance Play starts; a menu to switch only appears when there's a choice. */
function InstancePicker({ instances, selected, onSelect, locked }: { instances: Instance[]; selected: Instance; onSelect: (id: string) => void; locked: boolean }) {
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const choosable = instances.length > 1 && !locked;
  const body = (
    <>
      <InstanceAvatar instance={selected} className="size-11 text-lg" />
      <span className="flex min-w-0 flex-col text-left">
        <span className="max-w-[180px] truncate text-[15px] font-bold">{selected.name}</span>
        <span className="text-xs text-cream-400">{instanceLabel(selected)}</span>
      </span>
    </>
  );

  if (!choosable) {
    return <div className="flex h-[60px] items-center gap-3 rounded-full border border-cream/15 bg-ink-900/55 pl-2 pr-5">{body}</div>;
  }
  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Instance: ${selected.name}. Change instance`}
        onClick={() => setOpen((isOpen) => !isOpen)}
        className="flex h-[60px] items-center gap-3 rounded-full border border-cream/20 bg-ink-900/55 pl-2 pr-4 transition-colors hover:border-cream/40"
      >
        {body}
        <ChevronsUpDown size={16} className="ml-1 text-cream-400" aria-hidden="true" />
      </button>
      <Popover open={open} anchorRef={buttonRef} placement="top-start" onClose={() => setOpen(false)} label="Choose an instance" className="w-[280px]">
        <div role="menu" aria-label="Instances" className="flex max-h-72 flex-col overflow-y-auto">
          {instances.map((instance) => (
            <MenuItem
              key={instance.id}
              icon={<InstanceAvatar instance={instance} className="size-6 rounded-md text-[11px]" />}
              label={instance.name}
              hint={instanceLabel(instance)}
              checked={instance.id === selected.id}
              onSelect={() => {
                onSelect(instance.id);
                setOpen(false);
              }}
            />
          ))}
        </div>
      </Popover>
    </>
  );
}

function HeroLoading() {
  return (
    <Loading label="Loading your library" className="relative flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <Skeleton className="h-8 w-44 rounded-full" />
        <Skeleton className="h-14 w-64 rounded-2xl" />
        <Skeleton className="mt-2 h-4 w-72 rounded-full" />
      </div>
      <div className="flex gap-3">
        <Skeleton className="h-14 w-44 rounded-full" />
        <Skeleton className="h-14 w-36 rounded-full" />
      </div>
    </Loading>
  );
}
