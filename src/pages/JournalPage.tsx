import { Camera, Clock, Gamepad2, HardDrive, NotebookPen } from "lucide-react";
import { m } from "motion/react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { InstanceAvatar } from "../components/instances/InstanceAvatar";
import { laneColor, NoteComposer, Timeline } from "../components/journal/Timeline";
import { useJournal } from "../components/journal/useJournal";
import { Lightbox, shotFromJournal } from "../components/screenshots/Lightbox";
import { EmptyState } from "../components/ui/EmptyState";
import { ErrorState } from "../components/ui/ErrorState";
import { Page } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { PageTabs } from "../components/ui/PageTabs";
import { Select, type SelectOption } from "../components/ui/Select";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { playTime } from "../components/worlds/WorldArt";
import { addNote, getJournal, getRestorePointUsage } from "../data/api";
import type { JournalEntry, JournalKind, Screenshot } from "../data/types";
import { cn } from "../lib/cn";
import { formatBytes } from "../lib/format";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

type Filter = "all" | "sessions" | "notes" | "restore" | "content" | "fixes";
const FILTERS: { value: Filter; label: string; kinds: JournalKind[] }[] = [
  { value: "all", label: "Everything", kinds: [] },
  { value: "sessions", label: "Sessions", kinds: ["session"] },
  { value: "notes", label: "Notes", kinds: ["note"] },
  { value: "restore", label: "Restore points", kinds: ["restore-point", "restored"] },
  { value: "content", label: "Mods & packs", kinds: ["content-added", "content-removed", "modpack-installed", "instance-created"] },
  { value: "fixes", label: "Fixes", kinds: ["fixed", "fix-undone"] },
];
const NO_WORLD = "none";
const WEEK = 7 * 86_400_000;

/** The whole history, like a git log: a lane per instance, newest at the top. */
// The last numbers and lanes read, shown at once when you come back (from the Screenshots tab, say)
// while they're read again.
const seen: { week: JournalEntry[] | null; usage: number | null; lanes: string[] } = { week: null, usage: null, lanes: [] };

export function JournalPage() {
  const { home, now, journalVersion, openWorld } = useApp();
  const [filter, setFilter] = useState<Filter>("all");
  const [instanceId, setInstanceId] = useState<string | null>(null);
  const kinds = FILTERS.find((option) => option.value === filter)?.kinds ?? [];
  const journal = useJournal({ instanceId, kinds });

  // This week's numbers, from every session regardless of the filters.
  const [week, setWeek] = useState<JournalEntry[] | null>(seen.week);
  const [usage, setUsage] = useState<number | null>(seen.usage);
  useEffect(() => {
    let active = true;
    getJournal({ kinds: ["session"], limit: 500 }).then((found) => {
      seen.week = found.filter((entry) => entry.at > Date.now() - WEEK);
      if (active) setWeek(seen.week);
    }, () => active && setWeek([]));
    getRestorePointUsage().then((bytes) => {
      seen.usage = bytes;
      if (active) setUsage(bytes);
    }, () => active && setUsage(null));
    return () => {
      active = false;
    };
  }, [journalVersion]);
  const stats = useMemo(() => {
    let seconds = 0;
    let shots = 0;
    for (const entry of week ?? []) {
      if (entry.event.kind !== "session") continue;
      seconds += Math.max(0, (entry.event.endedAt - entry.event.startedAt) / 1000);
      shots += entry.event.screenshots.length;
    }
    return { seconds, sessions: week?.length ?? 0, shots };
  }, [week]);

  // Instances that appear in the journal, coloured like their lanes (lanes follow the unfiltered order).
  const [allLanes, setAllLanes] = useState<string[]>(seen.lanes);
  useEffect(() => {
    let active = true;
    getJournal({ limit: 400 }).then((found) => {
      if (!active) return;
      const order: string[] = [];
      for (const entry of found) if (entry.instanceId && !order.includes(entry.instanceId)) order.push(entry.instanceId);
      seen.lanes = order;
      setAllLanes(order);
    }, () => {});
    return () => {
      active = false;
    };
  }, [journalVersion]);

  // A note about any world (or none).
  const worldOptions: SelectOption<string>[] = [
    { value: NO_WORLD, label: "No world in particular" },
    ...(home?.worlds ?? []).map((world) => ({ value: world.id, label: world.name, note: world.instanceName })),
  ];
  const [noteWorld, setNoteWorld] = useState(NO_WORLD);
  const saveNote = async (text: string) => {
    const world = home?.worlds.find((candidate) => candidate.id === noteWorld);
    await addNote(world?.instanceId ?? null, world?.folder ?? null, text);
  };

  // Screenshots opened from a session.
  const [lightbox, setLightbox] = useState<{ shots: Screenshot[]; index: number } | null>(null);
  const openShot = (entry: JournalEntry, file: string) => {
    if (entry.event.kind !== "session" || !entry.instanceId) return;
    const instanceName = home?.instances.find((instance) => instance.id === entry.instanceId)?.name ?? entry.instanceId;
    const onlyWorld = entry.event.worlds.length === 1 ? entry.event.worlds[0] : null;
    const world = onlyWorld ? { folder: onlyWorld, name: home?.worlds.find((candidate) => candidate.instanceId === entry.instanceId && candidate.folder === onlyWorld)?.name ?? onlyWorld } : null;
    const shots = entry.event.screenshots.map((name) => shotFromJournal(entry.instanceId as string, instanceName, name, entry.at, world));
    setLightbox({ shots, index: Math.max(0, entry.event.screenshots.indexOf(file)) });
  };

  const instances = (home?.instances ?? []).filter((instance) => allLanes.includes(instance.id)).sort((a, b) => allLanes.indexOf(a.id) - allLanes.indexOf(b.id));
  const empty = journal.entries !== null && journal.entries.length === 0 && filter === "all" && instanceId === null;

  return (
    <Page className="max-w-[1100px]">
      <PageHeader
        title="Journal"
        subtitle="Your game's history, like a git log you'd actually want to scroll: every session with its worlds and screenshots, your notes, restore points, mods coming and going, and what Wisp fixed."
      />
      <PageTabs
        label="Journal"
        current="journal"
        tabs={[
          { id: "journal", label: "Timeline" },
          { id: "screenshots", label: "Screenshots" },
        ]}
      />

      <m.dl variants={fadeUp} className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Stat icon={<Clock size={15} aria-hidden="true" />} label="Played this week" value={week === null ? null : stats.seconds > 0 ? playTime(stats.seconds) : "None yet"} />
        <Stat icon={<Gamepad2 size={15} aria-hidden="true" />} label="Sessions this week" value={week === null ? null : String(stats.sessions)} />
        <Stat icon={<Camera size={15} aria-hidden="true" />} label="Screenshots this week" value={week === null ? null : String(stats.shots)} />
        <Stat icon={<HardDrive size={15} aria-hidden="true" />} label="Restore points use" value={usage === null ? "—" : formatBytes(usage)} />
      </m.dl>

      <m.div variants={fadeUp} className="wisp-surface flex flex-col gap-3 rounded-[22px] p-4">
        <div className="flex flex-wrap items-center gap-3">
          <span className="flex items-center gap-2 text-sm font-bold">
            <NotebookPen size={15} className="text-mute" aria-hidden="true" />
            Write a note
          </span>
          <span className="text-[13px] font-semibold text-mute">about</span>
          <Select label="World the note is about" value={noteWorld} options={worldOptions} onChange={setNoteWorld} searchable={worldOptions.length > 8} className="w-[260px]" />
        </div>
        <NoteComposer placeholder="Plans, coordinates, what you were in the middle of…" autoFocus={false} onSave={saveNote} />
      </m.div>

      <m.div variants={fadeUp} className="flex flex-wrap items-center gap-x-4 gap-y-2.5">
        <div role="radiogroup" aria-label="Show" className="flex flex-wrap gap-1.5">
          {FILTERS.map((option) => (
            <Chip key={option.value} active={filter === option.value} onClick={() => setFilter(option.value)}>
              {option.label}
            </Chip>
          ))}
        </div>
        {instances.length > 1 && (
          <div role="radiogroup" aria-label="Instance" className="flex flex-wrap items-center gap-1.5">
            <span className="mr-1 text-[13px] font-semibold text-mute">Lanes</span>
            <Chip active={instanceId === null} onClick={() => setInstanceId(null)}>
              All
            </Chip>
            {instances.map((instance) => (
              <Chip key={instance.id} active={instanceId === instance.id} onClick={() => setInstanceId(instanceId === instance.id ? null : instance.id)}>
                <span className="size-2 rounded-full" style={{ background: laneColor(allLanes.indexOf(instance.id)) }} aria-hidden="true" />
                <InstanceAvatar instance={instance} className="size-5 rounded-md text-[10px]" />
                {instance.name}
              </Chip>
            ))}
          </div>
        )}
      </m.div>

      {journal.failed ? (
        <ErrorState title="Couldn't read the journal" body="Something went wrong reading Wisp's journal file." onRetry={journal.retry} />
      ) : journal.entries === null ? (
        <Loading label="Loading the journal" className="flex flex-col gap-3 pl-8">
          <Skeleton className="h-4 w-24 rounded-full" />
          <Skeleton className="h-24 w-full rounded-2xl" />
          <Skeleton className="h-12 w-full rounded-2xl" />
          <Skeleton className="h-24 w-full rounded-2xl" />
        </Loading>
      ) : empty ? (
        <EmptyState
          art={<TimelineArt />}
          title="Nothing written yet"
          body="Play a world and Wisp writes the session down here, with your screenshots and a restore point when you quit. Mods you add and notes you leave go here too."
        />
      ) : journal.entries.length === 0 ? (
        <p className="rounded-2xl border border-dashed border-cream/12 px-5 py-10 text-center text-sm text-mute">Nothing like that yet.</p>
      ) : (
        <m.div variants={fadeUp} className="flex flex-col">
          <Timeline
            entries={journal.entries}
            home={home}
            now={now}
            lanes={instanceId === null}
            laneOrder={allLanes}
            onOpenWorld={openWorld}
            onOpenScreenshot={(owner, file) => {
              const entry = journal.entries?.find((candidate) => candidate.instanceId === owner && candidate.event.kind === "session" && candidate.event.screenshots.includes(file));
              if (entry) openShot(entry, file);
            }}
          />
          {journal.more && (
            <button
              type="button"
              onClick={journal.loadMore}
              disabled={journal.loadingMore}
              className="mt-2 self-center rounded-full border border-cream/15 px-4 py-2 text-sm font-bold text-cream-300 transition-colors hover:border-cream/35 hover:text-cream"
            >
              {journal.loadingMore ? "Loading…" : "Show older"}
            </button>
          )}
        </m.div>
      )}

      {lightbox && <Lightbox shots={lightbox.shots} index={lightbox.index} onIndex={(index) => setLightbox(index === null ? null : { ...lightbox, index })} onOpenWorld={openWorld} />}
    </Page>
  );
}

function Stat({ icon, label, value }: { icon: ReactNode; label: string; value: string | null }) {
  return (
    <div className="wisp-surface rounded-2xl px-4 py-3.5">
      <dt className="flex items-center gap-1.5 text-xs font-semibold text-mute">
        {icon}
        {label}
      </dt>
      <dd className="mt-1 font-display text-xl font-bold tabular-nums">{value ?? <Skeleton className="mt-1 h-5 w-16 rounded-full" />}</dd>
    </div>
  );
}

function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: ReactNode }) {
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

/** Decorative: a little branch graph. */
function TimelineArt() {
  return (
    <svg aria-hidden="true" width="150" height="104" viewBox="0 0 150 104" fill="none" className="mb-1">
      <path d="M40 8 V96" stroke="#f2979a" strokeOpacity="0.5" strokeWidth="2" strokeLinecap="round" />
      <path d="M40 30 C 40 44, 76 40, 76 54 V 96" stroke="#9db8ff" strokeOpacity="0.5" strokeWidth="2" strokeLinecap="round" />
      <path d="M76 66 C 76 78, 110 74, 110 88" stroke="#8fe0b6" strokeOpacity="0.5" strokeWidth="2" strokeLinecap="round" />
      <circle cx="40" cy="14" r="6" fill="#f2979a" />
      <circle cx="40" cy="48" r="5" fill="#0e0d0c" stroke="#f2979a" strokeWidth="2" />
      <rect x="70" y="58" width="11" height="11" rx="2.5" transform="rotate(45 75.5 63.5)" fill="#0e0d0c" stroke="#9db8ff" strokeWidth="2" />
      <circle cx="40" cy="84" r="6" fill="#f2979a" />
      <circle cx="110" cy="90" r="5" fill="#8fe0b6" />
    </svg>
  );
}
