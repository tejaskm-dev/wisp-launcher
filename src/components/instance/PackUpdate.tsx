import { LoaderCircle, Package } from "lucide-react";
import { useEffect, useState } from "react";
import { checkPackUpdates, updateInstancePack } from "../../data/api";
import type { Instance, PackRelease, PackUpdates } from "../../data/types";
import { useApp } from "../../state/AppContext";
import { Sparkle } from "../brand/Sparkle";
import { Button } from "../ui/Button";
import { VersionTag } from "../instances/VersionTag";
import { message } from "./shared";

/**
 * A newer version of the modpack an instance came from, and the button that moves to it. Shows
 * nothing when the pack is up to date (or the instance isn't from a Modrinth pack).
 */
export function PackUpdate({ instance, locked, onUpdated, quietWhenCurrent = true }: { instance: Instance; locked: boolean; onUpdated: () => void; quietWhenCurrent?: boolean }) {
  const { notify, refreshHome } = useApp();
  const [updates, setUpdates] = useState<PackUpdates | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const pack = instance.modpack;

  useEffect(() => {
    if (!pack) return;
    let live = true;
    checkPackUpdates(instance.id).then(
      (found) => live && setUpdates(found),
      () => live && setUpdates({ sameGame: null, newerGame: null }),
    );
    return () => {
      live = false;
    };
  }, [instance.id, pack?.versionId]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!pack) return null;
  const current = updates !== null && !updates.sameGame && !updates.newerGame;
  if (updates === null || (current && quietWhenCurrent)) return null;

  const update = async (release: PackRelease) => {
    setBusy(release.versionId);
    try {
      const done = await updateInstancePack(instance.id, release.versionId, release.gameVersions);
      await refreshHome();
      notify({
        tone: "success",
        title: `${pack.title} is now ${done.to}`,
        body: [
          `${done.added === 1 ? "1 file" : `${done.added} files`} new or changed.`,
          done.removed > 0 ? `${done.removed === 1 ? "1 file" : `${done.removed} files`} the new version left out can be put back from Content.` : "",
          done.movedTo ? `It runs ${done.movedTo} now; your worlds got a restore point first.` : "Your own mods and settings stayed.",
        ]
          .filter(Boolean)
          .join(" "),
        lifetimeMs: 12_000,
      });
      onUpdated();
    } catch (failure) {
      notify({ tone: "info", title: `Couldn't update ${pack.title}`, body: message(failure) });
    } finally {
      setBusy(null);
    }
  };

  if (current) {
    return (
      <p className="flex items-center gap-2 text-[13px] text-mute">
        <Package size={14} aria-hidden="true" />
        {pack.title} {pack.versionNumber} is the newest version for Minecraft <VersionTag version={instance.gameVersion} />
      </p>
    );
  }

  const same = updates.sameGame;
  const newer = updates.newerGame;
  return (
    <div className="flex flex-col gap-3 rounded-[20px] border border-ember/25 bg-ember/[0.06] p-4">
      {same && (
        <div className="flex flex-wrap items-center gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-ember/15 text-ember">
            <Sparkle className="size-4" />
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-bold">
              {pack.title} {same.versionNumber} is out
            </p>
            <p className="mt-0.5 text-xs leading-relaxed text-mute">
              You have {pack.versionNumber}. Your own mods and settings stay; files the new version leaves out can be put back.
            </p>
          </div>
          <Button disabled={locked || busy !== null} onClick={() => void update(same)} className="h-10 px-4 text-[13px]">
            {busy === same.versionId ? "Updating…" : "Update the pack"}
          </Button>
        </div>
      )}
      {newer && (
        <div className="flex flex-wrap items-center gap-3">
          <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-cream/6 text-cream-300">
            {busy === newer.versionId ? <LoaderCircle size={16} className="animate-spin" aria-hidden="true" /> : <Package size={16} aria-hidden="true" />}
          </span>
          <div className="min-w-0 flex-1">
            <p className="text-sm font-bold">
              {pack.title} {newer.versionNumber} is for Minecraft <VersionTag version={newer.gameVersions[0] ?? "?"} />
            </p>
            <p className="mt-0.5 text-xs leading-relaxed text-mute">
              Moving to it changes this instance's Minecraft version. Every world gets a restore point first, in case it doesn't take to the new one.
            </p>
          </div>
          <Button variant="secondary" disabled={locked || busy !== null} onClick={() => void update(newer)} className="h-10 px-4 text-[13px]">
            {busy === newer.versionId ? "Moving…" : `Move to ${newer.versionNumber}`}
          </Button>
        </div>
      )}
    </div>
  );
}
