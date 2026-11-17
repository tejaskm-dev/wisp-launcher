import { Braces, CircleAlert, Earth, FileQuestion, LoaderCircle, Package, Palette, Plus, Puzzle, Sparkles, type LucideIcon } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { addContentFiles, inspectImportFiles, pickImportFiles } from "../../data/api";
import type { ImportKind, InspectedFile } from "../../data/types";
import { cn } from "../../lib/cn";
import { loaderName } from "../../lib/format";
import { useApp } from "../../state/AppContext";
import { Sparkle } from "../brand/Sparkle";
import { InstanceAvatar } from "../instances/InstanceAvatar";
import { InstanceLabel, VersionTag } from "../instances/VersionTag";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Loading, Skeleton } from "../ui/Skeleton";

const KINDS: Record<ImportKind, { icon: LucideIcon; label: string }> = {
  modpack: { icon: Package, label: "Modpack" },
  mod: { icon: Puzzle, label: "Mod" },
  "resource-pack": { icon: Palette, label: "Resource pack" },
  "shader-pack": { icon: Sparkles, label: "Shader pack" },
  "data-pack": { icon: Braces, label: "Data pack" },
  world: { icon: Earth, label: "World" },
  unknown: { icon: FileQuestion, label: "Not something Wisp can add" },
};

const CONTENT: ImportKind[] = ["mod", "resource-pack", "shader-pack"];

type FileState = { phase: "idle" } | { phase: "working" } | { phase: "done"; note: string | null } | { phase: "error"; message: string };

const failure = (error: unknown) => (error instanceof Error ? error.message : String(error));

/**
 * Brings in files you have, picked or dropped on the window. Each is read for what it really is:
 * a modpack (Modrinth's .mrpack or a CurseForge zip) becomes an instance of its own, and mods,
 * resource packs and shader packs go into the instance you pick. Mount it fresh for each use.
 */
export function ImportDialog({ files, instanceId, open, onClose }: { files: string[] | null; instanceId: string | null; open: boolean; onClose: () => void }) {
  const { home, notify, startModpackFileInstall, importWorlds, openNewInstance } = useApp();
  const [paths, setPaths] = useState<string[] | null>(files);
  const [inspected, setInspected] = useState<InspectedFile[] | null>(null);
  const [states, setStates] = useState<Record<string, FileState>>({});
  const [target, setTarget] = useState<string | null>(null);

  // No files given: the picker first (the dialog stays out of the way until something's picked).
  useEffect(() => {
    if (paths !== null) return;
    let active = true;
    pickImportFiles().then(
      (picked) => {
        if (!active) return;
        if (picked.length === 0) onClose();
        else setPaths(picked);
      },
      (error: unknown) => {
        notify({ tone: "info", title: "Couldn't open the file picker", body: failure(error) });
        onClose();
      },
    );
    return () => {
      active = false;
    };
    // Once, when the dialog opens.
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    if (paths === null) return;
    let active = true;
    inspectImportFiles(paths).then(
      (found) => active && setInspected(found),
      () => active && setInspected([]),
    );
    return () => {
      active = false;
    };
  }, [paths]);

  const setState = (path: string, state: FileState) => setStates((current) => ({ ...current, [path]: state }));
  const of = (kinds: ImportKind[]) => (inspected ?? []).filter((file) => kinds.includes(file.kind));
  const packs = of(["modpack"]);
  const content = of(CONTENT);
  const worlds = of(["world"]);
  const dataPacks = of(["data-pack"]);
  const unknown = of(["unknown"]);

  const install = (file: InspectedFile) => {
    setState(file.path, { phase: "done", note: "Installing as a new instance. You can close this: it carries on." });
    startModpackFileInstall(file.path, file.pack?.name ?? file.name ?? file.fileName.replace(/\.(mrpack|zip)$/i, ""));
  };

  const addTo = async (instanceId: string, instanceName: string) => {
    const waiting = content.filter((file) => (states[file.path]?.phase ?? "idle") !== "done");
    if (waiting.length === 0) return;
    setTarget(instanceId);
    for (const file of waiting) setState(file.path, { phase: "working" });
    try {
      const results = await addContentFiles(instanceId, waiting.map((file) => file.path));
      for (const result of results) {
        setState(result.path, result.added ? { phase: "done", note: result.added.note } : { phase: "error", message: result.error ?? "Couldn't add it." });
      }
      const added = results.filter((result) => result.added).length;
      if (added > 0) notify({ tone: "success", title: added === 1 ? `Added to ${instanceName}` : `${added} files added to ${instanceName}`, body: "They load the next time you play it." });
    } catch (error) {
      for (const file of waiting) setState(file.path, { phase: "error", message: failure(error) });
    }
  };

  const instances = home?.instances ?? [];
  const onlyMods = content.length > 0 && content.every((file) => file.kind === "mod");
  const ordered = [...instances].sort((a, b) => Number(b.id === instanceId) - Number(a.id === instanceId));

  return (
    <Dialog
      open={open && paths !== null}
      onClose={onClose}
      title={paths && paths.length === 1 ? "Add a file" : "Add files"}
      description="Wisp reads each file for what it is. Modpacks become instances of their own; mods and packs go into the instance you pick."
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
      {inspected === null ? (
        <Loading label="Reading the files" className="flex flex-col gap-2">
          {(paths ?? [0, 1]).slice(0, 4).map((_, index) => (
            <Skeleton key={index} className="h-14 w-full rounded-2xl" />
          ))}
        </Loading>
      ) : (
        <div className="flex flex-col gap-5">
          {packs.length > 0 && (
            <Section title="Modpacks" note="Each becomes an instance of its own, with everything it lists downloaded.">
              {packs.map((file) => {
                const state = states[file.path] ?? { phase: "idle" };
                return (
                  <FileRow
                    key={file.path}
                    file={file}
                    state={state}
                    detail={
                      file.problem ? (
                        <span className="text-coral">{file.problem}</span>
                      ) : file.pack ? (
                        <>
                          {loaderName(file.pack.loader)} · <VersionTag version={file.pack.gameVersion} />
                          {file.pack.from === "curse-forge" ? " · from CurseForge" : ""}
                        </>
                      ) : null
                    }
                    action={
                      !file.problem &&
                      state.phase === "idle" && (
                        <Button size="md" onClick={() => install(file)} className="h-9 px-4 text-[13px]">
                          Install
                        </Button>
                      )
                    }
                  />
                );
              })}
            </Section>
          )}

          {content.length > 0 && (
            <Section title={onlyMods ? (content.length === 1 ? "A mod" : "Mods") : "Mods and packs"} note="They go into the instance you pick.">
              {content.map((file) => (
                <FileRow
                  key={file.path}
                  file={file}
                  state={states[file.path] ?? { phase: "idle" }}
                  detail={
                    <>
                      {KINDS[file.kind].label}
                      {file.version && (
                        <>
                          {" · "}
                          <VersionTag version={file.version} />
                        </>
                      )}
                      {file.loaders.length > 0 && ` · for ${file.loaders.map(loaderName).join(" or ")}`}
                    </>
                  }
                />
              ))}
              {instances.length === 0 ? (
                <p className="rounded-2xl border border-dashed border-cream/15 px-4 py-5 text-center text-sm text-mute">Make an instance first, then add these to it.</p>
              ) : (
                <ul className="mt-1 flex flex-col gap-2">
                  {ordered.map((instance) => {
                    const blocked = onlyMods && instance.loader === "vanilla";
                    const busy = content.some((file) => states[file.path]?.phase === "working");
                    const chosen = target === instance.id;
                    return (
                      <li key={instance.id} className={cn("flex items-center gap-3 rounded-2xl border border-cream/8 p-2.5", blocked && "opacity-55", chosen && "ghost-ring border-transparent bg-ink-700 [--ring:#3a3632]")}>
                        <InstanceAvatar instance={instance} className="size-9 text-sm" />
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-sm font-bold">{instance.name}</p>
                          <p className="truncate text-xs text-mute">{blocked ? "Vanilla can't run mods" : <InstanceLabel instance={instance} />}</p>
                        </div>
                        <Button variant="secondary" disabled={blocked || busy} onClick={() => void addTo(instance.id, instance.name)} className="h-9 px-4 text-[13px]">
                          {chosen && busy ? <LoaderCircle size={14} className="animate-spin" aria-hidden="true" /> : null}
                          {chosen ? "Add again" : "Add here"}
                        </Button>
                      </li>
                    );
                  })}
                </ul>
              )}
            </Section>
          )}

          {worlds.length > 0 && (
            <Section
              title={worlds.length === 1 ? "A world" : "Worlds"}
              note="They import like any world, into the instance you pick."
              action={
                <Button
                  variant="secondary"
                  onClick={() => {
                    importWorlds(worlds.map((file) => file.path));
                    onClose();
                  }}
                  className="h-9 px-4 text-[13px]"
                >
                  Import {worlds.length === 1 ? "it" : "them"}
                </Button>
              }
            >
              {worlds.map((file) => (
                <FileRow key={file.path} file={file} state={{ phase: "idle" }} detail="World" />
              ))}
            </Section>
          )}

          {dataPacks.length > 0 && (
            <Section title="Data packs" note="A data pack belongs to one world: open that world in Worlds and add it there.">
              {dataPacks.map((file) => (
                <FileRow key={file.path} file={file} state={{ phase: "idle" }} detail="Data pack" />
              ))}
            </Section>
          )}

          {unknown.length > 0 && (
            <Section title="Can't tell what these are" note="Wisp takes .mrpack modpacks, CurseForge modpack zips, .jar mods, and resource, shader and data pack zips.">
              {unknown.map((file) => (
                <FileRow key={file.path} file={file} state={{ phase: "idle" }} detail={null} muted />
              ))}
            </Section>
          )}
        </div>
      )}
    </Dialog>
  );
}

function Section({ title, note, action, children }: { title: string; note: string; action?: ReactNode; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2">
      <header className="flex items-end gap-3">
        <div className="min-w-0 flex-1">
          <h3 className="text-[11px] font-bold uppercase tracking-[0.16em] text-mute-2">{title}</h3>
          <p className="mt-0.5 text-[13px] leading-snug text-mute">{note}</p>
        </div>
        {action}
      </header>
      {children}
    </section>
  );
}

function FileRow({ file, state, detail, action, muted = false }: { file: InspectedFile; state: FileState; detail: ReactNode; action?: ReactNode; muted?: boolean }) {
  const { icon: Icon } = KINDS[file.kind];
  return (
    <div className={cn("flex items-center gap-3 rounded-2xl bg-cream/4 p-2.5", muted && "opacity-60")}>
      <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-ink-700 text-cream-300">
        <Icon size={17} aria-hidden="true" />
      </span>
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-bold" title={file.fileName}>
          {file.pack?.name ?? file.name ?? file.fileName}
        </p>
        <p className={cn("truncate text-xs", state.phase === "error" ? "text-coral" : state.phase === "done" && state.note ? "text-amber" : "text-mute")}>
          {state.phase === "error" ? state.message : state.phase === "done" && state.note ? state.note : detail}
        </p>
      </div>
      {state.phase === "working" ? (
        <LoaderCircle size={16} className="shrink-0 animate-spin text-mute" aria-label="Adding" />
      ) : state.phase === "done" ? (
        <span role="img" aria-label="Done" className="shrink-0">
          <Sparkle className="size-3.5 text-ember" />
        </span>
      ) : state.phase === "error" ? (
        <CircleAlert size={16} className="shrink-0 text-coral" aria-label="Didn't work" />
      ) : (
        action
      )}
    </div>
  );
}
