import { LoaderCircle, Play, Square, Users } from "lucide-react";
import type { HostedView } from "../../data/types";
import { cn } from "../../lib/cn";
import { InstanceAvatar } from "../instances/InstanceAvatar";
import { RowButton } from "../servers/ServerRow";
import { ProgressBar } from "../ui/ProgressBar";
import { contentCount, hostedStatus, softwareName, TONE_DOT, TONE_TEXT } from "./describe";

/** A server hosted on this computer, as a card: what it runs, whether it's up, who's on. */
export function HostedServerCard({
  server,
  now,
  busy,
  onOpen,
  onStart,
  onStop,
}: {
  server: HostedView;
  now: number;
  busy: boolean;
  onOpen: () => void;
  onStart: () => void;
  onStop: () => void;
}) {
  const status = hostedStatus(server, now);
  const live = server.state !== "stopped" && server.state !== "crashed";
  return (
    <article className="wisp-card lift lit group relative flex w-full gap-4 p-4">
      <button type="button" onClick={onOpen} aria-label={`Open ${server.name}`} className="absolute inset-0 rounded-[inherit]" />
      <span className="relative flex shrink-0 self-start">
        <InstanceAvatar instance={{ name: server.name, loader: server.loader, icon: server.icon }} className="size-14 text-xl" />
        <span aria-hidden="true" className={cn("absolute -bottom-0.5 -right-0.5 size-3.5 rounded-full border-[2.5px] border-ink-800", TONE_DOT[status.tone], status.busy && "animate-pulse")} />
      </span>
      <div className="pointer-events-none flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <h3 className="truncate font-display text-[17px] font-bold leading-tight">{server.name}</h3>
            <p className="truncate text-xs text-mute-2">
              {softwareName(server)} {server.gameVersion}
              {contentCount(server) && ` · ${contentCount(server)}`}
              {server.instanceName && ` · played with ${server.instanceName}`}
            </p>
          </div>
          <span className={cn("mt-0.5 shrink-0 text-xs font-bold", TONE_TEXT[status.tone])}>{status.label}</span>
        </div>
        <p className="line-clamp-2 text-[13px] leading-snug text-mute">{status.detail}</p>
        {status.progress !== null && <ProgressBar value={status.progress} label={`Setting up ${server.name}`} tone="amber" className="mt-1" />}
        <div className="mt-auto flex flex-wrap items-center gap-2 pt-1">
          {server.players.length > 0 && (
            <span className="flex items-center gap-1.5 rounded-full bg-moss/12 px-2.5 py-1 text-[11px] font-bold text-moss-soft" title={server.players.join(", ")}>
              <Users size={11} aria-hidden="true" />
              {server.players.slice(0, 3).join(", ")}
              {server.players.length > 3 && ` +${server.players.length - 3}`}
            </span>
          )}
          {server.state === "running" && server.lanAddress && <span className="truncate font-mono text-[11px] text-mute-2">{server.lanAddress}</span>}
          <div className="pointer-events-auto relative ml-auto flex items-center gap-1.5">
            {live ? (
              <RowButton onClick={onStop} disabled={busy || server.state === "stopping"} label={`Stop ${server.name}`}>
                {server.state === "stopping" || busy ? <LoaderCircle size={11} className="animate-spin" aria-hidden="true" /> : <Square size={9} fill="currentColor" strokeWidth={0} aria-hidden="true" />}
                {server.state === "preparing" ? "Cancel" : "Stop"}
              </RowButton>
            ) : (
              <RowButton primary onClick={onStart} disabled={busy} label={`Start ${server.name}`}>
                {busy ? <LoaderCircle size={11} className="animate-spin" aria-hidden="true" /> : <Play size={11} fill="currentColor" strokeWidth={0} aria-hidden="true" />}
                Start
              </RowButton>
            )}
          </div>
        </div>
      </div>
    </article>
  );
}
