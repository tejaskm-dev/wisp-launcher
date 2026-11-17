import { FolderOpen, LoaderCircle, Package, Pencil, Play, Plus, RotateCw, Square } from "lucide-react";
import { useState } from "react";
import { m } from "motion/react";
import { Ghost } from "../components/brand/Ghost";
import { EditInstanceDialog } from "../components/instances/EditInstanceDialog";
import { InstanceAvatar } from "../components/instances/InstanceAvatar";
import { InstanceLabel } from "../components/instances/VersionTag";
import { describeLaunch, type LaunchView } from "../components/launch/describe";
import { Button } from "../components/ui/Button";
import { EmptyState } from "../components/ui/EmptyState";
import { Page } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { openInstanceFolder } from "../data/api";
import type { Instance } from "../data/types";
import { cn } from "../lib/cn";
import { loaderName, timeAgo } from "../lib/format";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

const GRID = "grid grid-cols-[repeat(auto-fill,minmax(270px,1fr))] gap-4";

export function InstancesPage() {
  const { home, now, openNewInstance, notify, launches, play, stop } = useApp();
  const [editing, setEditing] = useState<{ instance: Instance; open: boolean; session: number } | null>(null);
  const instances = home?.instances ?? null;

  const openFolder = (instance: Instance) =>
    openInstanceFolder(instance.id).catch((failure: unknown) =>
      notify({ tone: "info", title: "Couldn't open the folder", body: failure instanceof Error ? failure.message : String(failure) }),
    );

  return (
    <Page>
      <PageHeader
        title="Instances"
        subtitle="Each instance is its own Minecraft: a game version, a mod loader, and the mods and settings that go with them."
        actions={
          instances && instances.length > 0 ? (
            <Button icon={Plus} onClick={openNewInstance}>
              New instance
            </Button>
          ) : undefined
        }
      />

      {instances === null ? (
        <Loading label="Loading instances" className={GRID}>
          {[0, 1, 2].map((index) => (
            <div key={index} className="wisp-card flex flex-col gap-4 p-4">
              <div className="flex items-center gap-3.5">
                <Skeleton className="size-14 rounded-xl" />
                <div className="flex flex-1 flex-col gap-2">
                  <Skeleton className="h-4 w-3/4 rounded-full" />
                  <Skeleton className="h-3 w-1/2 rounded-full" />
                </div>
              </div>
              <Skeleton className="h-12 w-full rounded-xl" />
              <Skeleton className="h-10 w-full rounded-full" />
            </div>
          ))}
        </Loading>
      ) : instances.length === 0 ? (
        <EmptyState
          art={<BlockStack />}
          title="No instances yet"
          body="Make one to pick a Minecraft version and, if you like, a mod loader. Keep as many as you want, each with its own mods."
          actions={
            <Button icon={Plus} onClick={openNewInstance}>
              New instance
            </Button>
          }
        />
      ) : (
        <m.ul variants={fadeUp} className={GRID}>
          {instances.map((instance) => (
            <li key={instance.id} className="min-w-0">
              <InstanceCard
                instance={instance}
                now={now}
                view={describeLaunch(launches[instance.id], now)}
                onPlay={() => play(instance.id)}
                onStop={() => stop(instance.id)}
                onOpenFolder={() => void openFolder(instance)}
                onEdit={() => setEditing((current) => ({ instance, open: true, session: (current?.session ?? 0) + 1 }))}
              />
            </li>
          ))}
          <li>
            <button
              type="button"
              onClick={openNewInstance}
              className="group flex h-full min-h-[200px] w-full flex-col items-center justify-center gap-2.5 rounded-[22px] border-[1.5px] border-dashed border-cream/18 bg-cream/2 text-[15px] font-semibold text-cream-200 transition-colors hover:border-cream/40 hover:bg-cream/5"
            >
              <span className="grid size-12 place-items-center rounded-full bg-cream/8 transition-transform duration-300 ease-out-expo group-hover:rotate-90 group-hover:scale-110">
                <Plus size={22} aria-hidden="true" />
              </span>
              New instance
            </button>
          </li>
        </m.ul>
      )}
      {editing && (
        <EditInstanceDialog
          key={editing.session}
          instance={editing.instance}
          open={editing.open}
          onClose={() => setEditing((current) => (current ? { ...current, open: false } : null))}
        />
      )}
    </Page>
  );
}

interface InstanceCardProps {
  instance: Instance;
  now: number;
  view: LaunchView;
  onPlay: () => void;
  onStop: () => void;
  onOpenFolder: () => void;
  onEdit: () => void;
}

function InstanceCard({ instance, now, view, onPlay, onStop, onOpenFolder, onEdit }: InstanceCardProps) {
  const busy = view.kind === "busy";
  return (
    <article className="wisp-card lift lit group flex h-full flex-col gap-4 p-4">
      <div className="flex items-center gap-3.5">
        <InstanceAvatar instance={instance} className="size-14 text-2xl transition-transform duration-500 ease-out-expo group-hover:-rotate-6 group-hover:scale-105" />
        <div className="min-w-0 flex-1">
          <h3 className="truncate font-display text-lg font-bold leading-tight">{instance.name}</h3>
          <p className="mt-0.5 truncate text-[13px] text-mute">
            <InstanceLabel instance={instance} />
          </p>
          {instance.modpack && (
            <p className="mt-0.5 flex items-center gap-1 truncate text-xs text-mute-2">
              <Package size={11} aria-hidden="true" />
              {instance.modpack.title} {instance.modpack.versionNumber}
            </p>
          )}
        </div>
        <button
          type="button"
          onClick={onEdit}
          aria-label={`Edit ${instance.name}`}
          title="Edit"
          className="grid size-9 shrink-0 place-items-center self-start rounded-full text-mute transition-colors hover:bg-cream/8 hover:text-cream"
        >
          <Pencil size={15} aria-hidden="true" />
        </button>
      </div>

      <dl className="grid grid-cols-2 gap-2 text-xs">
        <div className="rounded-xl bg-cream/4 px-3 py-2">
          <dt className="text-mute-2">Last played</dt>
          <dd className="mt-0.5 truncate font-semibold">{instance.lastPlayed !== null ? timeAgo(instance.lastPlayed, now) : "Never"}</dd>
        </div>
        <div className="rounded-xl bg-cream/4 px-3 py-2">
          <dt className="text-mute-2">{instance.loaderVersion ? `${loaderName(instance.loader)} version` : "Created"}</dt>
          <dd className="mt-0.5 truncate font-semibold">{instance.loaderVersion ?? timeAgo(instance.created, now)}</dd>
        </div>
      </dl>

      {view.kind === "problem" && <p className="-mb-1 line-clamp-2 text-xs leading-snug text-coral">{view.detail}</p>}
      {view.kind === "running" && <p className="-mb-1 text-xs font-semibold text-moss-soft">{view.detail}</p>}

      <div className="mt-auto flex items-center gap-2">
        <button
          type="button"
          onClick={view.kind === "running" ? onStop : busy ? undefined : onPlay}
          aria-disabled={busy}
          aria-label={view.kind === "running" ? `Stop ${instance.name}` : busy ? `${instance.name}: ${view.label}` : `Play ${instance.name}`}
          className={cn(
            "relative flex h-10 flex-1 items-center justify-center gap-2 rounded-full text-sm font-bold transition-transform duration-200 ease-out-expo",
            view.kind === "running"
              ? "border border-moss/40 bg-moss/12 text-moss-soft hover:bg-moss/20"
              : "tail bg-cream text-ink-900 shadow-[0_10px_30px_-10px_rgba(244,238,228,0.45)] [--tail:20px]",
            !busy && "hover:scale-[1.02] active:scale-[0.97]",
            view.kind !== "running" && !busy && "wag",
          )}
        >
          {busy && view.progress !== null && (
            <span aria-hidden="true" className="absolute inset-0 overflow-hidden rounded-[inherit]">
              <span className="absolute inset-0 origin-left bg-cream-400/80 transition-transform duration-150" style={{ transform: `scaleX(${view.progress})` }} />
            </span>
          )}
          <span className="relative flex items-center gap-2 tabular-nums">
            {busy ? (
              <LoaderCircle size={14} className="animate-spin" aria-hidden="true" />
            ) : view.kind === "running" ? (
              <Square size={11} fill="currentColor" strokeWidth={0} aria-hidden="true" />
            ) : view.kind === "problem" ? (
              <RotateCw size={14} aria-hidden="true" />
            ) : (
              <Play size={14} fill="currentColor" strokeWidth={0} aria-hidden="true" />
            )}
            {view.kind === "running" ? "Stop" : view.label}
          </span>
        </button>
        <button
          type="button"
          onClick={onOpenFolder}
          aria-label={`Open the ${instance.name} folder`}
          title="Open folder"
          className="grid size-10 shrink-0 place-items-center rounded-full border border-cream/15 text-cream-300 transition-colors hover:border-cream/35 hover:bg-cream/8 hover:text-cream"
        >
          <FolderOpen size={17} aria-hidden="true" />
        </button>
      </div>
    </article>
  );
}

/** Decorative: three blocks with the ghost peeking over them. */
function BlockStack() {
  const block = "absolute size-[58px] rounded-[14px] shadow-[inset_0_-4px_0_rgba(0,0,0,0.25),inset_0_3px_0_rgba(255,255,255,0.16),0_14px_30px_-10px_rgba(0,0,0,0.7)]";
  return (
    <div aria-hidden="true" className="relative mb-2 h-[104px] w-[170px]">
      <span className={`${block} left-0 top-[34px] -rotate-8 bg-linear-to-br from-[#6b9b45] to-[#3e6328] transition-transform duration-500 ease-out-expo group-hover:-translate-x-1 group-hover:-rotate-12`} />
      <span className={`${block} right-0 top-[38px] rotate-6 bg-linear-to-br from-[#8c8f96] to-[#55585e] transition-transform duration-500 ease-out-expo group-hover:translate-x-1 group-hover:rotate-12`} />
      <span className={`${block} left-[56px] top-[22px] z-10 bg-linear-to-br from-[#d1ad72] to-[#8d6b3b] transition-transform duration-500 ease-out-expo group-hover:-translate-y-1.5`} />
      <Ghost size={40} floating className="absolute left-[65px] -top-3 z-20 drop-shadow-[0_0_12px_rgba(244,238,228,0.3)]" />
    </div>
  );
}
