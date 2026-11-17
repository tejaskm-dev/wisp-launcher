import { ArrowLeft, Archive, Clock, FolderOpen, HardDrive, History, Images, LoaderCircle, Play, RotateCcw, Square, Trash2, Upload } from "lucide-react";
import { m } from "motion/react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { InstanceAvatar } from "../components/instances/InstanceAvatar";
import { NoteComposer, Timeline } from "../components/journal/Timeline";
import { useJournal } from "../components/journal/useJournal";
import { Lightbox } from "../components/screenshots/Lightbox";
import { Button } from "../components/ui/Button";
import { Dialog } from "../components/ui/Dialog";
import { EmptyState } from "../components/ui/EmptyState";
import { Page } from "../components/ui/Page";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { worldLaunch } from "../components/worlds/launch";
import { WorldArt, playTime, worldMode } from "../components/worlds/WorldArt";
import {
  addNote,
  createRestorePoint,
  deleteRestorePoint,
  deleteWorld,
  exportWorld,
  getScreenshots,
  getWorldExtras,
  openWorldFolder,
  restoreWorld,
  thumbUrl,
} from "../data/api";
import type { PageId, RestorePoint, Screenshot, WorldExtras } from "../data/types";
import { cn } from "../lib/cn";
import { formatBytes, timeAgo } from "../lib/format";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

const BACK_LABELS: Partial<Record<PageId, string>> = { home: "Home", worlds: "Worlds", journal: "Journal", screenshots: "Screenshots" };

/** One world: its picture and numbers, Play, its journal, restore points and screenshots. */
export function WorldPage({ worldId, backTo, onBack }: { worldId: string; backTo: PageId; onBack: () => void }) {
  const { home, now, launches, launchWorlds, play, stop, notify, journalVersion, openWorld } = useApp();
  const world = home?.worlds.find((candidate) => candidate.id === worldId) ?? null;
  const instance = world ? (home?.instances.find((candidate) => candidate.id === world.instanceId) ?? null) : null;

  const [extras, setExtras] = useState<WorldExtras | null>(null);
  const [shots, setShots] = useState<Screenshot[] | null>(null);
  useEffect(() => {
    if (!world) return;
    let active = true;
    getWorldExtras(world.instanceId, world.folder).then(
      (found) => active && setExtras(found),
      () => active && setExtras({ sizeBytes: 0, restorePoints: [] }),
    );
    getScreenshots().then(
      (all) => active && setShots(all.filter((shot) => shot.instanceId === world.instanceId && shot.worldFolder === world.folder)),
      () => active && setShots([]),
    );
    return () => {
      active = false;
    };
    // Reload when the world changes on disk (a session ends, a restore point is made).
  }, [world?.instanceId, world?.folder, world?.lastPlayed, journalVersion]); // eslint-disable-line react-hooks/exhaustive-deps

  const journal = useJournal({ instanceId: world?.instanceId ?? null, world: world?.folder ?? null });
  const [lightbox, setLightbox] = useState<number | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<{ kind: "delete" } | { kind: "restore"; point: RestorePoint } | { kind: "drop"; point: RestorePoint } | null>(null);

  const fail = (title: string) => (failure: unknown) => notify({ tone: "info", title, body: failure instanceof Error ? failure.message : String(failure) });
  const run = async (label: string, work: () => Promise<void>, failure: string) => {
    setBusy(label);
    try {
      await work();
    } catch (error: unknown) {
      fail(failure)(error);
    } finally {
      setBusy(null);
    }
  };

  const launch = world ? worldLaunch(world, launches, launchWorlds, now) : null;
  const restorePoints = extras?.restorePoints ?? null;
  const stats = useMemo(() => {
    if (!world) return [];
    const list: { icon: ReactNode; label: string; value: string }[] = [
      { icon: <Clock size={15} aria-hidden="true" />, label: "Last played", value: timeAgo(world.lastPlayed, now) },
      { icon: <History size={15} aria-hidden="true" />, label: "Time in world", value: world.playSeconds ? playTime(world.playSeconds) : "—" },
      { icon: <HardDrive size={15} aria-hidden="true" />, label: "Size", value: extras ? formatBytes(extras.sizeBytes) : "…" },
      { icon: <RotateCcw size={15} aria-hidden="true" />, label: "Restore points", value: restorePoints ? String(restorePoints.length) : "…" },
    ];
    return list;
  }, [world, now, extras, restorePoints]);

  if (!home) {
    return (
      <Page>
        <Loading label="Loading the world" className="flex flex-col gap-5">
          <Skeleton className="h-[280px] w-full rounded-[26px]" />
          <Skeleton className="h-24 w-full rounded-2xl" />
        </Loading>
      </Page>
    );
  }
  if (!world) {
    return (
      <Page>
        <BackButton label={BACK_LABELS[backTo] ?? "Back"} onBack={onBack} />
        <EmptyState title="That world isn't here anymore" body="It may have been deleted or moved. Its restore points are still in the Journal." actions={<Button onClick={onBack}>Go back</Button>} />
      </Page>
    );
  }

  const view = launch?.view;
  const running = view?.kind === "running";
  const starting = view?.kind === "busy";
  const mode = worldMode(world);
  const badges = [mode, world.version ? `Minecraft ${world.version}` : null, world.difficulty ? world.difficulty.replace(/^./, (c) => c.toUpperCase()) : null].filter(Boolean) as string[];

  return (
    <Page>
      <BackButton label={BACK_LABELS[backTo] ?? "Back"} onBack={onBack} />

      <m.section variants={fadeUp} className="relative isolate overflow-hidden rounded-[26px] border border-cream/8 bg-ink-850">
        <WorldArt world={world} width={1280} className="absolute inset-0 -z-10 opacity-80" iconClassName="size-28 rounded-2xl" />
        <div className="absolute inset-0 -z-10 bg-linear-to-t from-ink-900 via-ink-900/55 to-ink-900/10" />
        <div className="flex min-h-[280px] flex-col justify-end gap-5 p-8">
          <div>
            <p className="flex items-center gap-2 text-sm font-semibold text-cream-300">
              {instance && <InstanceAvatar instance={instance} className="size-6 rounded-md text-[11px]" />}
              {world.instanceName} · {world.loader}
            </p>
            <h1 className="mt-2 font-display text-[clamp(40px,4.4vw,60px)] font-extrabold leading-none tracking-[-0.04em]">{world.name}</h1>
            <div className="mt-3 flex flex-wrap gap-1.5">
              {badges.map((badge) => (
                <span key={badge} className={cn("rounded-full px-2.5 py-1 text-xs font-bold", badge.startsWith("Hardcore") ? "bg-ember/20 text-ember-soft" : "bg-ink-900/60 text-cream-300 backdrop-blur-sm")}>
                  {badge}
                </span>
              ))}
            </div>
          </div>
          <div className="flex flex-wrap items-center gap-2.5">
            <button
              type="button"
              onClick={running ? () => stop(world.instanceId) : starting || launch?.blocked ? undefined : () => play(world.instanceId, world.folder)}
              aria-disabled={starting || launch?.blocked}
              title={launch?.blocked ? `${world.instanceName} is already running` : undefined}
              className={cn(
                "relative flex h-14 min-w-[150px] items-center justify-center gap-3 rounded-full px-7 font-display text-[19px] font-extrabold transition-transform duration-200 ease-out-expo active:scale-[0.97]",
                running
                  ? "border border-moss/40 bg-moss/15 text-moss-soft"
                  : "tail wag bg-cream text-ink-900 shadow-[0_12px_40px_-6px_rgba(244,238,228,0.35)] [--tail:30px] hover:scale-[1.03]",
                launch?.blocked && "opacity-50",
              )}
            >
              {starting ? (
                <LoaderCircle size={18} className="animate-spin" aria-hidden="true" />
              ) : running ? (
                <Square size={14} fill="currentColor" strokeWidth={0} aria-hidden="true" />
              ) : (
                <Play size={18} fill="currentColor" strokeWidth={0} aria-hidden="true" />
              )}
              {running ? "Stop" : starting ? view?.label : "Play"}
            </button>
            <Action icon={<FolderOpen size={15} aria-hidden="true" />} label="Open folder" onClick={() => void openWorldFolder(world.instanceId, world.folder).catch(fail("Couldn't open the folder"))} />
            <Action
              icon={busy === "export" ? <LoaderCircle size={15} className="animate-spin" aria-hidden="true" /> : <Upload size={15} aria-hidden="true" />}
              label="Export"
              disabled={busy !== null}
              onClick={() =>
                void run(
                  "export",
                  async () => {
                    const saved = await exportWorld(world.instanceId, world.folder);
                    if (saved) notify({ tone: "success", title: "Exported", body: saved });
                  },
                  "Couldn't export it",
                )
              }
            />
            <Action
              icon={busy === "snapshot" ? <LoaderCircle size={15} className="animate-spin" aria-hidden="true" /> : <RotateCcw size={15} aria-hidden="true" />}
              label="Make a restore point"
              disabled={busy !== null || running || starting}
              onClick={() =>
                void run(
                  "snapshot",
                  async () => {
                    const point = await createRestorePoint(world.instanceId, world.folder);
                    notify({ tone: "success", title: "Restore point made", body: `${point.files} files, ${point.addedBytes > 0 ? `${formatBytes(point.addedBytes)} new` : "nothing new to store"}.` });
                  },
                  "Couldn't make a restore point",
                )
              }
            />
            <Action icon={<Trash2 size={15} aria-hidden="true" />} label="Delete" disabled={running || starting} onClick={() => setConfirm({ kind: "delete" })} />
          </div>
        </div>
      </m.section>

      <m.dl variants={fadeUp} className="grid grid-cols-2 gap-3 md:grid-cols-4">
        {stats.map((stat) => (
          <div key={stat.label} className="rounded-2xl border border-cream/7 bg-ink-800 px-4 py-3.5">
            <dt className="flex items-center gap-1.5 text-xs font-semibold text-mute">
              {stat.icon}
              {stat.label}
            </dt>
            <dd className="mt-1 font-display text-xl font-bold tabular-nums">{stat.value}</dd>
          </div>
        ))}
      </m.dl>

      <m.div variants={fadeUp} className="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_340px]">
        <section aria-labelledby="world-journal" className="flex min-w-0 flex-col gap-4">
          <h2 id="world-journal" className="font-display text-xl font-bold">
            Journal
          </h2>
          <NoteComposer placeholder={`A note about ${world.name}…`} autoFocus={false} onSave={async (text) => void (await addNote(world.instanceId, world.folder, text))} />
          {journal.entries === null ? (
            <Loading label="Loading the journal" className="flex flex-col gap-3">
              <Skeleton className="h-20 w-full rounded-2xl" />
              <Skeleton className="h-14 w-full rounded-2xl" />
            </Loading>
          ) : journal.entries.length === 0 ? (
            <p className="rounded-2xl border border-dashed border-cream/12 px-5 py-8 text-center text-sm leading-relaxed text-mute">
              Nothing here yet. Play the world and Wisp writes down each session, with the screenshots you take and a restore point when you quit.
            </p>
          ) : (
            <>
              <Timeline
                entries={journal.entries}
                home={home}
                now={now}
                lanes={false}
                onOpenScreenshot={(_, file) => {
                  const index = shots?.findIndex((shot) => shot.file === file) ?? -1;
                  if (index >= 0) setLightbox(index);
                }}
              />
              {journal.more && (
                <button type="button" onClick={journal.loadMore} disabled={journal.loadingMore} className="self-center rounded-full border border-cream/15 px-4 py-2 text-sm font-bold text-cream-300 hover:border-cream/35 hover:text-cream">
                  {journal.loadingMore ? "Loading…" : "Show older"}
                </button>
              )}
            </>
          )}
        </section>

        <aside className="flex flex-col gap-6 lg:sticky lg:top-24">
          <section aria-labelledby="world-restore" className="flex flex-col gap-3">
            <h2 id="world-restore" className="flex items-baseline gap-2 font-display text-xl font-bold">
              Restore points
              {restorePoints && restorePoints.length > 0 && <span className="font-sans text-sm font-semibold text-mute">{restorePoints.length}</span>}
            </h2>
            {restorePoints === null ? (
              <Skeleton className="h-16 w-full rounded-2xl" />
            ) : restorePoints.length === 0 ? (
              <p className="text-sm leading-relaxed text-mute">None yet. Wisp keeps one each time you finish playing (change that in Settings → Journal), or make one now.</p>
            ) : (
              <ul className="flex flex-col gap-2">
                {restorePoints.map((point) => (
                  <li key={point.id} className="group flex items-center gap-3 rounded-2xl border border-cream/7 bg-ink-800 px-3.5 py-2.5">
                    <code className="rounded-md bg-cream/8 px-1.5 py-0.5 font-mono text-[11px] font-semibold text-cream-300">{point.id.slice(0, 7)}</code>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-[13px] font-bold">{point.label ?? (point.reason === "auto" ? "After playing" : point.reason === "manual" ? "Made by you" : "Before a restore")}</p>
                      <p className="text-xs text-mute">
                        {timeAgo(point.createdAt, now)} · {formatBytes(point.totalBytes)}
                      </p>
                    </div>
                    <button
                      type="button"
                      onClick={() => setConfirm({ kind: "restore", point })}
                      disabled={running || starting}
                      className="rounded-full px-2.5 py-1 text-xs font-bold text-cream-300 transition-colors hover:bg-cream/8 hover:text-cream disabled:opacity-40"
                    >
                      Restore
                    </button>
                    <button
                      type="button"
                      aria-label={`Delete restore point ${point.id.slice(0, 7)}`}
                      onClick={() => setConfirm({ kind: "drop", point })}
                      className="rounded-full p-1.5 text-mute opacity-0 transition-opacity hover:bg-cream/8 hover:text-cream group-hover:opacity-100 focus-visible:opacity-100"
                    >
                      <Archive size={13} aria-hidden="true" />
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section aria-labelledby="world-shots" className="flex flex-col gap-3">
            <h2 id="world-shots" className="flex items-baseline gap-2 font-display text-xl font-bold">
              Screenshots
              {shots && shots.length > 0 && <span className="font-sans text-sm font-semibold text-mute">{shots.length}</span>}
            </h2>
            {shots === null ? (
              <Skeleton className="h-24 w-full rounded-2xl" />
            ) : shots.length === 0 ? (
              <p className="flex items-start gap-2 text-sm leading-relaxed text-mute">
                <Images size={15} className="mt-0.5 shrink-0" aria-hidden="true" />
                Press F2 in this world to take one. Wisp files them under the world you were in.
              </p>
            ) : (
              <ul className="grid grid-cols-2 gap-2">
                {shots.slice(0, 12).map((shot, index) => (
                  <li key={shot.id}>
                    <button type="button" onClick={() => setLightbox(index)} className="block w-full overflow-hidden rounded-xl ring-1 ring-cream/8 transition-transform hover:scale-[1.03]">
                      <img src={thumbUrl(shot.path, 320, shot.takenAt)} alt="" loading="lazy" className="aspect-video w-full bg-ink-700 object-cover" />
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </aside>
      </m.div>

      {shots && (
        <Lightbox
          shots={shots}
          index={lightbox}
          onIndex={setLightbox}
          onDeleted={(gone) => setShots((current) => current?.filter((shot) => shot.id !== gone.id) ?? null)}
          onOpenWorld={openWorld}
        />
      )}

      <Dialog
        open={confirm !== null}
        onClose={() => busy === null && setConfirm(null)}
        title={
          confirm?.kind === "delete"
            ? `Delete ${world.name}?`
            : confirm?.kind === "restore"
              ? `Restore ${world.name}?`
              : confirm?.kind === "drop"
                ? `Delete restore point ${confirm.point.id.slice(0, 7)}?`
                : ""
        }
        footer={
          <>
            <Button variant="secondary" disabled={busy !== null} onClick={() => setConfirm(null)}>
              Cancel
            </Button>
            {confirm?.kind === "delete" && (
              <Button
                icon={Trash2}
                disabled={busy !== null}
                onClick={() =>
                  void run(
                    "delete",
                    async () => {
                      await deleteWorld(world.instanceId, world.folder);
                      notify({ tone: "success", title: `${world.name} is in the Trash`, body: "Its restore points are kept, so it can come back." });
                      setConfirm(null);
                      onBack();
                    },
                    "Couldn't delete it",
                  )
                }
              >
                Move to the Trash
              </Button>
            )}
            {confirm?.kind === "restore" && (
              <Button
                icon={RotateCcw}
                disabled={busy !== null}
                onClick={() =>
                  void run(
                    "restore",
                    async () => {
                      await restoreWorld(confirm.point.id);
                      notify({ tone: "success", title: `${world.name} is restored`, body: "The world as it was is kept as a restore point too." });
                      setConfirm(null);
                    },
                    "Couldn't restore it",
                  )
                }
              >
                {busy === "restore" ? "Restoring…" : "Restore"}
              </Button>
            )}
            {confirm?.kind === "drop" && (
              <Button
                icon={Archive}
                disabled={busy !== null}
                onClick={() =>
                  void run(
                    "drop",
                    async () => {
                      const freed = await deleteRestorePoint(confirm.point.id);
                      notify({ tone: "success", title: "Restore point deleted", body: freed > 0 ? `${formatBytes(freed)} freed.` : "Its files are still used by other restore points." });
                      setConfirm(null);
                    },
                    "Couldn't delete it",
                  )
                }
              >
                Delete it
              </Button>
            )}
          </>
        }
      >
        <p className="text-sm leading-relaxed text-cream-300">
          {confirm?.kind === "delete" &&
            "It goes to your computer's Trash, so you can still get it back from there. Wisp keeps its restore points, which can bring it back too."}
          {confirm?.kind === "restore" &&
            `Back to ${new Date(confirm.point.createdAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}. Wisp keeps the world as it is now as a restore point first, so this can be undone.`}
          {confirm?.kind === "drop" && "This can't be undone. Files other restore points still use are kept."}
        </p>
      </Dialog>
    </Page>
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

function Action({ icon, label, onClick, disabled = false }: { icon: ReactNode; label: string; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="flex h-11 items-center gap-2 rounded-full border border-cream/18 bg-ink-900/50 px-4 text-sm font-bold text-cream-200 backdrop-blur-sm transition-colors enabled:hover:border-cream/40 enabled:hover:text-cream disabled:opacity-40"
    >
      {icon}
      {label}
    </button>
  );
}
