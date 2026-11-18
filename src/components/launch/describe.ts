import type { LaunchUpdate } from "../../data/types";
import { formatBytes } from "../../lib/format";

/** How a Play button, card or panel should present an instance's launch. */
export interface LaunchView {
  kind: "idle" | "busy" | "running" | "problem";
  /** Short, for buttons: "Downloading 42%". */
  label: string;
  /** A sentence for panels: "212 MB of 640 MB". */
  detail: string;
  /** 0 to 1, or null when there's nothing to measure. */
  progress: number | null;
}

const IDLE: LaunchView = { kind: "idle", label: "Play", detail: "", progress: null };

function duration(ms: number): string {
  const minutes = Math.max(0, Math.round(ms / 60_000));
  if (minutes < 1) return "a moment";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return `${hours} h ${minutes % 60} min`;
}

/** The line that best explains a crash: the last exception or error, else the last line. */
export function crashReason(logTail: string[]): string | null {
  const lines = logTail.map((line) => line.trim()).filter(Boolean);
  const telling = [...lines].reverse().find((line) => /exception|error|caused by/i.test(line));
  return telling ?? lines.at(-1) ?? null;
}

export function describeLaunch(update: LaunchUpdate | undefined, now: number): LaunchView {
  if (!update) return IDLE;
  switch (update.phase) {
    case "checking":
      return { kind: "busy", label: "Getting ready…", detail: "Checking the game files", progress: null };
    case "syncing":
      return { kind: "busy", label: "Syncing…", detail: `Getting the newest copy from ${update.folder}`, progress: null };
    case "downloading": {
      const progress = update.totalBytes > 0 ? Math.min(1, update.doneBytes / update.totalBytes) : null;
      return {
        kind: "busy",
        label: progress === null ? "Downloading…" : `Downloading ${Math.floor(progress * 100)}%`,
        detail: `${formatBytes(update.doneBytes)} of ${formatBytes(update.totalBytes)}`,
        progress,
      };
    }
    case "installing":
      return { kind: "busy", label: `Installing ${update.loader}…`, detail: `Running ${update.loader}'s installer. This happens once per version.`, progress: null };
    case "fixing":
      return { kind: "busy", label: "Fixing…", detail: update.message, progress: null };
    case "starting":
      return { kind: "busy", label: "Starting…", detail: "Opening Minecraft", progress: null };
    case "running":
      return { kind: "running", label: "Playing", detail: `Playing for ${duration(now - update.startedAt)}`, progress: null };
    case "saving":
      return { kind: "busy", label: "Saving…", detail: `Keeping a restore point of ${update.world}`, progress: null };
    case "exited":
      return update.crashed
        ? {
            kind: "problem",
            label: "Play again",
            detail: update.help?.found[0] ?? crashReason(update.logTail) ?? `Minecraft closed unexpectedly (code ${update.code ?? "?"}).`,
            progress: null,
          }
        : IDLE;
    case "failed":
      return { kind: "problem", label: "Try again", detail: update.message, progress: null };
  }
}
