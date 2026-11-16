import { Cloud, CloudOff, FolderSync, HardDrive, Laptop, Monitor, type LucideIcon } from "lucide-react";
import { m } from "motion/react";
import type { DeviceInfo, HomeSnapshot } from "../data/types";
import { rise } from "../lib/motion";
import { Loading, Skeleton } from "./ui/Skeleton";

export type StorageProvider = "OneDrive" | "Google Drive" | "a synced folder";

const PROVIDERS: { id: StorageProvider; label: string; hint: string; icon: LucideIcon }[] = [
  { id: "OneDrive", label: "OneDrive", hint: "5 GB free with your Microsoft account", icon: Cloud },
  { id: "Google Drive", label: "Google Drive", hint: "15 GB free", icon: HardDrive },
  { id: "a synced folder", label: "A synced folder", hint: "iCloud Drive, Dropbox, Syncthing…", icon: FolderSync },
];

interface SyncPanelProps {
  /** Null while the library is loading. */
  home: HomeSnapshot | null;
  onSetUp: (provider: StorageProvider) => void;
}

export function SyncPanel({ home, onSetUp }: SyncPanelProps) {
  return (
    <m.section
      variants={rise}
      aria-labelledby="sync-title"
      className="flex flex-col gap-4 rounded-[26px] border border-cream/8 bg-ink-800 p-[22px]"
    >
      <header className="flex items-center gap-3">
        <span className="grid size-[46px] place-items-center rounded-[14px] bg-cream/8 text-cream-300">
          <CloudOff size={22} strokeWidth={1.8} aria-hidden="true" />
        </span>
        <div>
          <h2 id="sync-title" className="font-display text-[19px] font-bold">
            Cloud sync
          </h2>
          {home === null ? (
            <Skeleton className="mt-1.5 h-3 w-12 rounded-full" />
          ) : (
            <p className="flex items-center gap-1.5 text-[13px] font-semibold text-mute">
              <span aria-hidden="true" className="size-[7px] rounded-full border border-mute" />
              Off
            </p>
          )}
        </div>
      </header>

      {home === null ? (
        <Loading label="Loading sync settings" className="flex flex-col gap-2.5">
          <Skeleton className="h-3.5 w-full rounded-full" />
          <Skeleton className="h-3.5 w-3/4 rounded-full" />
          <Skeleton className="mt-2 h-14 w-full rounded-[14px]" />
          <Skeleton className="h-14 w-full rounded-[14px]" />
        </Loading>
      ) : (
        <>
          <p className="text-[13px] leading-relaxed text-cream-400">
            Keep worlds, keybinds and settings on every computer. Everything is stored in your own cloud storage,
            never on Wisp's servers.
          </p>
          <ThisDevice device={home.device} />
          <ul aria-label="Where to keep your worlds" className="flex flex-col gap-1.5">
            {PROVIDERS.map((provider) => {
              const Icon = provider.icon;
              return (
                <li key={provider.id}>
                  <button
                    type="button"
                    onClick={() => onSetUp(provider.id)}
                    className="group flex w-full items-center gap-3 rounded-[14px] border border-cream/7 px-3 py-2.5 text-left transition-colors duration-200 hover:border-cream/18 hover:bg-cream/4"
                  >
                    <Icon size={18} strokeWidth={1.8} className="shrink-0 text-cream-400 transition-colors group-hover:text-cream" aria-hidden="true" />
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="text-sm font-bold">{provider.label}</span>
                      <span className="truncate text-xs text-mute">{provider.hint}</span>
                    </span>
                    <span className="text-xs font-bold text-cream-400 transition-colors group-hover:text-cream">Connect</span>
                  </button>
                </li>
              );
            })}
          </ul>
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
