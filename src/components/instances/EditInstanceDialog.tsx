import { PackagePlus, Trash2, Zap } from "lucide-react";
import { useState, type FormEvent } from "react";
import { deleteInstance, speedUpInstance, updateInstance } from "../../data/api";
import type { Instance, SpedUp } from "../../data/types";
import { instanceLabel } from "../../lib/format";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { IconPicker } from "./IconPicker";
import { InstanceAvatar } from "./InstanceAvatar";
import { DEFAULT_ICONS, iconId } from "./icons";

const FORM_ID = "edit-instance";

/** Rename an instance, change its icon, or delete it (after a second, explicit confirmation). */
export function EditInstanceDialog({ instance, open, onClose }: { instance: Instance; open: boolean; onClose: () => void }) {
  const { refreshHome, notify, importFiles } = useApp();
  const [speeding, setSpeeding] = useState(false);
  const [sped, setSped] = useState<SpedUp | null>(null);
  const speedUp = async () => {
    setSpeeding(true);
    setError(null);
    try {
      const result = await speedUpInstance(instance.id);
      setSped(result);
      await refreshHome();
      notify({
        tone: "success",
        title: result.added.length > 0 ? `${instance.name} is sped up` : `${instance.name} already has them`,
        body: result.movedToFabric ? "It runs on Fabric now, so it can use mods. Your worlds are as they were." : undefined,
      });
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setSpeeding(false);
    }
  };
  const [name, setName] = useState(instance.name);
  const [icon, setIcon] = useState(instance.icon ?? iconId(DEFAULT_ICONS[instance.loader]));
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (work: () => Promise<unknown>, done: string) => {
    setPending(true);
    setError(null);
    try {
      await work();
      await refreshHome();
      notify({ tone: "success", title: done });
      onClose();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setPending(false);
    }
  };

  const save = (event: FormEvent) => {
    event.preventDefault();
    if (!name.trim() || pending) return;
    void run(() => updateInstance(instance.id, { name: name.trim(), icon }), "Instance updated");
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Edit instance"
      description={instanceLabel(instance)}
      footer={
        confirmDelete ? (
          <>
            <p className="mr-auto text-sm font-semibold text-coral">Delete {instance.name} and its worlds for good?</p>
            <Button variant="secondary" onClick={() => setConfirmDelete(false)}>
              Keep it
            </Button>
            <button
              type="button"
              disabled={pending}
              onClick={() => void run(() => deleteInstance(instance.id), `${instance.name} deleted`)}
              className="flex h-11 items-center gap-2 rounded-full bg-coral px-5 text-sm font-bold text-ink-900 transition-transform active:scale-95 disabled:opacity-50"
            >
              <Trash2 size={15} aria-hidden="true" />
              Delete
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              onClick={() => setConfirmDelete(true)}
              className="mr-auto flex h-11 items-center gap-2 rounded-full px-4 text-sm font-bold text-coral transition-colors hover:bg-coral/10"
            >
              <Trash2 size={15} aria-hidden="true" />
              Delete…
            </button>
            <Button variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" form={FORM_ID} disabled={!name.trim() || pending}>
              {pending ? "Saving…" : "Save"}
            </Button>
          </>
        )
      }
    >
      <form id={FORM_ID} onSubmit={save} className="flex flex-col gap-5">
        <div className="flex items-center gap-4 rounded-[18px] bg-cream/4 p-3.5">
          <InstanceAvatar instance={{ name, loader: instance.loader, icon }} className="size-14 text-2xl" />
          <label className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="text-xs font-semibold text-mute">Name</span>
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              maxLength={64}
              autoFocus
              spellCheck={false}
              className="min-w-0 bg-transparent font-display text-xl font-bold text-cream outline-none"
            />
          </label>
        </div>
        <div>
          <p className="mb-2 text-sm font-bold">Icon</p>
          <IconPicker value={icon} onChange={setIcon} />
          {instance.icon?.startsWith("https://") && (
            <button type="button" onClick={() => setIcon(instance.icon ?? "")} className="mt-3 text-xs font-bold text-cream-300 underline-offset-2 hover:underline">
              Use the modpack's own icon
            </button>
          )}
        </div>
        <div className="flex flex-col gap-2">
          <div className="flex items-start gap-3.5 rounded-[18px] bg-cream/4 p-3.5">
            <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-ember/15 text-ember">
              <Zap size={18} aria-hidden="true" />
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-bold">Speed it up</p>
              <p className="mt-0.5 text-xs leading-relaxed text-mute">{sped ? spedSummary(sped) : SPEED_UP_NOTES[instance.loader]}</p>
            </div>
            <Button variant="secondary" disabled={speeding || pending} onClick={() => void speedUp()} className="h-9 shrink-0 px-4 text-[13px]">
              {speeding ? "Adding…" : sped ? "Check again" : "Speed up"}
            </Button>
          </div>
          <div className="flex items-start gap-3.5 rounded-[18px] bg-cream/4 p-3.5">
            <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-cream/8 text-cream-300">
              <PackagePlus size={18} aria-hidden="true" />
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-bold">Mods and packs of your own</p>
              <p className="mt-0.5 text-xs leading-relaxed text-mute">Add .jar mods and resource or shader pack zips you have.</p>
            </div>
            <Button
              variant="secondary"
              onClick={() => {
                onClose();
                importFiles(null, instance.id);
              }}
              className="h-9 shrink-0 px-4 text-[13px]"
            >
              Add files
            </Button>
          </div>
        </div>
        {error && (
          <p role="alert" className="rounded-2xl border border-coral/25 bg-coral/10 px-4 py-3 text-sm font-semibold text-coral">
            {error}
          </p>
        )}
      </form>
    </Dialog>
  );
}

const SPEED_UP_NOTES: Record<Instance["loader"], string> = {
  vanilla: "Moves it to Fabric and adds Sodium, Lithium, FerriteCore, ModernFix and friends: often several times the frame rate, and quicker loading. Your worlds stay as they are.",
  fabric: "Adds Sodium, Lithium, FerriteCore, ModernFix and friends where they have builds for this version: more frames, less memory, quicker loading.",
  quilt: "Adds Sodium, Lithium, FerriteCore, ModernFix and friends where they have builds for this version: more frames, less memory, quicker loading.",
  neoforge: "Adds Sodium (or Embeddium), FerriteCore, ModernFix and friends where they have builds for this version: more frames, less memory, quicker loading.",
  forge: "Adds Embeddium, FerriteCore, ModernFix and friends where they have builds for this version: more frames, less memory, quicker loading.",
};

/** "Added Sodium, Lithium. Already had FerriteCore. No build for this version: Dynamic FPS." */
function spedSummary(sped: SpedUp) {
  const added = sped.added.filter((item) => item.explicit).map((item) => item.title);
  const parts = [
    added.length > 0 ? `Added ${added.join(", ")}.` : "Nothing new to add.",
    sped.had.length > 0 ? `Already had ${sped.had.join(", ")}.` : "",
    sped.skipped.length > 0 ? `Left out (no build for this version, or it would clash): ${sped.skipped.join(", ")}.` : "",
  ];
  return parts.filter(Boolean).join(" ");
}
