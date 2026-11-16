import { ArrowRight, Cloud, CloudOff, FolderSync, HardDrive, Laptop, Link2, Monitor, MonitorSmartphone, type LucideIcon } from "lucide-react";
import { m } from "motion/react";
import { Ghost } from "../components/brand/Ghost";
import { Sparkle } from "../components/brand/Sparkle";
import { Page, Panel } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { Toggle } from "../components/ui/Toggle";
import type { SyncPrefs } from "../data/types";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

const PROVIDERS: { id: string; label: string; hint: string; icon: LucideIcon }[] = [
  { id: "OneDrive", label: "OneDrive", hint: "5 GB free with your Microsoft account", icon: Cloud },
  { id: "Google Drive", label: "Google Drive", hint: "15 GB free with a Google account", icon: HardDrive },
  { id: "a synced folder", label: "A synced folder", hint: "iCloud Drive, Dropbox, Syncthing…", icon: FolderSync },
];

const WHAT_SYNCS: { key: keyof SyncPrefs; label: string; description: string }[] = [
  { key: "worlds", label: "Worlds", description: "Every save, with a restore point each time it syncs." },
  { key: "options", label: "Options and keybinds", description: "Controls, sound, chat, language and other options." },
  { key: "servers", label: "Server list", description: "The multiplayer servers you've added." },
  { key: "resourcePacks", label: "Resource packs", description: "Packs in each instance's resource pack folder." },
  { key: "screenshots", label: "Screenshots", description: "These add up quickly, so they're off unless you want them." },
];

const STAYS_LOCAL = ["Resolution and fullscreen", "Render distance", "Graphics quality", "GUI scale"];

const STEPS = [
  { title: "Play here", body: "Wisp watches for the game to close." },
  { title: "It uploads", body: "Only what changed, to your storage." },
  { title: "Play anywhere", body: "The newest copy is waiting." },
];

export function SyncPage() {
  const { home, settings, updateSettings, comingSoon } = useApp();

  return (
    <Page className="max-w-[1180px]">
      <PageHeader
        title="Cloud sync"
        subtitle="Play on any computer and pick up where you left off. Your worlds live in your own cloud storage, never on Wisp's servers."
      />

      <m.div variants={fadeUp} className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
        <div className="flex min-w-0 flex-col gap-5">
          <section aria-labelledby="sync-status" className="relative overflow-hidden rounded-[24px] border border-cream/8 bg-ink-800 p-6">
            <div aria-hidden="true" className="absolute -right-16 -top-20 size-64 rounded-full bg-[radial-gradient(circle,rgba(244,238,228,0.09),transparent_65%)]" />
            <div className="relative flex items-start gap-4">
              <span className="grid size-12 shrink-0 place-items-center rounded-[15px] bg-cream/8 text-cream-300">
                <CloudOff size={22} aria-hidden="true" />
              </span>
              <div className="min-w-0 flex-1">
                <h2 id="sync-status" className="font-display text-[22px] font-bold tracking-[-0.01em]">
                  Sync is off
                </h2>
                <p className="mt-0.5 text-sm text-mute">Choose where to keep your worlds. You can switch later.</p>
              </div>
              <Ghost size={52} floating className="hidden shrink-0 drop-shadow-[0_0_16px_rgba(244,238,228,0.25)] sm:block" />
            </div>

            <ul aria-label="Where to keep your worlds" className="relative mt-5 grid gap-3 md:grid-cols-3">
              {PROVIDERS.map((provider) => {
                const Icon = provider.icon;
                return (
                  <li key={provider.id}>
                    <button
                      type="button"
                      onClick={() => comingSoon(`Syncing with ${provider.id}`)}
                      className="group flex h-full w-full flex-col gap-3 rounded-[18px] border border-cream/8 bg-ink-900/40 p-4 text-left transition-[border-color,background-color,translate] duration-300 ease-out-expo hover:-translate-y-0.5 hover:border-cream/25 hover:bg-cream/4"
                    >
                      <Icon size={22} strokeWidth={1.7} className="text-cream-300 transition-colors group-hover:text-cream" aria-hidden="true" />
                      <span className="flex flex-col">
                        <span className="text-[15px] font-bold">{provider.label}</span>
                        <span className="text-xs leading-snug text-mute">{provider.hint}</span>
                      </span>
                      <span className="mt-auto flex items-center gap-1 text-[13px] font-bold text-cream-300 transition-colors group-hover:text-cream">
                        Connect
                        <ArrowRight size={14} className="transition-transform duration-300 group-hover:translate-x-0.5" aria-hidden="true" />
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>

            <ol aria-label="How sync works" className="relative mt-6 grid gap-3 border-t border-cream/8 pt-5 sm:grid-cols-3">
              {STEPS.map((step, index) => (
                <li key={step.title} className="flex items-start gap-3">
                  <span className="grid size-7 shrink-0 place-items-center rounded-full bg-cream/8 font-display text-[13px] font-bold text-cream-300">{index + 1}</span>
                  <span className="flex flex-col">
                    <span className="text-sm font-bold">{step.title}</span>
                    <span className="text-xs leading-snug text-mute">{step.body}</span>
                  </span>
                </li>
              ))}
            </ol>
          </section>

          <Panel title="What syncs" description="Choose what follows you from computer to computer.">
            {settings === null ? (
              <Loading label="Loading sync choices" className="flex flex-col gap-4 py-3">
                {WHAT_SYNCS.map((item) => (
                  <div key={item.key} className="flex items-center justify-between gap-6">
                    <div className="flex flex-1 flex-col gap-2">
                      <Skeleton className="h-3.5 w-32 rounded-full" />
                      <Skeleton className="h-3 w-56 rounded-full" />
                    </div>
                    <Skeleton className="h-7 w-12 rounded-full" />
                  </div>
                ))}
              </Loading>
            ) : (
              <div className="divide-y divide-cream/7">
                {WHAT_SYNCS.map((item) => (
                  <Toggle
                    key={item.key}
                    label={item.label}
                    description={item.description}
                    checked={settings.sync[item.key]}
                    onChange={(value) => updateSettings((current) => ({ sync: { ...current.sync, [item.key]: value } }))}
                  />
                ))}
              </div>
            )}
          </Panel>
        </div>

        <div className="flex flex-col gap-5">
          <Panel title="Your computers" icon={<MonitorSmartphone size={19} aria-hidden="true" />}>
            {home === null ? (
              <Loading label="Loading this computer" className="mt-3 flex items-center gap-3 rounded-2xl bg-cream/4 p-3">
                <Skeleton className="size-9 rounded-xl" />
                <div className="flex flex-1 flex-col gap-2">
                  <Skeleton className="h-3.5 w-28 rounded-full" />
                  <Skeleton className="h-3 w-20 rounded-full" />
                </div>
              </Loading>
            ) : (
              <div className="mt-3 flex items-center gap-3 rounded-2xl bg-cream/4 p-3">
                <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-cream/8 text-cream-300">
                  {home.device.os === "macOS" ? <Laptop size={18} aria-hidden="true" /> : <Monitor size={18} aria-hidden="true" />}
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-sm font-bold">{home.device.name}</span>
                  <span className="text-xs text-mute">This computer{home.device.os && ` · ${home.device.os}`}</span>
                </span>
              </div>
            )}
            <button
              type="button"
              onClick={() => comingSoon("Linking computers")}
              className="group mt-2.5 flex w-full items-center gap-3 rounded-2xl border border-dashed border-cream/15 p-3 text-left transition-colors hover:border-cream/35 hover:bg-cream/4"
            >
              <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-cream/6 text-cream-300">
                <Link2 size={17} aria-hidden="true" />
              </span>
              <span className="flex min-w-0 flex-col">
                <span className="text-sm font-bold">Link another computer</span>
                <span className="text-xs text-mute">Sign in to Wisp there with the same storage</span>
              </span>
            </button>
          </Panel>

          <Panel title="Stays on each computer" description="These depend on each computer's screen and graphics, so every computer keeps its own.">
            <ul className="mt-3 flex flex-col gap-2">
              {STAYS_LOCAL.map((item) => (
                <li key={item} className="flex items-center gap-2.5 text-sm text-cream-300">
                  <Sparkle className="size-2.5 text-ember" />
                  {item}
                </li>
              ))}
            </ul>
          </Panel>
        </div>
      </m.div>
    </Page>
  );
}
