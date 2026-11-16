import { Check, ChevronDown } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { WorldSummary } from "../../data/types";
import { timeAgo } from "../../lib/format";
import { WorldThumb } from "../worlds/WorldArt";

interface WorldPickerProps {
  worlds: WorldSummary[];
  selected: WorldSummary;
  onSelect: (worldId: string) => void;
  disabled?: boolean;
  now: number;
}

/** Chooses which world the Play button launches. Opens upward because it sits at the bottom of the hero. */
export function WorldPicker({ worlds, selected, onSelect, disabled = false, now }: WorldPickerProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        disabled={disabled}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
        className="flex h-[60px] items-center gap-3 rounded-full border border-cream/20 bg-ink-900/70 py-2 pl-2 pr-4 transition-colors hover:border-cream/35 hover:bg-ink-900/85 disabled:opacity-60"
      >
        <WorldThumb world={selected} className="size-11 rounded-full" />
        <span className="flex flex-col items-start text-left">
          <span className="text-[15px] font-bold">{selected.name}</span>
          <span className="max-w-[200px] truncate text-xs text-mute">
            {selected.loader}
            {selected.modCount > 0 && ` · ${selected.modCount} mods`}
          </span>
        </span>
        <m.span animate={{ rotate: open ? 180 : 0 }} className="text-cream-400">
          <ChevronDown size={16} aria-hidden="true" />
        </m.span>
      </button>

      <AnimatePresence>
        {open && (
          <m.div
            role="menu"
            aria-label="Choose a world"
            initial={{ opacity: 0, y: 8, scale: 0.96 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 6, scale: 0.97 }}
            transition={{ type: "spring", stiffness: 420, damping: 30 }}
            className="absolute bottom-[calc(100%+10px)] left-0 z-20 flex max-h-80 w-[320px] origin-bottom-left flex-col overflow-y-auto rounded-2xl border border-cream/10 bg-ink-800/95 p-1.5 shadow-[0_24px_60px_-12px_rgba(0,0,0,0.75)] backdrop-blur-md"
          >
            {worlds.map((world) => {
              const isSelected = world.id === selected.id;
              return (
                <button
                  key={world.id}
                  type="button"
                  role="menuitemradio"
                  aria-checked={isSelected}
                  onClick={() => {
                    onSelect(world.id);
                    setOpen(false);
                  }}
                  className="flex w-full items-center gap-3 rounded-xl p-2 text-left transition-colors hover:bg-cream/6"
                >
                  <WorldThumb world={world} className="size-10 rounded-lg" />
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate text-sm font-bold">{world.name}</span>
                    <span className="truncate text-xs text-mute">
                      {world.instanceName} · {timeAgo(world.lastPlayed, now)}
                    </span>
                  </span>
                  {isSelected && <Check size={16} strokeWidth={2.6} className="text-moss" aria-hidden="true" />}
                </button>
              );
            })}
          </m.div>
        )}
      </AnimatePresence>
    </div>
  );
}

