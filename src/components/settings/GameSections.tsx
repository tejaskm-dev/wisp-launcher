import { AppWindow, CircleAlert, CircleCheck, Coffee, CopyPlus, LoaderCircle, MemoryStick, SquareTerminal, Trash2 } from "lucide-react";
import { useState } from "react";
import { deleteJavaRuntime, getJavaRuntimes, getSystemMemory, testJava } from "../../data/api";
import type { InstalledRuntime, JavaProbe, WindowMode } from "../../data/types";
import { cn } from "../../lib/cn";
import { formatBytes, instanceLabel } from "../../lib/format";
import { useResource } from "../../lib/useResource";
import { useApp } from "../../state/AppContext";
import { Panel } from "../ui/Page";
import { Select, type SelectOption } from "../ui/Select";
import { Loading, Skeleton } from "../ui/Skeleton";
import { Slider } from "../ui/Slider";
import { Toggle } from "../ui/Toggle";
import { Choice, Divider, Note, NumberField, Row, TextField, type SectionProps } from "./parts";

const formatGb = (mb: number) => `${Number((mb / 1024).toFixed(1))} GB`;
const failure = (error: unknown) => (error instanceof Error ? error.message : String(error));

const WINDOW_MODES: { value: WindowMode; label: string }[] = [
  { value: "default", label: "Minecraft's default" },
  { value: "custom", label: "Custom size" },
  { value: "fullscreen", label: "Fullscreen" },
];

const WINDOW_PRESETS = [
  [1280, 720],
  [1600, 900],
  [1920, 1080],
] as const;

export function GameSection({ settings, update }: SectionProps) {
  const { home } = useApp();
  const [memory] = useResource(getSystemMemory);
  const total = memory.status === "ready" ? memory.data : null;
  // Leave the computer at least 2 GB; never offer less than 2 GB or more than 32 GB.
  const ceiling = Math.max(settings.memoryMb, total ? Math.min(32768, Math.max(2048, Math.floor((total - 2048) / 512) * 512)) : 16384);
  const heavy = total !== null && settings.memoryMb > total * 0.75;
  const gameWindow = settings.gameWindow;
  const options = settings.newInstances;
  const setOption = (patch: Partial<typeof options>) => update((current) => ({ newInstances: { ...current.newInstances, ...patch } }));
  const copyFrom: SelectOption<string>[] = [
    { value: "", label: "Minecraft's defaults" },
    ...(home?.instances ?? []).map((instance) => ({ value: instance.id, label: instance.name, note: instanceLabel(instance) })),
  ];
  const copySource = home?.instances.some((instance) => instance.id === options.copyOptionsFrom) ? (options.copyOptionsFrom ?? "") : "";

  return (
    <>
      <Panel title="Memory" icon={<MemoryStick size={19} aria-hidden="true" />}>
        <Slider
          label="Memory for Minecraft"
          description={
            total
              ? `This computer has ${formatGb(total)}. 4 GB suits most worlds; big modpacks like 6 to 8 GB.`
              : "4 GB suits most worlds; big modpacks like 6 to 8 GB."
          }
          value={settings.memoryMb}
          min={1024}
          max={ceiling}
          step={512}
          format={formatGb}
          onChange={(memoryMb) => update({ memoryMb })}
        />
        {heavy && (
          <p className="flex items-start gap-2 rounded-xl bg-amber/10 px-3.5 py-2.5 text-[13px] leading-snug text-amber-soft">
            <CircleAlert size={15} className="mt-0.5 shrink-0" aria-hidden="true" />
            That's most of this computer's memory. Other apps (and Minecraft itself, outside Java) may slow down.
          </p>
        )}
      </Panel>

      <Panel title="Game window" icon={<AppWindow size={19} aria-hidden="true" />}>
        <Row label="Size when it opens" description="Minecraft remembers a size you drag it to, unless a size is set here." stacked>
          <Choice label="Game window size" value={gameWindow.mode} options={WINDOW_MODES} onChange={(mode) => update((current) => ({ gameWindow: { ...current.gameWindow, mode } }))} />
          {gameWindow.mode === "custom" && (
            <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
              <div className="flex items-center gap-2">
                <NumberField label="Width" value={gameWindow.width} min={320} max={7680} onCommit={(width) => update((current) => ({ gameWindow: { ...current.gameWindow, width } }))} />
                <span aria-hidden="true" className="text-mute">×</span>
                <NumberField label="Height" value={gameWindow.height} min={240} max={4320} suffix="pixels" onCommit={(height) => update((current) => ({ gameWindow: { ...current.gameWindow, height } }))} />
              </div>
              <div className="flex gap-1.5">
                {WINDOW_PRESETS.map(([width, height]) => (
                  <button
                    key={width}
                    type="button"
                    onClick={() => update((current) => ({ gameWindow: { ...current.gameWindow, width, height } }))}
                    className={cn(
                      "h-8 rounded-full px-3 text-xs font-bold tabular-nums transition-colors",
                      gameWindow.width === width && gameWindow.height === height ? "bg-cream/14 text-cream" : "text-mute hover:bg-cream/8 hover:text-cream",
                    )}
                  >
                    {width} × {height}
                  </button>
                ))}
              </div>
            </div>
          )}
        </Row>
      </Panel>

      <Panel title="New instances" icon={<CopyPlus size={19} aria-hidden="true" />} description="How instances you create from now on start out.">
        <Row label="Start with the settings of" description="Copies video settings, keybinds and the rest of options.txt from an instance you've set up.">
          <Select label="Start with the settings of" value={copySource} options={copyFrom} onChange={(value) => setOption({ copyOptionsFrom: value || null })} className="w-[240px]" />
        </Row>
        <Divider />
        <Toggle
          label="Skip the welcome screens"
          description="The accessibility screen and online-play warnings a fresh game shows first."
          checked={options.skipWelcome}
          onChange={(skipWelcome) => setOption({ skipWelcome })}
        />
        <Divider />
        <Toggle label="Turn off tutorial hints" description="The “Use WASD to move” style tips." checked={options.skipTutorial} onChange={(skipTutorial) => setOption({ skipTutorial })} />
        <Divider />
        <Toggle label="Turn off auto-jump" description="Stepping up blocks by walking into them." checked={options.autoJumpOff} onChange={(autoJumpOff) => setOption({ autoJumpOff })} />
        <Note>Modpacks that bring their own settings keep them.</Note>
      </Panel>
    </>
  );
}

const JAVA_VERSIONS = [
  { major: "25", games: "Minecraft 26.1 and newer" },
  { major: "21", games: "Minecraft 1.20.5 to 1.21" },
  { major: "17", games: "Minecraft 1.17 to 1.20.4" },
  { major: "8", games: "Minecraft 1.16.5 and older" },
] as const;

const PLATFORM_NAMES: Record<string, string> = {
  "mac-os-arm64": "Apple Silicon",
  "mac-os": "Intel (Rosetta)",
  "windows-x64": "Windows",
  "windows-arm64": "Windows on Arm",
  "windows-x86": "Windows 32-bit",
  linux: "Linux",
  "linux-i386": "Linux 32-bit",
};

export function JavaSection({ settings, update }: SectionProps) {
  const { notify } = useApp();
  const [runtimes, reload] = useResource(getJavaRuntimes);
  const [deleting, setDeleting] = useState<string | null>(null);

  const remove = async (runtime: InstalledRuntime) => {
    const key = `${runtime.component}/${runtime.platform}`;
    setDeleting(key);
    try {
      await deleteJavaRuntime(runtime.component, runtime.platform);
      notify({ tone: "success", title: `Deleted Java ${runtime.major ?? runtime.version}`, body: `${formatBytes(runtime.sizeBytes)} freed. Wisp downloads it again when a game needs it.` });
      reload();
    } catch (error) {
      notify({ tone: "info", title: "Couldn't delete that Java", body: failure(error) });
    } finally {
      setDeleting(null);
    }
  };

  return (
    <>
      <Panel
        title="Java"
        icon={<Coffee size={19} aria-hidden="true" />}
        description="Each Minecraft version needs a particular Java. Wisp downloads the right one by itself, the same builds the official launcher uses."
      >
        {runtimes.status === "loading" ? (
          <Loading label="Loading installed Java" className="flex flex-col gap-2 py-3">
            {[0, 1].map((index) => (
              <Skeleton key={index} className="h-12 w-full rounded-xl" />
            ))}
          </Loading>
        ) : runtimes.status === "error" || runtimes.data.length === 0 ? (
          <p className="py-3.5 text-[13px] text-mute">{runtimes.status === "error" ? "Couldn't read the Java folder." : "None yet. Wisp downloads Java the first time you play a version that needs it."}</p>
        ) : (
          <ul className="flex flex-col gap-1.5 py-2">
            {runtimes.data.map((runtime) => {
              const key = `${runtime.component}/${runtime.platform}`;
              return (
                <li key={key} className="flex items-center gap-3 rounded-xl bg-ink-900/50 px-3.5 py-2.5">
                  <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-cream/7 font-display text-sm font-bold">{runtime.major ?? "?"}</span>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-semibold">
                      Java {runtime.version} <span className="font-normal text-mute">· {PLATFORM_NAMES[runtime.platform] ?? runtime.platform}</span>
                    </p>
                    <p className="text-xs tabular-nums text-mute-2">{formatBytes(runtime.sizeBytes)}</p>
                  </div>
                  <button
                    type="button"
                    disabled={deleting !== null}
                    onClick={() => void remove(runtime)}
                    aria-label={`Delete Java ${runtime.version} (${PLATFORM_NAMES[runtime.platform] ?? runtime.platform})`}
                    className="flex h-8 items-center gap-1.5 rounded-full px-3 text-xs font-bold text-mute transition-colors enabled:hover:bg-ember/12 enabled:hover:text-ember-soft disabled:opacity-40"
                  >
                    {deleting === key ? <LoaderCircle size={13} className="animate-spin" aria-hidden="true" /> : <Trash2 size={13} aria-hidden="true" />}
                    Delete
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </Panel>

      <Panel title="Use your own Java" icon={<Coffee size={19} aria-hidden="true" />} description="For people who want a particular build. Leave a box empty to use Wisp's own.">
        {JAVA_VERSIONS.map(({ major, games }, index) => (
          <div key={major}>
            {index > 0 && <Divider />}
            <CustomJava
              major={major}
              games={games}
              path={settings.javaPaths[major] ?? ""}
              onChange={(path) =>
                update((current) => {
                  const javaPaths = { ...current.javaPaths };
                  if (path) javaPaths[major] = path;
                  else delete javaPaths[major];
                  return { javaPaths };
                })
              }
            />
          </div>
        ))}
        <Note>The path is to the java program itself, like /Library/Java/JavaVirtualMachines/…/Contents/Home/bin/java. If it goes missing, Wisp says so instead of starting the game.</Note>
      </Panel>

      <Panel title="Java arguments" icon={<SquareTerminal size={19} aria-hidden="true" />}>
        <Row label="Extra arguments" description="Added after Wisp's own (memory and garbage collector settings), so yours win. For example -XX:+UseZGC." stacked>
          <TextField label="Extra Java arguments" value={settings.jvmArgs} onCommit={(jvmArgs) => update({ jvmArgs })} placeholder="None" mono />
        </Row>
      </Panel>
    </>
  );
}

type Probe = { status: "idle" } | { status: "testing" } | { status: "ok"; result: JavaProbe } | { status: "error"; message: string };

function CustomJava({ major, games, path, onChange }: { major: string; games: string; path: string; onChange: (path: string) => void }) {
  const [probe, setProbe] = useState<Probe>({ status: "idle" });
  const test = async (candidate: string) => {
    if (!candidate) return setProbe({ status: "idle" });
    setProbe({ status: "testing" });
    try {
      setProbe({ status: "ok", result: await testJava(candidate) });
    } catch (error) {
      setProbe({ status: "error", message: failure(error) });
    }
  };
  const mismatch = probe.status === "ok" && String(probe.result.major) !== major && !(major === "17" && probe.result.major === 16);

  return (
    <Row label={`Java ${major}`} description={games} stacked>
      <div className="flex gap-2">
        <TextField
          label={`Java ${major} path`}
          value={path}
          placeholder="Wisp's own (recommended)"
          mono
          onCommit={(next) => {
            onChange(next);
            void test(next);
          }}
        />
        <button
          type="button"
          disabled={!path || probe.status === "testing"}
          onClick={() => void test(path)}
          className="flex h-10 shrink-0 items-center gap-1.5 rounded-xl border border-cream/12 px-3.5 text-[13px] font-bold text-cream-300 transition-colors enabled:hover:border-cream/30 enabled:hover:text-cream disabled:opacity-40"
        >
          {probe.status === "testing" && <LoaderCircle size={13} className="animate-spin" aria-hidden="true" />}
          Test
        </button>
      </div>
      {probe.status === "ok" && (
        <p className={cn("flex items-start gap-2 text-[13px]", mismatch ? "text-amber-soft" : "text-moss-soft")}>
          {mismatch ? <CircleAlert size={15} className="mt-0.5 shrink-0" aria-hidden="true" /> : <CircleCheck size={15} className="mt-0.5 shrink-0" aria-hidden="true" />}
          {mismatch ? `That's Java ${probe.result.major}, not Java ${major}. These versions may not start with it.` : probe.result.description}
        </p>
      )}
      {probe.status === "error" && (
        <p className="flex items-start gap-2 text-[13px] text-ember-soft">
          <CircleAlert size={15} className="mt-0.5 shrink-0" aria-hidden="true" />
          {probe.message}
        </p>
      )}
    </Row>
  );
}

const HOOK_VARIABLES = ["$INST_NAME", "$INST_ID", "$INST_DIR", "$INST_MC_DIR", "$INST_JAVA", "$INST_MC_VERSION", "$INST_LOADER"];

export function HooksSection({ settings, update }: SectionProps) {
  const setHook = (key: keyof typeof settings.hooks, value: string) => update((current) => ({ hooks: { ...current.hooks, [key]: value } }));
  return (
    <Panel
      title="Launch hooks"
      icon={<SquareTerminal size={19} aria-hidden="true" />}
      description="Commands Wisp runs around every game, like Prism Launcher's custom commands. They run in the instance's game folder."
    >
      <Row label="Before the game starts" description="If it fails, the game doesn't start and Wisp shows its last line of output." stacked>
        <TextField label="Pre-launch command" value={settings.hooks.preLaunch} onCommit={(value) => setHook("preLaunch", value)} placeholder="None" mono />
      </Row>
      <Divider />
      <Row label="Start Java through" description="A wrapper program, like gamemoderun, mangohud or prime-run on Linux." stacked>
        <TextField label="Wrapper command" value={settings.hooks.wrapper} onCommit={(value) => setHook("wrapper", value)} placeholder="None" mono />
      </Row>
      <Divider />
      <Row label="After the game closes" description="Runs on its own; Wisp doesn't wait for it." stacked>
        <TextField label="Post-exit command" value={settings.hooks.postExit} onCommit={(value) => setHook("postExit", value)} placeholder="None" mono />
      </Row>
      <div className="mt-3 rounded-xl bg-ink-900/50 p-3.5">
        <p className="text-xs font-semibold text-mute">Commands can use these variables:</p>
        <ul className="mt-2 flex flex-wrap gap-1.5">
          {HOOK_VARIABLES.map((variable) => (
            <li key={variable}>
              <code className="rounded-md bg-cream/7 px-2 py-1 font-mono text-[12px] text-cream-300 select-text">{variable}</code>
            </li>
          ))}
        </ul>
      </div>
    </Panel>
  );
}
