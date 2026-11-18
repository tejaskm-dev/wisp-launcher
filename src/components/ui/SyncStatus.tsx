import { ArrowDown, ArrowUp, Check, Hourglass, HardDrive, MonitorPlay, TriangleAlert, type LucideIcon } from "lucide-react";
import type { WorldSummary } from "../../data/types";
import { cn } from "../../lib/cn";
import { formatBytes, timeAgo } from "../../lib/format";

interface SyncStatusProps {
  world: WorldSummary;
  now: number;
  /** "sm" for cards, "md" for the hero where there is room to name the other computer. */
  size?: "sm" | "md";
}

export function SyncStatus({ world, now, size = "sm" }: SyncStatusProps) {
  const fromOtherDevice = size === "md" && world.lastDevice && !world.lastDeviceIsThis ? ` from ${world.lastDevice}` : "";
  const sync = world.sync;

  let Icon: LucideIcon;
  let tone: string;
  let label: string;
  switch (sync.kind) {
    case "local":
      Icon = HardDrive;
      tone = "text-mute";
      label = size === "md" ? "On this computer only" : "This computer";
      break;
    case "synced":
      Icon = Check;
      tone = "text-moss";
      label = `Synced${fromOtherDevice} · ${timeAgo(sync.at, now)}`;
      break;
    case "uploading":
      Icon = ArrowUp;
      tone = "text-amber";
      label = `Uploading · ${formatBytes(sync.totalBytes * (1 - sync.progress))} left`;
      break;
    case "cloud-newer":
      Icon = ArrowDown;
      tone = "text-sky";
      label = size === "md" ? `Newer copy${fromOtherDevice} downloads before you play` : "Newer copy in the cloud";
      break;
    case "conflict":
      Icon = TriangleAlert;
      tone = "text-coral";
      label = "Two copies, choose one";
      break;
    case "in-use":
      Icon = MonitorPlay;
      tone = "text-sky";
      label = size === "md" ? `Being played on ${sync.device}` : `Open on ${sync.device}`;
      break;
    case "waiting":
      Icon = Hourglass;
      tone = "text-amber";
      label = size === "md" ? "Newest copy still arriving from the cloud" : "Arriving";
      break;
  }

  return (
    <span className={cn("flex min-w-0 items-center gap-1.5 text-cream-400", size === "sm" ? "text-xs" : "text-sm")}>
      <Icon size={size === "sm" ? 12 : 14} strokeWidth={2.6} className={cn("shrink-0", tone)} aria-hidden="true" />
      <span className="truncate">{label}</span>
    </span>
  );
}
