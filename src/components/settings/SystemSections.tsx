import { ArrowUpCircle, Download, Eraser, FolderOpen, HardDrive, Info, LoaderCircle, RefreshCw, RotateCcw, ShieldCheck } from "lucide-react";
import { useState } from "react";
import { clearCache, getDataFolder, getStorageUsage, openDataFolder, openExternal, testerBuild, testerBuildNumber, versionLabel } from "../../data/api";
import { DEFAULT_SETTINGS } from "../../data/defaults";
import type { StorageUsage } from "../../data/types";
import { formatBytes } from "../../lib/format";
import { useResource } from "../../lib/useResource";
import { useApp } from "../../state/AppContext";
import { checkForUpdate, installUpdate, useUpdate } from "../../state/updates";
import { Ghost } from "../brand/Ghost";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Panel } from "../ui/Page";
import { Loading, Skeleton } from "../ui/Skeleton";
import { Slider } from "../ui/Slider";
import { Toggle } from "../ui/Toggle";
import { Note, Row, type SectionProps } from "./parts";

const failure = (error: unknown) => (error instanceof Error ? error.message : String(error));

const KINDS: { key: keyof StorageUsage; label: string; note: string; color: string }[] = [
  { key: "instances", label: "Instances", note: "Worlds, mods, screenshots and settings", color: "bg-cream" },
  { key: "gameFiles", label: "Game versions", note: "Minecraft itself and its libraries", color: "bg-sky" },
  { key: "assets", label: "Sounds and textures", note: "Shared by every version", color: "bg-moss" },
  { key: "java", label: "Java", note: "The runtimes games run on", color: "bg-amber" },
  { key: "cache", label: "Cache", note: "Installers and downloads, safe to clear", color: "bg-mute-2" },
];

export function StorageSection({ settings, update }: SectionProps) {
  const { notify } = useApp();
  const [folder] = useResource(getDataFolder);
  const [usage, reload] = useResource(getStorageUsage);
  const [clearing, setClearing] = useState(false);
  const total = usage.status === "ready" ? KINDS.reduce((sum, kind) => sum + usage.data[kind.key], 0) : 0;

  const openFolder = () =>
    openDataFolder().catch((error: unknown) => notify({ tone: "info", title: "Couldn't open the folder", body: failure(error) }));

  const clear = async () => {
    setClearing(true);
    try {
      const freed = await clearCache();
      notify({ tone: "success", title: "Cache cleared", body: `${formatBytes(freed)} freed.` });
      reload();
    } catch (error) {
      notify({ tone: "info", title: "Couldn't clear the cache", body: failure(error) });
    } finally {
      setClearing(false);
    }
  };

  return (
    <>
      <Panel title="Downloads" icon={<Download size={19} aria-hidden="true" />}>
        <Slider
          label="Parallel downloads"
          description="How many files download at once when installing a version or modpack. Lower it on a slow connection."
          value={settings.concurrentDownloads}
          min={1}
          max={16}
          step={1}
          format={String}
          onChange={(concurrentDownloads) => update({ concurrentDownloads })}
        />
      </Panel>

      <Panel title="Storage" icon={<HardDrive size={19} aria-hidden="true" />} description="Everything Wisp keeps lives in one folder.">
        <div className="mt-3 flex flex-wrap items-center gap-3">
          <code className="min-w-0 flex-1 truncate rounded-xl bg-ink-900/70 px-3.5 py-2.5 font-mono text-[13px] text-cream-300 select-text">
            {folder.status === "ready" ? folder.data : folder.status === "error" ? "Unavailable" : "…"}
          </code>
          <Button variant="secondary" icon={FolderOpen} onClick={() => void openFolder()}>
            Open folder
          </Button>
        </div>

        {usage.status === "loading" ? (
          <Loading label="Measuring disk use" className="mt-5 flex flex-col gap-3">
            <Skeleton className="h-3 w-full rounded-full" />
            {[0, 1, 2].map((index) => (
              <Skeleton key={index} className="h-4 w-2/3 rounded-full" />
            ))}
          </Loading>
        ) : usage.status === "error" ? (
          <p className="mt-5 text-[13px] text-mute">Couldn't measure the folder.</p>
        ) : (
          <div className="mt-5">
            <p className="text-sm text-mute">
              <span className="font-display text-xl font-bold tabular-nums text-cream">{formatBytes(total)}</span> in total
            </p>
            <div role="img" aria-label="Disk use by kind" className="mt-3 flex h-3 w-full gap-0.5 overflow-hidden rounded-full bg-ink-900/70">
              {total > 0 &&
                KINDS.map((kind) =>
                  usage.data[kind.key] > 0 ? (
                    <span key={kind.key} className={kind.color} style={{ width: `${Math.max(0.8, (usage.data[kind.key] / total) * 100)}%` }} />
                  ) : null,
                )}
            </div>
            <ul className="mt-4 grid gap-x-6 gap-y-3 sm:grid-cols-2">
              {KINDS.map((kind) => (
                <li key={kind.key} className="flex items-start gap-2.5">
                  <span aria-hidden="true" className={`mt-1.5 size-2.5 shrink-0 rounded-full ${kind.color}`} />
                  <div className="min-w-0">
                    <p className="text-sm font-semibold">
                      {kind.label} <span className="font-normal tabular-nums text-mute">{formatBytes(usage.data[kind.key])}</span>
                    </p>
                    <p className="text-xs text-mute-2">{kind.note}</p>
                  </div>
                </li>
              ))}
            </ul>
            <div className="mt-5 flex flex-wrap items-center gap-3 border-t border-cream/7 pt-4">
              <Button variant="secondary" icon={clearing ? LoaderCircle : Eraser} disabled={clearing || usage.data.cache === 0} onClick={() => void clear()}>
                {usage.data.cache === 0 ? "Cache is empty" : `Clear cache (${formatBytes(usage.data.cache)})`}
              </Button>
              <p className="min-w-0 flex-1 text-xs leading-relaxed text-mute-2">Mod loader installers, modpack downloads and unpacked libraries. Wisp gets them again when needed.</p>
            </div>
          </div>
        )}
      </Panel>
    </>
  );
}

const CONNECTIONS = [
  { who: "Mojang", why: "Minecraft, its Java runtimes and the news on Home." },
  { who: "Modrinth", why: "Discover, mods and modpacks." },
  { who: "Fabric, Quilt, Forge and NeoForge", why: "Their mod loaders, when an instance uses one." },
];

export function PrivacySection({ settings, update }: SectionProps) {
  return (
    <Panel title="Privacy" icon={<ShieldCheck size={19} aria-hidden="true" />}>
      <p className="mt-1 text-[13px] leading-relaxed text-cream-300">
        Wisp has no account of its own, no ads and no tracking. Your worlds and settings stay on this computer, and in your own cloud storage once you turn sync on.
      </p>
      <div className="mt-2">
        <Toggle
          label="Show images from other websites"
          description="Project descriptions often show images hosted elsewhere, which lets those sites see your IP address. Images hosted by Modrinth always show."
          checked={settings.externalImages}
          onChange={(externalImages) => update({ externalImages })}
        />
      </div>
      <Row label="Who Wisp talks to" stacked>
        <ul className="flex flex-col gap-2">
          {CONNECTIONS.map((connection) => (
            <li key={connection.who} className="rounded-xl bg-ink-900/50 px-3.5 py-2.5 text-[13px]">
              <span className="font-bold">{connection.who}</span> <span className="text-mute">· {connection.why}</span>
            </li>
          ))}
        </ul>
      </Row>
    </Panel>
  );
}

export function AboutSection({ settings, update }: SectionProps) {
  const { notify } = useApp();
  const [confirming, setConfirming] = useState(false);
  const reset = () => {
    update(() => DEFAULT_SETTINGS);
    setConfirming(false);
    notify({ tone: "success", title: "Settings reset", body: "Everything is back to how Wisp started." });
  };
  return (
    <>
      <Panel title="About Wisp" icon={<Info size={19} aria-hidden="true" />}>
        <div className="mt-2 flex items-center gap-4">
          <Ghost size={48} floating />
          <div className="text-[13px] leading-relaxed text-mute">
            <p className="text-[15px] font-bold text-cream">
              Wisp {__APP_VERSION__}
              {testerBuild && (
                <span className="ml-2 rounded-full bg-ember/15 px-2 py-0.5 align-middle text-[11px] font-bold text-ember-soft">
                  {testerBuildNumber ? `Tester build ${testerBuildNumber}` : "Tester build"}
                </span>
              )}
            </p>
            {testerBuild && (
              <p>
                Made for testers who own Minecraft: offline accounts are on.{" "}
                {testerBuildNumber ? "New tester builds arrive by themselves, like any update." : "It doesn't update itself."} Please don't share it.
              </p>
            )}
            <p>A Minecraft: Java Edition launcher that keeps your worlds on every computer.</p>
            <p className="mt-1 text-mute-2">Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.</p>
            <p className="mt-1 text-mute-2">
              Wisp's own developers make it and answer for it. Questions or problems:{" "}
              <button type="button" onClick={() => void openExternal("mailto:wisplaunchercontact@gmail.com")} className="font-semibold text-cream-300 underline decoration-cream/25 underline-offset-2 hover:text-cream">
                wisplaunchercontact@gmail.com
              </button>
            </p>
          </div>
        </div>
        <UpdateRow />
      </Panel>

      <Panel title="Start over" icon={<RotateCcw size={19} aria-hidden="true" />}>
        <Row label="Reset all settings" description="Puts every setting back to how Wisp started. Instances, worlds and accounts aren't touched.">
          <Button variant="secondary" icon={RotateCcw} onClick={() => setConfirming(true)}>
            Reset…
          </Button>
        </Row>
        <Note>{settings.curseforgeApiKey.trim() ? "Custom Java paths, launch hooks, Java arguments and your CurseForge key are cleared too." : "Custom Java paths, launch hooks and Java arguments are cleared too."}</Note>
      </Panel>

      <Dialog
        open={confirming}
        onClose={() => setConfirming(false)}
        title="Reset all settings?"
        description="Every setting goes back to its default. Your instances, worlds and accounts stay as they are."
        footer={
          <>
            <Button variant="secondary" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
            <Button onClick={reset}>Reset settings</Button>
          </>
        }
      >
        {null}
      </Dialog>
    </>
  );
}

/** The version check: Wisp also looks on its own every few hours and offers what it finds. */
function UpdateRow() {
  const update = useUpdate();
  const description =
    update.phase === "off"
      ? "This build doesn't update itself. Release builds look for new versions on their own and offer them."
      : update.phase === "checking"
        ? "Looking for a newer Wisp…"
        : update.phase === "available"
          ? `Wisp ${versionLabel(update.version)} is ready to install.`
          : update.phase === "downloading"
            ? `Downloading Wisp ${versionLabel(update.version)}…`
            : update.phase === "restarting"
              ? "Restarting on the new version…"
              : update.phase === "failed"
                ? `Couldn't check: ${update.message}`
                : update.checkedAt
                  ? testerBuild
                    ? "You have the newest tester build."
                    : "You have the newest Wisp."
                  : testerBuild
                    ? "Wisp looks for new tester builds on its own every few hours."
                    : "Wisp looks for new versions on its own every few hours.";
  return (
    <Row label="Updates" description={description}>
      {update.phase === "available" ? (
        <Button icon={ArrowUpCircle} onClick={() => void installUpdate()}>
          Update and restart
        </Button>
      ) : (
        <Button variant="secondary" icon={RefreshCw} disabled={update.phase === "off" || update.phase === "checking" || update.phase === "downloading" || update.phase === "restarting"} onClick={() => void checkForUpdate()}>
          Check now
        </Button>
      )}
    </Row>
  );
}
