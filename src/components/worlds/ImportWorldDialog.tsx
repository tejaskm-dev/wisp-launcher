import { Check, FolderInput } from "lucide-react";
import { useState } from "react";
import type { Instance } from "../../data/types";
import { cn } from "../../lib/cn";
import { instanceLabel } from "../../lib/format";
import { InstanceAvatar } from "../instances/InstanceAvatar";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";

/**
 * Asks which instance worlds go into, when there's more than one. Worlds need the instance's
 * Minecraft version (or newer) to open.
 */
export function ImportWorldDialog({
  open,
  instances,
  files,
  onClose,
  onImport,
}: {
  open: boolean;
  instances: Instance[];
  /** Dropped files, if that's how the import started (else a file picker opens after this). */
  files: string[] | null;
  onClose: () => void;
  onImport: (instanceId: string) => void;
}) {
  const [chosen, setChosen] = useState(instances[0]?.id ?? null);
  const count = files?.length ?? 0;
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={count > 1 ? `Import ${count} worlds` : "Import a world"}
      description="Pick the instance to put it in. A world opens in its own Minecraft version or a newer one."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button icon={FolderInput} disabled={!chosen} onClick={() => chosen && onImport(chosen)}>
            {files ? "Import" : "Choose files…"}
          </Button>
        </>
      }
    >
      <div role="radiogroup" aria-label="Instance" className="flex flex-col gap-1.5">
        {instances.map((instance) => {
          const selected = instance.id === chosen;
          return (
            <button
              key={instance.id}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => setChosen(instance.id)}
              className={cn(
                "flex items-center gap-3 rounded-2xl border p-2.5 text-left transition-colors",
                selected ? "border-cream/40 bg-cream/8" : "border-cream/8 hover:border-cream/20 hover:bg-cream/4",
              )}
            >
              <InstanceAvatar instance={instance} className="size-10 text-base" />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-sm font-bold">{instance.name}</span>
                <span className="text-xs text-mute">{instanceLabel(instance)}</span>
              </span>
              {selected && <Check size={16} strokeWidth={2.6} className="text-ember" aria-hidden="true" />}
            </button>
          );
        })}
      </div>
    </Dialog>
  );
}
