import { ChevronDown, ExternalLink, FolderOpen, History, LoaderCircle, MoreHorizontal, PackagePlus, Plus, Search, Trash2 } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  getInstanceItems,
  getInstanceUpdates,
  getProjectVersions,
  installItemVersion,
  openInstanceFolder,
  putBackItem,
  removeItem,
  setItemEnabled,
  updateAllItems,
} from "../../data/api";
import type { Instance, InstanceItem, ItemKind, ItemUpdate, ProjectVersion } from "../../data/types";
import { cn } from "../../lib/cn";
import { formatBytes, formatDay, instanceLabel } from "../../lib/format";
import { easeOutExpo } from "../../lib/motion";
import { useApp } from "../../state/AppContext";
import { Sparkle } from "../brand/Sparkle";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { EmptyState } from "../ui/EmptyState";
import { MenuItem, Popover } from "../ui/Popover";
import { Loading, Skeleton } from "../ui/Skeleton";
import { Toggle } from "../ui/Toggle";
import { PackUpdate } from "./PackUpdate";
import { ItemIcon, KIND_ICONS, KIND_NAMES, Reveal, listNames, message } from "./shared";

const KINDS: ItemKind[] = ["mod", "resource-pack", "shader", "data-pack"];

/** Everything in an instance: mods, resource packs, shaders and its worlds' data packs. */
export function ContentTab({ instance, locked }: { instance: Instance; locked: boolean }) {
  const { notify, openDiscoverFor, importFiles, journalVersion } = useApp();
  const [items, setItems] = useState<InstanceItem[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [updates, setUpdates] = useState<ItemUpdate[]>([]);
  const [kind, setKind] = useState<ItemKind>("mod");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [versionsFor, setVersionsFor] = useState<InstanceItem | null>(null);
  const [showLibraries, setShowLibraries] = useState(false);
  const [adding, setAdding] = useState(false);
  const addRef = useRef<HTMLButtonElement>(null);
  const id = instance.id;

  const load = useCallback(() => {
    setFailed(null);
    getInstanceItems(id).then(setItems, (failure: unknown) => setFailed(message(failure)));
    getInstanceUpdates(id).then(setUpdates, () => setUpdates([]));
  }, [id]);
  useEffect(load, [load, journalVersion, instance.gameVersion, instance.loader]);

  // The kind with something in it, the first time round.
  const firstLoad = useRef(true);
  useEffect(() => {
    if (!items || !firstLoad.current) return;
    firstLoad.current = false;
    if (!items.some((item) => item.kind === "mod")) setKind(KINDS.find((candidate) => items.some((item) => item.kind === candidate)) ?? "mod");
  }, [items]);

  const updateFor = useMemo(() => new Map(updates.map((update) => [update.file, update])), [updates]);
  const counts = useMemo(() => {
    const found = new Map<ItemKind, number>();
    for (const item of items ?? []) found.set(item.kind, (found.get(item.kind) ?? 0) + 1);
    return found;
  }, [items]);
  const needle = query.trim().toLowerCase();
  const shown = useMemo(
    () =>
      (items ?? [])
        .filter((item) => item.kind === kind)
        .filter((item) => !needle || item.name.toLowerCase().includes(needle) || item.description.toLowerCase().includes(needle) || item.file.toLowerCase().includes(needle))
        .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" })),
    [items, kind, needle],
  );

  const replace = (file: string, change: Partial<InstanceItem> | null) =>
    setItems((current) => (current ?? []).flatMap((item) => (item.file !== file ? [item] : change === null ? [] : [{ ...item, ...change }])));

  const toggle = async (item: InstanceItem, enabled: boolean) => {
    replace(item.file, { enabled });
    try {
      const file = await setItemEnabled(id, item.file, enabled);
      replace(item.file, { enabled, file });
      setUpdates((current) => current.map((update) => (update.file === item.file ? { ...update, file } : update)));
    } catch (failure) {
      replace(item.file, { enabled: item.enabled });
      notify({ tone: "info", title: `Couldn't turn ${enabled ? "on" : "off"} ${item.name}`, body: message(failure) });
    }
  };

  const remove = async (item: InstanceItem) => {
    setBusy(item.file);
    try {
      const removed = await removeItem(id, item.file, item.name);
      replace(item.file, null);
      notify({
        tone: "info",
        title: `Removed ${item.name}`,
        body: "It waits in the instance's folder in case you want it back.",
        lifetimeMs: 10_000,
        action: {
          label: "Put back",
          run: () =>
            void putBackItem(id, removed.id).then(load, (failure: unknown) => notify({ tone: "info", title: `Couldn't put ${item.name} back`, body: message(failure) })),
        },
      });
    } catch (failure) {
      notify({ tone: "info", title: `Couldn't remove ${item.name}`, body: message(failure) });
    } finally {
      setBusy(null);
    }
  };

  const update = async (item: InstanceItem, update: ItemUpdate) => {
    setBusy(item.file);
    try {
      await installItemVersion(id, item.file, update.versionId);
      notify({ tone: "success", title: `Updated ${item.name}`, body: `It's on ${update.versionNumber} now; the old file can be put back.` });
      load();
    } catch (failure) {
      notify({ tone: "info", title: `Couldn't update ${item.name}`, body: message(failure) });
    } finally {
      setBusy(null);
    }
  };

  const updateAll = async () => {
    setBusy("all");
    try {
      const done = await updateAllItems(id);
      notify({
        tone: done.failed.length > 0 ? "info" : "success",
        title: done.updated.length === 1 ? `Updated ${done.updated[0]}` : `Updated ${done.updated.length} things`,
        body: [done.updated.length > 1 ? listNames(done.updated, 4) : "", done.failed.length > 0 ? `Couldn't update ${listNames(done.failed.map(([name]) => name))}: ${done.failed[0][1]}` : ""]
          .filter(Boolean)
          .join(". "),
        lifetimeMs: 12_000,
      });
      load();
    } catch (failure) {
      notify({ tone: "info", title: "Couldn't update them", body: message(failure) });
    } finally {
      setBusy(null);
    }
  };

  const pendingUpdates = (items ?? []).filter((item) => updateFor.has(item.file)).length;
  const picked = kind === "mod" ? shown.filter((item) => !item.library) : shown;
  const libraries = kind === "mod" ? shown.filter((item) => item.library) : [];
  const librariesOpen = showLibraries || needle.length > 0 || picked.length === 0;
  const worlds = kind === "data-pack" ? [...new Set(shown.map((item) => item.world ?? ""))] : [];

  const row = (item: InstanceItem, index: number) => (
    <ItemRow
      key={item.file}
      index={index}
      item={item}
      instance={instance}
      update={updateFor.get(item.file) ?? null}
      busy={busy === item.file || busy === "all"}
      locked={locked}
      onToggle={(enabled) => void toggle(item, enabled)}
      onUpdate={(found) => void update(item, found)}
      onVersions={() => setVersionsFor(item)}
      onRemove={() => void remove(item)}
    />
  );

  return (
    <div className="flex flex-col gap-4">
      <PackUpdate instance={instance} locked={locked} onUpdated={load} />

      <div className="flex flex-wrap items-center gap-2.5">
        <div role="radiogroup" aria-label="What to show" className="flex flex-wrap gap-1.5">
          {KINDS.map((option) => {
            const selected = option === kind;
            const count = counts.get(option) ?? 0;
            return (
              <button
                key={option}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => setKind(option)}
                className={cn(
                  "flex h-9 items-center gap-1.5 rounded-full border px-3.5 text-[13px] font-bold transition-colors duration-200",
                  selected ? "border-cream bg-cream text-ink-900" : "lit border-cream/12 text-cream-300 hover:border-cream/30 hover:text-cream",
                )}
              >
                {selected ? <Sparkle className="-ml-0.5 size-2.5 text-ember" /> : (() => {
                  const Icon = KIND_ICONS[option];
                  return <Icon size={13} className="-ml-0.5 text-mute" aria-hidden="true" />;
                })()}
                {KIND_NAMES[option].many}
                <span className={cn("tabular-nums", selected ? "text-ink-900/60" : "text-mute-2")}>{items ? count : "·"}</span>
              </button>
            );
          })}
        </div>
        <label className="flex h-9 min-w-[200px] flex-1 items-center gap-2 rounded-full border border-cream/12 bg-ink-900/40 px-3.5 focus-within:border-cream/35">
          <Search size={14} className="shrink-0 text-mute" aria-hidden="true" />
          <span className="sr-only">Find</span>
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={`Find in ${KIND_NAMES[kind].many.toLowerCase()}`}
            spellCheck={false}
            className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-mute-2"
          />
        </label>
        {pendingUpdates > 0 && (
          <Button variant="secondary" disabled={locked || busy !== null} onClick={() => void updateAll()} className="h-9 px-4 text-[13px]">
            <span className="flex items-center gap-1.5">
              {busy === "all" ? <LoaderCircle size={14} className="animate-spin" aria-hidden="true" /> : <Sparkle className="size-3 text-ember" />}
              {busy === "all" ? "Updating…" : `Update all (${pendingUpdates})`}
            </span>
          </Button>
        )}
        <button
          ref={addRef}
          type="button"
          onClick={() => setAdding((open) => !open)}
          aria-haspopup="menu"
          aria-expanded={adding}
          className="tail wag relative flex h-9 items-center gap-1.5 rounded-full bg-cream px-4 text-[13px] font-bold text-ink-900 transition-transform active:scale-95 [--tail:16px]"
        >
          <Plus size={14} aria-hidden="true" />
          Add
        </button>
        <Popover open={adding} anchorRef={addRef} placement="bottom-end" onClose={() => setAdding(false)} label="Add to this instance" className="w-[260px]">
          <div role="menu" className="flex flex-col">
            <MenuItem icon={<Search size={15} aria-hidden="true" />} label="Find on Discover" hint="Mods, packs and shaders for this version" onSelect={() => (setAdding(false), openDiscoverFor(id))} />
            <MenuItem icon={<PackagePlus size={15} aria-hidden="true" />} label="Add files you have" hint=".jar mods, pack and shader zips" onSelect={() => (setAdding(false), importFiles(null, id))} />
          </div>
        </Popover>
      </div>

      {locked && <p className="-mt-1 text-[13px] text-mute">Close Minecraft to change what's in it.</p>}

      <AnimatePresence mode="popLayout" initial={false}>
      <m.div key={kind} initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0, transition: { duration: 0.34, ease: easeOutExpo } }} exit={{ opacity: 0, transition: { duration: 0.12 } }}>
      {failed ? (
        <EmptyState title="Couldn't read what's in it" body={failed} actions={<Button onClick={load}>Try again</Button>} />
      ) : items === null ? (
        <Loading label="Reading the instance" className="flex flex-col gap-2">
          {[0, 1, 2, 3, 4].map((index) => (
            <Skeleton key={index} className="h-[68px] w-full rounded-2xl" />
          ))}
        </Loading>
      ) : shown.length === 0 ? (
        needle ? (
          <p className="rounded-2xl border border-dashed border-cream/12 px-5 py-8 text-center text-sm text-mute">Nothing called “{query.trim()}” here.</p>
        ) : (
          <EmptyState
            title={KIND_NAMES[kind].empty}
            body={kind === "data-pack" ? "Data packs live inside worlds. Add one from Discover and pick the world." : `Find ${KIND_NAMES[kind].many.toLowerCase()} for ${instanceLabel(instance)} on Discover, or add files you already have.`}
            actions={
              <>
                <Button icon={Search} onClick={() => openDiscoverFor(id)}>
                  {KIND_NAMES[kind].discover}
                </Button>
                {kind !== "data-pack" && (
                  <Button variant="secondary" icon={PackagePlus} onClick={() => importFiles(null, id)}>
                    Add files
                  </Button>
                )}
              </>
            }
          />
        )
      ) : kind === "data-pack" ? (
        <div className="flex flex-col gap-5">
          {worlds.map((world) => (
            <section key={world} className="flex flex-col gap-2">
              <h3 className="text-[13px] font-bold text-mute">In {world || "a world"}</h3>
              <ul className="flex flex-col gap-2">
                <AnimatePresence initial={false}>{shown.filter((item) => (item.world ?? "") === world).map((item, index) => row(item, index))}</AnimatePresence>
              </ul>
            </section>
          ))}
        </div>
      ) : (
        <div className="flex flex-col gap-5">
          {picked.length > 0 && (
            <ul className="flex flex-col gap-2">
              <AnimatePresence initial={false}>{picked.map((item, index) => row(item, index))}</AnimatePresence>
            </ul>
          )}
          {libraries.length > 0 && (
            <section className="flex flex-col gap-2">
              <button
                type="button"
                onClick={() => setShowLibraries((open) => !open)}
                aria-expanded={librariesOpen}
                className="flex w-fit items-center gap-2 rounded-full px-1 text-[13px] font-bold text-mute transition-colors hover:text-cream"
              >
                <ChevronDown size={15} className={cn("transition-transform duration-300", !librariesOpen && "-rotate-90")} aria-hidden="true" />
                What your mods need
                <span className="tabular-nums text-mute-2">{libraries.length}</span>
                {!librariesOpen && libraries.some((item) => updateFor.has(item.file)) && <Sparkle className="size-2.5 text-ember" />}
              </button>
              <Reveal open={librariesOpen} className="-mx-1 px-1">
                <ul className="flex flex-col gap-2 pb-1">
                  <AnimatePresence initial={false}>{libraries.map((item, index) => row(item, index))}</AnimatePresence>
                </ul>
              </Reveal>
            </section>
          )}
        </div>
      )}

      </m.div>
      </AnimatePresence>

      {versionsFor && (
        <VersionsDialog
          instance={instance}
          item={versionsFor}
          onClose={() => setVersionsFor(null)}
          onChanged={() => {
            setVersionsFor(null);
            load();
          }}
        />
      )}
    </div>
  );
}

function originNote(item: InstanceItem, instance: Instance): string | null {
  if (item.origin === "pack") return instance.modpack ? `From ${instance.modpack.title}` : "From the modpack";
  if (item.origin === "yours") return "Added by you";
  return null;
}

function ItemRow({
  index,
  item,
  instance,
  update,
  busy,
  locked,
  onToggle,
  onUpdate,
  onVersions,
  onRemove,
}: {
  /** Its place in the list, so a list arriving cascades in rather than appearing at once. */
  index: number;
  item: InstanceItem;
  instance: Instance;
  update: ItemUpdate | null;
  busy: boolean;
  locked: boolean;
  onToggle: (enabled: boolean) => void;
  onUpdate: (update: ItemUpdate) => void;
  onVersions: () => void;
  onRemove: () => void;
}) {
  const { openProject } = useApp();
  const [menu, setMenu] = useState(false);
  const more = useRef<HTMLButtonElement>(null);
  const onModrinth = item.projectId !== null && !item.projectId.startsWith("curseforge:");
  const notes = [originNote(item, instance), item.neededBy.length > 0 ? `Needed by ${listNames(item.neededBy)}` : null, item.folder ? "A folder" : null].filter(Boolean) as string[];

  return (
    <m.li
      layout="position"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0, transition: { duration: 0.38, ease: easeOutExpo, delay: Math.min(index, 14) * 0.022 } }}
      exit={{ opacity: 0, x: 24, transition: { duration: 0.18 } }}
      transition={{ type: "spring", stiffness: 420, damping: 36 }}
      className={cn("group flex items-center gap-3.5 rounded-2xl border border-cream/6 bg-ink-800 px-3.5 py-3 transition-colors hover:border-cream/12", !item.enabled && "bg-ink-850")}
    >
      <ItemIcon item={item} className={cn("size-11 transition-opacity", !item.enabled && "opacity-40 grayscale")} />
      <div className={cn("min-w-0 flex-1 transition-opacity", !item.enabled && "opacity-55")}>
        <p className="flex min-w-0 items-baseline gap-2">
          <span className="truncate text-[15px] font-bold">{item.name}</span>
          {item.version && <span className="shrink-0 truncate font-mono text-[11px] text-mute-2">{item.version}</span>}
          {!item.enabled && <span className="shrink-0 rounded-full bg-cream/8 px-2 py-0.5 text-[11px] font-bold text-mute">Off</span>}
        </p>
        {item.description && <p className="mt-0.5 truncate text-[13px] text-mute">{item.description}</p>}
        {notes.length > 0 && <p className="mt-0.5 truncate text-xs text-mute-2">{notes.join(" · ")}</p>}
      </div>
      {update && (
        <button
          type="button"
          disabled={busy || locked}
          onClick={() => onUpdate(update)}
          title={`Update to ${update.versionNumber}`}
          className="flex h-8 max-w-[180px] shrink-0 items-center gap-1.5 rounded-full border border-ember/35 bg-ember/10 px-3 text-xs font-bold text-ember-soft transition-colors enabled:hover:border-ember/60 enabled:hover:bg-ember/15 disabled:opacity-50"
        >
          {busy ? <LoaderCircle size={12} className="animate-spin" aria-hidden="true" /> : <Sparkle className="size-2.5" />}
          <span className="truncate">{update.versionNumber}</span>
        </button>
      )}
      {busy && !update && <LoaderCircle size={15} className="shrink-0 animate-spin text-mute" aria-hidden="true" />}
      {!item.folder && <Toggle compact label={item.enabled ? `Turn off ${item.name}` : `Turn on ${item.name}`} checked={item.enabled} disabled={busy || locked} onChange={onToggle} />}
      <button
        ref={more}
        type="button"
        onClick={() => setMenu((open) => !open)}
        aria-label={`More for ${item.name}`}
        aria-haspopup="menu"
        aria-expanded={menu}
        className="grid size-8 shrink-0 place-items-center rounded-full text-mute transition-colors hover:bg-cream/8 hover:text-cream"
      >
        <MoreHorizontal size={16} aria-hidden="true" />
      </button>
      <Popover open={menu} anchorRef={more} placement="bottom-end" onClose={() => setMenu(false)} label={`${item.name} options`} className="w-[250px]">
        <div role="menu" className="flex flex-col">
          {onModrinth && (
            <MenuItem icon={<History size={15} aria-hidden="true" />} label="Other versions" hint="Pick another build of it" disabled={locked} onSelect={() => (setMenu(false), onVersions())} />
          )}
          {onModrinth && item.projectId && <MenuItem icon={<ExternalLink size={15} aria-hidden="true" />} label="Open its page" onSelect={() => (setMenu(false), openProject(item.projectId ?? ""))} />}
          <MenuItem
            icon={<FolderOpen size={15} aria-hidden="true" />}
            label="Show the folder"
            hint={item.size > 0 ? `${item.file.split("/").at(-1)} · ${formatBytes(item.size)}` : item.file}
            onSelect={() => (setMenu(false), void openInstanceFolder(instance.id))}
          />
          <MenuItem icon={<Trash2 size={15} aria-hidden="true" />} label="Remove" hint="You can put it back" disabled={locked || busy} onSelect={() => (setMenu(false), onRemove())} />
        </div>
      </Popover>
    </m.li>
  );
}

function loadersFor(instance: Instance, kind: ItemKind): string[] {
  switch (kind) {
    case "mod":
      return instance.loader === "quilt" ? ["quilt", "fabric"] : [instance.loader];
    case "shader":
      return ["iris", "optifine", "canvas", "vanilla"];
    case "resource-pack":
      return ["minecraft"];
    case "data-pack":
      return ["datapack"];
  }
}

/** The builds of a mod or pack for this instance's version, to swap in another one. */
function VersionsDialog({ instance, item, onClose, onChanged }: { instance: Instance; item: InstanceItem; onClose: () => void; onChanged: () => void }) {
  const { notify } = useApp();
  const [versions, setVersions] = useState<ProjectVersion[] | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  useEffect(() => {
    if (!item.projectId) return;
    let live = true;
    getProjectVersions(item.projectId, loadersFor(instance, item.kind), [instance.gameVersion]).then(
      (found) => live && setVersions(found),
      (failure: unknown) => live && setFailed(message(failure)),
    );
    return () => {
      live = false;
    };
  }, [item.projectId, item.kind, instance]);

  const pick = async (version: ProjectVersion) => {
    setBusy(version.id);
    try {
      await installItemVersion(instance.id, item.file, version.id);
      notify({ tone: "success", title: `${item.name} is on ${version.versionNumber}`, body: "The one it replaced can be put back." });
      onChanged();
    } catch (failure) {
      notify({ tone: "info", title: "Couldn't switch it", body: message(failure) });
      setBusy(null);
    }
  };

  return (
    <Dialog open onClose={onClose} title={`Versions of ${item.name}`} description={`Builds for ${instanceLabel(instance)}, newest first.`} footer={<Button variant="secondary" onClick={onClose}>Close</Button>}>
      {failed ? (
        <p className="text-sm text-coral">{failed}</p>
      ) : versions === null ? (
        <Loading label="Loading versions" className="flex flex-col gap-2">
          {[0, 1, 2].map((index) => (
            <Skeleton key={index} className="h-14 w-full rounded-2xl" />
          ))}
        </Loading>
      ) : versions.length === 0 ? (
        <p className="text-sm leading-relaxed text-mute">Modrinth has no builds of it for {instanceLabel(instance)}.</p>
      ) : (
        <ul className="flex max-h-[50vh] flex-col gap-1.5 overflow-y-auto pr-1">
          {versions.map((version) => {
            const installed = version.id === item.versionId;
            return (
              <li key={version.id} className={cn("flex items-center gap-3 rounded-2xl px-3.5 py-2.5", installed ? "bg-cream/8" : "bg-cream/[0.03]")}>
                <div className="min-w-0 flex-1">
                  <p className="flex items-center gap-2 text-sm font-bold">
                    <span className="truncate">{version.versionNumber}</span>
                    {version.versionType !== "release" && <span className="shrink-0 rounded-full bg-ember/15 px-2 py-0.5 text-[11px] font-bold capitalize text-ember-soft">{version.versionType}</span>}
                  </p>
                  <p className="mt-0.5 text-xs text-mute">{formatDay(version.datePublished)}</p>
                </div>
                {installed ? (
                  <span className="text-xs font-bold text-moss-soft">Installed</span>
                ) : (
                  <Button variant="secondary" disabled={busy !== null} onClick={() => void pick(version)} className="h-8 px-3.5 text-xs">
                    {busy === version.id ? "Switching…" : "Use this"}
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </Dialog>
  );
}
