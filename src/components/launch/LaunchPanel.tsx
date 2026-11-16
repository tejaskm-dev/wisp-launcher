import { Check, Cpu, FolderOpen, LoaderCircle, Play, RotateCw, Square, Undo2, Wrench, X } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useState } from "react";
import { applyFix, installRosetta, openInstanceFolder, undoFix } from "../../data/api";
import type { FixOffer, FixReport } from "../../data/types";
import { cn } from "../../lib/cn";
import { useApp } from "../../state/AppContext";
import { Ghost } from "../brand/Ghost";
import { InstanceAvatar } from "../instances/InstanceAvatar";
import { ProgressBar } from "../ui/ProgressBar";
import { describeLaunch } from "./describe";

/**
 * One card per instance that's starting, running or just failed. Lives in the bottom-right
 * corner on every page, so a launch's progress is never out of sight.
 */
/** Apple's installer runs behind a macOS password prompt; once it's done, the game starts. */
function InstallRosettaButton({ onInstalled }: { onInstalled: () => void }) {
  const { notify } = useApp();
  const [busy, setBusy] = useState(false);
  const install = async () => {
    setBusy(true);
    try {
      if (await installRosetta()) {
        notify({ tone: "success", title: "Rosetta is installed", body: "Starting Minecraft…" });
        onInstalled();
      }
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't install Rosetta", body: failure instanceof Error ? failure.message : String(failure) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <button
      type="button"
      disabled={busy}
      onClick={() => void install()}
      title="Apple's translator for Intel apps. macOS asks for your password."
      className="flex h-8 items-center gap-1.5 rounded-full bg-cream px-3.5 text-xs font-bold text-ink-900 transition-transform active:scale-95 disabled:opacity-70"
    >
      {busy ? <LoaderCircle size={12} className="animate-spin" aria-hidden="true" /> : <Cpu size={12} aria-hidden="true" />}
      {busy ? "Installing Rosetta…" : "Install Rosetta"}
    </button>
  );
}

/** What Wisp worked out about a crash: what it fixed (with Undo), what it can do with a click, and what's left. */
function CrashHelp({ instanceId, help }: { instanceId: string; help: FixReport }) {
  const { notify } = useApp();
  const [undone, setUndone] = useState(false);
  const [applied, setApplied] = useState<string[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const fail = (title: string) => (failure: unknown) => notify({ tone: "info", title, body: failure instanceof Error ? failure.message : String(failure) });

  const undo = () => {
    if (!help.entry) return;
    setBusy("undo");
    undoFix(help.entry).then(() => setUndone(true), fail("Couldn't undo it")).finally(() => setBusy(null));
  };
  const take = (offer: FixOffer) => {
    setBusy(offer.label);
    applyFix(instanceId, offer.action)
      .then(() => {
        setApplied((current) => [...current, offer.label]);
        notify({ tone: "success", title: "Done", body: `${offer.label.replace(/^Turn off/, "Turned off")}. Press Play again; it's in the journal if you want it back.` });
      }, fail("Couldn't do that"))
      .finally(() => setBusy(null));
  };
  const more = help.found.slice(1, 4);

  return (
    <div className="flex flex-col gap-2.5 rounded-xl bg-ink-900/55 p-3 text-xs leading-snug">
      {more.length > 0 && (
        <ul className="flex flex-col gap-1 text-cream-300">
          {more.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
      )}
      {help.fixed.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <p className="flex items-center gap-1.5 font-bold text-moss-soft">
            <Wrench size={12} aria-hidden="true" />
            {undone ? "Undone" : "Wisp fixed it"}
          </p>
          <ul className={cn("flex flex-col gap-1", undone && "opacity-50")}>
            {help.fixed.map((line) => (
              <li key={line} className="flex gap-1.5 text-cream-300">
                <Check size={12} className="mt-px shrink-0 text-moss-soft" aria-hidden="true" />
                {line}
              </li>
            ))}
          </ul>
          {help.entry && !undone && (
            <button type="button" disabled={busy !== null} onClick={undo} className="flex w-fit items-center gap-1 font-bold text-cream-300 transition-colors hover:text-cream disabled:opacity-50">
              <Undo2 size={12} aria-hidden="true" />
              Undo
            </button>
          )}
        </div>
      )}
      {help.offers.map((offer) => {
        const done = applied.includes(offer.label);
        return (
          <div key={offer.label} className="flex flex-col gap-1.5">
            <p className="text-mute">{offer.detail}</p>
            <button
              type="button"
              disabled={done || busy !== null}
              onClick={() => take(offer)}
              className="flex h-7 w-fit items-center gap-1.5 rounded-full border border-cream/20 px-3 font-bold text-cream-300 transition-colors enabled:hover:border-cream/45 enabled:hover:text-cream disabled:opacity-60"
            >
              {done ? <Check size={12} aria-hidden="true" /> : busy === offer.label ? <LoaderCircle size={12} className="animate-spin" aria-hidden="true" /> : null}
              {done ? "Done" : offer.label}
            </button>
          </div>
        );
      })}
      {help.unsolved.map((line) => (
        <p key={line} className="text-mute">
          {line}
        </p>
      ))}
    </div>
  );
}

export function LaunchPanel() {
  const { launches, home, now, play, stop, dismissLaunch } = useApp();
  const entries = Object.entries(launches);

  return (
    <AnimatePresence initial={false}>
      {entries.map(([id, update]) => {
        const instance = home?.instances.find((candidate) => candidate.id === id);
        const view = describeLaunch(update, now);
        const name = instance?.name ?? "Minecraft";
        return (
          <m.section
            key={id}
            aria-label={`${name}: ${view.label}`}
            initial={{ opacity: 0, y: 16, scale: 0.96 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, x: 28, transition: { duration: 0.2 } }}
            transition={{ type: "spring", stiffness: 380, damping: 30 }}
            className={cn(
              "pointer-events-auto flex flex-col gap-3 rounded-2xl border bg-ink-700/95 p-3.5 shadow-[0_20px_50px_-12px_rgba(0,0,0,0.8)]",
              view.kind === "problem" ? "border-coral/30" : "border-cream/10",
            )}
          >
            <div className="flex items-center gap-3">
              {instance ? <InstanceAvatar instance={instance} className="size-10 text-base" /> : <Ghost size={34} />}
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-bold">
                  {name}
                  <span className={cn("ml-2 text-xs font-bold", view.kind === "problem" ? "text-coral" : view.kind === "running" ? "text-moss-soft" : "text-mute")}>
                    {view.kind === "problem" ? (update.phase === "exited" ? "Crashed" : "Didn't start") : view.label}
                  </span>
                </p>
                <p className={cn("mt-0.5 text-xs leading-snug", view.kind === "problem" ? "line-clamp-3 text-cream-300" : "truncate text-mute")}>
                  {view.detail}
                </p>
              </div>
              {view.kind === "problem" ? (
                <button
                  type="button"
                  aria-label="Dismiss"
                  onClick={() => dismissLaunch(id)}
                  className="self-start rounded-lg p-1 text-mute transition-colors hover:bg-cream/8 hover:text-cream"
                >
                  <X size={14} aria-hidden="true" />
                </button>
              ) : (
                <button
                  type="button"
                  onClick={() => stop(id)}
                  className="flex h-8 shrink-0 items-center gap-1.5 rounded-full border border-cream/15 px-3 text-xs font-bold text-cream-300 transition-colors hover:border-cream/35 hover:text-cream"
                >
                  <Square size={10} fill="currentColor" strokeWidth={0} aria-hidden="true" />
                  {view.kind === "running" ? "Stop" : "Cancel"}
                </button>
              )}
            </div>

            {view.kind === "busy" && <ProgressBar value={view.progress} label={view.label} />}
            {view.kind === "busy" && update.phase === "downloading" && (
              <p className="-mt-1.5 text-[11px] text-mute-2">The first launch of a version downloads it. Later launches start straight away.</p>
            )}

            {update.phase === "exited" && update.help && <CrashHelp instanceId={id} help={update.help} />}

            {view.kind === "problem" && (
              <div className="flex gap-2">
                {update.phase === "failed" && update.fix === "install-rosetta" ? (
                  <InstallRosettaButton onInstalled={() => play(id)} />
                ) : (
                  <button
                    type="button"
                    onClick={() => play(id)}
                    className="flex h-8 items-center gap-1.5 rounded-full bg-cream px-3.5 text-xs font-bold text-ink-900 transition-transform active:scale-95"
                  >
                    {update.phase === "exited" ? (
                      <Play size={11} fill="currentColor" strokeWidth={0} aria-hidden="true" />
                    ) : (
                      <RotateCw size={12} aria-hidden="true" />
                    )}
                    {view.label}
                  </button>
                )}
                {/* Logs only exist once the game has run. */}
                {instance && update.phase === "exited" && (
                  <button
                    type="button"
                    onClick={() => void openInstanceFolder(instance.id)}
                    className="flex h-8 items-center gap-1.5 rounded-full px-3 text-xs font-bold text-cream-300 transition-colors hover:bg-cream/8 hover:text-cream"
                  >
                    <FolderOpen size={13} aria-hidden="true" />
                    Logs
                  </button>
                )}
              </div>
            )}
          </m.section>
        );
      })}
    </AnimatePresence>
  );
}
