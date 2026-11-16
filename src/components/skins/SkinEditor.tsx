import { ArrowLeft, Eraser, FlipHorizontal2, Grid3x3, Layers, PaintBucket, Pipette, Pencil, Redo2, Rotate3d, Undo2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { MOUSE, Raycaster, Vector2, type Object3D } from "three";
import type { SkinViewer } from "skinview3d";
import type { SkinModel } from "../../data/types";
import { cn } from "../../lib/cn";
import { Button } from "../ui/Button";
import { Segmented } from "../ui/Segmented";
import { SkinStage } from "./SkinStage";
import { PALETTE, SIZE, faceOutlines, floodFill, hexToRgba, isOverlay, makeDefaultSkin, mirrorPixel, rgbaToHex } from "./skinTexture";

type Tool = "pencil" | "eraser" | "fill" | "picker" | "rotate";
type Layer = "base" | "overlay";

const ZOOM = 7;
const HISTORY = 80;

interface SkinEditorProps {
  name: string;
  model: SkinModel;
  /** The picture to start from (copied, never changed). */
  source: HTMLCanvasElement;
  isNew: boolean;
  onSave: (png: string, name: string, model: SkinModel, asNew: boolean) => Promise<void>;
  onCancel: () => void;
}

/** Paint a skin on the 3D model or on its flat texture. */
export function SkinEditor({ name: initialName, model: initialModel, source, isNew, onSave, onCancel }: SkinEditorProps) {
  // The picture being edited: one 64×64 canvas everything reads and writes.
  const doc = useMemo(() => {
    const canvas = document.createElement("canvas");
    canvas.width = SIZE;
    canvas.height = SIZE;
    canvas.getContext("2d", { willReadFrequently: true })?.drawImage(source, 0, 0);
    return canvas;
  }, [source]);
  const context = useMemo(() => doc.getContext("2d", { willReadFrequently: true }) as CanvasRenderingContext2D, [doc]);

  const [name, setName] = useState(initialName);
  const [model, setModel] = useState<SkinModel>(initialModel);
  const [tool, setTool] = useState<Tool>("pencil");
  const [color, setColor] = useState("#e5484d");
  const [recent, setRecent] = useState<string[]>([]);
  const [mirror, setMirror] = useState(false);
  const [layer, setLayer] = useState<Layer>("base");
  const [showOverlay, setShowOverlay] = useState(true);
  const [grid, setGrid] = useState(true);
  const [version, setVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const history = useRef<ImageData[]>([]);
  const future = useRef<ImageData[]>([]);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const viewer = useRef<SkinViewer | null>(null);
  const slim = model === "slim";

  const changed = useCallback(() => setVersion((count) => count + 1), []);
  const snapshot = useCallback(() => {
    history.current.push(context.getImageData(0, 0, SIZE, SIZE));
    if (history.current.length > HISTORY) history.current.shift();
    future.current = [];
    setCanUndo(true);
    setCanRedo(false);
  }, [context]);
  const undo = useCallback(() => {
    const previous = history.current.pop();
    if (!previous) return;
    future.current.push(context.getImageData(0, 0, SIZE, SIZE));
    context.putImageData(previous, 0, 0);
    setCanUndo(history.current.length > 0);
    setCanRedo(true);
    changed();
  }, [context, changed]);
  const redo = useCallback(() => {
    const next = future.current.pop();
    if (!next) return;
    history.current.push(context.getImageData(0, 0, SIZE, SIZE));
    context.putImageData(next, 0, 0);
    setCanUndo(true);
    setCanRedo(future.current.length > 0);
    changed();
  }, [context, changed]);

  const remember = (hex: string) => setRecent((current) => [hex, ...current.filter((known) => known !== hex)].slice(0, 10));

  /** Applies the current tool at one texture pixel (and its mirror). Returns false if nothing to do. */
  const apply = useCallback(
    (x: number, y: number) => {
      if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) return;
      const targets: [number, number][] = [[x, y]];
      if (mirror) {
        const twin = mirrorPixel(x, y, slim);
        if (twin && (twin[0] !== x || twin[1] !== y)) targets.push(twin);
      }
      if (tool === "picker") {
        const pixel = context.getImageData(x, y, 1, 1).data;
        if (pixel[3] > 0) {
          const hex = rgbaToHex(pixel);
          setColor(hex);
          remember(hex);
        }
        setTool("pencil");
        return;
      }
      if (tool === "fill") {
        const image = context.getImageData(0, 0, SIZE, SIZE);
        for (const [tx, ty] of targets) floodFill(image, tx, ty, hexToRgba(color), slim);
        context.putImageData(image, 0, 0);
        remember(color);
        return;
      }
      for (const [tx, ty] of targets) {
        if (tool === "eraser") context.clearRect(tx, ty, 1, 1);
        else {
          context.fillStyle = color;
          context.fillRect(tx, ty, 1, 1);
        }
      }
    },
    [context, mirror, slim, tool, color],
  );

  // Copy the picture into the 3D model's texture whenever it changes.
  useEffect(() => {
    const view = viewer.current;
    if (!view) return;
    const target = view.skinCanvas.getContext("2d");
    if (!target) return;
    if (view.skinCanvas.width !== SIZE || view.skinCanvas.height !== SIZE) {
      view.loadSkin(doc, { model: slim ? "slim" : "default" });
      return;
    }
    target.clearRect(0, 0, SIZE, SIZE);
    target.drawImage(doc, 0, 0);
    const map = view.playerObject.skin.map;
    if (map) map.needsUpdate = true;
  }, [version, doc, slim]);

  useEffect(() => {
    const view = viewer.current;
    if (!view) return;
    view.playerObject.skin.modelType = slim ? "slim" : "default";
  }, [slim]);

  useEffect(() => {
    viewer.current?.playerObject.skin.setOuterLayerVisible(showOverlay || layer === "overlay");
  }, [showOverlay, layer]);

  // Left-drag paints; right-drag (or the rotate tool) turns the model.
  useEffect(() => {
    const controls = viewer.current?.controls;
    if (!controls) return;
    controls.mouseButtons.LEFT = tool === "rotate" ? MOUSE.ROTATE : (-1 as MOUSE);
    controls.mouseButtons.RIGHT = MOUSE.ROTATE;
  }, [tool, version]);

  const onViewer = useCallback(
    (view: SkinViewer | null) => {
      viewer.current = view;
      if (!view) return;
      // Arms and legs a little apart, so their sides can be reached.
      const skin = view.playerObject.skin;
      skin.leftArm.rotation.z = 0.32;
      skin.rightArm.rotation.z = -0.32;
      skin.leftLeg.rotation.z = 0.06;
      skin.rightLeg.rotation.z = -0.06;
      view.controls.mouseButtons.LEFT = -1 as MOUSE;
      view.controls.mouseButtons.RIGHT = MOUSE.ROTATE;
      changed();
    },
    [changed],
  );

  // ── Painting on the model ──
  const raycaster = useMemo(() => new Raycaster(), []);
  const stroke = useRef<{ painting: boolean; last: string | null }>({ painting: false, last: null });
  const hitPixel = (event: ReactPointerEvent<HTMLDivElement>): [number, number] | null => {
    const view = viewer.current;
    if (!view) return null;
    const rect = view.canvas.getBoundingClientRect();
    const pointer = new Vector2(((event.clientX - rect.left) / rect.width) * 2 - 1, -((event.clientY - rect.top) / rect.height) * 2 + 1);
    raycaster.setFromCamera(pointer, view.camera);
    const skin = view.playerObject.skin;
    const parts = [skin.head, skin.body, skin.rightArm, skin.leftArm, skin.rightLeg, skin.leftLeg];
    const targets: Object3D[] = parts.map((part) => (layer === "overlay" ? part.outerLayer : part.innerLayer));
    const hit = raycaster.intersectObjects(targets, false)[0];
    if (!hit?.uv) return null;
    const x = Math.min(SIZE - 1, Math.floor(hit.uv.x * SIZE));
    const y = Math.min(SIZE - 1, Math.floor((1 - hit.uv.y) * SIZE));
    // A stray hit on the wrong layer's pixels is ignored.
    if (isOverlay(x, y, slim) !== (layer === "overlay")) return null;
    return [x, y];
  };
  const onModelDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || tool === "rotate") return;
    const pixel = hitPixel(event);
    if (!pixel) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    if (tool !== "picker") snapshot();
    stroke.current = { painting: tool === "pencil" || tool === "eraser", last: pixel.join() };
    apply(pixel[0], pixel[1]);
    if (tool === "pencil") remember(color);
    changed();
  };
  const onModelMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (!stroke.current.painting) return;
    const pixel = hitPixel(event);
    if (!pixel || pixel.join() === stroke.current.last) return;
    stroke.current.last = pixel.join();
    apply(pixel[0], pixel[1]);
    changed();
  };
  const endStroke = () => {
    stroke.current = { painting: false, last: null };
  };

  // ── Painting on the flat texture ──
  const flat = useRef<HTMLCanvasElement>(null);
  const flatStroke = useRef<{ painting: boolean; last: [number, number] | null }>({ painting: false, last: null });
  const [hover, setHover] = useState<string | null>(null);
  useEffect(() => {
    const canvas = flat.current;
    const g = canvas?.getContext("2d");
    if (!canvas || !g) return;
    const px = SIZE * ZOOM;
    g.clearRect(0, 0, px, px);
    // A checkerboard shows see-through pixels.
    for (let y = 0; y < SIZE; y += 1) {
      for (let x = 0; x < SIZE; x += 1) {
        g.fillStyle = (x + y) % 2 === 0 ? "#1c1a18" : "#242120";
        g.fillRect(x * ZOOM, y * ZOOM, ZOOM, ZOOM);
      }
    }
    // Areas that aren't part of the skin are dimmed.
    g.imageSmoothingEnabled = false;
    g.drawImage(doc, 0, 0, px, px);
    for (const face of faceOutlines(slim)) {
      g.strokeStyle = face.overlay ? "rgba(157,184,255,0.45)" : "rgba(244,238,228,0.4)";
      g.lineWidth = 1;
      g.strokeRect(face.x * ZOOM + 0.5, face.y * ZOOM + 0.5, face.w * ZOOM - 1, face.h * ZOOM - 1);
    }
    if (grid) {
      g.strokeStyle = "rgba(244,238,228,0.06)";
      g.beginPath();
      for (let i = 1; i < SIZE; i += 1) {
        g.moveTo(i * ZOOM + 0.5, 0);
        g.lineTo(i * ZOOM + 0.5, px);
        g.moveTo(0, i * ZOOM + 0.5);
        g.lineTo(px, i * ZOOM + 0.5);
      }
      g.stroke();
    }
  }, [version, doc, grid, slim]);

  const flatPixel = (event: ReactPointerEvent<HTMLCanvasElement>): [number, number] => {
    const rect = event.currentTarget.getBoundingClientRect();
    return [Math.floor(((event.clientX - rect.left) / rect.width) * SIZE), Math.floor(((event.clientY - rect.top) / rect.height) * SIZE)];
  };
  const line = (from: [number, number], to: [number, number]) => {
    // Bresenham, so fast strokes leave no gaps.
    let [x0, y0] = from;
    const [x1, y1] = to;
    const dx = Math.abs(x1 - x0);
    const dy = -Math.abs(y1 - y0);
    const sx = x0 < x1 ? 1 : -1;
    const sy = y0 < y1 ? 1 : -1;
    let error = dx + dy;
    for (;;) {
      apply(x0, y0);
      if (x0 === x1 && y0 === y1) break;
      const e2 = 2 * error;
      if (e2 >= dy) {
        error += dy;
        x0 += sx;
      }
      if (e2 <= dx) {
        error += dx;
        y0 += sy;
      }
    }
  };
  const onFlatDown = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    if (event.button !== 0 || tool === "rotate") return;
    const pixel = flatPixel(event);
    event.currentTarget.setPointerCapture(event.pointerId);
    if (tool !== "picker") snapshot();
    flatStroke.current = { painting: tool === "pencil" || tool === "eraser", last: pixel };
    apply(pixel[0], pixel[1]);
    if (tool === "pencil") remember(color);
    changed();
  };
  const onFlatMove = (event: ReactPointerEvent<HTMLCanvasElement>) => {
    const pixel = flatPixel(event);
    setHover(`${pixel[0]}, ${pixel[1]}`);
    const current = flatStroke.current;
    if (!current.painting || !current.last) return;
    if (pixel[0] === current.last[0] && pixel[1] === current.last[1]) return;
    line(current.last, pixel);
    current.last = pixel;
    changed();
  };

  // Shortcuts: B E G I R tools, M mirror, ⌘Z undo, ⇧⌘Z redo.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      const mod = event.metaKey || event.ctrlKey;
      if (mod && event.key.toLowerCase() === "z") {
        event.preventDefault();
        if (event.shiftKey) redo();
        else undo();
        return;
      }
      if (mod && event.key.toLowerCase() === "y") {
        event.preventDefault();
        redo();
        return;
      }
      if (mod || event.altKey) return;
      const keys: Record<string, Tool> = { b: "pencil", e: "eraser", g: "fill", i: "picker", r: "rotate" };
      const next = keys[event.key.toLowerCase()];
      if (next) setTool(next);
      else if (event.key.toLowerCase() === "m") setMirror((on) => !on);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [undo, redo]);

  const save = async (asNew: boolean) => {
    setSaving(true);
    try {
      await onSave(doc.toDataURL("image/png"), name.trim() || "Untitled skin", model, asNew);
    } finally {
      setSaving(false);
    }
  };

  const startOver = () => {
    snapshot();
    context.clearRect(0, 0, SIZE, SIZE);
    context.drawImage(makeDefaultSkin(slim), 0, 0);
    changed();
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <button type="button" onClick={onCancel} className="flex items-center gap-1.5 rounded-full px-2 py-1 text-sm font-semibold text-cream-300 hover:text-cream">
          <ArrowLeft size={16} aria-hidden="true" />
          Skins
        </button>
        <label className="flex h-11 min-w-[220px] flex-1 items-center rounded-xl border border-cream/12 bg-ink-800 px-3.5 focus-within:border-cream/35 sm:max-w-[320px]">
          <span className="sr-only">Skin name</span>
          <input value={name} onChange={(event) => setName(event.target.value)} maxLength={48} spellCheck={false} className="min-w-0 flex-1 bg-transparent font-display text-lg font-bold text-cream outline-none" />
        </label>
        <Segmented
          label="Arms"
          value={model}
          options={[
            { value: "classic", label: "Classic arms" },
            { value: "slim", label: "Slim arms" },
          ]}
          onChange={setModel}
        />
        <div className="ml-auto flex items-center gap-2">
          <IconButton label="Undo (⌘Z)" onClick={undo} disabled={!canUndo}>
            <Undo2 size={16} aria-hidden="true" />
          </IconButton>
          <IconButton label="Redo (⇧⌘Z)" onClick={redo} disabled={!canRedo}>
            <Redo2 size={16} aria-hidden="true" />
          </IconButton>
          <Button variant="secondary" onClick={onCancel} disabled={saving}>
            Cancel
          </Button>
          {!isNew && (
            <Button variant="secondary" onClick={() => void save(true)} disabled={saving}>
              Save as new
            </Button>
          )}
          <Button onClick={() => void save(false)} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>

      <div className="grid items-start gap-4 lg:grid-cols-[52px_minmax(0,1fr)_auto]">
        <div role="toolbar" aria-label="Tools" aria-orientation="vertical" className="flex flex-row gap-1.5 rounded-2xl border border-cream/8 bg-ink-800 p-1.5 lg:flex-col">
          <ToolButton label="Pencil (B)" active={tool === "pencil"} onClick={() => setTool("pencil")} icon={<Pencil size={17} aria-hidden="true" />} />
          <ToolButton label="Eraser (E)" active={tool === "eraser"} onClick={() => setTool("eraser")} icon={<Eraser size={17} aria-hidden="true" />} />
          <ToolButton label="Fill one face (G)" active={tool === "fill"} onClick={() => setTool("fill")} icon={<PaintBucket size={17} aria-hidden="true" />} />
          <ToolButton label="Pick a colour (I)" active={tool === "picker"} onClick={() => setTool("picker")} icon={<Pipette size={17} aria-hidden="true" />} />
          <ToolButton label="Turn the model (R, or right-drag)" active={tool === "rotate"} onClick={() => setTool("rotate")} icon={<Rotate3d size={17} aria-hidden="true" />} />
          <span className="mx-1 my-0.5 h-px bg-cream/10 lg:mx-1.5" aria-hidden="true" />
          <ToolButton label="Mirror left and right (M)" active={mirror} onClick={() => setMirror((on) => !on)} icon={<FlipHorizontal2 size={17} aria-hidden="true" />} />
          <ToolButton label="Show the overlay layer" active={showOverlay} onClick={() => setShowOverlay((on) => !on)} icon={<Layers size={17} aria-hidden="true" />} />
          <ToolButton label="Grid on the texture" active={grid} onClick={() => setGrid((on) => !on)} icon={<Grid3x3 size={17} aria-hidden="true" />} />
        </div>

        <div className="relative h-[560px] overflow-hidden rounded-[22px] border border-cream/8 bg-[radial-gradient(circle_at_50%_38%,rgba(244,238,228,0.09),transparent_62%)] bg-ink-850">
          <div
            className={cn("absolute inset-0", tool === "rotate" ? "cursor-grab" : "cursor-crosshair")}
            onPointerDown={onModelDown}
            onPointerMove={onModelMove}
            onPointerUp={endStroke}
            onPointerCancel={endStroke}
            onContextMenu={(event) => event.preventDefault()}
          >
            <SkinStage skin={doc} model={model} emote={null} onViewer={onViewer} zoom={0.78} />
          </div>
          <div className="pointer-events-none absolute left-4 top-4 flex flex-col gap-2">
            <Segmented
              label="Paint on"
              value={layer}
              options={[
                { value: "base", label: "Base" },
                { value: "overlay", label: "Overlay" },
              ]}
              onChange={setLayer}
              className="pointer-events-auto"
            />
          </div>
          <p className="pointer-events-none absolute inset-x-0 bottom-3 text-center text-xs text-mute">
            {tool === "rotate" ? "Drag to turn the model." : "Paint with a click or drag. Right-drag turns the model; scroll zooms."}
          </p>
        </div>

        <div className="flex w-full flex-col gap-4 lg:w-[400px]">
          <div className="rounded-[22px] border border-cream/8 bg-ink-800 p-3">
            <canvas
              ref={flat}
              width={SIZE * ZOOM}
              height={SIZE * ZOOM}
              aria-label="Skin texture"
              onPointerDown={onFlatDown}
              onPointerMove={onFlatMove}
              onPointerUp={() => (flatStroke.current = { painting: false, last: null })}
              onPointerLeave={() => setHover(null)}
              className="block aspect-square w-full cursor-crosshair rounded-lg [image-rendering:pixelated]"
            />
            <p className="mt-2 flex items-center justify-between text-xs text-mute">
              <span>
                <span className="mr-1 inline-block size-2 rounded-sm border border-cream/50 align-middle" /> base
                <span className="ml-3 mr-1 inline-block size-2 rounded-sm border border-sky/70 align-middle" /> overlay
              </span>
              <span className="tabular-nums">{hover ?? "64 × 64"}</span>
            </p>
          </div>

          <div className="flex flex-col gap-3 rounded-[22px] border border-cream/8 bg-ink-800 p-4">
            <div className="flex items-center gap-3">
              <label className="relative size-11 shrink-0 overflow-hidden rounded-xl ring-2 ring-cream/20" style={{ background: color }}>
                <span className="sr-only">Colour</span>
                <input type="color" value={color} onChange={(event) => setColor(event.target.value)} className="absolute inset-0 size-full cursor-pointer opacity-0" />
              </label>
              <input
                value={color}
                onChange={(event) => /^#[0-9a-f]{6}$/i.test(event.target.value) && setColor(event.target.value.toLowerCase())}
                spellCheck={false}
                aria-label="Colour as hex"
                className="h-10 w-[110px] rounded-xl border border-cream/12 bg-ink-900/60 px-3 font-mono text-sm uppercase text-cream outline-none focus:border-cream/35"
              />
              <button type="button" onClick={startOver} className="ml-auto rounded-full px-3 py-1.5 text-xs font-bold text-cream-300 hover:bg-cream/8 hover:text-cream">
                Start from Wisp's skin
              </button>
            </div>
            <Swatches title="Palette" colors={PALETTE} current={color} onPick={setColor} />
            {recent.length > 0 && <Swatches title="Recent" colors={recent} current={color} onPick={setColor} />}
          </div>
        </div>
      </div>
    </div>
  );
}

function Swatches({ title, colors, current, onPick }: { title: string; colors: string[]; current: string; onPick: (hex: string) => void }) {
  return (
    <div>
      <p className="mb-1.5 text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">{title}</p>
      <div className="flex flex-wrap gap-1.5">
        {colors.map((hex) => (
          <button
            key={hex}
            type="button"
            aria-label={hex}
            aria-pressed={hex === current}
            onClick={() => onPick(hex)}
            className={cn("size-7 rounded-lg ring-1 ring-cream/10 transition-transform hover:scale-110", hex === current && "ring-2 ring-cream")}
            style={{ background: hex }}
          />
        ))}
      </div>
    </div>
  );
}

function ToolButton({ label, active, onClick, icon }: { label: string; active: boolean; onClick: () => void; icon: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={active}
      title={label}
      onClick={onClick}
      className={cn("grid size-10 place-items-center rounded-xl transition-colors", active ? "bg-cream text-ink-900" : "text-cream-300 hover:bg-cream/8 hover:text-cream")}
    >
      {icon}
    </button>
  );
}

function IconButton({ label, onClick, disabled, children }: { label: string; onClick: () => void; disabled: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      disabled={disabled}
      className="grid size-11 place-items-center rounded-full border border-cream/15 text-cream-300 transition-colors enabled:hover:border-cream/35 enabled:hover:text-cream disabled:opacity-35"
    >
      {children}
    </button>
  );
}
