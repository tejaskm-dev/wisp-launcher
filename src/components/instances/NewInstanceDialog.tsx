import { ChevronDown, Info, Search } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { createInstance, getLoaderGameVersions, getLoaderVersions, getVersions } from "../../data/api";
import type { Loader, LoaderVersion } from "../../data/types";
import { cn } from "../../lib/cn";
import { formatDay, loaderName } from "../../lib/format";
import { useResource } from "../../lib/useResource";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { ErrorState } from "../ui/ErrorState";
import { Segmented, type SegmentOption } from "../ui/Segmented";
import { Select, type SelectOption } from "../ui/Select";
import { Loading, Skeleton } from "../ui/Skeleton";
import { IconPicker } from "./IconPicker";
import { InstanceAvatar } from "./InstanceAvatar";
import { DEFAULT_ICONS, iconId } from "./icons";
import { Sparkle } from "../brand/Sparkle";

const LOADERS: SegmentOption<Loader>[] = [
  { value: "vanilla", label: "Vanilla" },
  { value: "fabric", label: "Fabric" },
  { value: "quilt", label: "Quilt" },
  { value: "forge", label: "Forge" },
  { value: "neoforge", label: "NeoForge" },
];

const LOADER_NOTES: Record<Loader, string> = {
  vanilla: "Minecraft exactly as Mojang ships it, no mods.",
  fabric: "Light and quick to update. Most performance and quality-of-life mods are made for it.",
  quilt: "A Fabric fork that runs most Fabric mods too.",
  forge: "The long-standing loader behind many big modpacks, especially on older versions.",
  neoforge: "Forge's community successor, where many big mods live from 1.20.2 on.",
};

type Channel = "release" | "snapshot" | "old";

const CHANNELS: SegmentOption<Channel>[] = [
  { value: "release", label: "Releases" },
  { value: "snapshot", label: "Snapshots" },
  { value: "old", label: "Beta & Alpha" },
];

const inChannel = (kind: string, channel: Channel) =>
  channel === "old" ? kind === "old_beta" || kind === "old_alpha" : kind === channel;

/** Plenty for scrolling; anything else is a search away. */
const MAX_ROWS = 200;
const FORM_ID = "new-instance";
const RECOMMENDED = "recommended";

// Which Minecraft versions each loader supports, kept while the app is open.
const supportedCache = new Map<Loader, Promise<string[] | null>>();
function supportedVersions(loader: Loader) {
  let pending = supportedCache.get(loader);
  if (!pending) {
    pending = getLoaderGameVersions(loader).catch((error: unknown) => {
      supportedCache.delete(loader);
      throw error;
    });
    supportedCache.set(loader, pending);
  }
  return pending;
}

/** Mount it fresh for each use (App keys it), so every open starts from a clean form. */
export function NewInstanceDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { refreshHome, notify, navigate } = useApp();
  const [versions, retryVersions] = useResource(getVersions);
  const [name, setName] = useState("");
  const [loader, setLoader] = useState<Loader>("vanilla");
  const [channel, setChannel] = useState<Channel>("release");
  const [picked, setPicked] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [icon, setIcon] = useState<string | null>(null);
  const [showIcons, setShowIcons] = useState(false);
  const [loaderVersion, setLoaderVersion] = useState(RECOMMENDED);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The versions this loader supports (null: all), and its own versions for the chosen one.
  const [supported, setSupported] = useState<{ loader: Loader; versions: Set<string> | null } | null>(null);
  useEffect(() => {
    let active = true;
    supportedVersions(loader).then(
      (list) => active && setSupported({ loader, versions: list ? new Set(list) : null }),
      () => active && setSupported({ loader, versions: null }),
    );
    return () => {
      active = false;
    };
  }, [loader]);
  const supportReady = loader === "vanilla" || supported?.loader === loader;
  const supportedSet = supportReady && loader !== "vanilla" ? (supported?.versions ?? null) : null;

  const rows = useMemo(() => {
    if (versions.status !== "ready") return [];
    const needle = filter.trim().toLowerCase();
    return versions.data.versions.filter(
      (entry) =>
        inChannel(entry.kind, channel) &&
        (!supportedSet || supportedSet.has(entry.id)) &&
        (!needle || entry.id.toLowerCase().includes(needle)),
    );
  }, [versions, channel, filter, supportedSet]);

  const latest = versions.status === "ready" ? (channel === "release" ? versions.data.latestRelease : channel === "snapshot" ? versions.data.latestSnapshot : null) : null;
  // The pick, if it's still in the list; else the newest the loader supports in this channel.
  const version = picked && rows.some((row) => row.id === picked) ? picked : (rows.find((row) => !filter.trim() || row.id === latest)?.id ?? rows[0]?.id ?? null);

  const [loaderVersions, setLoaderVersions] = useState<{ key: string; list: LoaderVersion[] } | null>(null);
  const loaderKey = `${loader}/${version ?? ""}`;
  useEffect(() => {
    if (loader === "vanilla" || !version) return;
    let active = true;
    getLoaderVersions(loader, version).then(
      (list) => active && setLoaderVersions({ key: `${loader}/${version}`, list }),
      () => active && setLoaderVersions({ key: `${loader}/${version}`, list: [] }),
    );
    return () => {
      active = false;
    };
  }, [loader, version]);
  const loaderList = loaderVersions?.key === loaderKey ? loaderVersions.list : null;
  const recommended = loaderList?.find((entry) => entry.recommended) ?? loaderList?.[0] ?? null;
  const loaderOptions: SelectOption<string>[] = [
    { value: RECOMMENDED, label: recommended ? `Recommended (${recommended.version})` : "Recommended" },
    ...(loaderList ?? []).map((entry) => ({ value: entry.version, label: entry.version, note: entry.stable ? undefined : "Beta" })),
  ];

  const shownIcon = icon ?? iconId(DEFAULT_ICONS[loader]);
  const suggestedName = version ? `${loaderName(loader)} ${version}` : "My instance";
  const loaderMissing = loader !== "vanilla" && loaderList !== null && loaderList.length === 0;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!version || pending || loaderMissing) return;
    setPending(true);
    setError(null);
    try {
      const instance = await createInstance({
        name: name.trim() || suggestedName,
        gameVersion: version,
        loader,
        loaderVersion: loader === "vanilla" || loaderVersion === RECOMMENDED ? null : loaderVersion,
        icon: shownIcon,
      });
      await refreshHome();
      notify({ tone: "success", title: `${instance.name} is ready`, body: "Minecraft downloads the first time you play it." });
      onClose();
      navigate("instances");
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setPending(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="New instance"
      description="Its own Minecraft, with a version, mods and settings of its own."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form={FORM_ID} disabled={!version || pending || loaderMissing}>
            {pending ? "Creating…" : "Create instance"}
          </Button>
        </>
      }
    >
      <form id={FORM_ID} onSubmit={submit} className="flex flex-col gap-6">
        <div className="flex flex-col gap-3 rounded-[18px] bg-cream/4 p-3.5">
          <div className="flex items-center gap-4">
            <button
              type="button"
              onClick={() => setShowIcons((shown) => !shown)}
              aria-expanded={showIcons}
              aria-label="Choose an icon"
              className="group relative shrink-0 rounded-xl"
            >
              <InstanceAvatar instance={{ name: name.trim() || suggestedName, loader, icon: shownIcon }} className="size-14 text-2xl" />
              <span className="absolute -bottom-1 -right-1 grid size-5 place-items-center rounded-full bg-cream text-ink-900 shadow transition-transform group-hover:scale-110">
                <ChevronDown size={12} strokeWidth={2.6} className={cn("transition-transform", showIcons && "rotate-180")} aria-hidden="true" />
              </span>
            </button>
            <label className="flex min-w-0 flex-1 flex-col gap-0.5">
              <span className="text-xs font-semibold text-mute">Name</span>
              <input
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={suggestedName}
                maxLength={64}
                autoFocus
                spellCheck={false}
                className="min-w-0 bg-transparent font-display text-xl font-bold text-cream outline-none placeholder:text-cream/35"
              />
            </label>
          </div>
          {showIcons && (
            <div className="border-t border-cream/8 pt-3">
              <p className="mb-2 text-xs font-semibold text-mute">Icon</p>
              <IconPicker value={shownIcon} onChange={setIcon} />
            </div>
          )}
        </div>

        <fieldset className="flex flex-col gap-2.5">
          <legend className="mb-2.5 text-sm font-bold">Mod loader</legend>
          <Segmented
            label="Mod loader"
            value={loader}
            options={LOADERS}
            onChange={(next) => {
              setLoader(next);
              setLoaderVersion(RECOMMENDED);
            }}
            className="self-start"
          />
          <p className="text-[13px] text-mute">{LOADER_NOTES[loader]}</p>
        </fieldset>

        <fieldset className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <legend className="text-sm font-bold">Minecraft version</legend>
            <Segmented label="Show" value={channel} options={CHANNELS} onChange={(next) => setChannel(next)} />
          </div>
          {loader !== "vanilla" && supportedSet && (
            <p className="-mt-1 text-xs text-mute-2">Showing the versions {loaderName(loader)} supports.</p>
          )}

          {versions.status === "error" ? (
            <ErrorState title="Couldn't load versions" body="Mojang's version list didn't load. Check your connection." onRetry={retryVersions} />
          ) : (
            <div className="overflow-hidden rounded-[18px] border border-cream/8 bg-ink-900/50">
              <label className="flex items-center gap-2.5 border-b border-cream/8 px-3.5 text-mute focus-within:text-cream-300">
                <Search size={15} aria-hidden="true" />
                <span className="sr-only">Find a version</span>
                <input
                  value={filter}
                  onChange={(event) => setFilter(event.target.value)}
                  placeholder="Find a version"
                  spellCheck={false}
                  className="h-10 min-w-0 flex-1 bg-transparent text-sm text-cream outline-none placeholder:text-mute"
                />
              </label>
              {versions.status === "loading" || !supportReady ? (
                <Loading label="Loading versions" className="flex flex-col gap-1 p-1.5">
                  {[0, 1, 2, 3, 4].map((index) => (
                    <div key={index} className="flex items-center justify-between px-3 py-2.5">
                      <Skeleton className="h-3.5 w-16 rounded-full" />
                      <Skeleton className="h-3 w-12 rounded-full" />
                    </div>
                  ))}
                </Loading>
              ) : rows.length === 0 ? (
                <p className="px-4 py-6 text-center text-sm text-mute">
                  {filter.trim()
                    ? `Nothing matches “${filter.trim()}”.`
                    : `${loaderName(loader)} has no ${channel === "old" ? "beta or alpha" : channel === "release" ? "release" : "snapshot"} versions.`}
                </p>
              ) : (
                <div role="radiogroup" aria-label="Minecraft version" className="max-h-56 overflow-y-auto p-1.5">
                  {rows.slice(0, MAX_ROWS).map((entry) => {
                    const checked = entry.id === version;
                    return (
                      <button
                        key={entry.id}
                        type="button"
                        role="radio"
                        aria-checked={checked}
                        onClick={() => setPicked(entry.id)}
                        className={cn(
                          "flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left text-sm transition-colors duration-150",
                          checked ? "bg-cream text-ink-900" : "text-cream-300 hover:bg-cream/6 hover:text-cream",
                        )}
                      >
                        <span className="font-pixel text-[15px] tracking-[0.02em]">{entry.id}</span>
                        {entry.id === latest && (
                          <span className={cn("rounded-full px-2 py-0.5 text-[11px] font-bold", checked ? "bg-ink-900/10 text-ink-900" : "bg-ember/15 text-ember-soft")}>
                            Latest
                          </span>
                        )}
                        <span className={cn("ml-auto text-xs", checked ? "text-ink-900/60" : "text-mute-2")}>{formatDay(entry.released)}</span>
                        <Sparkle className={cn("size-3.5 shrink-0 text-ember", !checked && "invisible")} />
                      </button>
                    );
                  })}
                  {rows.length > MAX_ROWS && <p className="px-3 py-2 text-xs text-mute-2">{rows.length - MAX_ROWS} more. Search to find one.</p>}
                </div>
              )}
            </div>
          )}
        </fieldset>

        {loader !== "vanilla" && version && (
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <p className="text-sm font-bold">{loaderName(loader)} version</p>
              <p className="text-xs text-mute">The recommended one suits almost everyone.</p>
            </div>
            {loaderList === null ? (
              <Skeleton className="h-10 w-52 rounded-xl" />
            ) : loaderMissing ? (
              <p className="text-sm font-semibold text-coral">
                {loaderName(loader)} has no build for {version}.
              </p>
            ) : (
              <Select label={`${loaderName(loader)} version`} value={loaderVersion} options={loaderOptions} onChange={setLoaderVersion} searchable className="w-56" />
            )}
          </div>
        )}

        <p className="flex items-start gap-2.5 text-[13px] leading-relaxed text-mute">
          <Info size={16} className="mt-0.5 shrink-0" aria-hidden="true" />
          {loader === "forge" || loader === "neoforge"
            ? `Nothing downloads yet. The first time you play, Wisp downloads Minecraft and runs ${loaderName(loader)}'s installer, which takes a minute.`
            : loader === "vanilla"
              ? "Nothing downloads yet. Minecraft downloads the first time you play."
              : `Nothing downloads yet. Minecraft and ${loaderName(loader)} download the first time you play.`}
        </p>

        {error && (
          <p role="alert" className="rounded-2xl border border-coral/25 bg-coral/10 px-4 py-3 text-sm font-semibold text-coral">
            {error}
          </p>
        )}
      </form>
    </Dialog>
  );
}
