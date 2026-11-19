import { Copy, Cpu, Layers, LoaderCircle, Package, Palette, Rocket, SlidersHorizontal, Trash2, Upload, Zap } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  deleteInstance,
  duplicateInstance,
  exportInstance,
  getInstancesKeepingSettings,
  getLoaderGameVersions,
  getScreenshots,
  getInstanceMemory,
  getSystemMemory,
  getVersions,
  moveInstance,
  planInstanceMove,
  setInstanceJava,
  setInstanceKeepsSettings,
  setInstanceMemory,
  shareInstanceSettings,
  speedUpInstance,
  thumbUrl,
  updateInstance,
} from "../../data/api";
import type { GameWindow, Instance, InstanceMemory, LaunchHooks, Loader, LocalSettings, MovePlan, Screenshot, SpedUp, WindowMode } from "../../data/types";
import { cn } from "../../lib/cn";
import { formatBytes, instanceLabel, loaderName } from "../../lib/format";
import { sharesSettings } from "../../lib/sharedSettings";
import { useApp } from "../../state/AppContext";
import { IconPicker } from "../instances/IconPicker";
import { DEFAULT_ICONS, iconId } from "../instances/icons";
import { VersionTag } from "../instances/VersionTag";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Segmented } from "../ui/Segmented";
import { Select } from "../ui/Select";
import { Slider } from "../ui/Slider";
import { Toggle } from "../ui/Toggle";
import { PackUpdate } from "./PackUpdate";
import { memoryLabel } from "./QuickSettings";
import { Panel, Reveal, listNames, message } from "./shared";

const LOADERS: Loader[] = ["vanilla", "fabric", "quilt", "forge", "neoforge"];
const JAVA_CHOICES = ["auto", "8", "17", "21", "25"] as const;

const SPEED_UP_NOTES: Record<Loader, string> = {
  vanilla: "Moves it to Fabric and adds Sodium, Lithium, FerriteCore, ModernFix and friends: often several times the frame rate, and quicker loading. Your worlds stay as they are.",
  fabric: "Adds Sodium, Lithium, FerriteCore, ModernFix and friends where they have builds for this version: more frames, less memory, quicker loading.",
  quilt: "Adds Sodium, Lithium, FerriteCore, ModernFix and friends where they have builds for this version: more frames, less memory, quicker loading.",
  neoforge: "Adds Sodium (or Embeddium), FerriteCore, ModernFix and friends where they have builds for this version: more frames, less memory, quicker loading.",
  forge: "Adds Embeddium, FerriteCore, ModernFix and friends where they have builds for this version: more frames, less memory, quicker loading.",
};

function spedSummary(sped: SpedUp) {
  const added = sped.added.filter((item) => item.explicit).map((item) => item.title);
  return [
    added.length > 0 ? `Added ${added.join(", ")}.` : "Nothing new to add.",
    sped.had.length > 0 ? `Already had ${sped.had.join(", ")}.` : "",
    sped.skipped.length > 0 ? `Left out (no build for this version, or it would clash): ${sped.skipped.join(", ")}.` : "",
  ]
    .filter(Boolean)
    .join(" ");
}

/** A control with its label shown above it: it's always clear what a control changes. */
function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-xs font-semibold text-mute">{label}</span>
      {children}
    </div>
  );
}


function TextField({ label, value, onCommit, placeholder, mono = false }: { label: string; value: string; onCommit: (value: string) => void; placeholder?: string; mono?: boolean }) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <label className="flex flex-col gap-1">
      <span className="text-xs font-semibold text-mute">{label}</span>
      <input
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => draft !== value && onCommit(draft)}
        onKeyDown={(event) => event.key === "Enter" && (event.currentTarget as HTMLInputElement).blur()}
        placeholder={placeholder}
        spellCheck={false}
        className={cn("h-10 rounded-xl border border-cream/12 bg-ink-900/50 px-3 text-sm outline-none focus:border-cream/35", mono && "font-mono text-[13px]")}
      />
    </label>
  );
}

/** Everything about an instance that can change: name and look, version, modpack, memory and Java, launching, game settings, copies and exports. */
export function SettingsTab({
  instance,
  locked,
  local,
  onLocal,
  onDeleted,
}: {
  instance: Instance;
  locked: boolean;
  /** What it does differently on this computer (null while loading), and how to change it. */
  local: LocalSettings | null;
  onLocal: (patch: Partial<LocalSettings>) => void;
  onDeleted: () => void;
}) {
  const { notify, refreshHome, settings, openInstance } = useApp();
  const saveLocal = onLocal;
  const saved = async (work: Promise<unknown>, failTitle: string) => {
    try {
      await work;
      await refreshHome();
    } catch (failure) {
      notify({ tone: "info", title: failTitle, body: message(failure) });
    }
  };

  return (
    <div className="grid items-start gap-4 xl:grid-cols-2">
      <div className="flex flex-col gap-4">
        <NameAndLook instance={instance} local={local} onBanner={(banner) => saveLocal({ banner })} saved={saved} />
        <VersionMove instance={instance} locked={locked} />
        {instance.modpack && (
          <Panel icon={Package} title="Modpack" description={`Made from ${instance.modpack.title} ${instance.modpack.versionNumber}.`}>
            <PackUpdate instance={instance} locked={locked} onUpdated={() => void refreshHome()} quietWhenCurrent={false} />
          </Panel>
        )}
        <SharedSettings instance={instance} shareOptions={settings?.shareOptions === true} />
      </div>
      <div className="flex flex-col gap-4">
        <MemoryAndJava instance={instance} saved={saved} />
        <Launching local={local} wisp={settings ? { jvmArgs: settings.jvmArgs, gameWindow: settings.gameWindow, hooks: settings.hooks } : null} onChange={saveLocal} />
        <SpeedUp instance={instance} locked={locked} />
        <CopyExportDelete instance={instance} locked={locked} onDeleted={onDeleted} onCopied={(copy) => openInstance(copy.id)} />
      </div>
    </div>
  );
}

function NameAndLook({ instance, local, onBanner, saved }: { instance: Instance; local: LocalSettings | null; onBanner: (banner: string | null) => void; saved: (work: Promise<unknown>, failTitle: string) => Promise<void> }) {
  const [shots, setShots] = useState<Screenshot[] | null>(null);
  const banner = local?.banner ?? null;
  const mode = banner === "none" ? "none" : banner?.startsWith("screenshot:") ? "pick" : "auto";
  const [picking, setPicking] = useState(false);
  useEffect(() => {
    if (!picking && mode !== "pick") return;
    let live = true;
    getScreenshots().then(
      (all) => live && setShots(all.filter((shot) => shot.instanceId === instance.id).sort((a, b) => b.takenAt - a.takenAt)),
      () => live && setShots([]),
    );
    return () => {
      live = false;
    };
  }, [picking, mode, instance.id]);
  const icon = instance.icon ?? iconId(DEFAULT_ICONS[instance.loader]);

  return (
    <Panel icon={Palette} title="Name and look" description="What it's called, its icon, and the picture at the top of this page.">
      <div className="flex flex-col gap-5">
        <TextField label="Name" value={instance.name} onCommit={(name) => name.trim() && void saved(updateInstance(instance.id, { name: name.trim() }), "Couldn't rename it")} />
        <div>
          <p className="mb-2 text-xs font-semibold text-mute">Icon</p>
          <IconPicker value={icon} onChange={(next) => void saved(updateInstance(instance.id, { icon: next }), "Couldn't change the icon")} />
        </div>
        <div className="flex flex-col gap-2.5">
          <Field label="Banner">
          <Segmented
            label="Banner"
            value={picking ? "pick" : mode}
            options={[
              { value: "auto", label: "Newest screenshot" },
              { value: "pick", label: "Pick one" },
              { value: "none", label: "None" },
            ]}
            onChange={(value) => {
              if (value === "pick") return setPicking(true);
              setPicking(false);
              onBanner(value === "none" ? "none" : null);
            }}
          />
          </Field>
          <Reveal open={picking || mode === "pick"}>
          {(picking || mode === "pick") &&
            (shots === null ? (
              <p className="text-xs text-mute">Loading its screenshots…</p>
            ) : shots.length === 0 ? (
              <p className="text-xs text-mute">No screenshots in it yet. Press F2 in the game to take one.</p>
            ) : (
              <ul className="grid max-h-[220px] grid-cols-3 gap-1.5 overflow-y-auto pr-1">
                {shots.map((shot) => {
                  const chosen = banner === `screenshot:${shot.file}`;
                  return (
                    <li key={shot.id}>
                      <button
                        type="button"
                        onClick={() => (setPicking(false), onBanner(`screenshot:${shot.file}`))}
                        aria-pressed={chosen}
                        className={cn("block aspect-video w-full overflow-hidden rounded-lg ring-2 transition-shadow", chosen ? "ring-cream" : "ring-transparent hover:ring-cream/30")}
                      >
                        <img src={thumbUrl(shot.path, 320)} alt="" loading="lazy" className="size-full object-cover" />
                      </button>
                    </li>
                  );
                })}
              </ul>
            ))}
          </Reveal>
        </div>
      </div>
    </Panel>
  );
}

function VersionMove({ instance, locked }: { instance: Instance; locked: boolean }) {
  const { notify, refreshHome } = useApp();
  const [loader, setLoader] = useState<Loader>(instance.loader);
  const [versions, setVersions] = useState<string[] | null>(null);
  const [target, setTarget] = useState(instance.gameVersion);
  const [plan, setPlan] = useState<MovePlan | null>(null);
  const [busy, setBusy] = useState<"plan" | "move" | null>(null);

  useEffect(() => {
    let live = true;
    setVersions(null);
    const found =
      loader === "vanilla"
        ? getVersions().then((list) => list.versions.filter((version) => version.kind === "release").map((version) => version.id))
        : getLoaderGameVersions(loader).then(async (list) => list ?? (await getVersions()).versions.filter((version) => version.kind === "release").map((version) => version.id));
    found.then(
      (list) => {
        if (!live) return;
        setVersions(list);
        setTarget((current) => (list.includes(current) ? current : (list[0] ?? current)));
      },
      () => live && setVersions([instance.gameVersion]),
    );
    return () => {
      live = false;
    };
  }, [loader, instance.gameVersion]);
  useEffect(() => setPlan(null), [loader, target]);

  const same = loader === instance.loader && target === instance.gameVersion;
  const older = versions !== null && versions.indexOf(target) > versions.indexOf(instance.gameVersion) && versions.includes(instance.gameVersion);
  const label = `${loaderName(loader)} ${target}`;

  const check = async () => {
    setBusy("plan");
    try {
      setPlan(await planInstanceMove(instance.id, target, loader));
    } catch (failure) {
      notify({ tone: "info", title: "Couldn't check that version", body: message(failure) });
    } finally {
      setBusy(null);
    }
  };
  const move = async () => {
    setBusy("move");
    try {
      const done = await moveInstance(instance.id, target, loader, null);
      await refreshHome();
      setPlan(null);
      notify({
        tone: done.moved.failed.length > 0 ? "info" : "success",
        title: `${instance.name} runs ${instanceLabel(done.instance)} now`,
        body: [
          done.moved.updated.length > 0 ? `${done.moved.updated.length} mods moved to their builds for it.` : "",
          done.moved.turnedOff.length > 0 ? `Turned off ${listNames(done.moved.turnedOff)}: no build yet.` : "",
          done.moved.failed.length > 0 ? `Couldn't move ${listNames(done.moved.failed.map(([name]) => name))}.` : "",
          done.restorePoints > 0 ? `Every world got a restore point first.` : "",
        ]
          .filter(Boolean)
          .join(" "),
        lifetimeMs: 14_000,
      });
    } catch (failure) {
      notify({ tone: "info", title: "Couldn't move it", body: message(failure) });
    } finally {
      setBusy(null);
    }
  };

  const group = (title: string, names: string[], tone: string, note?: string): ReactNode =>
    names.length > 0 && (
      <li className="rounded-xl bg-cream/[0.04] px-3 py-2.5">
        <p className={cn("text-[13px] font-bold", tone)}>
          {title} <span className="tabular-nums">{names.length}</span>
        </p>
        <p className="mt-0.5 text-xs leading-relaxed text-mute">
          {listNames(names, 6)}
          {note && <span className="text-mute-2"> · {note}</span>}
        </p>
      </li>
    );

  return (
    <Panel
      icon={Layers}
      title="Version"
      description={
        <>
          It runs {loaderName(instance.loader)}
          {instance.loaderVersion ? ` ${instance.loaderVersion}` : ""} for Minecraft <VersionTag version={instance.gameVersion} />. Moving it swaps each mod for its build for the new version.
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <Field label="Mod loader">
          <Segmented label="Mod loader" value={loader} options={LOADERS.map((value) => ({ value, label: loaderName(value) }))} onChange={setLoader} />
        </Field>
        <div className="flex flex-wrap items-end gap-2.5">
          <div className="min-w-[180px] flex-1">
            {versions === null ? (
              <p className="text-xs text-mute">Loading versions…</p>
            ) : (
              <Field label="Minecraft version">
                <Select label="Minecraft version" value={target} options={versions.map((value) => ({ value, label: value }))} onChange={setTarget} searchable />
              </Field>
            )}
          </div>
          <Button variant="secondary" disabled={same || locked || busy !== null || versions === null} onClick={() => void check()} className="h-10 px-4 text-[13px]">
            {busy === "plan" ? "Checking…" : "See what changes"}
          </Button>
        </div>
        <Reveal open={plan !== null}>
        {plan && (
          <div className="flex flex-col gap-2.5">
            <ul className="flex flex-col gap-1.5">
              {group("Get their build for it", plan.updates.map((item) => item.name), "text-moss-soft")}
              {group("Already right", plan.unchanged.map((item) => item.name), "text-cream-300")}
              {group("No build yet, so turned off", plan.withoutBuild.map((item) => item.name), "text-coral", "back on when you move back")}
              {group("Not on Modrinth, left as they are", plan.unknown.map((item) => item.name), "text-ember-soft", "check these yourself")}
            </ul>
            {loader === "vanilla" && instance.loader !== "vanilla" && <p className="text-xs leading-relaxed text-mute">Plain Minecraft runs no mods, so they're all turned off (not deleted).</p>}
            {older && <p className="text-xs leading-relaxed text-ember-soft">Older Minecraft versions can't always open worlds made in newer ones, so every world gets a restore point first.</p>}
            <Button disabled={locked || busy !== null} onClick={() => void move()} className="h-10 self-start px-4 text-[13px]">
              {busy === "move" ? "Moving…" : `Move to ${label}`}
            </Button>
          </div>
        )}
        </Reveal>
      </div>
    </Panel>
  );
}

function MemoryAndJava({ instance, saved }: { instance: Instance; saved: (work: Promise<unknown>, failTitle: string) => Promise<void> }) {
  const { settings } = useApp();
  const [system, setSystem] = useState<number | null>(null);
  const [memory, setMemory] = useState<number | null>(instance.memoryMb ?? null);
  // What Wisp picks when it has none of its own (more than the setting for a big modpack).
  const [automatic, setAutomatic] = useState<InstanceMemory | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    getSystemMemory().then(setSystem, () => setSystem(null));
  }, []);
  useEffect(() => setMemory(instance.memoryMb ?? null), [instance.memoryMb]);
  useEffect(() => {
    getInstanceMemory(instance.id).then(setAutomatic, () => setAutomatic(null));
  }, [instance.id, instance.memoryMb, settings?.memoryMb]);
  const wisp = automatic?.automaticMb ?? settings?.memoryMb ?? 4096;
  const max = Math.max(4096, Math.floor(((system ?? 16384) * 0.75) / 512) * 512);
  const commit = (value: number | null) => {
    setMemory(value);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => void saved(setInstanceMemory(instance.id, value), "Couldn't change its memory"), 450);
  };
  const java = instance.javaMajor ? String(instance.javaMajor) : "auto";

  return (
    <Panel icon={Cpu} title="Memory and Java" description="How much memory the game gets, and which Java runs it.">
      <div className="flex flex-col gap-2">
        <Toggle
          label="Its own memory"
          description={
            memory !== null
              ? "Instead of what Wisp picks. Big modpacks often want more."
              : automatic && automatic.automaticMb > automatic.settingMb
                ? `Wisp gives it ${memoryLabel(wisp)} for its ${automatic.mods} mods (your setting is ${memoryLabel(automatic.settingMb)}).`
                : `Uses Wisp's setting: ${memoryLabel(wisp)}.`
          }
          checked={memory !== null}
          onChange={(own) => commit(own ? Math.min(max, Math.max(wisp, 6144)) : null)}
        />
        <Reveal open={memory !== null}>
          <Slider label="Memory" value={memory ?? wisp} min={1024} max={max} step={512} onChange={(value) => commit(value)} format={memoryLabel} />
        </Reveal>
        <Field label="Java">
          <Select
            label="Java"
            value={java}
            options={JAVA_CHOICES.map((value) => ({ value, label: value === "auto" ? "The one Minecraft asks for" : `Java ${value}` }))}
            onChange={(value) => void saved(setInstanceJava(instance.id, value === "auto" ? null : Number(value)), "Couldn't change its Java")}
          />
        </Field>
      </div>
    </Panel>
  );
}

function Launching({ local, wisp, onChange }: { local: LocalSettings | null; wisp: { jvmArgs: string; gameWindow: GameWindow; hooks: LaunchHooks } | null; onChange: (patch: Partial<LocalSettings>) => void }) {
  if (!local || !wisp) return null;
  const window = local.gameWindow ?? null;
  const hooks = local.hooks ?? null;
  const windowMode: "wisp" | WindowMode = window ? window.mode : "wisp";
  return (
    <Panel icon={Rocket} title="Launching" description="Settings for starting this instance on this computer. Each one can follow Wisp's or be its own.">
      <div className="flex flex-col gap-2">
        <Toggle
          label="Its own Java arguments"
          description={local.jvmArgs == null ? (wisp.jvmArgs ? `Uses Wisp's: ${wisp.jvmArgs}` : "Uses Wisp's (none).") : "Added after the memory setting."}
          checked={local.jvmArgs != null}
          onChange={(own) => onChange({ jvmArgs: own ? wisp.jvmArgs : null })}
        />
        <Reveal open={local.jvmArgs != null}>
          <TextField label="Java arguments" value={local.jvmArgs ?? ""} onCommit={(jvmArgs) => onChange({ jvmArgs })} placeholder="-XX:+UseZGC" mono />
        </Reveal>
        <div className="flex flex-col gap-2.5 py-3">
          <Field label="Game window">
          <Segmented
            label="Window"
            value={windowMode}
            options={[
              { value: "wisp", label: "Wisp's" },
              { value: "default", label: "Default" },
              { value: "custom", label: "Size" },
              { value: "fullscreen", label: "Fullscreen" },
            ]}
            onChange={(mode) => onChange({ gameWindow: mode === "wisp" ? null : { ...(window ?? wisp.gameWindow), mode } })}
          />
          </Field>
          <Reveal open={window?.mode === "custom"}>
            {window && (
              <div className="grid grid-cols-2 gap-2.5">
                <TextField label="Width" value={String(window.width)} onCommit={(value) => Number(value) > 0 && onChange({ gameWindow: { ...window, width: Math.round(Number(value)) } })} />
                <TextField label="Height" value={String(window.height)} onCommit={(value) => Number(value) > 0 && onChange({ gameWindow: { ...window, height: Math.round(Number(value)) } })} />
              </div>
            )}
          </Reveal>
        </div>
        <Toggle
          label="Its own launch commands"
          description={hooks ? "Run before it starts, wrapped around it, and after it closes." : "Uses Wisp's (Settings, Launching)."}
          checked={hooks !== null}
          onChange={(own) => onChange({ hooks: own ? wisp.hooks : null })}
        />
        <Reveal open={hooks !== null}>
        {hooks && (
          <div className="flex flex-col gap-2.5">
            <TextField label="Before it starts" value={hooks.preLaunch} onCommit={(preLaunch) => onChange({ hooks: { ...hooks, preLaunch } })} placeholder="A command, or empty" mono />
            <TextField label="Start it through" value={hooks.wrapper} onCommit={(wrapper) => onChange({ hooks: { ...hooks, wrapper } })} placeholder="gamemoderun, prime-run…" mono />
            <TextField label="After it closes" value={hooks.postExit} onCommit={(postExit) => onChange({ hooks: { ...hooks, postExit } })} placeholder="A command, or empty" mono />
          </div>
        )}
        </Reveal>
      </div>
    </Panel>
  );
}

function SharedSettings({ instance, shareOptions }: { instance: Instance; shareOptions: boolean }) {
  const { notify } = useApp();
  const shares = shareOptions && sharesSettings(instance.gameVersion);
  const [keepsOwn, setKeepsOwn] = useState<boolean | null>(null);
  const [sharing, setSharing] = useState(false);
  useEffect(() => {
    if (!shares) return;
    let live = true;
    getInstancesKeepingSettings().then(
      (ids) => live && setKeepsOwn(ids.includes(instance.id)),
      () => live && setKeepsOwn(false),
    );
    return () => {
      live = false;
    };
  }, [shares, instance.id]);
  if (!shares || keepsOwn === null) return null;

  const keep = async (value: boolean) => {
    setKeepsOwn(value);
    try {
      await setInstanceKeepsSettings(instance.id, value);
    } catch (failure) {
      setKeepsOwn(!value);
      notify({ tone: "info", title: "Couldn't change that", body: message(failure) });
    }
  };
  const shareAll = async () => {
    setSharing(true);
    try {
      const count = await shareInstanceSettings(instance.id);
      notify({ tone: "success", title: "Shared", body: `Its settings go to ${count === 1 ? "1 other instance" : `${count} other instances`} the next time each starts.` });
    } catch (failure) {
      notify({ tone: "info", title: "Couldn't share them", body: message(failure) });
    } finally {
      setSharing(false);
    }
  };
  return (
    <Panel icon={SlidersHorizontal} title="Game settings" description="FOV, keybinds, sound and the rest follow you between instances on this computer.">
      <Toggle
        label="Keep its own settings"
        description="It stops sharing them with your other instances, both ways. Handy for a heavy modpack that needs a lower render distance."
        checked={keepsOwn}
        onChange={(value) => void keep(value)}
      />
      {!keepsOwn && (
        <Button variant="secondary" disabled={sharing} onClick={() => void shareAll()} className="mt-2 h-10 px-4 text-[13px]">
          {sharing ? "Sharing…" : "Share its settings with all"}
        </Button>
      )}
    </Panel>
  );
}

function SpeedUp({ instance, locked }: { instance: Instance; locked: boolean }) {
  const { notify, refreshHome } = useApp();
  const [busy, setBusy] = useState(false);
  const [sped, setSped] = useState<SpedUp | null>(null);
  const run = async () => {
    setBusy(true);
    try {
      const result = await speedUpInstance(instance.id);
      setSped(result);
      await refreshHome();
      notify({
        tone: "success",
        title: result.added.length > 0 ? `${instance.name} is sped up` : `${instance.name} already has them`,
        body: result.movedToFabric ? "It runs on Fabric now, so it can use mods. Your worlds are as they were." : undefined,
      });
    } catch (failure) {
      notify({ tone: "info", title: "Couldn't speed it up", body: message(failure) });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Panel
      icon={Zap}
      title="Speed it up"
      description={sped ? spedSummary(sped) : SPEED_UP_NOTES[instance.loader]}
      action={
        <Button variant="secondary" icon={busy ? undefined : Zap} disabled={busy || locked} onClick={() => void run()} className="h-10 px-4 text-[13px]">
          {busy ? "Adding…" : sped ? "Check again" : "Speed up"}
        </Button>
      }
    />
  );
}

function CopyExportDelete({ instance, locked, onDeleted, onCopied }: { instance: Instance; locked: boolean; onDeleted: () => void; onCopied: (copy: Instance) => void }) {
  const { notify, refreshHome, home } = useApp();
  const [dialog, setDialog] = useState<"copy" | "export" | "delete" | null>(null);
  const [busy, setBusy] = useState(false);
  const [name, setName] = useState(`${instance.name} copy`);
  const [withWorlds, setWithWorlds] = useState(false);
  const [version, setVersion] = useState("1.0.0");
  const [configs, setConfigs] = useState(true);
  const [options, setOptions] = useState(false);
  const worlds = useMemo(() => (home?.worlds ?? []).filter((world) => world.instanceId === instance.id).length, [home?.worlds, instance.id]);

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    try {
      await work();
      setDialog(null);
    } catch (failure) {
      notify({ tone: "info", title: dialog === "export" ? "Couldn't export it" : dialog === "copy" ? "Couldn't copy it" : "Couldn't delete it", body: message(failure) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Panel icon={Copy} title="Copy, export or delete" description="A copy is a new instance to try things in. An export is a .mrpack anyone can install, in Wisp or another launcher.">
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" icon={Copy} onClick={() => setDialog("copy")} className="h-10 px-4 text-[13px]">
          Make a copy
        </Button>
        <Button variant="secondary" icon={Upload} onClick={() => setDialog("export")} className="h-10 px-4 text-[13px]">
          Export as a modpack
        </Button>
        <button
          type="button"
          disabled={locked}
          onClick={() => setDialog("delete")}
          className="flex h-10 items-center gap-2 rounded-full px-4 text-[13px] font-bold text-coral transition-colors hover:bg-coral/10 disabled:opacity-40"
        >
          <Trash2 size={15} aria-hidden="true" />
          Delete…
        </button>
      </div>

      <Dialog
        open={dialog === "copy"}
        onClose={() => setDialog(null)}
        title={`Copy ${instance.name}`}
        description="Its mods, packs, configs and settings come along. Logs and screenshots stay behind."
        footer={
          <>
            <Button variant="secondary" onClick={() => setDialog(null)}>
              Cancel
            </Button>
            <Button
              disabled={busy || !name.trim()}
              onClick={() =>
                void run(async () => {
                  const copy = await duplicateInstance(instance.id, name.trim(), withWorlds);
                  await refreshHome();
                  notify({ tone: "success", title: `Made ${copy.name}` });
                  onCopied(copy);
                })
              }
            >
              {busy ? "Copying…" : "Make the copy"}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-3">
          <TextField label="Name" value={name} onCommit={setName} />
          <Toggle
            label="Copy its worlds too"
            description={worlds === 0 ? "It has no worlds." : `${worlds === 1 ? "1 world" : `${worlds} worlds`}. Without them, the copy starts with none.`}
            checked={withWorlds}
            disabled={worlds === 0 || locked}
            onChange={setWithWorlds}
          />
        </div>
      </Dialog>

      <Dialog
        open={dialog === "export"}
        onClose={() => setDialog(null)}
        title={`Export ${instance.name}`}
        description="Mods and packs on Modrinth are listed for download; anything else travels inside the file. Worlds and screenshots stay out."
        footer={
          <>
            <Button variant="secondary" onClick={() => setDialog(null)}>
              Cancel
            </Button>
            <Button
              disabled={busy || !version.trim()}
              onClick={() =>
                void run(async () => {
                  const done = await exportInstance(instance.id, { version: version.trim(), configs, options });
                  if (done) {
                    notify({
                      tone: "success",
                      title: "Exported",
                      body: `${done.downloads} downloads from Modrinth and ${done.bundled} files of its own, ${formatBytes(done.bytes)}. Saved to ${done.path}`,
                      lifetimeMs: 12_000,
                    });
                  }
                })
              }
            >
              {busy ? "Exporting…" : "Choose where to save"}
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-2">
          <TextField label="Its version" value={version} onCommit={setVersion} placeholder="1.0.0" />
          <Toggle label="Include configs" description="config/ and the other folders mods keep settings in. Usually wanted: they're part of how a pack plays." checked={configs} onChange={setConfigs} />
          <Toggle label="Include game settings" description="Your options.txt: FOV, keybinds, video settings. Players get yours as their starting point." checked={options} onChange={setOptions} />
        </div>
      </Dialog>

      <Dialog
        open={dialog === "delete"}
        onClose={() => setDialog(null)}
        title={`Delete ${instance.name}?`}
        description="Its worlds, mods and settings go for good. Restore points of its worlds stay in the journal."
        footer={
          <>
            <Button variant="secondary" onClick={() => setDialog(null)}>
              Keep it
            </Button>
            <button
              type="button"
              disabled={busy || locked}
              onClick={() =>
                void run(async () => {
                  await deleteInstance(instance.id);
                  await refreshHome();
                  notify({ tone: "success", title: `${instance.name} deleted` });
                  onDeleted();
                })
              }
              className="flex h-11 items-center gap-2 rounded-full bg-coral px-5 text-sm font-bold text-ink-900 transition-transform active:scale-95 disabled:opacity-50"
            >
              {busy ? <LoaderCircle size={15} className="animate-spin" aria-hidden="true" /> : <Trash2 size={15} aria-hidden="true" />}
              Delete
            </button>
          </>
        }
      >
        <p className="text-sm leading-relaxed text-cream-300">
          {worlds === 0 ? "It has no worlds." : worlds === 1 ? "Its 1 world goes with it." : `Its ${worlds} worlds go with it.`} Think of a copy or an export first if you might want it back.
        </p>
      </Dialog>
    </Panel>
  );
}
