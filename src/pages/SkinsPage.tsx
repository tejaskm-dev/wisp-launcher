import { Check, ChevronsDown, Crown, Download, FolderInput, Info, MoreHorizontal, MoveHorizontal, Music2, Pencil, PersonStanding, Plus, Rotate3d, Shirt, Trash2, Type, Upload, UserRound, Zap, type LucideIcon } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import stageArt from "../assets/art/skin-stage.jpg";
import { EMOTES, type EmoteId } from "../components/skins/emotes";
import type { SavedLayer } from "../components/skins/layers";
import { SkinEditor, type StartLayer } from "../components/skins/SkinEditor";
import { SkinDoll, announceSkinsChanged, loadSkinImage } from "../components/skins/SkinDoll";
import { skinSource, useSkinSource } from "../components/skins/skinSource";
import { SkinStage } from "../components/skins/SkinStage";
import { SIZE, makeDefaultSkin } from "../components/skins/skinTexture";
import { Button } from "../components/ui/Button";
import { Dialog } from "../components/ui/Dialog";
import { Page } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { MenuItem, Popover } from "../components/ui/Popover";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { addSkin, deleteSkin, exportSkin, fetchPlayerSkin, getSkinLayers, getSkins, importSkinPaths, isDesktop, onFileDrop, pickSkinFiles, setActiveSkin, setSkinLayers, updateSkin } from "../data/api";
import type { PlayerSkin, Skin, SkinImport, SkinLibrary, SkinModel } from "../data/types";
import { cn } from "../lib/cn";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

const PREFS_KEY = "wisp.skinStage";
const EMOTE_ICONS: Record<EmoteId, LucideIcon> = { throne: Crown, dance: Music2, aura: Zap, tpose: MoveHorizontal, crouch: ChevronsDown };

interface StagePrefs {
  emote: EmoteId | "idle";
  spin: boolean;
}

function readPrefs(): StagePrefs {
  try {
    const stored = JSON.parse(localStorage.getItem(PREFS_KEY) ?? "{}") as Partial<StagePrefs>;
    const known = EMOTES.some((emote) => emote.id === stored.emote);
    return { emote: known ? (stored.emote as EmoteId) : "idle", spin: stored.spin ?? false };
  } catch {
    return { emote: "idle", spin: false };
  }
}

type Editing = { skin: Skin | null; source: HTMLCanvasElement; layers: StartLayer[] | null; name: string; model: SkinModel };

/** The layers a skin was saved with, read back into canvases. Null when it has none (or they can't be read). */
async function readLayers(skin: Skin): Promise<StartLayer[] | null> {
  try {
    const saved = await getSkinLayers(skin.id);
    if (!saved || saved.length === 0) return null;
    return await Promise.all(
      saved.map(async (layer) => {
        const picture = await loadSkinImage(layer.png.startsWith("data:") ? layer.png : `data:image/png;base64,${layer.png}`);
        const canvas = document.createElement("canvas");
        canvas.width = SIZE;
        canvas.height = SIZE;
        canvas.getContext("2d")?.drawImage(picture, 0, 0);
        return { name: layer.name, visible: layer.visible, opacity: layer.opacity, canvas };
      }),
    );
  } catch {
    return null;
  }
}

export function SkinsPage() {
  const { home, notify } = useApp();
  const [library, setLibrary] = useState<SkinLibrary | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [prefs, setPrefs] = useState(readPrefs);
  // Pressing the playing emote again plays it from the start (the throne gathers again).
  const [take, setTake] = useState(0);
  const [editing, setEditing] = useState<Editing | null>(null);
  const [lookup, setLookup] = useState(false);
  const [dragging, setDragging] = useState(false);
  const defaultSkin = useMemo(() => makeDefaultSkin(false), []);

  useEffect(() => {
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify(prefs));
    } catch {
      // Only a convenience.
    }
  }, [prefs]);

  const reload = useCallback(async () => {
    try {
      const found = await getSkins();
      setLibrary(found);
      return found;
    } catch {
      setLibrary({ skins: [], active: null });
      return null;
    }
  }, []);
  useEffect(() => {
    void reload();
  }, [reload]);

  const fail = useCallback((title: string) => (failure: unknown) => notify({ tone: "info", title, body: failure instanceof Error ? failure.message : String(failure) }), [notify]);
  const reportImport = useCallback(
    (result: SkinImport) => {
      if (result.added.length > 0) {
        setSelectedId(result.added[0].id);
        notify({ tone: "success", title: result.added.length === 1 ? `Added ${result.added[0].name}` : `Added ${result.added.length} skins`, body: result.failed.length ? `Skipped: ${result.failed.join("; ")}` : undefined });
      } else if (result.failed.length > 0) {
        notify({ tone: "info", title: "Those aren't skins Wisp can read", body: result.failed.join("; ") });
      }
      void reload().then(() => announceSkinsChanged());
    },
    [notify, reload],
  );

  // Skin PNGs dropped on the window.
  useEffect(
    () =>
      onFileDrop((drop) => {
        if (editing) return;
        if (drop.type === "over") setDragging(true);
        else if (drop.type === "leave") setDragging(false);
        else {
          setDragging(false);
          const pngs = drop.paths.filter((path) => path.toLowerCase().endsWith(".png"));
          if (pngs.length) importSkinPaths(pngs).then(reportImport, fail("Couldn't import those"));
        }
      }),
    [editing, reportImport, fail],
  );

  const skins = library?.skins ?? [];
  const selected = skins.find((skin) => skin.id === selectedId) ?? skins.find((skin) => skin.id === library?.active) ?? skins[0] ?? null;
  // Keep showing the last skin until the next one has loaded, so it never flashes back to Wisp's.
  const selectedSource = useSkinSource(selected);
  const [stageSkin, setStageSkin] = useState<string | HTMLCanvasElement>(defaultSkin);
  useEffect(() => {
    if (!selected) setStageSkin(defaultSkin);
    else if (selectedSource) setStageSkin(selectedSource);
  }, [selected, selectedSource, defaultSkin]);

  // In the browser preview there's no file picker from Rust: read the file here.
  const browserInput = useRef<HTMLInputElement>(null);
  const importFiles = () => {
    if (isDesktop) pickSkinFiles().then(reportImport, fail("Couldn't import that"));
    else browserInput.current?.click();
  };
  const onBrowserFiles = async (files: FileList | null) => {
    const result: SkinImport = { added: [], failed: [] };
    for (const file of Array.from(files ?? [])) {
      const data = await new Promise<string>((resolve) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result));
        reader.readAsDataURL(file);
      });
      try {
        result.added.push(await addSkin(file.name.replace(/\.png$/i, ""), null, data, "file"));
      } catch (failure: unknown) {
        result.failed.push(`${file.name}: ${failure instanceof Error ? failure.message : String(failure)}`);
      }
    }
    reportImport(result);
  };

  const startEditing = async (skin: Skin | null) => {
    const canvas = document.createElement("canvas");
    canvas.width = SIZE;
    canvas.height = SIZE;
    const g = canvas.getContext("2d") as CanvasRenderingContext2D;
    if (skin) {
      try {
        g.drawImage(await loadSkinImage(await skinSource(skin)), 0, 0);
      } catch (failure: unknown) {
        fail("Couldn't open that skin")(failure);
        return;
      }
    } else {
      g.drawImage(makeDefaultSkin(false), 0, 0);
    }
    const layers = skin ? await readLayers(skin) : null;
    setEditing({ skin, source: canvas, layers, name: skin?.name ?? "My skin", model: skin?.model ?? "classic" });
  };

  const saveEdit = async (png: string, name: string, model: SkinModel, asNew: boolean, layers: SavedLayer[] | null) => {
    if (!editing) return;
    try {
      const saved = editing.skin && !asNew ? await updateSkin(editing.skin.id, { png, name, model }) : await addSkin(name, model, png, "editor");
      // The layers stay beside the skin, for the next edit (one layer is just the skin).
      await setSkinLayers(saved.id, layers ?? []).catch(() => {});
      await reload();
      announceSkinsChanged();
      setSelectedId(saved.id);
      setEditing(null);
      notify({ tone: "success", title: `Saved ${saved.name}`, body: "It's in your skins." });
    } catch (failure: unknown) {
      fail("Couldn't save the skin")(failure);
    }
  };

  const playEmote = (id: EmoteId | "idle") => {
    if (prefs.emote === id) setTake((count) => count + 1);
    else setPrefs((current) => ({ ...current, emote: id }));
  };

  if (editing) {
    return (
      <Page>
        <SkinEditor name={editing.name} model={editing.model} source={editing.source} layers={editing.layers} isNew={editing.skin === null} onSave={saveEdit} onCancel={() => setEditing(null)} />
      </Page>
    );
  }

  return (
    <Page>
      <PageHeader
        title="Skins"
        subtitle="Your skin in 3D. Strike a pose, take a throne, edit it pixel by pixel, or bring one in from a file or any player's name."
        actions={
          <>
            <Button variant="secondary" icon={UserRound} onClick={() => setLookup(true)}>
              From a player
            </Button>
            <Button variant="secondary" icon={Upload} onClick={importFiles}>
              Import
            </Button>
            <Button icon={Plus} onClick={() => void startEditing(null)}>
              New skin
            </Button>
          </>
        }
      />
      <input ref={browserInput} type="file" accept="image/png" multiple hidden onChange={(event) => void onBrowserFiles(event.target.files)} />

      <m.div variants={fadeUp} className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_320px]">
        <section aria-label="Preview" className="flex min-w-0 flex-col gap-3">
          <div className="relative aspect-[3/2] w-full overflow-hidden rounded-[26px] border border-cream/8 bg-ink-850">
            <img src={stageArt} alt="" aria-hidden="true" draggable={false} className="absolute inset-0 size-full object-cover" />
            {/* Settle the art into the dark interface, and ground the player on the stone circle. */}
            <div className="pointer-events-none absolute inset-0 bg-[radial-gradient(ellipse_at_50%_55%,transparent_55%,rgba(10,9,8,0.55))]" />
            <div className="pointer-events-none absolute left-1/2 top-[73.5%] h-[5%] w-[22%] -translate-x-1/2 rounded-[100%] bg-black/45 blur-md" />
            <SkinStage
              skin={stageSkin}
              model={selected?.model ?? "classic"}
              emote={prefs.emote}
              autoRotate={prefs.spin}
              nameTag={home?.account?.name ?? null}
              zoom={0.62}
              lift={2.2}
              replay={take}
              className="absolute inset-0"
            />
            <div className="pointer-events-none absolute inset-x-4 top-4 flex items-start gap-3">
              <div className="min-w-0 rounded-2xl bg-ink-950/45 px-3.5 py-2 backdrop-blur-sm">
                <p className="truncate font-display text-lg font-bold leading-tight">{selected?.name ?? "Wisp"}</p>
                <p className="text-xs text-cream-300">{selected ? `${selected.model === "slim" ? "Slim" : "Classic"} arms` : "Wisp's own skin, until you add yours"}</p>
              </div>
              <button
                type="button"
                aria-pressed={prefs.spin}
                onClick={() => setPrefs((current) => ({ ...current, spin: !current.spin }))}
                className={cn(
                  "pointer-events-auto ml-auto flex h-9 items-center gap-2 rounded-full border px-3.5 text-xs font-bold backdrop-blur-sm transition-colors",
                  prefs.spin ? "border-cream/60 bg-cream text-ink-900" : "border-cream/15 bg-ink-950/45 text-cream-300 hover:border-cream/35 hover:text-cream",
                )}
              >
                <Rotate3d size={14} aria-hidden="true" />
                Turntable
              </button>
            </div>
          </div>

          <div role="group" aria-label="Emotes" className="flex flex-wrap items-center gap-2">
            <EmoteButton icon={PersonStanding} label="Stand" hint="Just stand there, breathing." active={prefs.emote === "idle"} onClick={() => playEmote("idle")} />
            <span className="mx-1 h-6 w-px bg-cream/10" aria-hidden="true" />
            {EMOTES.map((emote) => (
              <EmoteButton key={emote.id} icon={EMOTE_ICONS[emote.id]} label={emote.label} hint={emote.hint} active={prefs.emote === emote.id} onClick={() => playEmote(emote.id)} />
            ))}
            <p className="ml-auto text-xs text-mute-2">Drag to turn, scroll to zoom</p>
          </div>
        </section>

        <aside aria-label="Your skins" className="flex flex-col gap-3">
          <div className="flex items-baseline gap-2">
            <h2 className="font-display text-xl font-bold">Your skins</h2>
            {skins.length > 0 && <span className="text-sm font-semibold text-mute">{skins.length}</span>}
          </div>
          {library === null ? (
            <Loading label="Loading skins" className="grid grid-cols-3 gap-2.5">
              {[0, 1, 2].map((index) => (
                <Skeleton key={index} className="h-[150px] rounded-2xl" />
              ))}
            </Loading>
          ) : skins.length === 0 ? (
            <div className="flex flex-col items-center gap-3 rounded-[22px] border border-dashed border-cream/15 bg-cream/2 px-5 py-8 text-center">
              <Shirt size={30} className="text-mute" aria-hidden="true" />
              <p className="text-sm leading-relaxed text-mute">No skins yet. Import a PNG (or drop it here), grab any player's current skin by name, or paint your own from Wisp's.</p>
              <div className="flex flex-wrap justify-center gap-2">
                <Button variant="secondary" icon={UserRound} onClick={() => setLookup(true)}>
                  From a player
                </Button>
                <Button icon={Pencil} onClick={() => void startEditing(null)}>
                  Paint one
                </Button>
              </div>
            </div>
          ) : (
            <ul className="grid grid-cols-3 gap-2.5">
              {skins.map((skin) => (
                <li key={skin.id}>
                  <SkinCard
                    skin={skin}
                    selected={skin.id === selected?.id}
                    active={skin.id === library.active}
                    onSelect={() => setSelectedId(skin.id)}
                    onWear={() =>
                      setActiveSkin(skin.id).then(async () => {
                        await reload();
                        announceSkinsChanged();
                        notify({ tone: "success", title: `Wearing ${skin.name}`, body: "It's your avatar in Wisp now." });
                      }, fail("Couldn't change skins"))
                    }
                    onEdit={() => void startEditing(skin)}
                    onRename={(name) => updateSkin(skin.id, { name }).then(() => void reload().then(announceSkinsChanged), fail("Couldn't rename it"))}
                    onModel={(model) => updateSkin(skin.id, { model }).then(() => void reload(), fail("Couldn't change it"))}
                    onExport={() => exportSkin(skin.id).then((path) => path && notify({ tone: "success", title: "Exported", body: path }), fail("Couldn't export it"))}
                    onDelete={() =>
                      deleteSkin(skin.id).then(async () => {
                        await reload();
                        announceSkinsChanged();
                      }, fail("Couldn't delete it"))
                    }
                  />
                </li>
              ))}
              <li>
                <button
                  type="button"
                  onClick={importFiles}
                  className="flex h-full min-h-[150px] w-full flex-col items-center justify-center gap-2 rounded-2xl border-[1.5px] border-dashed border-cream/18 text-xs font-bold text-cream-300 transition-colors hover:border-cream/40 hover:text-cream"
                >
                  <Plus size={18} aria-hidden="true" />
                  Add
                </button>
              </li>
            </ul>
          )}
          <p className="flex items-start gap-2 rounded-2xl bg-cream/[0.04] px-3.5 py-3 text-xs leading-relaxed text-mute">
            <Info size={14} className="mt-px shrink-0" aria-hidden="true" />
            The skin you wear is your avatar in Wisp. Minecraft itself only loads skins from Mojang, so wearing one in game comes with Microsoft sign-in. Emotes are just for Wisp.
          </p>
        </aside>
      </m.div>

      <PlayerSkinDialog
        open={lookup}
        onClose={() => setLookup(false)}
        onAdd={async (found) => {
          try {
            const skin = await addSkin(found.name, found.model, found.png, `player:${found.name}`);
            await reload();
            announceSkinsChanged();
            setSelectedId(skin.id);
            setLookup(false);
            notify({ tone: "success", title: `Added ${found.name}'s skin`, body: "Wear it, edit it, or take it to the throne." });
          } catch (failure: unknown) {
            fail("Couldn't add it")(failure);
          }
        }}
      />

      <AnimatePresence>
        {dragging && (
          <m.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} className="pointer-events-none fixed inset-0 z-[70] grid place-items-center bg-ink-950/70 backdrop-blur-sm">
            <div className="flex flex-col items-center gap-3 rounded-[28px] border-2 border-dashed border-cream/40 bg-ink-800/90 px-14 py-12 text-center">
              <FolderInput size={40} aria-hidden="true" />
              <p className="font-display text-2xl font-bold">Drop to add skins</p>
              <p className="text-sm text-mute">PNG skins, 64 × 64 (old 64 × 32 ones are converted)</p>
            </div>
          </m.div>
        )}
      </AnimatePresence>
    </Page>
  );
}

function EmoteButton({ icon: Icon, label, hint, active, onClick }: { icon: LucideIcon; label: string; hint: string; active: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      aria-pressed={active}
      title={active ? `${hint} Press again to replay.` : hint}
      onClick={onClick}
      className={cn(
        "flex h-10 items-center gap-2 rounded-full border px-4 text-[13px] font-bold transition-colors duration-150",
        active ? "border-cream/60 bg-cream text-ink-900" : "border-cream/12 bg-cream/[0.04] text-cream-300 hover:border-cream/30 hover:text-cream",
      )}
    >
      <Icon size={15} aria-hidden="true" />
      {label}
    </button>
  );
}

interface SkinCardProps {
  skin: Skin;
  selected: boolean;
  active: boolean;
  onSelect: () => void;
  onWear: () => void;
  onEdit: () => void;
  onRename: (name: string) => void;
  onModel: (model: SkinModel) => void;
  onExport: () => void;
  onDelete: () => void;
}

function SkinCard({ skin, selected, active, onSelect, onWear, onEdit, onRename, onModel, onExport, onDelete }: SkinCardProps) {
  const [menu, setMenu] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [draft, setDraft] = useState(skin.name);
  const more = useRef<HTMLButtonElement>(null);
  const source = useSkinSource(skin);
  const from = skin.source?.startsWith("player:") ? `from ${skin.source.slice(7)}` : skin.source === "editor" ? "made here" : null;

  return (
    <div className={cn("group relative flex flex-col items-center gap-1.5 rounded-2xl border p-2.5 pt-3 transition-colors", selected ? "border-cream/45 bg-cream/6" : "border-cream/8 bg-ink-800 hover:border-cream/20")}>
      <button type="button" onClick={onSelect} aria-label={`Preview ${skin.name}`} className="absolute inset-0 rounded-2xl" />
      {source ? (
        <SkinDoll src={source} slim={skin.model === "slim"} className="pointer-events-none relative h-[88px] w-[44px] drop-shadow-[0_8px_10px_rgba(0,0,0,0.5)] transition-transform duration-300 ease-out-expo group-hover:-translate-y-0.5" />
      ) : (
        <Skeleton className="h-[88px] w-[44px] rounded-md" />
      )}
      {renaming ? (
        <input
          autoFocus
          value={draft}
          maxLength={48}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={() => {
            setRenaming(false);
            if (draft.trim() && draft.trim() !== skin.name) onRename(draft.trim());
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") event.currentTarget.blur();
            if (event.key === "Escape") {
              setDraft(skin.name);
              setRenaming(false);
            }
          }}
          className="relative z-10 w-full rounded-md bg-ink-900 px-1.5 text-center text-xs font-bold text-cream outline-none ring-1 ring-cream/30"
        />
      ) : (
        <p className="pointer-events-none relative w-full truncate text-center text-xs font-bold">{skin.name}</p>
      )}
      <p className="pointer-events-none relative -mt-1 w-full truncate text-center text-[10px] text-mute-2">{from ?? (skin.model === "slim" ? "slim" : "classic")}</p>
      {active ? (
        <span className="relative flex items-center gap-1 rounded-full bg-moss/15 px-2 py-0.5 text-[10px] font-bold text-moss-soft">
          <Check size={10} strokeWidth={3} aria-hidden="true" />
          Wearing
        </span>
      ) : (
        <button type="button" onClick={onWear} className="relative z-10 rounded-full bg-cream/10 px-2.5 py-0.5 text-[10px] font-bold text-cream-300 transition-colors hover:bg-cream hover:text-ink-900">
          Wear
        </button>
      )}
      <button
        ref={more}
        type="button"
        aria-label={`More for ${skin.name}`}
        aria-expanded={menu}
        onClick={() => setMenu((open) => !open)}
        className="absolute right-1 top-1 z-10 grid size-7 place-items-center rounded-full text-mute opacity-0 transition-opacity hover:bg-cream/8 hover:text-cream group-hover:opacity-100 focus-visible:opacity-100 aria-expanded:opacity-100"
      >
        <MoreHorizontal size={15} aria-hidden="true" />
      </button>
      <Popover open={menu} anchorRef={more} placement="bottom-end" onClose={() => setMenu(false)} label={`${skin.name} options`} className="w-[220px]">
        <div role="menu" className="flex flex-col">
          <MenuItem icon={<Pencil size={15} aria-hidden="true" />} label="Edit pixels" onSelect={() => (setMenu(false), onEdit())} />
          <MenuItem icon={<Type size={15} aria-hidden="true" />} label="Rename" onSelect={() => (setMenu(false), setRenaming(true))} />
          <MenuItem icon={<Shirt size={15} aria-hidden="true" />} label="Classic arms" checked={skin.model === "classic"} onSelect={() => (setMenu(false), onModel("classic"))} />
          <MenuItem icon={<Shirt size={15} aria-hidden="true" />} label="Slim arms" checked={skin.model === "slim"} onSelect={() => (setMenu(false), onModel("slim"))} />
          <MenuItem icon={<Download size={15} aria-hidden="true" />} label="Export PNG" onSelect={() => (setMenu(false), onExport())} />
          <div className="mx-2 my-1 h-px bg-cream/8" />
          {confirming ? (
            <div className="flex items-center gap-1 px-2 py-1.5 text-xs">
              <span className="flex-1 font-semibold text-cream-300">Delete it?</span>
              <button type="button" onClick={() => (setMenu(false), setConfirming(false), onDelete())} className="rounded-full bg-coral px-2.5 py-1 font-bold text-ink-900">
                Delete
              </button>
              <button type="button" onClick={() => setConfirming(false)} className="rounded-full px-2 py-1 font-bold text-cream-300 hover:bg-cream/8">
                Keep
              </button>
            </div>
          ) : (
            <MenuItem icon={<Trash2 size={15} aria-hidden="true" />} label="Delete" onSelect={() => setConfirming(true)} />
          )}
        </div>
      </Popover>
    </div>
  );
}

/** Looks up a player by name and shows their skin before adding it. */
function PlayerSkinDialog({ open, onClose, onAdd }: { open: boolean; onClose: () => void; onAdd: (skin: PlayerSkin) => Promise<void> }) {
  const [name, setName] = useState("");
  const [found, setFound] = useState<PlayerSkin | null>(null);
  const [state, setState] = useState<"idle" | "looking" | "adding">("idle");
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    if (!open) {
      setFound(null);
      setError(null);
      setState("idle");
    }
  }, [open]);
  const look = async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    setState("looking");
    setError(null);
    setFound(null);
    try {
      setFound(await fetchPlayerSkin(trimmed));
    } catch (failure: unknown) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setState("idle");
    }
  };
  const preview = found ? `data:image/png;base64,${found.png}` : "";
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Get a player's skin"
      description="Type any Minecraft: Java Edition name. Wisp asks Mojang for the skin that player wears right now."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            icon={Plus}
            disabled={!found || state !== "idle"}
            onClick={() => {
              if (!found) return;
              setState("adding");
              void onAdd(found).finally(() => setState("idle"));
            }}
          >
            Add to my skins
          </Button>
        </>
      }
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void look();
        }}
        className="flex gap-2"
      >
        <label className="flex h-11 flex-1 items-center gap-2 rounded-xl border border-cream/12 bg-ink-900/60 px-3.5 focus-within:border-cream/35">
          <UserRound size={16} className="text-mute" aria-hidden="true" />
          <span className="sr-only">Player name</span>
          <input autoFocus value={name} maxLength={16} spellCheck={false} onChange={(event) => setName(event.target.value)} placeholder="Player name" className="min-w-0 flex-1 bg-transparent text-sm text-cream outline-none placeholder:text-mute" />
        </label>
        <Button type="submit" disabled={!name.trim() || state === "looking"}>
          {state === "looking" ? "Looking…" : "Look up"}
        </Button>
      </form>
      {error && <p className="mt-3 rounded-xl bg-coral/10 px-3.5 py-2.5 text-sm text-coral">{error.replace(/^./, (first) => first.toUpperCase())}.</p>}
      {found && (
        <div className="mt-4 flex items-center gap-4 rounded-2xl bg-cream/[0.04] p-4">
          <SkinDoll src={preview} slim={found.model === "slim"} className="h-[96px] w-[48px]" />
          <div>
            <p className="font-display text-lg font-bold">{found.name}</p>
            <p className="text-sm text-mute">{found.model === "slim" ? "Slim" : "Classic"} arms</p>
          </div>
        </div>
      )}
    </Dialog>
  );
}
