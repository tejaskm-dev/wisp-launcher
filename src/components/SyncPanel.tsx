import { ArrowRight, Cloud, CloudAlert, CloudOff, Hourglass, Laptop, Monitor, TriangleAlert, type LucideIcon } from "lucide-react";
import { m } from "motion/react";
import type { DeviceInfo, HomeSnapshot } from "../data/types";
import { timeAgo } from "../lib/format";
import { rise } from "../lib/motion";
import { useApp } from "../state/AppContext";
import { Loading, Skeleton } from "./ui/Skeleton";

interface SyncPanelProps {
  /** Null while the library is loading. */
  home: HomeSnapshot | null;
  onOpen: () => void;
}

/** Cloud sync on Home: where it stands, and the way to set it up or see more. */
export function SyncPanel({ home, onOpen }: SyncPanelProps) {
  const { now } = useApp();
  const sync = home?.sync ?? null;
  const on = sync?.state === "on" ? sync : null;

  let Icon: LucideIcon = CloudOff;
  let status = "Off";
  let dot = "border border-mute";
  if (on) {
    if (on.conflicts > 0) [Icon, status, dot] = [TriangleAlert, on.conflicts === 1 ? "A world to choose" : `${on.conflicts} worlds to choose`, "bg-coral"];
    else if (on.phase === "unreachable" || on.phase === "failed") [Icon, status, dot] = [CloudAlert, `Can't reach ${on.label}`, "bg-coral"];
    else if (on.phase === "waiting") [Icon, status, dot] = [Hourglass, "Bringing changes", "bg-amber"];
    else [Icon, status, dot] = [Cloud, on.lastSynced ? `Synced ${timeAgo(on.lastSynced, now)}` : "On", "bg-moss"];
  }

  return (
    <m.section variants={rise} aria-labelledby="sync-title" className="wisp-surface flex flex-col gap-4 rounded-[26px] p-[22px]">
      <header className="flex items-center gap-3">
        <span className="grid size-[46px] place-items-center rounded-[14px] bg-cream/8 text-cream-300">
          <Icon size={22} strokeWidth={1.8} aria-hidden="true" />
        </span>
        <div className="min-w-0">
          <h2 id="sync-title" className="font-display text-[19px] font-bold">
            Cloud sync
          </h2>
          {sync === null ? (
            <Skeleton className="mt-1.5 h-3 w-12 rounded-full" />
          ) : (
            <p className="flex items-center gap-1.5 truncate text-[13px] font-semibold text-mute">
              <span aria-hidden="true" className={`size-[7px] shrink-0 rounded-full ${dot}`} />
              {status}
            </p>
          )}
        </div>
      </header>

      {home === null ? (
        <Loading label="Loading sync" className="flex flex-col gap-2.5">
          <Skeleton className="h-3.5 w-full rounded-full" />
          <Skeleton className="h-3.5 w-3/4 rounded-full" />
          <Skeleton className="mt-2 h-14 w-full rounded-[14px]" />
        </Loading>
      ) : (
        <>
          <p className="text-[13px] leading-relaxed text-cream-400">
            {on
              ? `Your worlds, instances and keybinds follow you through ${on.label}. Newer copies come down before you play.`
              : "Keep worlds, instances and keybinds on every computer, through a cloud folder you already have. Nothing goes to Wisp's servers."}
          </p>
          <ThisDevice device={home.device} />
          <button
            type="button"
            onClick={onOpen}
            className="group flex w-full items-center gap-3 rounded-[14px] border border-cream/7 px-3 py-2.5 text-left transition-colors duration-200 hover:border-cream/18 hover:bg-cream/4"
          >
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="text-sm font-bold">{on ? (on.conflicts > 0 ? "Choose which copies to keep" : "See what's synced") : "Set up sync"}</span>
              <span className="truncate text-xs text-mute">{on ? `With ${on.label}` : "iCloud Drive, OneDrive, Google Drive, Dropbox or any folder"}</span>
            </span>
            <ArrowRight size={16} className="text-cream-400 transition-transform group-hover:translate-x-0.5 group-hover:text-cream" aria-hidden="true" />
          </button>
        </>
      )}
    </m.section>
  );
}

function ThisDevice({ device }: { device: DeviceInfo }) {
  const Icon = device.os === "macOS" ? Laptop : Monitor;
  return (
    <div className="flex items-center gap-3 rounded-[14px] bg-cream/4 px-3 py-2.5">
      <Icon size={20} strokeWidth={1.7} className="shrink-0 text-cream-400" aria-hidden="true" />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm font-bold">{device.name}</span>
        <span className="text-xs text-mute">This computer{device.os && ` · ${device.os}`}</span>
      </span>
    </div>
  );
}
