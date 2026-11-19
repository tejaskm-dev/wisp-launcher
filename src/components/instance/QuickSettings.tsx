import { MemoryStick, SlidersHorizontal } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { getInstanceMemory, getInstancesKeepingSettings, setInstanceKeepsSettings, setInstanceMemory, shareInstanceSettings } from "../../data/api";
import type { Instance, InstanceMemory } from "../../data/types";
import { cn } from "../../lib/cn";
import { sharesSettings } from "../../lib/sharedSettings";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Popover } from "../ui/Popover";
import { Slider } from "../ui/Slider";
import { Toggle } from "../ui/Toggle";
import { message } from "./shared";

// The two things people look for before playing an instance, right beside Play: how much memory
// it gets, and whether its game settings (FOV, keybinds and the rest) follow the others.

/** Memory in MB as people say it: "4 GB", "6.5 GB". */
export function memoryLabel(mb: number): string {
  const gb = mb / 1024;
  return Number.isInteger(gb) ? `${gb} GB` : `${gb.toFixed(1)} GB`;
}

/** A pill beside Play that shows a setting and opens it. */
function QuickButton({ anchor, icon, label, open, onClick }: { anchor: RefObject<HTMLButtonElement | null>; icon: ReactNode; label: string; open: boolean; onClick: () => void }) {
  return (
    <button
      ref={anchor}
      type="button"
      onClick={onClick}
      aria-haspopup="dialog"
      aria-expanded={open}
      className={cn(
        "flex h-11 items-center gap-2 rounded-full border bg-ink-900/50 px-4 text-sm font-bold backdrop-blur-sm transition-colors hover:border-cream/40 hover:text-cream",
        open ? "border-cream/40 text-cream" : "border-cream/18 text-cream-200",
      )}
    >
      {icon}
      {label}
    </button>
  );
}

function Heading({ children }: { children: ReactNode }) {
  return <p className="text-[15px] font-bold text-cream">{children}</p>;
}

function Text({ children, className }: { children: ReactNode; className?: string }) {
  return <p className={cn("mt-1 text-[13px] leading-relaxed text-mute", className)}>{children}</p>;
}

/** The memory this instance's next start gets, and a slider to give it its own. */
export function MemoryButton({ instance }: { instance: Instance }) {
  const { notify, refreshHome } = useApp();
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [memory, setMemory] = useState<InstanceMemory | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const load = useCallback(() => getInstanceMemory(instance.id).then(setMemory, () => setMemory(null)), [instance.id]);
  useEffect(() => {
    void load();
  }, [load, instance.memoryMb]);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  if (!memory) return null;

  // By hand it can go further than Wisp would pick by itself, up to three quarters of the computer's.
  const max = Math.max(4096, memory.mb, Math.floor(((memory.systemMb ?? 16384) * 0.75) / 512) * 512);
  const save = (value: number | null) => {
    setMemory({ ...memory, mb: value ?? memory.automaticMb, own: value !== null });
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      setInstanceMemory(instance.id, value).then(
        () => void refreshHome(),
        (failure: unknown) => {
          notify({ tone: "info", title: "Couldn't change its memory", body: message(failure) });
          void load();
        },
      );
    }, 450);
  };
  const why = memory.own
    ? `Set for this instance. Wisp would pick ${memoryLabel(memory.automaticMb)}.`
    : memory.automaticMb > memory.settingMb
      ? `Wisp picked this for its ${memory.mods} mods. Big modpacks run out of memory at the ${memoryLabel(memory.settingMb)} vanilla is happy with.`
      : "Your setting for every instance. Instances with lots of mods get more by themselves.";

  return (
    <>
      <QuickButton anchor={anchor} icon={<MemoryStick size={15} aria-hidden="true" />} label={`${memoryLabel(memory.mb)} memory`} open={open} onClick={() => setOpen((value) => !value)} />
      <Popover open={open} anchorRef={anchor} placement="bottom-start" onClose={() => setOpen(false)} label="Memory" className="w-[340px]">
        <div className="p-4">
          <Heading>Memory</Heading>
          <Text>{why}</Text>
          <Slider label={memory.own ? "Its own memory" : "Memory"} value={memory.mb} min={1024} max={max} step={512} format={memoryLabel} onChange={(value) => save(value)} />
          {memory.own ? (
            <Button variant="secondary" onClick={() => save(null)} className="h-10 px-4 text-[13px]">
              Let Wisp choose
            </Button>
          ) : (
            <Text className="mt-0">Moving the slider gives this instance its own.</Text>
          )}
          {memory.systemMb !== null && <p className="mt-3 text-xs text-mute-2">This computer has {memoryLabel(memory.systemMb)}. A change counts from the next start.</p>}
        </div>
      </Popover>
    </>
  );
}

/** Whether this instance's FOV, keybinds and other game settings follow the rest, and the switch. */
export function SharingButton({ instance }: { instance: Instance }) {
  const { settings, updateSettings, notify } = useApp();
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [keepsOwn, setKeepsOwn] = useState<boolean | null>(null);
  const [sharing, setSharing] = useState(false);
  const can = sharesSettings(instance.gameVersion);
  useEffect(() => {
    if (!can) return;
    let live = true;
    getInstancesKeepingSettings().then(
      (ids) => live && setKeepsOwn(ids.includes(instance.id)),
      () => live && setKeepsOwn(false),
    );
    return () => {
      live = false;
    };
  }, [can, instance.id]);
  if (!settings || (can && keepsOwn === null)) return null;

  const on = settings.shareOptions;
  const label = !can ? "Own settings" : !on ? "Settings not shared" : keepsOwn ? "Own settings" : "Shared settings";
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
      notify({ tone: "success", title: "Every instance gets these settings", body: `All ${count} of them, the next time each starts.` });
    } catch (failure) {
      notify({ tone: "info", title: "Couldn't share them", body: message(failure) });
    } finally {
      setSharing(false);
    }
  };

  return (
    <>
      <QuickButton anchor={anchor} icon={<SlidersHorizontal size={15} aria-hidden="true" />} label={label} open={open} onClick={() => setOpen((value) => !value)} />
      <Popover open={open} anchorRef={anchor} placement="bottom-start" onClose={() => setOpen(false)} label="Game settings" className="w-[340px]">
        <div className="p-4">
          <Heading>Game settings</Heading>
          {!can ? (
            <Text>Minecraft {instance.gameVersion} keeps its own. Versions before 1.13 store keybinds in a way newer ones can't read.</Text>
          ) : !on ? (
            <>
              <Text>Sharing is off for every instance, so FOV, keybinds and the rest are set in each one.</Text>
              <Button onClick={() => updateSettings({ shareOptions: true })} className="mt-3 h-10 px-4 text-[13px]">
                Share settings between instances
              </Button>
            </>
          ) : (
            <>
              <Text>FOV, keybinds, sound, video and the rest match your other instances on this computer. Change them in any one, and the others follow when they next start. Resource packs stay its own.</Text>
              <div className="mt-1">
                <Toggle label="Share with my other instances" checked={!keepsOwn} onChange={(share) => void keep(!share)} />
              </div>
              {!keepsOwn && (
                <Button variant="secondary" disabled={sharing} onClick={() => void shareAll()} className="h-10 px-4 text-[13px]">
                  {sharing ? "Sharing…" : "Give every instance these settings"}
                </Button>
              )}
            </>
          )}
        </div>
      </Popover>
    </>
  );
}
