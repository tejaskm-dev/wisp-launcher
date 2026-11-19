import {
  ArrowLeft,
  Blocks,
  BookOpen,
  ChevronDown,
  Clock,
  FileText,
  FolderOpen,
  Gamepad2,
  Globe2,
  Hourglass,
  History,
  Images,
  LoaderCircle,
  Play,
  RotateCw,
  Server,
  ServerCog,
  Settings2,
  Square,
  type LucideIcon,
} from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ContentTab } from "../components/instance/ContentTab";
import { LogsTab } from "../components/instance/LogsTab";
import { JournalTab, ScreenshotsTab, ServersTab, WorldsTab } from "../components/instance/PlayTabs";
import { MemoryButton, SharingButton } from "../components/instance/QuickSettings";
import { SettingsTab } from "../components/instance/SettingsTab";
import { message } from "../components/instance/shared";
import { InstanceAvatar } from "../components/instances/InstanceAvatar";
import { InstanceLabel } from "../components/instances/VersionTag";
import { describeLaunch } from "../components/launch/describe";
import { Button } from "../components/ui/Button";
import { EmptyState } from "../components/ui/EmptyState";
import { Page } from "../components/ui/Page";
import { MenuItem, Popover } from "../components/ui/Popover";
import { Skeleton } from "../components/ui/Skeleton";
import { playTime } from "../components/worlds/WorldArt";
import { getInstanceStats, getLocalSettings, getScreenshots, openInstanceFolder, setLocalSettings, thumbUrl } from "../data/api";
import type { InstanceStats, LocalSettings, PageId, Screenshot } from "../data/types";
import { cn } from "../lib/cn";
import { timeAgo } from "../lib/format";
import { easeOutExpo, fadeUp } from "../lib/motion";
import { useApp, type InstanceTab } from "../state/AppContext";

const BACK_LABELS: Partial<Record<PageId, string>> = {
  home: "Home",
  instances: "Instances",
  worlds: "Worlds",
  journal: "Journal",
  screenshots: "Screenshots",
  servers: "Servers",
  discover: "Discover",
  world: "Back to the world",
};

const TABS: { id: InstanceTab; label: string; icon: LucideIcon }[] = [
  { id: "content", label: "Content", icon: Blocks },
  { id: "worlds", label: "Worlds", icon: Globe2 },
  { id: "screenshots", label: "Screenshots", icon: Images },
  { id: "journal", label: "Journal", icon: BookOpen },
  { id: "servers", label: "Servers", icon: Server },
  { id: "logs", label: "Logs", icon: FileText },
  { id: "settings", label: "Settings", icon: Settings2 },
];

/** A tab's content slides in from the side of the tab you came from. */
const panel = {
  enter: (direction: number) => ({ opacity: 0, x: direction * 28 }),
  show: { opacity: 1, x: 0, transition: { duration: 0.42, ease: easeOutExpo } },
  leave: (direction: number) => ({ opacity: 0, x: direction * -28, transition: { duration: 0.18, ease: easeOutExpo } }),
};

/** One instance, all of it: Play, what's in it, its worlds, screenshots, journal, servers, logs and settings. */
export function InstancePage({ instanceId, initialTab, backTo, onBack }: { instanceId: string; initialTab: InstanceTab; backTo: PageId; onBack: () => void }) {
  const { home, now, launches, play, stop, notify, journalVersion, hostServer } = useApp();
  const instance = home?.instances.find((candidate) => candidate.id === instanceId) ?? null;
  const [tab, setTabState] = useState<InstanceTab>(initialTab);
  // Which way the last tab change went, so the new content slides in from that side.
  const [direction, setDirection] = useState(0);
  const setTab = (next: InstanceTab) => {
    setDirection(Math.sign(TABS.findIndex((option) => option.id === next) - TABS.findIndex((option) => option.id === tab)));
    setTabState(next);
  };
  const [stats, setStats] = useState<InstanceStats | null>(null);
  const [local, setLocal] = useState<LocalSettings | null>(null);
  const [shots, setShots] = useState<Screenshot[] | null>(null);
  const [picking, setPicking] = useState(false);
  const pickRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let live = true;
    getInstanceStats(instanceId).then(
      (found) => live && setStats(found),
      () => live && setStats(null),
    );
    getScreenshots().then(
      (all) => live && setShots(all.filter((shot) => shot.instanceId === instanceId).sort((a, b) => b.takenAt - a.takenAt)),
      () => live && setShots([]),
    );
    return () => {
      live = false;
    };
  }, [instanceId, journalVersion]);
  useEffect(() => {
    let live = true;
    getLocalSettings(instanceId).then(
      (found) => live && setLocal(found),
      () => live && setLocal({}),
    );
    return () => {
      live = false;
    };
  }, [instanceId]);

  const saveLocal = (patch: Partial<LocalSettings>) => {
    const next = { ...(local ?? {}), ...patch };
    setLocal(next);
    setLocalSettings(instanceId, next).catch((failure: unknown) => notify({ tone: "info", title: "Couldn't save that", body: message(failure) }));
  };

  const worlds = useMemo(() => (home?.worlds ?? []).filter((world) => world.instanceId === instanceId).sort((a, b) => b.lastPlayed - a.lastPlayed), [home?.worlds, instanceId]);
  const banner = useMemo(() => {
    if (local?.banner === "none") return null;
    const chosen = local?.banner?.startsWith("screenshot:") ? shots?.find((shot) => shot.file === local.banner?.slice("screenshot:".length)) : null;
    return (chosen ?? shots?.[0])?.path ?? null;
  }, [local?.banner, shots]);

  if (home === null) {
    return (
      <Page>
        <Skeleton className="h-[280px] w-full rounded-[26px]" />
        <Skeleton className="h-20 w-full rounded-2xl" />
      </Page>
    );
  }
  if (!instance) {
    return (
      <Page>
        <BackButton label={BACK_LABELS[backTo] ?? "Back"} onBack={onBack} />
        <EmptyState title="That instance isn't here anymore" body="It may have been deleted. Its worlds' restore points are still in the Journal." actions={<Button onClick={onBack}>Go back</Button>} />
      </Page>
    );
  }

  const view = describeLaunch(launches[instance.id], now);
  const running = view.kind === "running";
  const starting = view.kind === "busy";
  const locked = running || starting;
  const failed = view.kind === "problem";

  return (
    <Page>
      <BackButton label={BACK_LABELS[backTo] ?? "Back"} onBack={onBack} />

      <m.section variants={fadeUp} className="relative isolate overflow-hidden rounded-[26px] border border-cream/8 bg-ink-850">
        {banner ? (
          <m.img
            key={banner}
            src={thumbUrl(banner, 1600)}
            alt=""
            initial={{ opacity: 0, scale: 1.08 }}
            animate={{ opacity: 0.75, scale: 1 }}
            transition={{ duration: 1.4, ease: easeOutExpo }}
            className="absolute inset-0 -z-10 size-full object-cover"
          />
        ) : (
          <div aria-hidden="true" className="absolute inset-0 -z-10 bg-[radial-gradient(120%_90%_at_85%_0%,rgba(244,238,228,0.09),transparent_55%),radial-gradient(80%_70%_at_0%_100%,rgba(229,72,77,0.08),transparent_60%)]" />
        )}
        <div aria-hidden="true" className="absolute inset-0 -z-10 bg-linear-to-t from-ink-900 via-ink-900/60 to-ink-900/5" />
        <div className="flex min-h-[270px] flex-col justify-end gap-6 p-8">
          <div className="flex items-end gap-5">
            <InstanceAvatar instance={instance} className="size-20 shrink-0 rounded-[22px] text-3xl shadow-[0_18px_40px_-12px_rgba(0,0,0,0.8)]" />
            <div className="min-w-0">
              <div className="flex flex-wrap gap-1.5">
                <span className="rounded-full bg-ink-900/60 px-2.5 py-1 text-xs font-bold text-cream-300 backdrop-blur">
                  <InstanceLabel instance={instance} />
                  {instance.loaderVersion ? <span className="text-mute"> · {instance.loaderVersion}</span> : null}
                </span>
                {instance.modpack && (
                  <span className="rounded-full bg-ink-900/60 px-2.5 py-1 text-xs font-bold text-cream-300 backdrop-blur">
                    {instance.modpack.title} {instance.modpack.versionNumber}
                  </span>
                )}
              </div>
              <h1 className="mt-2 truncate font-display text-[clamp(36px,4.2vw,58px)] font-extrabold leading-none tracking-[-0.04em]">{instance.name}</h1>
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2.5">
            <div className="flex items-center">
              <button
                type="button"
                onClick={running ? () => stop(instance.id) : starting ? undefined : () => play(instance.id)}
                aria-disabled={starting}
                className={cn(
                  "relative flex h-14 min-w-[150px] items-center justify-center gap-3 px-7 font-display text-[19px] font-extrabold transition-transform duration-200 ease-out-expo active:scale-[0.97]",
                  worlds.length > 0 && !running ? "rounded-l-full pr-5" : "rounded-full",
                  running ? "border border-moss/40 bg-moss/15 text-moss-soft" : "tail wag bg-cream text-ink-900 shadow-[0_12px_40px_-6px_rgba(244,238,228,0.35)] [--tail:30px] hover:scale-[1.02]",
                )}
              >
                {starting ? (
                  <LoaderCircle size={18} className="animate-spin" aria-hidden="true" />
                ) : running ? (
                  <Square size={14} fill="currentColor" strokeWidth={0} aria-hidden="true" />
                ) : failed ? (
                  <RotateCw size={17} aria-hidden="true" />
                ) : (
                  <Play size={18} fill="currentColor" strokeWidth={0} aria-hidden="true" />
                )}
                {running ? "Stop" : starting ? view.label : failed ? "Try again" : "Play"}
              </button>
              {worlds.length > 0 && !running && (
                <>
                  <button
                    ref={pickRef}
                    type="button"
                    disabled={starting}
                    onClick={() => setPicking((open) => !open)}
                    aria-label="Play straight into a world"
                    aria-haspopup="menu"
                    aria-expanded={picking}
                    className="ml-px grid h-14 w-12 place-items-center rounded-r-full bg-cream text-ink-900 transition-colors hover:bg-cream-200 disabled:opacity-50"
                  >
                    <ChevronDown size={18} className={cn("transition-transform duration-300", picking && "rotate-180")} aria-hidden="true" />
                  </button>
                  <Popover open={picking} anchorRef={pickRef} placement="bottom-start" onClose={() => setPicking(false)} label="Play straight into a world" className="w-[280px]">
                    <div role="menu" className="flex max-h-[320px] flex-col overflow-y-auto">
                      {worlds.slice(0, 12).map((world) => (
                        <MenuItem
                          key={world.id}
                          icon={<Globe2 size={15} aria-hidden="true" />}
                          label={world.name}
                          hint={`Played ${timeAgo(world.lastPlayed, now)}`}
                          onSelect={() => (setPicking(false), play(instance.id, world.folder))}
                        />
                      ))}
                    </div>
                  </Popover>
                </>
              )}
            </div>
            <MemoryButton instance={instance} />
            <SharingButton instance={instance} />
            <Action icon={<FolderOpen size={15} aria-hidden="true" />} label="Open folder" onClick={() => void openInstanceFolder(instance.id).catch((failure: unknown) => notify({ tone: "info", title: "Couldn't open the folder", body: message(failure) }))} />
            <Action icon={<ServerCog size={15} aria-hidden="true" />} label="Host a server" onClick={() => hostServer({ instanceId: instance.id })} />
            {failed && <Action icon={<History size={15} aria-hidden="true" />} label="See the log" onClick={() => setTab("logs")} />}
          </div>
        </div>
      </m.section>

      <m.dl variants={fadeUp} className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Stat icon={<Clock size={13} aria-hidden="true" />} label="Last played" value={instance.lastPlayed !== null ? timeAgo(instance.lastPlayed, now) : "Never"} />
        <Stat icon={<Hourglass size={13} aria-hidden="true" />} label="Played for" value={stats ? (stats.playtimeMs > 0 ? playTime(Math.round(stats.playtimeMs / 1000)) : "Not yet") : "…"} />
        <Stat icon={<Gamepad2 size={13} aria-hidden="true" />} label="Sessions" value={stats ? stats.sessions.toLocaleString() : "…"} />
        <Stat icon={<Globe2 size={13} aria-hidden="true" />} label="Worlds" value={worlds.length.toLocaleString()} />
      </m.dl>

      <m.div variants={fadeUp} className="flex flex-col gap-5">
        <div role="tablist" aria-label={`${instance.name}`} className="flex w-fit max-w-full gap-1 overflow-x-auto rounded-full border border-cream/8 bg-ink-850 p-1">
          {TABS.map((option) => {
            const selected = option.id === tab;
            const Icon = option.icon;
            return (
              <button
                key={option.id}
                type="button"
                role="tab"
                id={`instance-tab-${option.id}`}
                aria-selected={selected}
                aria-controls={`instance-panel-${option.id}`}
                onClick={() => setTab(option.id)}
                className={cn("group/tab relative h-9 shrink-0 rounded-full px-4 text-sm font-bold transition-colors duration-200", selected ? "text-ink-900" : "text-cream-300 hover:text-cream")}
              >
                {selected && <m.span layoutId={`instance-tab-${instance.id}`} className="tail absolute inset-0 rounded-full bg-cream [--tail:14px]" transition={{ type: "spring", stiffness: 520, damping: 40 }} />}
                <span className="relative flex items-center gap-1.5">
                  <Icon size={14} className={cn("transition-transform duration-300 ease-out-expo", selected ? "scale-110" : "group-hover/tab:-translate-y-px")} aria-hidden="true" />
                  {option.label}
                </span>
              </button>
            );
          })}
        </div>
        <div className="relative">
          <AnimatePresence mode="popLayout" initial={false} custom={direction}>
            <m.div
              key={tab}
              role="tabpanel"
              id={`instance-panel-${tab}`}
              aria-labelledby={`instance-tab-${tab}`}
              custom={direction}
              variants={panel}
              initial="enter"
              animate="show"
              exit="leave"
            >
              {tab === "content" && <ContentTab instance={instance} locked={locked} />}
              {tab === "worlds" && <WorldsTab instance={instance} locked={locked} />}
              {tab === "screenshots" && <ScreenshotsTab instance={instance} />}
              {tab === "journal" && <JournalTab instance={instance} />}
              {tab === "servers" && <ServersTab instance={instance} locked={locked} />}
              {tab === "logs" && <LogsTab instance={instance} />}
              {tab === "settings" && <SettingsTab instance={instance} locked={locked} local={local} onLocal={saveLocal} onDeleted={onBack} />}
            </m.div>
          </AnimatePresence>
        </div>
      </m.div>
    </Page>
  );
}

function Stat({ icon, label, value }: { icon?: ReactNode; label: string; value: string }) {
  return (
    <div className="rounded-2xl border border-cream/7 bg-ink-800 px-4 py-3.5">
      <dt className="flex items-center gap-1.5 text-xs font-semibold text-mute">
        {icon}
        {label}
      </dt>
      <dd className="mt-1 font-display text-xl font-bold tabular-nums">{value}</dd>
    </div>
  );
}

function BackButton({ label, onBack }: { label: string; onBack: () => void }) {
  return (
    <m.button
      variants={fadeUp}
      type="button"
      onClick={onBack}
      className="group -mb-2 flex w-fit items-center gap-1.5 rounded-full px-2 py-1 text-sm font-semibold text-cream-300 transition-colors hover:text-cream"
    >
      <ArrowLeft size={16} className="transition-transform duration-300 group-hover:-translate-x-0.5" aria-hidden="true" />
      {label}
    </m.button>
  );
}

function Action({ icon, label, onClick }: { icon: ReactNode; label: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex h-11 items-center gap-2 rounded-full border border-cream/18 bg-ink-900/50 px-4 text-sm font-bold text-cream-200 backdrop-blur-sm transition-colors hover:border-cream/40 hover:text-cream"
    >
      {icon}
      {label}
    </button>
  );
}
