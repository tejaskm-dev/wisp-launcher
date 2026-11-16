import { Check, CircleAlert, Earth, LoaderCircle, Package, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { getInstanceContent, getInstanceWorlds, getProject, installProject } from "../../data/api";
import type { InstalledContent, ProjectType, WorldEntry } from "../../data/types";
import { cn } from "../../lib/cn";
import { instanceLabel, timeAgo } from "../../lib/format";
import { useResource } from "../../lib/useResource";
import { useApp } from "../../state/AppContext";
import { InstanceAvatar } from "../instances/InstanceAvatar";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Select } from "../ui/Select";
import { Loading, Skeleton } from "../ui/Skeleton";
import { fits, type Availability } from "./compat";

/** What to install: a project, optionally a specific version of it. */
export interface InstallTarget {
  projectId: string;
  title: string;
  projectType: ProjectType;
  iconUrl: string | null;
  /** A specific version (from the project page); otherwise the newest that fits. */
  version?: { id: string; label: string; loaders: string[]; gameVersions: string[] };
}

const NOUN: Partial<Record<ProjectType, string>> = { mod: "mod", resourcepack: "resource pack", shader: "shader", world: "world" };

type RowState = { phase: "idle" } | { phase: "pending" } | { phase: "done"; extras: string[]; note?: string } | { phase: "error"; message: string };

const failure = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** Mount it fresh for each use (App keys it). */
export function InstallDialog({ target, open, onClose }: { target: InstallTarget; open: boolean; onClose: () => void }) {
  if (target.projectType === "modpack") return <ModpackInstall target={target} open={open} onClose={onClose} />;
  if (target.projectType === "datapack") return <AddToWorld target={target} open={open} onClose={onClose} />;
  return <AddToInstance target={target} open={open} onClose={onClose} />;
}

function ProjectHeader({ target, note }: { target: InstallTarget; note: string }) {
  return (
    <div className="flex items-center gap-3.5 rounded-[18px] bg-cream/4 p-3.5">
      {target.iconUrl ? (
        <img src={target.iconUrl} alt="" className="size-12 shrink-0 rounded-xl bg-ink-700 object-cover" />
      ) : (
        <span className="grid size-12 shrink-0 place-items-center rounded-xl bg-ink-700 text-cream-300">
          <Package size={20} aria-hidden="true" />
        </span>
      )}
      <div className="min-w-0">
        <p className="truncate font-display text-lg font-bold">{target.title}</p>
        <p className="truncate text-xs text-mute">{note}</p>
      </div>
    </div>
  );
}

function ModpackInstall({ target, open, onClose }: { target: InstallTarget; open: boolean; onClose: () => void }) {
  const { startModpackInstall } = useApp();
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Install modpack"
      description="A modpack becomes an instance of its own, with every mod and setting it comes with."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            icon={Plus}
            onClick={() => {
              startModpackInstall(target);
              onClose();
            }}
          >
            Install as new instance
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <ProjectHeader target={target} note={target.version ? `Version ${target.version.label}` : "Newest release"} />
        <p className="text-[13px] leading-relaxed text-mute">
          Big packs download hundreds of mods, which can take a few minutes. Progress shows at the bottom right, and you
          can keep using Wisp meanwhile.
        </p>
      </div>
    </Dialog>
  );
}

function AddToInstance({ target, open, onClose }: { target: InstallTarget; open: boolean; onClose: () => void }) {
  const { home, openNewInstance, notify } = useApp();
  const [project] = useResource(() => getProject(target.projectId));
  const [rows, setRows] = useState<Record<string, RowState>>({});
  const [installed, setInstalled] = useState<Record<string, boolean>>({});
  const instances = home?.instances ?? [];

  // Which instances already have it.
  useEffect(() => {
    let active = true;
    Promise.all(instances.map((instance) => getInstanceContent(instance.id).then((items) => [instance.id, items] as const))).then(
      (entries) => {
        if (!active) return;
        setInstalled(Object.fromEntries(entries.map(([id, items]) => [id, items.some((item: InstalledContent) => item.projectId === target.projectId)])));
      },
      () => {},
    );
    return () => {
      active = false;
    };
    // Read once when the dialog opens; adding from here shows up in `rows` instead.
  }, [target.projectId]);

  const availability: Availability | null = target.version
    ? { projectType: target.projectType, loaders: target.version.loaders, gameVersions: target.version.gameVersions }
    : project.status === "ready"
      ? { projectType: project.data.projectType, loaders: project.data.loaders, gameVersions: project.data.gameVersions }
      : null;

  const add = async (instanceId: string, instanceName: string) => {
    setRows((current) => ({ ...current, [instanceId]: { phase: "pending" } }));
    try {
      const added = await installProject(instanceId, target.projectId, target.version?.id ?? null);
      const extras = added.filter((item) => item.projectId !== target.projectId).map((item) => item.title);
      // A world lands in the saves folder, under a name no other world there has.
      const folder = target.projectType === "world" ? added[0]?.world : null;
      setRows((current) => ({ ...current, [instanceId]: { phase: "done", extras, note: folder ? `Added as “${folder}”` : undefined } }));
      notify({
        tone: "success",
        title: `${target.title} added to ${instanceName}`,
        body: folder ? `It's in the world list as “${folder}”.` : extras.length ? `Also added what it needs: ${extras.join(", ")}.` : undefined,
      });
    } catch (failure) {
      setRows((current) => ({ ...current, [instanceId]: { phase: "error", message: failure instanceof Error ? failure.message : String(failure) } }));
    }
  };

  const noun = NOUN[target.projectType] ?? "project";
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={`Add ${target.title}`}
      description={
        target.projectType === "world"
          ? "Choose an instance. Wisp unpacks the world into it, ready to play from the world list."
          : `Choose an instance. Wisp picks the newest version of this ${noun} that fits it, and adds anything it requires.`
      }
      footer={
        <>
          <Button
            variant="secondary"
            icon={Plus}
            onClick={() => {
              onClose();
              openNewInstance();
            }}
            className="mr-auto"
          >
            New instance
          </Button>
          <Button onClick={onClose}>Done</Button>
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <ProjectHeader target={target} note={target.version ? `Version ${target.version.label}` : "Newest version that fits"} />
        {instances.length === 0 ? (
          <p className="rounded-2xl border border-dashed border-cream/15 px-4 py-6 text-center text-sm text-mute">
            You don't have any instances yet. Make one, then add {target.title} to it.
          </p>
        ) : availability === null ? (
          <Loading label="Checking instances" className="flex flex-col gap-2">
            {instances.slice(0, 3).map((instance) => (
              <Skeleton key={instance.id} className="h-16 w-full rounded-2xl" />
            ))}
          </Loading>
        ) : (
          <ul className="flex flex-col gap-2">
            {[...instances]
              .sort((a, b) => Number(fits(availability, b).ok) - Number(fits(availability, a).ok))
              .map((instance) => {
                const verdict = fits(availability, instance);
                const state = rows[instance.id] ?? { phase: "idle" };
                const already = installed[instance.id] && state.phase !== "done";
                return (
                  <li
                    key={instance.id}
                    className={cn("flex items-center gap-3 rounded-2xl border border-cream/8 p-3", !verdict.ok && "opacity-60")}
                  >
                    <InstanceAvatar instance={instance} className="size-10 text-base" />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-bold">{instance.name}</p>
                      <p className={cn("truncate text-xs", state.phase === "error" ? "text-coral" : "text-mute")}>
                        {state.phase === "error"
                          ? state.message
                          : state.phase === "done"
                            ? (state.note ??
                              (state.extras.length ? `Added, with ${state.extras.join(", ")}` : "Added"))
                            : verdict.ok
                              ? already
                                ? `${instanceLabel(instance)} · already has it`
                                : instanceLabel(instance)
                              : `${instanceLabel(instance)} · ${verdict.reason}`}
                      </p>
                    </div>
                    {state.phase === "done" ? (
                      <span className="flex h-9 items-center gap-1.5 rounded-full bg-moss/15 px-3.5 text-[13px] font-bold text-moss-soft">
                        <Check size={14} aria-hidden="true" />
                        Added
                      </span>
                    ) : (
                      <button
                        type="button"
                        disabled={!verdict.ok || state.phase === "pending"}
                        onClick={() => void add(instance.id, instance.name)}
                        className="flex h-9 shrink-0 items-center gap-1.5 rounded-full bg-cream px-4 text-[13px] font-bold text-ink-900 transition-transform enabled:active:scale-95 disabled:cursor-not-allowed disabled:bg-cream/15 disabled:text-mute"
                      >
                        {state.phase === "pending" && <LoaderCircle size={14} className="animate-spin" aria-hidden="true" />}
                        {state.phase === "pending" ? "Adding…" : already ? "Update" : "Add"}
                      </button>
                    )}
                  </li>
                );
              })}
          </ul>
        )}
      </div>
    </Dialog>
  );
}

type WorldRow = { phase: "pending" } | { phase: "done" } | { phase: "error"; message: string };

/** Data packs live inside a world: pick the instance, then the world. */
function AddToWorld({ target, open, onClose }: { target: InstallTarget; open: boolean; onClose: () => void }) {
  const { home, notify, now } = useApp();
  const instances = home?.instances ?? [];
  const [project] = useResource(() => getProject(target.projectId));
  const gameVersions = target.version?.gameVersions ?? (project.status === "ready" ? project.data.gameVersions : null);
  const [picked, setPicked] = useState<string | null>(null);
  const suggested = gameVersions ? instances.find((instance) => gameVersions.includes(instance.gameVersion)) : undefined;
  const instance = instances.find((candidate) => candidate.id === picked) ?? suggested ?? instances[0] ?? null;
  const [worlds, setWorlds] = useState<WorldEntry[] | null>(null);
  const [rows, setRows] = useState<Record<string, WorldRow>>({});

  useEffect(() => {
    if (!instance) return;
    let active = true;
    setWorlds(null);
    getInstanceWorlds(instance.id).then(
      (found) => active && setWorlds(found),
      () => active && setWorlds([]),
    );
    return () => {
      active = false;
    };
  }, [instance?.id]);

  const made = instance && gameVersions && gameVersions.length > 0 && !gameVersions.includes(instance.gameVersion);
  const add = async (folder: string) => {
    if (!instance) return;
    const key = `${instance.id}/${folder}`;
    setRows((current) => ({ ...current, [key]: { phase: "pending" } }));
    try {
      await installProject(instance.id, target.projectId, target.version?.id ?? null, folder);
      setRows((current) => ({ ...current, [key]: { phase: "done" } }));
      notify({ tone: "success", title: `${target.title} added to “${folder}”`, body: "It loads the next time you open that world." });
    } catch (error) {
      setRows((current) => ({ ...current, [key]: { phase: "error", message: failure(error) } }));
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={`Add ${target.title} to a world`}
      description="Data packs live inside a world. Pick the instance, then the world to add it to."
      footer={<Button onClick={onClose}>Done</Button>}
    >
      <div className="flex flex-col gap-4">
        <ProjectHeader target={target} note={target.version ? `Version ${target.version.label}` : "Newest version for the instance's Minecraft"} />
        {instance === null ? (
          <p className="rounded-2xl border border-dashed border-cream/15 px-4 py-6 text-center text-sm text-mute">
            You don't have any instances yet. Make one and create a world in it, then add {target.title}.
          </p>
        ) : (
          <>
            <div className="flex items-center gap-3">
              <span className="text-[13px] font-semibold text-mute">Instance</span>
              <Select
                label="Instance"
                value={instance.id}
                options={instances.map((candidate) => ({
                  value: candidate.id,
                  label: candidate.name,
                  note: gameVersions && gameVersions.length > 0 && !gameVersions.includes(candidate.gameVersion) ? `${candidate.gameVersion} · not made for it` : candidate.gameVersion,
                }))}
                onChange={setPicked}
                className="min-w-0 flex-1"
              />
            </div>
            {made && (
              <p className="flex items-start gap-2 rounded-xl bg-amber/10 px-3.5 py-2.5 text-[13px] leading-snug text-amber-soft">
                <CircleAlert size={15} className="mt-0.5 shrink-0" aria-hidden="true" />
                {target.title} has no version for Minecraft {instance.gameVersion}, so adding it to this instance will fail.
              </p>
            )}
            {worlds === null ? (
              <Loading label="Loading worlds" className="flex flex-col gap-2">
                {[0, 1].map((index) => (
                  <Skeleton key={index} className="h-14 w-full rounded-2xl" />
                ))}
              </Loading>
            ) : worlds.length === 0 ? (
              <p className="rounded-2xl border border-dashed border-cream/15 px-4 py-6 text-center text-sm text-mute">
                {instance.name} has no worlds yet. Play it once and create a world, then come back.
              </p>
            ) : (
              <ul className="flex max-h-[280px] flex-col gap-2 overflow-y-auto">
                {worlds.map((world) => {
                  const state = rows[`${instance.id}/${world.folder}`];
                  return (
                    <li key={world.folder} className="flex items-center gap-3 rounded-2xl border border-cream/8 p-3">
                      <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-cream/7 text-cream-300">
                        <Earth size={18} aria-hidden="true" />
                      </span>
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm font-bold">{world.folder}</p>
                        <p className={cn("truncate text-xs", state?.phase === "error" ? "text-coral" : "text-mute")}>
                          {state?.phase === "error" ? state.message : world.lastPlayed ? `Played ${timeAgo(world.lastPlayed, now)}` : "Never played"}
                        </p>
                      </div>
                      {state?.phase === "done" ? (
                        <span className="flex h-9 items-center gap-1.5 rounded-full bg-moss/15 px-3.5 text-[13px] font-bold text-moss-soft">
                          <Check size={14} aria-hidden="true" />
                          Added
                        </span>
                      ) : (
                        <button
                          type="button"
                          disabled={state?.phase === "pending"}
                          onClick={() => void add(world.folder)}
                          className="flex h-9 shrink-0 items-center gap-1.5 rounded-full bg-cream px-4 text-[13px] font-bold text-ink-900 transition-transform enabled:active:scale-95 disabled:opacity-60"
                        >
                          {state?.phase === "pending" && <LoaderCircle size={14} className="animate-spin" aria-hidden="true" />}
                          {state?.phase === "pending" ? "Adding…" : "Add"}
                        </button>
                      )}
                    </li>
                  );
                })}
              </ul>
            )}
            <p className="text-xs leading-relaxed text-mute-2">World generation packs only shape land that hasn't been explored yet.</p>
          </>
        )}
      </div>
    </Dialog>
  );
}
