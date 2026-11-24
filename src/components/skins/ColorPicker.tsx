import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { hexToHsv, hsvToHex, hexToRgb, rgbToHex, type Hsv } from "./color";

/**
 * Wisp's own colour picker: shade on a square (how strong across, how light down), hue on a strip,
 * and the colour as hex or red, green and blue. Changes as you drag.
 */
export function ColorPicker({ color, onChange }: { color: string; onChange: (hex: string) => void }) {
  const [hsv, setHsv] = useState<Hsv>(() => hexToHsv(color));
  const last = useRef(color);
  // A colour picked elsewhere (a swatch, the eyedropper) moves the picker; its own changes don't
  // bounce back (they'd lose the hue of greys).
  useEffect(() => {
    if (color.toLowerCase() !== last.current.toLowerCase()) {
      setHsv(hexToHsv(color));
      last.current = color;
    }
  }, [color]);
  const set = (next: Hsv) => {
    setHsv(next);
    const hex = hsvToHex(next);
    last.current = hex;
    onChange(hex);
  };

  const square = useRef<HTMLDivElement>(null);
  const strip = useRef<HTMLDivElement>(null);
  const drag = (target: React.RefObject<HTMLDivElement | null>, onMove: (x: number, y: number) => void) => (event: ReactPointerEvent<HTMLDivElement>) => {
    const box = target.current?.getBoundingClientRect();
    if (!box) return;
    const at = (clientX: number, clientY: number) => onMove(Math.min(1, Math.max(0, (clientX - box.left) / box.width)), Math.min(1, Math.max(0, (clientY - box.top) / box.height)));
    at(event.clientX, event.clientY);
    const move = (next: PointerEvent) => at(next.clientX, next.clientY);
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const [r, g, b] = hexToRgb(hsvToHex(hsv));
  const [hexText, setHexText] = useState(color);
  useEffect(() => setHexText(hsvToHex(hsv)), [hsv]);
  const channel = (index: 0 | 1 | 2, text: string) => {
    const value = Math.min(255, Math.max(0, Number(text.replace(/[^0-9]/g, "")) || 0));
    const rgb: [number, number, number] = [r, g, b];
    rgb[index] = value;
    set(hexToHsv(rgbToHex(...rgb)));
  };
  const hueColor = hsvToHex({ h: hsv.h, s: 1, v: 1 });

  return (
    <div className="flex flex-col gap-3">
      <div
        ref={square}
        role="slider"
        aria-label="Shade"
        aria-valuetext={`${Math.round(hsv.s * 100)}% colour, ${Math.round(hsv.v * 100)}% light`}
        tabIndex={0}
        onPointerDown={drag(square, (x, y) => set({ ...hsv, s: x, v: 1 - y }))}
        onKeyDown={(event) => {
          const step = event.shiftKey ? 0.1 : 0.02;
          const moves: Record<string, Partial<Hsv>> = { ArrowLeft: { s: hsv.s - step }, ArrowRight: { s: hsv.s + step }, ArrowUp: { v: hsv.v + step }, ArrowDown: { v: hsv.v - step } };
          const move = moves[event.key];
          if (!move) return;
          event.preventDefault();
          set({ h: hsv.h, s: Math.min(1, Math.max(0, move.s ?? hsv.s)), v: Math.min(1, Math.max(0, move.v ?? hsv.v)) });
        }}
        className="relative h-36 w-full cursor-crosshair touch-none rounded-xl outline-none ring-cream/40 focus-visible:ring-2"
        style={{ background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, ${hueColor})` }}
      >
        <span
          aria-hidden="true"
          className="pointer-events-none absolute size-4 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.5)]"
          style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%`, background: hsvToHex(hsv) }}
        />
      </div>
      <div
        ref={strip}
        role="slider"
        aria-label="Hue"
        aria-valuemin={0}
        aria-valuemax={360}
        aria-valuenow={Math.round(hsv.h)}
        tabIndex={0}
        onPointerDown={drag(strip, (x) => set({ ...hsv, h: x * 359.9 }))}
        onKeyDown={(event) => {
          const step = event.shiftKey ? 15 : 3;
          if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
            event.preventDefault();
            set({ ...hsv, h: (hsv.h + (event.key === "ArrowLeft" ? -step : step) + 360) % 360 });
          }
        }}
        className="relative h-4 w-full cursor-pointer touch-none rounded-full outline-none ring-cream/40 focus-visible:ring-2"
        style={{ background: "linear-gradient(to right, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)" }}
      >
        <span aria-hidden="true" className="pointer-events-none absolute top-1/2 size-5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-white shadow-[0_0_0_1px_rgba(0,0,0,0.5)]" style={{ left: `${(hsv.h / 360) * 100}%`, background: hueColor }} />
      </div>
      <div className="flex items-center gap-2">
        <span className="size-9 shrink-0 rounded-lg ring-1 ring-cream/20" style={{ background: hsvToHex(hsv) }} aria-hidden="true" />
        <input
          value={hexText}
          onChange={(event) => {
            const text = event.target.value.trim();
            setHexText(text);
            const full = text.startsWith("#") ? text : `#${text}`;
            if (/^#[0-9a-f]{6}$/i.test(full)) set(hexToHsv(full.toLowerCase()));
          }}
          spellCheck={false}
          aria-label="Colour as hex"
          className="h-9 w-[92px] rounded-lg border border-cream/12 bg-ink-900/60 px-2 font-mono text-xs uppercase text-cream outline-none focus:border-cream/35"
        />
        {(["R", "G", "B"] as const).map((label, index) => (
          <label key={label} className="flex h-9 min-w-0 flex-1 items-center gap-1 rounded-lg border border-cream/12 bg-ink-900/60 px-2 focus-within:border-cream/35">
            <span className="text-[10px] font-bold text-mute-2">{label}</span>
            <input
              value={[r, g, b][index]}
              inputMode="numeric"
              onChange={(event) => channel(index as 0 | 1 | 2, event.target.value)}
              aria-label={`${label === "R" ? "Red" : label === "G" ? "Green" : "Blue"} (0 to 255)`}
              className="w-full min-w-0 bg-transparent font-mono text-xs text-cream outline-none"
            />
          </label>
        ))}
      </div>
    </div>
  );
}
