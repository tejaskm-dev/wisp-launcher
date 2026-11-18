import { Info } from "lucide-react";
import { useState, type FormEvent } from "react";
import { addOfflineAccount, testerBuild } from "../../data/api";
import type { Account } from "../../data/types";
import { Ghost } from "../brand/Ghost";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";

const FORM_ID = "add-account";
const USERNAME = /^\w{3,16}$/;

/** Developer builds only: an offline account, so launching can be tested before Microsoft sign-in. */
export function AddAccountDialog({ open, onClose, onAdded }: { open: boolean; onClose: () => void; onAdded: (account: Account) => void }) {
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const trimmed = name.trim();
  const valid = USERNAME.test(trimmed);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!valid || pending) return;
    setPending(true);
    setError(null);
    try {
      onAdded(await addOfflineAccount(trimmed));
      onClose();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setPending(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Add an offline account"
      description="Pick the name you'll have in game."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form={FORM_ID} disabled={!valid || pending}>
            {pending ? "Adding…" : "Add account"}
          </Button>
        </>
      }
    >
      <form id={FORM_ID} onSubmit={submit} className="flex flex-col gap-5">
        <label className="flex items-center gap-4 rounded-[18px] bg-cream/4 p-3.5">
          <Ghost size={44} />
          <span className="flex min-w-0 flex-1 flex-col gap-0.5">
            <span className="text-xs font-semibold text-mute">Username</span>
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Steve"
              maxLength={16}
              autoFocus
              spellCheck={false}
              autoComplete="off"
              className="min-w-0 bg-transparent font-display text-xl font-bold text-cream outline-none placeholder:text-cream/35"
            />
          </span>
        </label>
        <p className={trimmed && !valid ? "-mt-2 text-[13px] font-semibold text-coral" : "-mt-2 text-[13px] text-mute"}>
          3 to 16 letters, numbers or underscores.
        </p>
        <p className="flex items-start gap-2.5 rounded-2xl bg-cream/4 px-4 py-3 text-[13px] leading-relaxed text-mute">
          <Info size={16} className="mt-0.5 shrink-0" aria-hidden="true" />
          <span>
            {testerBuild
              ? "This is a tester build, so offline accounts are on until Microsoft sign-in is ready. Use one only if you own Minecraft: Java Edition. They play singleplayer, LAN and servers with online mode off. Released versions of Wisp ask for a Microsoft account that owns Minecraft first."
              : "You're on a developer build, so offline accounts are available for testing. They play singleplayer and offline servers. Released versions of Wisp will ask for a Microsoft account that owns Minecraft first."}
          </span>
        </p>
        {error && (
          <p role="alert" className="rounded-2xl border border-coral/25 bg-coral/10 px-4 py-3 text-sm font-semibold text-coral">
            {error}
          </p>
        )}
      </form>
    </Dialog>
  );
}
