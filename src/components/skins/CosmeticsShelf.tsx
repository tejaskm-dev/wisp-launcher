import { Check, MoreHorizontal, Pencil, Plus, Shirt, Trash2 } from "lucide-react";
import { useRef, useState } from "react";
import { addCosmetic, deleteCosmetic, renameCosmetic, wearCosmetic } from "../../data/api";
import type { Cosmetic, SkinLibrary } from "../../data/types";
import { cn } from "../../lib/cn";
import { useApp } from "../../state/AppContext";
import { MenuItem, Popover } from "../ui/Popover";
import { outerOnly, useWorn } from "./cosmetics";
import { SkinDoll, loadSkinImage } from "./SkinDoll";

const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure));

/**
 * Cosmetics: outer layers you keep (a suit, a hoodie, armour) and wear over any skin, switched in
 * one click. Each shows on the skin you're looking at; the one you pick is what you wear, in Wisp
 * and in game.
 */
export function CosmeticsShelf({ library, skin, skinName, slim, onChanged }: { library: SkinLibrary; skin: string | HTMLCanvasElement | null; skinName: string; slim: boolean; onChanged: () => Promise<unknown> }) {
  const { notify } = useApp();
  const cosmetics = library.cosmetics ?? [];
  const worn = library.wornCosmetic ?? null;
  const [saving, setSaving] = useState(false);

  const wear = (id: string | null) => wearCosmetic(id).then(onChanged, (failure: unknown) => notify({ tone: "info", title: "Couldn't change it", body: message(failure) }));
  const keepOuterLayer = async () => {
    if (!skin) return;
    setSaving(true);
    try {
      const picture = typeof skin === "string" ? await loadSkinImage(skin) : skin;
      const made = await addCosmetic(`${skinName}'s outfit`, outerOnly(picture));
      await onChanged();
      notify({ tone: "success", title: "Kept as a cosmetic", body: `Wear “${made.name}” over any skin from here. Rename it from its menu.` });
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't keep it", body: message(failure) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <section aria-label="Cosmetics" className="flex flex-col gap-2.5">
      <div className="flex items-baseline gap-2">
        <h2 className="font-display text-lg font-bold">Cosmetics</h2>
        <p className="text-xs text-mute">Outer layers to wear over any skin: a hood, a suit, armour.</p>
      </div>
      <ul className="flex gap-2.5 overflow-x-auto pb-1">
        <li className="shrink-0">
          <CosmeticCard name="The skin's own" cosmetic={null} skin={skin} slim={slim} worn={worn === null} onWear={() => void wear(null)} />
        </li>
        {cosmetics.map((cosmetic) => (
          <li key={cosmetic.id} className="shrink-0">
            <CosmeticCard
              name={cosmetic.name}
              cosmetic={cosmetic}
              skin={skin}
              slim={slim}
              worn={worn === cosmetic.id}
              onWear={() => void wear(cosmetic.id)}
              onRename={(name) => void renameCosmetic(cosmetic.id, name).then(onChanged)}
              onDelete={() => void deleteCosmetic(cosmetic.id).then(onChanged)}
            />
          </li>
        ))}
        <li className="shrink-0">
          <button
            type="button"
            onClick={() => void keepOuterLayer()}
            disabled={!skin || saving}
            title="Keep this skin's outer layer as a cosmetic"
            className="flex h-[150px] w-[104px] flex-col items-center justify-center gap-2 rounded-2xl border-[1.5px] border-dashed border-cream/18 px-2 text-center text-xs font-bold text-cream-300 transition-colors hover:border-cream/40 hover:text-cream disabled:opacity-50"
          >
            <Plus size={18} aria-hidden="true" />
            {saving ? "Keeping…" : "Keep this outer layer"}
          </button>
        </li>
      </ul>
    </section>
  );
}

function CosmeticCard({
  name,
  cosmetic,
  skin,
  slim,
  worn,
  onWear,
  onRename,
  onDelete,
}: {
  name: string;
  cosmetic: Cosmetic | null;
  skin: string | HTMLCanvasElement | null;
  slim: boolean;
  worn: boolean;
  onWear: () => void;
  onRename?: (name: string) => void;
  onDelete?: () => void;
}) {
  const shown = useWorn(skin, cosmetic?.id ?? null);
  const [menu, setMenu] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(name);
  const more = useRef<HTMLButtonElement>(null);
  return (
    <div className={cn("group relative flex h-[150px] w-[104px] flex-col items-center rounded-2xl border bg-ink-800 px-2 pb-2 pt-3 transition-colors", worn ? "border-ember/70 bg-ember/8" : "border-cream/8 hover:border-cream/20")}>
      <button type="button" onClick={onWear} aria-pressed={worn} aria-label={`Wear ${name}`} className="absolute inset-0 z-0 rounded-2xl" />
      {shown ? (
        <SkinDoll src={shown} slim={slim} className="pointer-events-none relative h-[84px] w-[42px] drop-shadow-[0_8px_10px_rgba(0,0,0,0.5)]" />
      ) : (
        <Shirt size={28} className="pointer-events-none relative mt-6 text-mute" aria-hidden="true" />
      )}
      {renaming ? (
        <form
          className="relative z-10 mt-auto w-full"
          onSubmit={(event) => {
            event.preventDefault();
            setRenaming(false);
            if (draft.trim() && draft !== name) onRename?.(draft.trim());
          }}
        >
          <input value={draft} onChange={(event) => setDraft(event.target.value)} onBlur={() => setRenaming(false)} autoFocus aria-label="Cosmetic name" className="w-full rounded-md bg-ink-900 px-1.5 py-1 text-center text-[11px] text-cream outline-none" />
        </form>
      ) : (
        <p className="pointer-events-none relative mt-auto line-clamp-2 text-center text-[11px] font-bold leading-tight">{name}</p>
      )}
      {worn && (
        <span className="pointer-events-none absolute left-2 top-2 grid size-5 place-items-center rounded-full bg-ember text-ink-900" aria-hidden="true">
          <Check size={12} strokeWidth={3} />
        </span>
      )}
      {cosmetic && (
        <>
          <button
            ref={more}
            type="button"
            aria-label={`More for ${name}`}
            aria-expanded={menu}
            onClick={() => setMenu((open) => !open)}
            className="absolute right-1.5 top-1.5 z-10 grid size-7 place-items-center rounded-full text-mute opacity-0 transition-opacity hover:bg-cream/8 hover:text-cream focus-visible:opacity-100 group-hover:opacity-100"
          >
            <MoreHorizontal size={15} aria-hidden="true" />
          </button>
          <Popover open={menu} anchorRef={more} placement="bottom-end" onClose={() => setMenu(false)} label={`More for ${name}`}>
            <div className="flex w-44 flex-col p-1.5">
              <MenuItem icon={<Pencil size={15} aria-hidden="true" />} label="Rename" onSelect={() => (setMenu(false), setDraft(name), setRenaming(true))} />
              <MenuItem icon={<Trash2 size={15} aria-hidden="true" />} label="Delete" onSelect={() => (setMenu(false), onDelete?.())} />
            </div>
          </Popover>
        </>
      )}
    </div>
  );
}
