import { ArrowUpCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { formatBytes } from "../lib/format";
import { renderMarkdown } from "../lib/markdown";
import { useApp } from "../state/AppContext";
import { checkForUpdate, installUpdate, updateLater, useUpdate } from "../state/updates";
import { Sparkle } from "./brand/Sparkle";
import { describeLaunch } from "./launch/describe";
import { Button } from "./ui/Button";
import { Dialog } from "./ui/Dialog";
import { ProgressBar } from "./ui/ProgressBar";

/** How often a running Wisp looks for a new version. */
const EVERY = 6 * 60 * 60 * 1000;
/** How long "Remind me later" puts a version away. A newer one comes up straight away. */
const SNOOZE = 24 * 60 * 60 * 1000;
const SNOOZE_KEY = "wisp.updateSnoozed";

function snoozed(version: string): boolean {
  try {
    const saved = JSON.parse(localStorage.getItem(SNOOZE_KEY) ?? "null") as { version: string; until: number } | null;
    return saved?.version === version && Date.now() < saved.until;
  } catch {
    return false;
  }
}

function snooze(version: string) {
  try {
    localStorage.setItem(SNOOZE_KEY, JSON.stringify({ version, until: Date.now() + SNOOZE }));
  } catch {
    // Without storage it simply asks again next time.
  }
}

/** The "What's new" part of the release notes (they also hold download tables and the like). */
export function whatsNew(notes: string): string {
  const lines = notes.split("\n");
  const start = lines.findIndex((line) => /^#{1,4}\s*what['’]s new/i.test(line.trim()));
  if (start === -1) return notes.split(/\n-{3,}\s*\n/)[0].trim();
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((line) => /^#{1,4}\s/.test(line.trim()) || /^-{3,}\s*$/.test(line.trim()));
  return (end === -1 ? rest : rest.slice(0, end)).join("\n").trim();
}

/**
 * A new Wisp, announced the way desktop apps do it: a window with what's new, Update now, and
 * Remind me later. It never interrupts a game: one found while playing waits until the game
 * closes. Updating downloads in the window and restarts Wisp right where it was.
 */
export function UpdatePanel() {
  const { launches, now } = useApp();
  const update = useUpdate();
  // Bumped by "Remind me later", so the snooze is read again.
  const [dismissed, setDismissed] = useState(0);

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
  const version = "version" in update ? update.version : null;
  const busy = update.phase === "downloading" || update.phase === "restarting";
  const offered = update.phase === "available" && !update.later && !playing && !snoozed(update.version);
  const open = offered || busy || (update.phase === "failed" && update.version !== null);
  void dismissed;

  const notes = update.phase === "available" ? whatsNew(update.notes) : "";
  const html = useMemo(() => (notes ? renderMarkdown(notes, { externalImages: false }).html : ""), [notes]);
  const progress = update.phase === "downloading" && update.total ? update.done / update.total : null;

  const later = () => {
    if (version) snooze(version);
    updateLater();
    setDismissed((count) => count + 1);
  };

  return (
    <Dialog
      open={open}
      onClose={busy ? () => {} : later}
      title={update.phase === "failed" ? "The update didn't finish" : update.phase === "restarting" ? "Restarting on the new Wisp" : `Wisp ${version ?? ""} is here`}
      description={
        update.phase === "failed"
          ? update.message
          : busy
            ? "Back in a moment, right where you were."
            : `You have ${__APP_VERSION__}. Updating takes a moment, and Wisp restarts right where you were.`
      }
      footer={
        update.phase === "downloading" || update.phase === "restarting" ? (
          <div className="flex w-full flex-col gap-2">
            <ProgressBar pixel value={update.phase === "restarting" ? 1 : progress} label={`Updating to Wisp ${version ?? ""}`} />
            <p className="text-xs text-mute">
              {update.phase === "restarting" ? "Installed. Restarting…" : update.total ? `${formatBytes(update.done)} of ${formatBytes(update.total)}` : `${formatBytes(update.done)} so far`}
            </p>
          </div>
        ) : (
          <>
            <Button variant="secondary" onClick={later}>
              Remind me later
            </Button>
            <Button onClick={() => void (update.phase === "failed" ? checkForUpdate().then(installUpdate) : installUpdate())}>
              {update.phase === "failed" ? "Try again" : "Update now"}
            </Button>
          </>
        )
      }
    >
      <div className="flex flex-col gap-3">
        <div className="flex items-center gap-3 rounded-[18px] bg-cream/[0.04] p-3.5">
          <span className="relative grid size-11 shrink-0 place-items-center rounded-xl bg-cream/8 text-cream">
            <ArrowUpCircle size={21} aria-hidden="true" />
            <Sparkle className="absolute -right-1 -top-1 size-3.5 text-ember" />
          </span>
          <p className="text-sm leading-relaxed text-cream-300">
            <span className="font-bold text-cream">{__APP_VERSION__}</span> to <span className="font-bold text-cream">{version}</span>
          </p>
        </div>
        {html && (
          <div className="max-h-[42vh] overflow-y-auto rounded-[18px] border border-cream/8 bg-ink-850 p-4">
            <p className="mb-2 text-xs font-bold text-mute">What's new</p>
            {/* Sanitised in renderMarkdown: no scripts, frames, forms or styles survive. */}
            <div className="prose-wisp text-[13px]" dangerouslySetInnerHTML={{ __html: html }} />
          </div>
        )}
      </div>
    </Dialog>
  );
}
