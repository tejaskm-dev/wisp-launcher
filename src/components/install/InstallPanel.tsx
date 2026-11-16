import { Package, RotateCw, X } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { formatBytes } from "../../lib/format";
import { useApp } from "../../state/AppContext";
import { ProgressBar } from "../ui/ProgressBar";

/** Bottom-right cards for modpacks installing in the background. */
export function InstallPanel() {
  const { installs, dismissInstall, startModpackInstall } = useApp();
  return (
    <AnimatePresence initial={false}>
      {Object.values(installs).map((job) => {
        const { update } = job;
        const failed = update.phase === "failed";
        const progress = update.phase === "downloading" && update.totalBytes > 0 ? update.doneBytes / update.totalBytes : null;
        const detail =
          update.phase === "downloading"
            ? `${update.doneFiles} of ${update.totalFiles} files · ${formatBytes(update.doneBytes)} of ${formatBytes(update.totalBytes)}`
            : update.phase === "unpacking"
              ? "Unpacking configs and extras"
              : update.phase === "failed"
                ? update.message
                : "Getting the pack";
        return (
          <m.section
            key={job.jobId}
            aria-label={`${job.title}: ${failed ? "install failed" : "installing"}`}
            initial={{ opacity: 0, y: 16, scale: 0.96 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, x: 28, transition: { duration: 0.2 } }}
            transition={{ type: "spring", stiffness: 380, damping: 30 }}
            className={`pointer-events-auto flex flex-col gap-3 rounded-2xl border bg-ink-700/95 p-3.5 shadow-[0_20px_50px_-12px_rgba(0,0,0,0.8)] ${failed ? "border-coral/30" : "border-cream/10"}`}
          >
            <div className="flex items-center gap-3">
              <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-cream/8 text-cream-300">
                <Package size={18} aria-hidden="true" />
              </span>
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-bold">
                  {job.title}
                  <span className={`ml-2 text-xs font-bold ${failed ? "text-coral" : "text-mute"}`}>{failed ? "Didn't install" : "Installing"}</span>
                </p>
                <p className={`mt-0.5 text-xs leading-snug ${failed ? "line-clamp-3 text-cream-300" : "truncate text-mute"}`}>{detail}</p>
              </div>
              {failed && (
                <button
                  type="button"
                  aria-label="Dismiss"
                  onClick={() => dismissInstall(job.jobId)}
                  className="self-start rounded-lg p-1 text-mute transition-colors hover:bg-cream/8 hover:text-cream"
                >
                  <X size={14} aria-hidden="true" />
                </button>
              )}
            </div>
            {!failed && <ProgressBar value={progress} label={`Installing ${job.title}`} />}
            {failed && (
              <button
                type="button"
                onClick={() => {
                  dismissInstall(job.jobId);
                  startModpackInstall({ projectId: job.jobId, title: job.title, projectType: "modpack", iconUrl: null });
                }}
                className="flex h-8 items-center gap-1.5 self-start rounded-full bg-cream px-3.5 text-xs font-bold text-ink-900 transition-transform active:scale-95"
              >
                <RotateCw size={12} aria-hidden="true" />
                Try again
              </button>
            )}
          </m.section>
        );
      })}
    </AnimatePresence>
  );
}
