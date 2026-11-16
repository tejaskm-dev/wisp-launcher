import { Plus, Server } from "lucide-react";
import { useEffect, useState } from "react";
import { addServer } from "../../data/api";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { InstancePicker } from "./InstancePicker";
import { TextField } from "./TextField";

/** Adds a server to one instance's list (the list Minecraft's Multiplayer screen shows). */
export function AddServerDialog({ open, onClose, initial }: { open: boolean; onClose: () => void; initial?: { name: string; address: string } | null }) {
  const { home, notify, refreshHome } = useApp();
  const instances = home?.instances ?? [];
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [instanceId, setInstanceId] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setName(initial?.name ?? "");
    setAddress(initial?.address ?? "");
    setError(null);
    setBusy(false);
    setInstanceId(instances[0]?.id ?? "");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  const add = async () => {
    setBusy(true);
    setError(null);
    try {
      const saved = await addServer(instanceId, name, address);
      await refreshHome();
      notify({ tone: "success", title: `Added ${saved.name}`, body: "It's in the instance's server list, here and in the game." });
      onClose();
    } catch (failure: unknown) {
      setError(failure instanceof Error ? failure.message : String(failure));
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Add a server"
      description="It goes into the instance's own server list, the one Minecraft's Multiplayer screen shows."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button icon={Plus} disabled={!address.trim() || !instanceId || busy} onClick={() => void add()}>
            Add server
          </Button>
        </>
      }
    >
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          if (address.trim() && instanceId) void add();
        }}
      >
        <TextField label="Address" value={address} onChange={setAddress} placeholder="play.example.net" icon={<Server size={15} className="text-mute" aria-hidden="true" />} autoFocus maxLength={260} />
        <TextField label="Name" value={name} onChange={setName} placeholder="Defaults to the address" maxLength={64} />
        {instances.length > 0 ? (
          <InstancePicker instances={instances} value={instanceId} onChange={setInstanceId} label="Add to" />
        ) : (
          <p className="text-sm text-mute">Make an instance first: each instance keeps its own server list.</p>
        )}
        {error && <p className="rounded-xl bg-coral/10 px-3.5 py-2.5 text-sm text-coral">{error}</p>}
        <button type="submit" hidden />
      </form>
    </Dialog>
  );
}
