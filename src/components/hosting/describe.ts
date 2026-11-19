import type { HostedView } from "../../data/types";
import { timeAgo } from "../../lib/format";
import { describeLaunch } from "../launch/describe";

export interface HostedStatus {
  /** Short: "2 of 10 on", "Starting…". */
  label: string;
  /** A line under it. */
  detail: string;
  tone: "moss" | "amber" | "coral" | "mute";
  /** Something is happening that Stop would cut short. */
  busy: boolean;
  /** 0 to 1 while downloading. */
  progress: number | null;
  /** Up and taking players. */
  running: boolean;
}

function duration(ms: number): string {
  const minutes = Math.max(1, Math.round(ms / 60_000));
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return minutes % 60 === 0 ? `${hours} h` : `${hours} h ${minutes % 60} min`;
}

/** What a hosted server is doing, in words. */
export function hostedStatus(server: HostedView, now: number): HostedStatus {
  switch (server.state) {
    case "preparing": {
      const view = describeLaunch(server.progress ?? { phase: "checking" }, now);
      return { label: view.label === "Getting ready…" ? "Setting up…" : view.label, detail: view.detail ?? "Getting the server's files", tone: "amber", busy: true, progress: view.progress ?? null, running: false };
    }
    case "starting":
      return { label: "Starting…", detail: "Loading the world", tone: "amber", busy: true, progress: null, running: false };
    case "running":
      return {
        label: `${server.players.length} of ${server.maxPlayers} on`,
        detail: server.readyAt ? `Up for ${duration(now - server.readyAt)}` : "Up",
        tone: "moss",
        busy: false,
        progress: null,
        running: true,
      };
    case "stopping":
      return { label: "Stopping…", detail: "Saving the world", tone: "amber", busy: true, progress: null, running: false };
    case "crashed":
      return { label: "Stopped unexpectedly", detail: server.failure ?? "Its console shows the last thing it said.", tone: "coral", busy: false, progress: null, running: false };
    default:
      return {
        label: "Stopped",
        detail: server.failure ?? (server.lastStarted ? `Last ran ${timeAgo(server.lastStarted, now)}` : "Hasn't run yet"),
        tone: server.failure ? "coral" : "mute",
        busy: false,
        progress: null,
        running: false,
      };
  }
}

export const TONE_DOT: Record<HostedStatus["tone"], string> = { moss: "bg-moss", amber: "bg-amber", coral: "bg-coral", mute: "bg-mute-2" };
export const TONE_TEXT: Record<HostedStatus["tone"], string> = { moss: "text-moss-soft", amber: "text-amber-soft", coral: "text-coral", mute: "text-mute" };
