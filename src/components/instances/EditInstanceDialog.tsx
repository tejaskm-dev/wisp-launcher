import { Trash2 } from "lucide-react";
import { useState, type FormEvent } from "react";
import { deleteInstance, updateInstance } from "../../data/api";
import type { Instance } from "../../data/types";
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
  const { refreshHome, notify } = useApp();
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
        {error && (
          <p role="alert" className="rounded-2xl border border-coral/25 bg-coral/10 px-4 py-3 text-sm font-semibold text-coral">
            {error}
          </p>
        )}
      </form>
    </Dialog>
  );
}
