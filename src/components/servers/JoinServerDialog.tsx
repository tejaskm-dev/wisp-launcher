import { Play, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { addServer } from "../../data/api";
import type { ServerStatus } from "../../data/types";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { InstancePicker } from "./InstancePicker";

export interface JoinTarget {
  name: string;
  address: string;
  /** Instances that already list it, best first. */
  instanceIds?: string[];
}

/** Picks the instance to join a server with, and whether to keep it in that instance's list. */
export function JoinServerDialog({ target, status, onClose }: { target: JoinTarget | null; status?: ServerStatus; onClose: () => void }) {
  const { home, play, notify, refreshHome, openNewInstance } = useApp();
  const instances = home?.instances ?? [];
  const [instanceId, setInstanceId] = useState("");
  const [keep, setKeep] = useState(true);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!target) return;
    setInstanceId(target.instanceIds?.find((id) => instances.some((instance) => instance.id === id)) ?? instances[0]?.id ?? "");
    setKeep(true);
    setBusy(false);
    // Only when a new server is picked, not on every library refresh.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target]);

  const listed = !!target?.instanceIds?.includes(instanceId);
  const join = async () => {
    if (!target || !instanceId) return;
    setBusy(true);
    if (keep && !listed) {
      try {
        await addServer(instanceId, target.name, target.address);
        void refreshHome();
      } catch {
        // Already there, or the list couldn't be written: joining still works.
      }
    }
    play(instanceId, null, target.address);
    notify({ tone: "info", title: `Joining ${target.name}`, body: "Minecraft goes straight to the server once it's open." });
    onClose();
  };

  return (
    <Dialog
      open={target !== null}
      onClose={onClose}
      title={target ? `Join ${target.name}` : "Join a server"}
      description={target ? `${target.address}${status?.version ? ` · runs ${status.version}` : ""}` : undefined}
      footer={
        instances.length === 0 ? (
          <Button icon={Plus} onClick={() => (onClose(), openNewInstance())}>
            New instance
          </Button>
        ) : (
          <>
            <Button variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button icon={Play} disabled={!instanceId || busy} onClick={() => void join()}>
              Join
            </Button>
          </>
        )
      }
    >
      {instances.length === 0 ? (
        <p className="text-sm leading-relaxed text-mute">Servers are joined from an instance. Make one first; a plain Vanilla one on the latest version suits most servers.</p>
      ) : (
        <div className="flex flex-col gap-4">
          <InstancePicker instances={instances} value={instanceId} onChange={setInstanceId} />
          <p className="text-[13px] leading-relaxed text-mute">
            Most servers let several Minecraft versions in. Modded servers need the matching modpack, so join those with its instance.
          </p>
          {!listed && (
            <label className="flex cursor-pointer items-center gap-2.5 text-sm font-semibold text-cream-300">
              <input type="checkbox" checked={keep} onChange={(event) => setKeep(event.target.checked)} className="size-4 accent-[#f4eee4]" />
              Keep it in this instance's server list
            </label>
          )}
        </div>
      )}
    </Dialog>
  );
}
