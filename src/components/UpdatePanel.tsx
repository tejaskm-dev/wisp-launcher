import { ArrowUpCircle, X } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect } from "react";
import { formatBytes } from "../lib/format";
import { useApp } from "../state/AppContext";
import { checkForUpdate, installUpdate, updateLater, useUpdate } from "../state/updates";
import { Sparkle } from "./brand/Sparkle";
import { describeLaunch } from "./launch/describe";
import { ProgressBar } from "./ui/ProgressBar";

/** How often a running Wisp looks for a new version. */
const EVERY = 6 * 60 * 60 * 1000;

/**
 * A new Wisp, offered bottom right: install it and restart, or later. It waits while a game is
 * running, since restarting Wisp mid-game would lose track of the session.
 */
export function UpdatePanel() {
  const { launches, now } = useApp();
  const update = useUpdate();

  // A first look shortly after startup (once the window has settled), then every few hours.
  useEffect(() => {
    const first = window.setTimeout(() => void checkForUpdate(), 8000);
    const later = window.setInterval(() => void checkForUpdate(), EVERY);
    return () => {
      window.clearTimeout(first);
      window.clearInterval(later);
    };
  }, []);

  const playing = Object.values(launches).some((launch) => {
    const view = describeLaunch(launch, now);
    return view.kind === "busy" || view.kind === "running";
  });
  const shown =
    (update.phase === "available" && !update.later) || update.phase === "downloading" || update.phase === "restarting" || (update.phase === "failed" && update.version !== null);
  const version = "version" in update ? update.version : null;
  const progress = update.phase === "downloading" && update.total ? update.done / update.total : null;
  const firstLine = update.phase === "available" ? update.notes.split("\n").find((line) => line.trim() && !line.startsWith("#"))?.replace(/^[-*]\s*/, "") : null;

  return (
    <AnimatePresence initial={false}>
      {shown && (
        <m.section
          key="update"
          aria-label={`Wisp ${version ?? ""} update`}
          initial={{ opacity: 0, y: 16, scale: 0.96 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, x: 28, transition: { duration: 0.2 } }}
          transition={{ type: "spring", stiffness: 380, damping: 30 }}
          className="tail pointer-events-auto relative flex flex-col gap-3 rounded-2xl bg-ink-700 p-3.5 shadow-[inset_0_1px_0_rgba(244,238,228,0.07)] [--tail:26px] [filter:drop-shadow(0_20px_32px_rgba(0,0,0,0.65))]"
        >
          <div className="flex items-start gap-3">
            <span className="relative grid size-10 shrink-0 place-items-center rounded-xl bg-cream/8 text-cream">
              <ArrowUpCircle size={19} aria-hidden="true" />
              <Sparkle className="absolute -right-1 -top-1 size-3 text-ember" />
            </span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-bold">
                {update.phase === "downloading" ? `Updating to Wisp ${version}` : update.phase === "restarting" ? "Restarting on the new Wisp" : update.phase === "failed" ? "The update didn't finish" : `Wisp ${version} is ready`}
              </p>
              <p className={`mt-0.5 text-xs leading-snug ${update.phase === "failed" ? "line-clamp-3 text-coral" : "line-clamp-2 text-mute"}`}>
                {update.phase === "failed"
                  ? update.message
                  : update.phase === "downloading"
                    ? update.total
                      ? `${formatBytes(update.done)} of ${formatBytes(update.total)}`
                      : formatBytes(update.done)
                    : update.phase === "restarting"
                      ? "Back in a moment."
                      : playing
                        ? "It installs once you're done playing."
                        : (firstLine ?? "Install it now: Wisp restarts on the new version, right where you were.")}
              </p>
            </div>
            {(update.phase === "available" || update.phase === "failed") && (
              <button type="button" aria-label="Later" onClick={updateLater} className="rounded-lg p-1 text-mute transition-colors hover:bg-cream/8 hover:text-cream">
                <X size={14} aria-hidden="true" />
              </button>
            )}
          </div>
          {update.phase === "downloading" && <ProgressBar pixel value={progress} label={`Updating to Wisp ${version}`} />}
          {(update.phase === "available" || update.phase === "failed") && (
            <div className="flex items-center gap-2">
              <button
                type="button"
                disabled={playing}
                onClick={() => void (update.phase === "failed" ? checkForUpdate().then(installUpdate) : installUpdate())}
                className="tail wag relative flex h-8 items-center gap-1.5 rounded-full bg-cream px-3.5 text-xs font-bold text-ink-900 transition-transform active:scale-95 disabled:opacity-45 [--tail:17px]"
              >
                {update.phase === "failed" ? "Try again" : "Update and restart"}
              </button>
              {update.phase === "available" && (
                <button type="button" onClick={updateLater} className="h-8 rounded-full px-3 text-xs font-bold text-cream-300 transition-colors hover:bg-cream/8 hover:text-cream">
                  Later
                </button>
              )}
            </div>
          )}
        </m.section>
      )}
    </AnimatePresence>
  );
}
