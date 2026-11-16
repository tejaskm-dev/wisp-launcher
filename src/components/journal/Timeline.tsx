import { Archive, Box, Check, Download, FolderInput, History, NotebookPen, Package, Pencil, RotateCcw, Trash2, TriangleAlert, Undo2, Upload, Wrench } from "lucide-react";
import { m } from "motion/react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { addNote, deleteNote, editNote, getJournal, getRestorePoints, restoreWorld, thumbUrl, undoFix } from "../../data/api";
import type { HomeSnapshot, JournalEntry, JournalEvent } from "../../data/types";
import { cn } from "../../lib/cn";
import { formatBytes } from "../../lib/format";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { playTime } from "../worlds/WorldArt";

/** Lane colours, one per instance, like branches in a git graph. */
const LANE_COLORS = ["#f2979a", "#9db8ff", "#8fe0b6", "#f4c890", "#c9a7ff", "#7fd6e0", "#f07a5a", "#e9e1d5"];
export const laneColor = (lane: number) => LANE_COLORS[lane % LANE_COLORS.length];

const LANE_GAP = 18;
const GUTTER_PAD = 12;
/** Where a row's node sits, from the row's top: level with its first line of text. */
const NODE_Y = 24;

type Item = { kind: "day"; key: string; label: string } | { kind: "entry"; key: string; entry: JournalEntry; lane: number };

function startOfDay(time: number) {
  return new Date(time).setHours(0, 0, 0, 0);
}

function dayLabel(at: number, now: number): string {
  const day = startOfDay(at);
  const today = startOfDay(now);
  if (day === today) return "Today";
  if (day === today - 86_400_000) return "Yesterday";
  const date = new Date(at);
  if (today - day < 6 * 86_400_000) return date.toLocaleDateString(undefined, { weekday: "long" });
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  return date.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric", year: sameYear ? undefined : "numeric" });
}

const clock = (at: number) => new Date(at).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });

/** Each instance's lane: its place in `order` if given, else its order of appearance (newest first). */
export function useLanes(entries: JournalEntry[], order?: string[]) {
  return useMemo(() => {
    const lanes = new Map<string, number>();
    for (const id of order ?? []) lanes.set(id, lanes.size);
    for (const entry of entries) {
      const id = entry.instanceId ?? "";
      if (!lanes.has(id)) lanes.set(id, lanes.size);
    }
    // Only lanes with something on them take room; their colours stay put.
    const used = new Set(entries.map((entry) => entry.instanceId ?? ""));
    return new Map([...lanes].filter(([id]) => used.has(id)));
  }, [entries, order]);
}

interface TimelineProps {
  entries: JournalEntry[];
  home: HomeSnapshot | null;
  now: number;
  /** Lane per instance (the whole journal); off for one world's timeline. */
  lanes?: boolean;
  /** Instance ids in lane order, so colours match elsewhere on the page. */
  laneOrder?: string[];
  onOpenWorld?: (worldId: string) => void;
  onOpenScreenshot?: (instanceId: string, file: string) => void;
}

/** The journal as a git-style graph: a line per instance, a node per thing that happened, days as headings. */
export function Timeline({ entries, home, now, lanes = true, laneOrder, onOpenWorld, onOpenScreenshot }: TimelineProps) {
  const { notify, journalVersion } = useApp();
  const laneOf = useLanes(entries, laneOrder);
  // Columns for the used lanes only, packed left; colours come from the lane number.
  const columns = useMemo(() => new Map([...laneOf.values()].sort((a, b) => a - b).map((lane, column) => [lane, column])), [laneOf]);
  const laneCount = lanes ? Math.max(1, columns.size) : 1;
  const gutter = GUTTER_PAD * 2 + (laneCount - 1) * LANE_GAP;
  const columnOf = (lane: number) => (lanes ? (columns.get(lane) ?? 0) : 0);

  // Which restore points still exist (older automatic ones are pruned).
  const [liveRestorePoints, setLiveRestorePoints] = useState<Set<string> | null>(null);
  useEffect(() => {
    let active = true;
    getRestorePoints().then(
      (points) => active && setLiveRestorePoints(new Set(points.map((point) => point.id))),
      () => active && setLiveRestorePoints(new Set()),
    );
    return () => {
      active = false;
    };
  }, [journalVersion]);

  const items = useMemo(() => {
    const list: Item[] = [];
    let lastDay = -1;
    for (const entry of entries) {
      const day = startOfDay(entry.at);
      if (day !== lastDay) {
        list.push({ kind: "day", key: `day-${day}`, label: dayLabel(entry.at, now) });
        lastDay = day;
      }
      list.push({ kind: "entry", key: entry.id, entry, lane: lanes ? (laneOf.get(entry.instanceId ?? "") ?? 0) : 0 });
    }
    return list;
  }, [entries, lanes, laneOf, now]);

  // Each lane runs from its first node to its last.
  const spans = useMemo(() => {
    const found = new Map<number, { first: number; last: number }>();
    items.forEach((item, index) => {
      if (item.kind !== "entry") return;
      const span = found.get(item.lane);
      if (span) span.last = index;
      else found.set(item.lane, { first: index, last: index });
    });
    return found;
  }, [items]);

  const worldName = (instanceId: string | null | undefined, folder: string) =>
    home?.worlds.find((world) => world.instanceId === instanceId && world.folder === folder)?.name ?? folder;
  const worldExists = (instanceId: string | null | undefined, folder: string) =>
    Boolean(home?.worlds.some((world) => world.instanceId === instanceId && world.folder === folder));
  const instanceName = (instanceId: string | null | undefined) => home?.instances.find((instance) => instance.id === instanceId)?.name ?? null;

  const [restoring, setRestoring] = useState<{ id: string; world: string; at: number } | null>(null);
  const [busy, setBusy] = useState(false);
  const confirmRestore = async () => {
    if (!restoring) return;
    setBusy(true);
    try {
      await restoreWorld(restoring.id);
      notify({ tone: "success", title: `${restoring.world} is restored`, body: "The world as it was before is kept as a restore point too." });
      setRestoring(null);
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't restore it", body: failure instanceof Error ? failure.message : String(failure) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <ol aria-label="Journal" className="flex flex-col">
        {items.map((item, index) => (
          <li key={item.key} className="relative flex">
            <div aria-hidden="true" className="relative shrink-0" style={{ width: gutter }}>
              {[...spans.entries()].map(([lane, span]) => {
                if (index < span.first || index > span.last || span.first === span.last) return null;
                const top = index === span.first ? NODE_Y : 0;
                const bottom = index === span.last ? `calc(100% - ${NODE_Y}px)` : 0;
                return (
                  <span
                    key={lane}
                    className="absolute w-0.5 -translate-x-1/2 rounded-full opacity-45"
                    style={{ left: GUTTER_PAD + columnOf(lane) * LANE_GAP, top, bottom, background: laneColor(lane) }}
                  />
                );
              })}
              {item.kind === "entry" && <Node event={item.entry.event} color={laneColor(item.lane)} left={GUTTER_PAD + columnOf(item.lane) * LANE_GAP} />}
            </div>
            {item.kind === "day" ? (
              <h3 className="pb-2 pt-4 text-[11px] font-bold uppercase tracking-[0.14em] text-mute-2 first:pt-0">{item.label}</h3>
            ) : (
              <m.div
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.35, delay: Math.min(index, 12) * 0.02 }}
                className="min-w-0 flex-1 pb-4 pl-1"
              >
                <EntryCard
                  entry={item.entry}
                  laneColor={lanes && (laneOf.size > 1 || (laneOrder?.length ?? 0) > 1) ? laneColor(item.lane) : null}
                  instanceName={instanceName(item.entry.instanceId)}
                  worldName={worldName}
                  worldExists={worldExists}
                  restorable={liveRestorePoints}
                  onOpenWorld={onOpenWorld}
                  onOpenScreenshot={onOpenScreenshot}
                  onRestore={(id, world) => setRestoring({ id, world, at: item.entry.at })}
                />
              </m.div>
            )}
          </li>
        ))}
      </ol>

      <Dialog
        open={restoring !== null}
        onClose={() => !busy && setRestoring(null)}
        title={restoring ? `Restore ${restoring.world}?` : "Restore"}
        description={restoring ? `Back to how it was on ${new Date(restoring.at).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}.` : undefined}
        footer={
          <>
            <Button variant="secondary" onClick={() => setRestoring(null)} disabled={busy}>
              Cancel
            </Button>
            <Button icon={RotateCcw} onClick={() => void confirmRestore()} disabled={busy}>
              {busy ? "Restoring…" : "Restore"}
            </Button>
          </>
        }
      >
        <p className="text-sm leading-relaxed text-cream-300">
          Wisp keeps the world as it is right now as another restore point first, so you can come back to it. Close Minecraft before restoring.
        </p>
      </Dialog>
    </>
  );
}

function Node({ event, color, left }: { event: JournalEvent; color: string; left: number }) {
  const style = { left, top: NODE_Y };
  const base = "absolute -translate-x-1/2 -translate-y-1/2";
  switch (event.kind) {
    case "session":
      return (
        <span
          className={cn(base, "size-3.5 rounded-full ring-4 ring-ink-900")}
          style={{ ...style, background: event.crashed ? "#f07a5a" : color, boxShadow: `0 0 14px ${event.crashed ? "#f07a5a" : color}66` }}
        />
      );
    case "restore-point":
    case "restored":
      return <span className={cn(base, "size-3 rotate-45 rounded-[3px] border-2 bg-ink-900 ring-4 ring-ink-900")} style={{ ...style, borderColor: color }} />;
    case "note":
      return <span className={cn(base, "size-3 rounded-full border-2 bg-ink-900 ring-4 ring-ink-900")} style={{ ...style, borderColor: color }} />;
    default:
      return <span className={cn(base, "size-2 rounded-full ring-4 ring-ink-900")} style={{ ...style, background: color, opacity: 0.8 }} />;
  }
}

interface EntryCardProps {
  entry: JournalEntry;
  laneColor: string | null;
  instanceName: string | null;
  worldName: (instanceId: string | null | undefined, folder: string) => string;
  worldExists: (instanceId: string | null | undefined, folder: string) => boolean;
  restorable: Set<string> | null;
  onOpenWorld?: (worldId: string) => void;
  onOpenScreenshot?: (instanceId: string, file: string) => void;
  onRestore: (snapshotId: string, worldName: string) => void;
}

/** The instance a row belongs to, coloured like its lane. */
function InstanceTag({ name, color }: { name: string | null; color: string | null }) {
  if (!name || !color) return null;
  return (
    <span className="inline-flex max-w-[160px] items-center gap-1.5 truncate rounded-full bg-cream/5 px-2 py-0.5 text-[11px] font-bold text-cream-300">
      <span className="size-1.5 shrink-0 rounded-full" style={{ background: color }} aria-hidden="true" />
      <span className="truncate">{name}</span>
    </span>
  );
}

function Line({ icon, children, time, tag }: { icon: ReactNode; children: ReactNode; time: number; tag?: ReactNode }) {
  return (
    <div className="flex min-h-12 items-center gap-2.5 rounded-xl px-1 py-1.5">
      <span className="grid size-7 shrink-0 place-items-center rounded-lg bg-cream/5 text-cream-400">{icon}</span>
      <p className="min-w-0 flex-1 text-sm text-cream-300">{children}</p>
      {tag}
      <time className="shrink-0 text-xs tabular-nums text-mute-2">{clock(time)}</time>
    </div>
  );
}

function EntryCard({ entry, laneColor: color, instanceName, worldName, worldExists, restorable, onOpenWorld, onOpenScreenshot, onRestore }: EntryCardProps) {
  const event = entry.event;
  const tag = <InstanceTag name={instanceName} color={color} />;
  const worldLink = (folder: string) => {
    const name = worldName(entry.instanceId, folder);
    return onOpenWorld && entry.instanceId && worldExists(entry.instanceId, folder) ? (
      <button
        key={folder}
        type="button"
        onClick={() => onOpenWorld(`${entry.instanceId}/${folder}`)}
        className="font-bold text-cream underline decoration-cream/25 underline-offset-4 transition-colors hover:decoration-cream/70"
      >
        {name}
      </button>
    ) : (
      <span key={folder} className="font-bold text-cream">
        {name}
      </span>
    );
  };

  switch (event.kind) {
    case "session":
      return <SessionCard entry={entry} event={event} tag={tag} worldLink={worldLink} onOpenScreenshot={onOpenScreenshot} />;
    case "note":
      return <NoteCard entry={entry} text={event.text} tag={tag} worldLink={worldLink} />;
    case "restore-point": {
      const world = entry.world ?? "";
      const name = worldName(entry.instanceId, world);
      const live = restorable?.has(event.snapshot) ?? false;
      const title = event.label ?? (event.reason === "auto" ? "Restore point after playing" : event.reason === "manual" ? "Restore point" : "Kept before a restore");
      return (
        <div className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-2xl border border-cream/8 bg-ink-800/70 px-3.5 py-3">
          <code className="rounded-md bg-cream/8 px-1.5 py-0.5 font-mono text-xs font-semibold text-cream-300" title={event.snapshot}>
            {event.snapshot.slice(0, 7)}
          </code>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-bold">
              {title} <span className="font-normal text-mute">·</span> {world ? worldLink(world) : null}
            </p>
            <p className="text-xs text-mute">
              {event.files.toLocaleString()} files · {formatBytes(event.totalBytes)} ·{" "}
              {event.addedBytes > 0 ? <span className="text-moss-soft">+{formatBytes(event.addedBytes)} stored</span> : "nothing new to store"}
              {!live && restorable && " · since removed"}
            </p>
          </div>
          {tag}
          <time className="text-xs tabular-nums text-mute-2">{clock(entry.at)}</time>
          {live && (
            <button
              type="button"
              onClick={() => onRestore(event.snapshot, name)}
              className="flex h-8 items-center gap-1.5 rounded-full border border-cream/15 px-3 text-xs font-bold text-cream-300 transition-colors hover:border-cream/35 hover:bg-cream/8 hover:text-cream"
            >
              <RotateCcw size={12} aria-hidden="true" />
              Restore
            </button>
          )}
        </div>
      );
    }
    case "restored":
      return (
        <Line icon={<History size={14} aria-hidden="true" />} time={entry.at} tag={tag}>
          Restored {entry.world ? worldLink(entry.world) : "a world"} to <code className="font-mono text-xs text-cream-400">{event.snapshot.slice(0, 7)}</code>
        </Line>
      );
    case "content-added":
      return (
        <Line icon={<Download size={14} aria-hidden="true" />} time={entry.at} tag={tag}>
          <span className="font-mono text-moss-soft">+ </span>
          <span className="font-bold text-cream">{event.title}</span> <span className="text-mute">{event.version}</span>
          {event.dependencies > 0 && <span className="text-mute"> · with {event.dependencies === 1 ? "1 requirement" : `${event.dependencies} requirements`}</span>}
        </Line>
      );
    case "content-removed":
      return (
        <Line icon={<Trash2 size={14} aria-hidden="true" />} time={entry.at} tag={tag}>
          <span className="font-mono text-coral">− </span>
          <span className="font-bold text-cream">{event.title}</span> <span className="text-mute">removed</span>
        </Line>
      );
    case "instance-created":
      return (
        <Line icon={<Box size={14} aria-hidden="true" />} time={entry.at} tag={tag}>
          Made the instance <span className="font-bold text-cream">{event.name}</span> <span className="text-mute">· {event.label}</span>
        </Line>
      );
    case "modpack-installed":
      return (
        <Line icon={<Package size={14} aria-hidden="true" />} time={entry.at} tag={tag}>
          Installed the modpack <span className="font-bold text-cream">{event.title}</span> <span className="text-mute">{event.version}</span>
        </Line>
      );
    case "world-imported":
      return (
        <Line icon={<FolderInput size={14} aria-hidden="true" />} time={entry.at} tag={tag}>
          Imported {entry.world ? worldLink(entry.world) : <span className="font-bold text-cream">{event.name}</span>}
        </Line>
      );
    case "world-exported":
      return (
        <Line icon={<Upload size={14} aria-hidden="true" />} time={entry.at} tag={tag}>
          Exported <span className="font-bold text-cream">{event.name}</span> as a zip
        </Line>
      );
    case "world-deleted":
      return (
        <Line icon={<Archive size={14} aria-hidden="true" />} time={entry.at} tag={tag}>
          Moved <span className="font-bold text-cream">{event.name}</span> to the Trash <span className="text-mute">· its restore points are kept</span>
        </Line>
      );
    case "fixed":
      return <FixCard entry={entry} fixed={event.fixed} afterCrash={event.afterCrash} tag={tag} />;
    case "fix-undone":
      return (
        <Line icon={<Undo2 size={14} aria-hidden="true" />} time={entry.at} tag={tag}>
          Put one of Wisp's fixes back the way it was
        </Line>
      );
  }
}

/** Fixes that have been undone, by the id of their entry. */
function useUndoneFixes(): Set<string> {
  const { journalVersion } = useApp();
  const [undone, setUndone] = useState<Set<string>>(new Set());
  useEffect(() => {
    let active = true;
    getJournal({ kinds: ["fix-undone"], limit: 500 }).then(
      (found) => active && setUndone(new Set(found.flatMap((entry) => (entry.event.kind === "fix-undone" ? [entry.event.fix] : [])))),
      () => {},
    );
    return () => {
      active = false;
    };
  }, [journalVersion]);
  return undone;
}

/** What Wisp fixed, before a launch or after a crash, with Undo. */
function FixCard({ entry, fixed, afterCrash, tag }: { entry: JournalEntry; fixed: string[]; afterCrash: boolean; tag: ReactNode }) {
  const { notify } = useApp();
  const undone = useUndoneFixes().has(entry.id);
  const [busy, setBusy] = useState(false);
  const undo = () => {
    setBusy(true);
    undoFix(entry.id).then(
      () => notify({ tone: "success", title: "Put back the way it was" }),
      (failure: unknown) => {
        setBusy(false);
        notify({ tone: "info", title: "Couldn't undo it", body: failure instanceof Error ? failure.message : String(failure) });
      },
    );
  };
  return (
    <div className={cn("flex flex-col gap-2 rounded-2xl border px-3.5 py-3", undone ? "border-cream/8 bg-ink-800/50 opacity-70" : "border-moss/25 bg-moss/5")}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <Wrench size={14} className="text-moss-soft" aria-hidden="true" />
        <p className="min-w-0 flex-1 text-sm font-bold">
          {afterCrash ? "Wisp fixed what made the game crash" : "Wisp fixed the mods before starting"}
          {undone && <span className="font-normal text-mute"> · undone</span>}
        </p>
        {tag}
        <time className="text-xs tabular-nums text-mute-2">{clock(entry.at)}</time>
        {!undone && (
          <button
            type="button"
            disabled={busy}
            onClick={undo}
            className="flex h-8 items-center gap-1.5 rounded-full border border-cream/15 px-3 text-xs font-bold text-cream-300 transition-colors hover:border-cream/35 hover:bg-cream/8 hover:text-cream disabled:opacity-50"
          >
            <Undo2 size={12} aria-hidden="true" />
            Undo
          </button>
        )}
      </div>
      <ul className="flex flex-col gap-1 pl-[22px]">
        {fixed.map((line) => (
          <li key={line} className="flex gap-2 text-[13px] leading-snug text-cream-300">
            <Check size={13} className="mt-0.5 shrink-0 text-moss-soft" aria-hidden="true" />
            {line}
          </li>
        ))}
      </ul>
    </div>
  );
}

function SessionCard({
  entry,
  event,
  tag,
  worldLink,
  onOpenScreenshot,
}: {
  entry: JournalEntry;
  event: Extract<JournalEvent, { kind: "session" }>;
  tag: ReactNode;
  worldLink: (folder: string) => ReactNode;
  onOpenScreenshot?: (instanceId: string, file: string) => void;
}) {
  const [noting, setNoting] = useState(false);
  const seconds = Math.max(0, (event.endedAt - event.startedAt) / 1000);
  const shots = event.screenshots;
  const instanceId = entry.instanceId;
  return (
    <div className={cn("rounded-2xl border bg-ink-800 p-3.5", event.crashed ? "border-coral/25" : "border-cream/8")}>
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <p className="text-[15px] leading-snug text-cream-300">
            {event.worlds.length === 0 ? (
              <>
                Played <span className="font-bold text-cream">{event.label}</span>
              </>
            ) : (
              <>
                Played{" "}
                {event.worlds.map((folder, index) => (
                  <span key={folder}>
                    {index > 0 && (index === event.worlds.length - 1 ? " and " : ", ")}
                    {worldLink(folder)}
                  </span>
                ))}
              </>
            )}
          </p>
          <p className="mt-0.5 text-xs text-mute">
            {playTime(seconds)} · {event.label} · on {event.device}
            {event.newWorlds.length > 0 && <span className="text-amber-soft"> · {event.newWorlds.length === 1 ? "a new world" : `${event.newWorlds.length} new worlds`}</span>}
          </p>
        </div>
        {event.crashed && (
          <span className="flex items-center gap-1 rounded-full bg-coral/15 px-2 py-0.5 text-[11px] font-bold text-coral">
            <TriangleAlert size={11} aria-hidden="true" />
            Crashed
          </span>
        )}
        {tag}
        <time className="shrink-0 text-xs tabular-nums text-mute-2">{clock(entry.at)}</time>
      </div>
      {shots.length > 0 && instanceId && (
        <ul className="no-scrollbar -mx-1 mt-3 flex gap-2 overflow-x-auto px-1">
          {shots.slice(0, 8).map((file) => (
            <ShotThumb key={file} src={thumbUrl(`instances/${instanceId}/minecraft/screenshots/${file}`, 240, entry.at)} file={file} onOpen={() => onOpenScreenshot?.(instanceId, file)} />
          ))}
          {shots.length > 8 && <li className="grid h-[68px] shrink-0 place-items-center px-3 text-xs font-bold text-mute">+{shots.length - 8}</li>}
        </ul>
      )}
      {event.worlds.length > 0 && instanceId && (
        <div className="mt-2.5">
          {noting ? (
            <NoteComposer
              placeholder={`A note about ${event.worlds.length === 1 ? "this session" : "these worlds"}…`}
              onCancel={() => setNoting(false)}
              onSave={async (text) => {
                await addNote(instanceId, event.worlds[0], text, entry.id);
                setNoting(false);
              }}
            />
          ) : (
            <button
              type="button"
              onClick={() => setNoting(true)}
              className="flex h-7 items-center gap-1.5 rounded-full px-2 text-xs font-bold text-mute transition-colors hover:bg-cream/6 hover:text-cream"
            >
              <NotebookPen size={12} aria-hidden="true" />
              Add a note
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/** A screenshot in a session; hidden if the file has been deleted since. */
function ShotThumb({ src, file, onOpen }: { src: string; file: string; onOpen: () => void }) {
  const [missing, setMissing] = useState(false);
  if (missing) return null;
  return (
    <li className="shrink-0">
      <button
        type="button"
        onClick={onOpen}
        aria-label={`Screenshot ${file}`}
        className="block overflow-hidden rounded-lg ring-1 ring-cream/10 transition-transform duration-200 hover:scale-[1.04]"
      >
        <img src={src} alt="" loading="lazy" onError={() => setMissing(true)} className="h-[68px] w-[120px] bg-ink-700 object-cover" />
      </button>
    </li>
  );
}

function NoteCard({ entry, text, tag, worldLink }: { entry: JournalEntry; text: string; tag: ReactNode; worldLink: (folder: string) => ReactNode }) {
  const { notify } = useApp();
  const [editing, setEditing] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const fail = (failure: unknown) => notify({ tone: "info", title: "Couldn't change the note", body: failure instanceof Error ? failure.message : String(failure) });

  if (editing) {
    return (
      <NoteComposer
        initial={text}
        placeholder="Your note…"
        onCancel={() => setEditing(false)}
        onSave={async (next) => {
          await editNote(entry.id, next);
          setEditing(false);
        }}
      />
    );
  }
  return (
    <div className="group/note rounded-2xl bg-cream/[0.035] px-4 py-3">
      <p className="whitespace-pre-wrap font-serif text-[19px] italic leading-snug text-cream-200">“{text}”</p>
      <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-mute">
        {entry.world && <span>about {worldLink(entry.world)}</span>}
        {tag}
        <time className="tabular-nums text-mute-2">{clock(entry.at)}</time>
        <span className="ml-auto flex items-center gap-1 opacity-0 transition-opacity group-hover/note:opacity-100 focus-within:opacity-100">
          {confirming ? (
            <>
              <span className="font-semibold text-cream-300">Delete this note?</span>
              <button type="button" onClick={() => void deleteNote(entry.id).catch(fail)} className="rounded-full px-2 py-1 font-bold text-coral hover:bg-coral/10">
                Delete
              </button>
              <button type="button" onClick={() => setConfirming(false)} className="rounded-full px-2 py-1 font-bold text-cream-300 hover:bg-cream/8">
                Keep
              </button>
            </>
          ) : (
            <>
              <button type="button" onClick={() => setEditing(true)} className="flex items-center gap-1 rounded-full px-2 py-1 font-bold text-cream-300 hover:bg-cream/8 hover:text-cream">
                <Pencil size={11} aria-hidden="true" />
                Edit
              </button>
              <button type="button" onClick={() => setConfirming(true)} className="flex items-center gap-1 rounded-full px-2 py-1 font-bold text-cream-300 hover:bg-cream/8 hover:text-cream">
                <Trash2 size={11} aria-hidden="true" />
                Delete
              </button>
            </>
          )}
        </span>
      </div>
    </div>
  );
}

/** A note being written: Save with the button or ⌘/Ctrl+Enter, Escape to cancel. */
export function NoteComposer({
  initial = "",
  placeholder,
  onSave,
  onCancel,
  autoFocus = true,
}: {
  initial?: string;
  placeholder: string;
  onSave: (text: string) => Promise<void>;
  onCancel?: () => void;
  autoFocus?: boolean;
}) {
  const { notify } = useApp();
  const [text, setText] = useState(initial);
  const [saving, setSaving] = useState(false);
  const save = async () => {
    if (!text.trim() || saving) return;
    setSaving(true);
    try {
      await onSave(text);
      setText("");
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't save the note", body: failure instanceof Error ? failure.message : String(failure) });
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="flex flex-col gap-2 rounded-2xl border border-cream/12 bg-ink-900/50 p-2.5 focus-within:border-cream/30">
      <textarea
        autoFocus={autoFocus}
        value={text}
        maxLength={2000}
        rows={2}
        aria-label="Note"
        placeholder={placeholder}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) void save();
          if (event.key === "Escape") onCancel?.();
        }}
        className="w-full resize-none bg-transparent px-1.5 py-1 font-serif text-[18px] italic leading-snug text-cream outline-none placeholder:text-mute-2"
      />
      <div className="flex items-center justify-end gap-2">
        {onCancel && (
          <button type="button" onClick={onCancel} className="h-8 rounded-full px-3 text-xs font-bold text-cream-300 hover:bg-cream/8 hover:text-cream">
            Cancel
          </button>
        )}
        <button
          type="button"
          disabled={!text.trim() || saving}
          onClick={() => void save()}
          className="h-8 rounded-full bg-cream px-3.5 text-xs font-bold text-ink-900 transition-opacity disabled:opacity-40"
        >
          {saving ? "Saving…" : "Save note"}
        </button>
      </div>
    </div>
  );
}
