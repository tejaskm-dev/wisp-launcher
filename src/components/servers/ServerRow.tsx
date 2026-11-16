import type { ReactNode } from "react";
import type { ServerStatus } from "../../data/types";
import { cn } from "../../lib/cn";
import { Motd } from "./Motd";
import { ServerIcon } from "./ServerIcon";
import { playersLabel } from "./useServerStatuses";

interface ServerRowProps {
  name: string;
  address: string;
  /** The icon the game last saw; the live one wins when there is one. */
  icon?: string | null;
  status?: ServerStatus;
  /** Shown when the server's own message isn't (yet) known. */
  description?: string;
  footer?: ReactNode;
  actions?: ReactNode;
  className?: string;
}

/** A server as a card: icon, name, its live message, players and ping. */
export function ServerRow({ name, address, icon, status, description, footer, actions, className }: ServerRowProps) {
  const live = status?.online ? status : undefined;
  return (
    <article className={cn("group flex gap-4 rounded-[22px] border border-cream/8 bg-ink-800 p-4 transition-colors duration-200 hover:border-cream/18", className)}>
      <ServerIcon icon={live?.icon ?? icon} name={name} className="size-14 text-xl" />
      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <h3 className="truncate font-display text-[17px] font-bold leading-tight">{name}</h3>
            <p className="truncate text-xs text-mute-2">{address}</p>
          </div>
          <StatusBadge status={status} />
        </div>
        {live && live.motd.length > 0 ? (
          <Motd spans={live.motd} className="line-clamp-2" />
        ) : description ? (
          <p className="line-clamp-2 text-[13px] leading-snug text-mute">{description}</p>
        ) : status && !status.online ? (
          <p className="text-[13px] text-mute">{status.error ?? "It didn't answer."}</p>
        ) : null}
        {(footer || actions) && (
          <div className="mt-auto flex flex-wrap items-center gap-2 pt-1">
            {footer}
            <div className="ml-auto flex items-center gap-1.5">{actions}</div>
          </div>
        )}
      </div>
    </article>
  );
}

function StatusBadge({ status }: { status?: ServerStatus }) {
  if (!status) return <span aria-label="Checking" className="mt-1 h-4 w-24 shrink-0 animate-pulse rounded-full bg-cream/6" />;
  if (!status.online) {
    return (
      <span className="mt-0.5 flex shrink-0 items-center gap-1.5 text-xs font-bold text-mute">
        <span aria-hidden="true" className="size-2 rounded-full bg-mute-2" />
        Offline
      </span>
    );
  }
  return (
    <span className="mt-0.5 flex shrink-0 items-center gap-3 text-xs font-bold text-cream-300" title={status.sample.length > 0 ? status.sample.join("\n") : undefined}>
      <span className="flex items-center gap-1.5">
        <span aria-hidden="true" className="size-2 rounded-full bg-moss" />
        {playersLabel(status)}
      </span>
      {status.pingMs !== null && <span className="tabular-nums text-mute">{status.pingMs} ms</span>}
    </span>
  );
}

/** A small round button for a server card. */
export function RowButton({ children, onClick, primary = false, disabled = false, label }: { children: ReactNode; onClick: () => void; primary?: boolean; disabled?: boolean; label?: string }) {
  return (
    <button
      type="button"
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex h-8 items-center gap-1.5 rounded-full px-3.5 text-xs font-bold transition-[transform,colors] duration-150 active:scale-95 disabled:cursor-not-allowed disabled:opacity-45",
        primary ? "bg-cream text-ink-900" : "border border-cream/15 text-cream-300 enabled:hover:border-cream/35 enabled:hover:text-cream",
      )}
    >
      {children}
    </button>
  );
}
