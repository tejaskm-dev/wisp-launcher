import { KeyRound, LoaderCircle, Search, Users } from "lucide-react";
import { useEffect, useState, type FormEvent } from "react";
import { joinServerCode, lookUpServerCode, onJoinProgress } from "../../data/api";
import type { CodeLookup, JoinProgress } from "../../data/types";
import { loaderName } from "../../lib/format";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";

const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure));
const runsName = (loader: string, software: string | null | undefined) => (software ? software.charAt(0).toUpperCase() + software.slice(1) : loaderName(loader));

/**
 * Joins a server hosted with Wisp by its code (WISP-XXXX-XXXX-XXXX): Wisp finds where the server
 * is now and joins it the way it joins a friend's, its mods and your skin along.
 */
export function JoinCodeDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { notify } = useApp();
  const [typed, setTyped] = useState("");
  const [found, setFound] = useState<CodeLookup | null>(null);
  const [looking, setLooking] = useState(false);
  const [joining, setJoining] = useState<JoinProgress | null>(null);
  const [consent, setConsent] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!found) return;
    return onJoinProgress((progress) => progress.code === found.code && setJoining(progress.step === "done" || progress.step === "failed" ? null : progress));
  }, [found]);

  const lookUp = async (event: FormEvent) => {
    event.preventDefault();
    setLooking(true);
    setError(null);
    setFound(null);
    setConsent(null);
    try {
      const result = await lookUpServerCode(typed);
      if (result.server) setFound(result);
      else setError(`No server has ${result.code}. Check it, or ask whoever runs it to start the server.`);
    } catch (failure: unknown) {
      setError(message(failure));
    } finally {
      setLooking(false);
    }
  };

  const join = async (allowHostFiles: boolean) => {
    if (!found) return;
    setConsent(null);
    setError(null);
    setJoining({ code: found.code, serverId: "", step: "asking", doneBytes: 0, totalBytes: 0, message: null });
    try {
      const result = await joinServerCode(found.code, allowHostFiles);
      if (result.kind === "needs-consent") setConsent(result.files);
      else if (result.kind === "joined") {
        notify({ tone: "success", title: `Joining ${found.server?.name ?? "the server"}`, body: "The game starts straight into it." });
        onClose();
      }
    } catch (failure: unknown) {
      setError(message(failure));
    } finally {
      setJoining(null);
    }
  };

  const server = found?.server ?? null;
  const progressLabel =
    joining?.step === "mods" && joining.totalBytes > 0
      ? `Getting mods ${Math.round((joining.doneBytes / joining.totalBytes) * 100)}%`
      : joining?.step === "launching"
        ? "Starting the game"
        : joining
          ? "Connecting"
          : null;

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Join with a code"
      description="Servers hosted with Wisp have a code. Type it in and Wisp finds the server wherever it is, and brings its mods and your skin along."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          {server && (
            <Button icon={progressLabel ? undefined : Users} disabled={!server.online || joining !== null} onClick={() => void join(consent !== null)}>
              {progressLabel ?? (consent ? "Install and join" : "Join")}
            </Button>
          )}
        </>
      }
    >
      <form onSubmit={(event) => void lookUp(event)} className="flex flex-col gap-4">
        <div className="flex gap-2">
          <label className="flex h-11 min-w-0 flex-1 items-center gap-2 rounded-xl border border-cream/12 bg-ink-900/60 px-3 focus-within:border-cream/35">
            <KeyRound size={16} className="shrink-0 text-mute" aria-hidden="true" />
            <input
              value={typed}
              onChange={(event) => setTyped(event.target.value.toUpperCase())}
              placeholder="WISP-7K2M-9QXA-4F2P"
              spellCheck={false}
              autoFocus
              aria-label="Server code"
              className="min-w-0 flex-1 bg-transparent font-mono text-[15px] tracking-wider text-cream outline-none placeholder:text-mute-2"
            />
          </label>
          <Button type="submit" variant="secondary" icon={looking ? undefined : Search} disabled={looking || typed.trim().length < 12}>
            {looking ? "Looking…" : "Look up"}
          </Button>
        </div>
        {server && (
          <div className="flex items-center gap-3 rounded-[18px] bg-cream/5 p-4">
            <span className={`size-2.5 shrink-0 rounded-full ${server.online ? "bg-moss" : "bg-mute-2"}`} aria-hidden="true" />
            <div className="min-w-0 flex-1">
              <p className="truncate font-bold">{server.name}</p>
              <p className="truncate text-xs text-mute">
                {runsName(server.loader, server.software)} {server.gameVersion}
                {server.mods > 0 ? ` · ${server.mods} mods` : ""}
                {server.online ? ` · ${server.players} of ${server.maxPlayers} on` : " · not running right now"}
              </p>
            </div>
            {joining && <LoaderCircle size={16} className="animate-spin text-mute" aria-label="Joining" />}
          </div>
        )}
        {consent && (
          <div className="flex flex-col gap-2 rounded-[18px] bg-amber/8 p-4 text-sm text-amber-soft">
            <p>Joining also installs files only this server has, not from Modrinth. Mods run code on your computer, so allow this only if you trust whoever runs it.</p>
            <ul className="max-h-32 overflow-y-auto font-mono text-xs text-cream-300">
              {consent.map((file) => (
                <li key={file} className="truncate">
                  {file}
                </li>
              ))}
            </ul>
          </div>
        )}
        {error && <p className="rounded-xl bg-coral/10 px-3.5 py-2.5 text-sm text-coral">{error}</p>}
      </form>
    </Dialog>
  );
}
