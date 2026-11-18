import {
  ArrowDown,
  ArrowUp,
  Check,
  Cloud,
  CloudDownload,
  CopyPlus,
  FolderOpen,
  FolderSync,
  HardDrive,
  Hourglass,
  Laptop,
  LoaderCircle,
  Monitor,
  MonitorPlay,
  MonitorSmartphone,
  Power,
  RefreshCw,
  TriangleAlert,
  Unplug,
  type LucideIcon,
} from "lucide-react";
import { m } from "motion/react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Ghost } from "../components/brand/Ghost";
import { Sparkle } from "../components/brand/Sparkle";
import { Button } from "../components/ui/Button";
import { Dialog } from "../components/ui/Dialog";
import { Page, Panel } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { ProgressBar } from "../components/ui/ProgressBar";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { Toggle } from "../components/ui/Toggle";
import {
  bringSyncItem,
  getSync,
  getSyncFolders,
  onSyncActivity,
  onSyncChange,
  openSyncFolder,
  pickSyncFolder,
  removeSyncItem,
  resolveSync,
  startSync,
  stopSync,
  syncNow,
} from "../data/api";
import type { CloudFolder, FolderKind, SyncActivity, SyncItem, SyncOverview, SyncPrefs, SyncResolution } from "../data/types";
import { cn } from "../lib/cn";
import { formatBytes, timeAgo } from "../lib/format";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

const KIND_ICON: Record<FolderKind, LucideIcon> = {
  icloud: Cloud,
  onedrive: Cloud,
  "google-drive": HardDrive,
  dropbox: FolderSync,
  box: FolderSync,
  nextcloud: Cloud,
  syncthing: RefreshCw,
  pcloud: Cloud,
  folder: FolderOpen,
};

const WHAT_SYNCS: { key: keyof SyncPrefs; label: string; description: string }[] = [
  { key: "worlds", label: "Worlds", description: "Every save, with a restore point each time it syncs. Worlds are never merged: if one changes on two computers, you choose." },
  { key: "instances", label: "Instances and mods", description: "Version, loader, mods and their settings. Mods Modrinth has download fresh on each computer; only ones you added yourself are copied." },
  { key: "options", label: "Options and keybinds", description: "Controls, sound, chat, language and other options, merged setting by setting." },
  { key: "servers", label: "Server list", description: "The multiplayer servers you've added, merged by address." },
  { key: "resourcePacks", label: "Resource packs and shaders", description: "Packs in each instance's resource pack and shader folders." },
  { key: "screenshots", label: "Screenshots", description: "These add up quickly, so they're off unless you want them." },
];

const STAYS_LOCAL = ["Resolution and fullscreen", "Render distance", "Graphics, smooth lighting and particles", "GUI scale and brightness", "Sodium, Iris and Distant Horizons settings"];

function message(failure: unknown): string {
  return failure instanceof Error ? failure.message : String(failure);
}

/** "Sam's PC" → "Sam's PC's", "Alexis" → "Alexis'". */
function possessive(name: string): string {
  return name.endsWith("s") ? `${name}'` : `${name}'s`;
}

export function SyncPage() {
  const { settings, updateSettings, notify, now, refreshHome } = useApp();
  const [overview, setOverview] = useState<SyncOverview | null>(null);
  const [folders, setFolders] = useState<CloudFolder[] | null>(null);
  const [activity, setActivity] = useState<SyncActivity | null>(null);
  /** The action under way: a folder being connected, "sync", "off", or an item key. */
  const [working, setWorking] = useState<string | null>(null);
  const [removing, setRemoving] = useState<SyncItem | null>(null);
  const [confirmOff, setConfirmOff] = useState(false);

  useEffect(() => {
    let active = true;
    getSync().then((current) => active && setOverview(current), () => active && setOverview(null));
    const stopChanges = onSyncChange((next) => setOverview(next));
    const stopActivity = onSyncActivity((next) => setActivity(next));
    return () => {
      active = false;
      stopChanges();
      stopActivity();
    };
  }, []);

  const on = overview?.folder != null;
  useEffect(() => {
    if (overview === null || on) return;
    let active = true;
    getSyncFolders().then((found) => active && setFolders(found), () => active && setFolders([]));
    return () => {
      active = false;
    };
  }, [overview, on]);

  const act = useCallback(
    async (label: string, work: () => Promise<SyncOverview>, failureTitle: string) => {
      setWorking(label);
      try {
        setOverview(await work());
        void refreshHome();
      } catch (failure) {
        notify({ tone: "info", title: failureTitle, body: message(failure) });
      } finally {
        setWorking(null);
      }
    },
    [notify, refreshHome],
  );

  const connect = (path: string) => act(`folder:${path}`, () => startSync(path), "Couldn't turn sync on");
  const pickFolder = async () => {
    const path = await pickSyncFolder().catch(() => null);
    if (path) await connect(path);
  };

  return (
    <Page className="max-w-[1180px]">
      <PageHeader
        title="Cloud sync"
        subtitle="Play on any computer and pick up where you left off. Everything stays in your own cloud storage, never on Wisp's servers."
      />

      <m.div variants={fadeUp} className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
        <div className="flex min-w-0 flex-col gap-5">
          {overview === null ? (
            <section className="wisp-surface rounded-[24px] p-6">
              <Loading label="Loading sync" className="flex flex-col gap-3">
                <Skeleton className="h-6 w-56 rounded-full" />
                <Skeleton className="h-3.5 w-80 rounded-full" />
                <Skeleton className="mt-3 h-28 w-full rounded-[18px]" />
              </Loading>
            </section>
          ) : on ? (
            <StatusCard overview={overview} activity={activity} now={now} working={working} onSyncNow={() => act("sync", syncNow, "Couldn't sync")} />
          ) : (
            <ChooseFolder folders={folders} working={working} onConnect={connect} onPick={pickFolder} />
          )}

          {on && overview && (
            <Attention
              overview={overview}
              now={now}
              working={working}
              onResolve={(item, resolution) => act(item.key, () => resolveSync(item.key, resolution), `Couldn't settle ${item.name}`)}
              onBring={(item) => act(item.key, () => bringSyncItem(item.key), `Couldn't bring ${item.name} here`)}
              onRetry={() => act("sync", syncNow, "Couldn't sync")}
            />
          )}

          <Panel title="What syncs" description="Choose what follows you from computer to computer. Each computer chooses for itself.">
            {settings === null ? (
              <Loading label="Loading sync choices" className="flex flex-col gap-4 py-3">
                {WHAT_SYNCS.map((item) => (
                  <div key={item.key} className="flex items-center justify-between gap-6">
                    <div className="flex flex-1 flex-col gap-2">
                      <Skeleton className="h-3.5 w-32 rounded-full" />
                      <Skeleton className="h-3 w-56 rounded-full" />
                    </div>
                    <Skeleton className="h-7 w-12 rounded-full" />
                  </div>
                ))}
              </Loading>
            ) : (
              <div className="divide-y divide-cream/7">
                {WHAT_SYNCS.map((item) => (
                  <Toggle
                    key={item.key}
                    label={item.label}
                    description={item.description}
                    checked={settings.sync[item.key]}
                    onChange={(value) => updateSettings((current) => ({ sync: { ...current.sync, [item.key]: value } }))}
                  />
                ))}
              </div>
            )}
          </Panel>

          {on && overview && <InTheCloud overview={overview} now={now} working={working} onRemove={setRemoving} />}
        </div>

        <div className="flex flex-col gap-5">
          <Computers overview={overview} now={now} />

          <Panel title="Stays on each computer" description="These depend on each computer's screen and graphics, so every computer keeps its own.">
            <ul className="mt-3 flex flex-col gap-2">
              {STAYS_LOCAL.map((item) => (
                <li key={item} className="flex items-center gap-2.5 text-sm text-cream-300">
                  <Sparkle className="size-2.5 text-ember" />
                  {item}
                </li>
              ))}
            </ul>
          </Panel>

          {on && (
            <Panel title="Turn off sync" description="Nothing is deleted: your worlds stay on this computer, and the folder in the cloud stays as it is." icon={<Power size={18} aria-hidden="true" />}>
              <Button variant="secondary" className="mt-3" disabled={working !== null} onClick={() => setConfirmOff(true)}>
                Turn off sync here
              </Button>
            </Panel>
          )}
        </div>
      </m.div>

      <Dialog
        open={confirmOff}
        onClose={() => setConfirmOff(false)}
        title="Turn off sync on this computer?"
        description={`Worlds you play here will stop going to ${overview?.folder?.label ?? "the cloud"}. Your other computers keep syncing.`}
        footer={
          <>
            <Button variant="secondary" onClick={() => setConfirmOff(false)}>
              Keep syncing
            </Button>
            <Button
              onClick={() => {
                setConfirmOff(false);
                void act("off", stopSync, "Couldn't turn sync off");
              }}
            >
              Turn off
            </Button>
          </>
        }
      >
        <p className="text-sm leading-relaxed text-cream-300">You can turn it back on any time by picking the same folder. Wisp will carry on where it left off.</p>
      </Dialog>

      <Dialog
        open={removing !== null}
        onClose={() => setRemoving(null)}
        title={`Take ${removing?.name ?? "this world"} out of the cloud?`}
        description="Every computer keeps the copy it has. They just stop syncing it."
        footer={
          <>
            <Button variant="secondary" onClick={() => setRemoving(null)}>
              Keep it synced
            </Button>
            <Button
              onClick={() => {
                const item = removing;
                setRemoving(null);
                if (item) void act(item.key, () => removeSyncItem(item.key), `Couldn't take ${item.name} out`);
              }}
            >
              Take it out
            </Button>
          </>
        }
      >
        <p className="text-sm leading-relaxed text-cream-300">Its history in the cloud is cleared out after a while. The restore points on each computer stay.</p>
      </Dialog>
    </Page>
  );
}

// ── Choosing a folder ──

function ChooseFolder({ folders, working, onConnect, onPick }: { folders: CloudFolder[] | null; working: string | null; onConnect: (path: string) => void; onPick: () => void }) {
  const joining = folders?.find((folder) => folder.existing);
  return (
    <section aria-labelledby="sync-choose" className="wisp-surface relative overflow-hidden rounded-[24px] p-6">
      <div aria-hidden="true" className="absolute -right-16 -top-20 size-64 rounded-full bg-[radial-gradient(circle,rgba(244,238,228,0.09),transparent_65%)]" />
      <div className="relative flex items-start gap-4">
        <div className="min-w-0 flex-1">
          <h2 id="sync-choose" className="font-display text-[22px] font-bold tracking-[-0.01em]">
            {joining ? "Your other computer is already syncing" : "Choose where to keep your worlds"}
          </h2>
          <p className="mt-1 max-w-[52ch] text-sm leading-relaxed text-mute">
            {joining
              ? `Pick ${joining.label} to bring its worlds and instances here, and keep both computers in step.`
              : "Wisp uses a cloud folder that's already on this computer. Nothing signs in, and you can switch later."}
          </p>
        </div>
        <Ghost size={52} floating className="hidden shrink-0 drop-shadow-[0_0_16px_rgba(244,238,228,0.25)] sm:block" />
      </div>

      {folders === null ? (
        <Loading label="Looking for cloud folders" className="relative mt-5 grid gap-3 md:grid-cols-2">
          <Skeleton className="h-[132px] rounded-[18px]" />
          <Skeleton className="h-[132px] rounded-[18px]" />
        </Loading>
      ) : folders.length === 0 ? (
        <div className="relative mt-5 rounded-[18px] border border-dashed border-cream/15 p-5">
          <p className="text-[15px] font-bold">No cloud folders found</p>
          <p className="mt-1 max-w-[56ch] text-sm leading-relaxed text-mute">
            Install iCloud Drive, OneDrive, Google Drive or Dropbox and sign in to it, then come back. Or pick any folder that another app keeps in step, like Syncthing or a shared drive.
          </p>
        </div>
      ) : (
        <ul aria-label="Cloud folders on this computer" className="relative mt-5 grid gap-3 md:grid-cols-2">
          {folders.map((folder) => {
            const Icon = KIND_ICON[folder.kind];
            const busy = working === `folder:${folder.path}`;
            const existing = folder.existing;
            return (
              <li key={folder.path}>
                <div className={cn("flex h-full flex-col gap-3 rounded-[18px] border bg-ink-900/40 p-4", existing ? "border-moss/40" : "border-cream/8")}>
                  <div className="flex items-start gap-3">
                    <span className="grid size-10 shrink-0 place-items-center rounded-[12px] bg-cream/7 text-cream-300">
                      <Icon size={20} strokeWidth={1.8} aria-hidden="true" />
                    </span>
                    <span className="flex min-w-0 flex-col">
                      <span className="text-[15px] font-bold">{folder.label}</span>
                      <span className="truncate text-xs text-mute" title={folder.path}>
                        {existing
                          ? `${existing.worlds} ${existing.worlds === 1 ? "world" : "worlds"} and ${existing.instances} ${existing.instances === 1 ? "instance" : "instances"} from ${existing.devices.join(", ") || "another computer"}`
                          : folder.kind === "folder"
                            ? folder.path
                            : "On this computer"}
                      </span>
                    </span>
                  </div>
                  <Button
                    variant={folder.path === (joining ?? folders[0]).path ? "primary" : "secondary"}
                    icon={busy ? LoaderCircle : undefined}
                    className={cn("mt-auto self-start", busy && "[&_svg]:animate-spin")}
                    disabled={working !== null}
                    onClick={() => onConnect(folder.path)}
                  >
                    {busy ? "Setting up…" : existing ? `Sync with ${folder.label}` : `Use ${folder.label}`}
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <div className="relative mt-5 flex flex-wrap items-center gap-x-4 gap-y-2 border-t border-cream/8 pt-4">
        <button type="button" onClick={onPick} disabled={working !== null} className="text-sm font-bold text-cream-300 underline decoration-cream/25 underline-offset-4 transition-colors hover:text-cream hover:decoration-cream disabled:opacity-50">
          Choose another folder…
        </button>
        <span className="text-xs text-mute">A Syncthing folder, a network drive, or anything else kept in step.</span>
      </div>
    </section>
  );
}

// ── Status ──

function StatusCard({ overview, activity, now, working, onSyncNow }: { overview: SyncOverview; activity: SyncActivity | null; now: number; working: string | null; onSyncNow: () => void }) {
  const folder = overview.folder!;
  const Icon = KIND_ICON[folder.kind];
  const worlds = overview.items.filter((item) => item.kind === "world" && item.state.kind === "synced").length;
  const instances = overview.items.filter((item) => item.kind === "instance" && item.state.kind === "synced").length;
  const syncing = working === "sync" || activity !== null;
  const trouble = overview.phase === "unreachable" || overview.phase === "failed";

  let line: string;
  if (activity) line = `${activity.direction === "up" ? "Sending" : "Bringing"} ${activity.name}: ${formatBytes(activity.done)} of ${formatBytes(activity.total)}`;
  else if (working === "sync") line = "Checking for changes…";
  else if (overview.message) line = overview.message;
  else if (overview.lastSynced) line = `Everything's up to date. Last synced ${timeAgo(overview.lastSynced, now)}.`;
  else line = "Getting started…";

  return (
    <section aria-labelledby="sync-status" className="wisp-surface relative overflow-hidden rounded-[24px] p-6">
      <div aria-hidden="true" className="absolute -right-16 -top-20 size-64 rounded-full bg-[radial-gradient(circle,rgba(244,238,228,0.09),transparent_65%)]" />
      <div className="relative flex items-start gap-4">
        <span className={cn("grid size-12 shrink-0 place-items-center rounded-[15px]", trouble ? "bg-coral/15 text-coral" : "bg-cream/8 text-cream-300")}>
          {trouble ? <Unplug size={22} aria-hidden="true" /> : <Icon size={22} aria-hidden="true" />}
        </span>
        <div className="min-w-0 flex-1">
          <h2 id="sync-status" className="font-display text-[22px] font-bold tracking-[-0.01em]">
            {trouble ? `Can't reach ${folder.label}` : `Syncing with ${folder.label}`}
          </h2>
          <p className="mt-0.5 text-sm text-mute" aria-live="polite">
            {line}
          </p>
        </div>
        <Ghost size={52} floating className="hidden shrink-0 drop-shadow-[0_0_16px_rgba(244,238,228,0.25)] sm:block" />
      </div>

      {activity && (
        <ProgressBar
          className="relative mt-5"
          value={activity.total > 0 ? activity.done / activity.total : null}
          tone={activity.direction === "up" ? "amber" : "moss"}
          label={`${activity.direction === "up" ? "Sending" : "Bringing"} ${activity.name}`}
        />
      )}

      <div className="relative mt-5 flex flex-wrap items-center gap-2.5 border-t border-cream/8 pt-4">
        <Button variant="secondary" icon={RefreshCw} disabled={syncing} className={cn(working === "sync" && "[&_svg]:animate-spin")} onClick={onSyncNow}>
          {trouble ? "Try again" : "Sync now"}
        </Button>
        <Button variant="secondary" icon={FolderOpen} onClick={() => void openSyncFolder()}>
          Show folder
        </Button>
        <span className="ml-auto text-xs text-mute">
          {worlds} {worlds === 1 ? "world" : "worlds"}, {instances} {instances === 1 ? "instance" : "instances"}
          {overview.usage !== null && ` · ${formatBytes(overview.usage)} in ${folder.label}`}
        </span>
      </div>
    </section>
  );
}

// ── Decisions ──

function Attention({
  overview,
  now,
  working,
  onResolve,
  onBring,
  onRetry,
}: {
  overview: SyncOverview;
  now: number;
  working: string | null;
  onResolve: (item: SyncItem, resolution: SyncResolution) => void;
  onBring: (item: SyncItem) => void;
  onRetry: () => void;
}) {
  const conflicts = overview.items.filter((item) => item.state.kind === "conflict");
  const offered = overview.items.filter((item) => item.state.kind === "cloud-only");
  const failed = overview.items.filter((item) => item.state.kind === "failed");
  if (conflicts.length + offered.length + failed.length === 0) return null;
  const instanceName = (item: SyncItem) => overview.items.find((other) => other.kind === "instance" && other.instanceId !== null && other.instanceId === item.instanceId)?.name;

  return (
    <Panel
      title={conflicts.length > 0 ? "Choose which copy to keep" : "Waiting for you"}
      description={conflicts.length > 0 ? "These worlds changed here and on another computer. Wisp never mixes two copies of a world, so pick one, or keep both as separate worlds." : undefined}
      icon={<TriangleAlert size={18} aria-hidden="true" />}
    >
      <ul className="mt-3 flex flex-col gap-2.5">
        {conflicts.map((item) => {
          if (item.state.kind !== "conflict") return null;
          const state = item.state;
          const busy = working === item.key;
          return (
            <li key={item.key} className="rounded-[18px] bg-cream/4 p-4">
              <p className="text-[15px] font-bold">
                {item.name}
                {instanceName(item) && <span className="font-normal text-mute"> in {instanceName(item)}</span>}
              </p>
              <p className="mt-0.5 text-[13px] text-mute">
                Changed here{state.hereAt ? ` ${timeAgo(state.hereAt, now)}` : ""} and on {state.choices.map((choice) => `${choice.device} ${timeAgo(choice.at, now)}`).join(" and on ")}.
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button variant="secondary" icon={Laptop} disabled={working !== null} onClick={() => onResolve(item, { keep: "here" })}>
                  Keep this computer's
                </Button>
                {state.choices.map((choice) => (
                  <Button key={choice.commit} variant="secondary" icon={CloudDownload} disabled={working !== null} onClick={() => onResolve(item, { keep: "cloud", commit: choice.commit })}>
                    Use {possessive(choice.device)}
                  </Button>
                ))}
                <Button
                  variant="secondary"
                  icon={busy ? LoaderCircle : CopyPlus}
                  className={cn(busy && "[&_svg]:animate-spin")}
                  disabled={working !== null}
                  onClick={() => onResolve(item, { keep: "both", commit: state.choices[0].commit })}
                >
                  Keep both
                </Button>
              </div>
            </li>
          );
        })}
        {offered.map((item) => {
          if (item.state.kind !== "cloud-only") return null;
          const state = item.state;
          const busy = working === item.key;
          return (
            <li key={item.key} className="flex flex-wrap items-center gap-3 rounded-[18px] bg-cream/4 p-4">
              <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-cream/7 text-cream-300">
                <CloudDownload size={17} aria-hidden="true" />
              </span>
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-[15px] font-bold">{item.name}</span>
                <span className="text-[13px] text-mute">
                  {item.kind === "instance" ? "Instance" : "World"} from {state.device}, {timeAgo(state.at, now)}
                  {state.bytes > 0 && ` · ${formatBytes(state.bytes)}`}
                  {state.needsInstance && ". Its instance isn't on this computer: turn on Instances and mods on both, and it comes along."}
                </span>
              </span>
              {!state.needsInstance && (
                <Button variant="secondary" icon={busy ? LoaderCircle : ArrowDown} className={cn(busy && "[&_svg]:animate-spin")} disabled={working !== null} onClick={() => onBring(item)}>
                  {busy ? "Bringing it here…" : "Bring it here"}
                </Button>
              )}
            </li>
          );
        })}
        {failed.map((item) => (
          <li key={item.key} className="flex flex-wrap items-center gap-3 rounded-[18px] bg-coral/8 p-4">
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-[15px] font-bold">{item.name} didn't sync</span>
              <span className="text-[13px] text-mute">{item.state.kind === "failed" ? item.state.message : ""}</span>
            </span>
            <Button variant="secondary" icon={RefreshCw} disabled={working !== null} onClick={onRetry}>
              Try again
            </Button>
          </li>
        ))}
      </ul>
    </Panel>
  );
}

// ── Everything in the cloud ──

const STATE_TEXT: Record<SyncItem["state"]["kind"], { icon: LucideIcon; tone: string }> = {
  synced: { icon: Check, tone: "text-moss" },
  pending: { icon: ArrowUp, tone: "text-amber" },
  "cloud-newer": { icon: ArrowDown, tone: "text-sky" },
  conflict: { icon: TriangleAlert, tone: "text-coral" },
  "in-use": { icon: MonitorPlay, tone: "text-sky" },
  waiting: { icon: Hourglass, tone: "text-amber" },
  "cloud-only": { icon: Cloud, tone: "text-mute" },
  removed: { icon: Unplug, tone: "text-mute" },
  failed: { icon: TriangleAlert, tone: "text-coral" },
};

function describeItem(item: SyncItem, now: number, here: string | null): string {
  const state = item.state;
  switch (state.kind) {
    case "synced":
      return `Synced ${timeAgo(state.at, now)}${state.device && state.device !== here ? `, last changed on ${state.device}` : ""}`;
    case "pending":
      return "Goes up when it can";
    case "cloud-newer":
      return `Newer copy from ${state.device} comes down before you play`;
    case "conflict":
      return "Changed in two places";
    case "in-use":
      return `Being played on ${state.device}`;
    case "waiting":
      return `${state.files} ${state.files === 1 ? "file" : "files"} still arriving`;
    case "cloud-only":
      return `Only in the cloud, from ${state.device}`;
    case "removed":
      return `Taken out of the cloud on ${state.device}`;
    case "failed":
      return state.message;
  }
}

function InTheCloud({ overview, now, working, onRemove }: { overview: SyncOverview; now: number; working: string | null; onRemove: (item: SyncItem) => void }) {
  const worlds = useMemo(() => overview.items.filter((item) => item.kind === "world" && item.state.kind !== "cloud-only").sort((a, b) => a.name.localeCompare(b.name)), [overview.items]);
  const here = overview.devices.find((device) => device.this)?.name ?? null;
  if (worlds.length === 0) return null;
  return (
    <Panel title="Worlds" description="Every world on this computer and where it stands.">
      <ul className="mt-2 divide-y divide-cream/7">
        {worlds.map((item) => {
          const look = STATE_TEXT[item.state.kind];
          const Icon = look.icon;
          return (
            <li key={item.key} className="group flex items-center gap-3 py-2.5">
              <Icon size={15} strokeWidth={2.4} className={cn("shrink-0", look.tone)} aria-hidden="true" />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-sm font-bold">{item.name}</span>
                <span className="truncate text-xs text-mute">{describeItem(item, now, here)}</span>
              </span>
              {item.state.kind === "synced" && (
                <button
                  type="button"
                  disabled={working !== null}
                  onClick={() => onRemove(item)}
                  className="shrink-0 rounded-full px-3 py-1.5 text-xs font-bold text-mute opacity-0 transition-[opacity,color,background-color] hover:bg-cream/6 hover:text-cream focus-visible:opacity-100 group-hover:opacity-100 disabled:opacity-40"
                >
                  Take out of the cloud
                </button>
              )}
            </li>
          );
        })}
      </ul>
    </Panel>
  );
}

// ── Computers ──

function Computers({ overview, now }: { overview: SyncOverview | null; now: number }) {
  const devices = overview?.devices ?? [];
  return (
    <Panel title="Your computers" icon={<MonitorSmartphone size={19} aria-hidden="true" />}>
      {overview === null ? (
        <Loading label="Loading your computers" className="mt-3 flex items-center gap-3 rounded-2xl bg-cream/4 p-3">
          <Skeleton className="size-9 rounded-xl" />
          <div className="flex flex-1 flex-col gap-2">
            <Skeleton className="h-3.5 w-28 rounded-full" />
            <Skeleton className="h-3 w-20 rounded-full" />
          </div>
        </Loading>
      ) : devices.length === 0 ? (
        <p className="mt-2 text-[13px] leading-relaxed text-mute">Once sync is on, every computer that syncs to the same folder shows up here, with what it's playing.</p>
      ) : (
        <ul className="mt-3 flex flex-col gap-2">
          {devices.map((device) => {
            const Icon = device.os === "macOS" ? Laptop : Monitor;
            const playing = device.playing.length > 0;
            return (
              <li key={device.id} className="flex items-center gap-3 rounded-2xl bg-cream/4 p-3">
                <span className="relative grid size-9 shrink-0 place-items-center rounded-xl bg-cream/8 text-cream-300">
                  <Icon size={18} aria-hidden="true" />
                  {playing && <span aria-hidden="true" className="absolute -right-0.5 -top-0.5 size-2.5 rounded-full bg-moss ring-2 ring-ink-800" />}
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm font-bold">{device.name}</span>
                  <span className="truncate text-xs text-mute">
                    {device.this ? "This computer" : playing ? `Playing ${device.playing.join(", ")}` : `Last synced ${timeAgo(device.seen, now)}`}
                    {device.os && ` · ${device.os}`}
                  </span>
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {overview?.folder && devices.length <= 1 && (
        <p className="mt-3 text-[13px] leading-relaxed text-mute">
          To add another computer, open Wisp there and pick {overview.folder.label} in Cloud sync. Your worlds and instances come across by themselves.
        </p>
      )}
    </Panel>
  );
}
