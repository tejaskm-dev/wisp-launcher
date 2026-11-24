import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Brush,
  Copy,
  Eraser,
  Eye,
  EyeOff,
  FlipHorizontal2,
  Grid3x3,
  Layers,
  Merge,
  Moon,
  PaintBucket,
  Pencil,
  Pipette,
  Plus,
  Redo2,
  Rotate3d,
  Shirt,
  Sparkles,
  Sun,
  Trash2,
  Undo2,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import { MOUSE, Raycaster, Vector2, type Object3D } from "three";
import type { SkinViewer } from "skinview3d";
import type { SkinModel } from "../../data/types";
import { cn } from "../../lib/cn";
import { Button } from "../ui/Button";
import { MenuItem, Popover } from "../ui/Popover";
import { Segmented } from "../ui/Segmented";
import { Slider } from "../ui/Slider";
import { PIECES, blankCanvas, composite, jitter, makeLayer, outlineLayer, pen, pieceLayer, saveLayers, shadingLayer, textureLayer, tint, type PaintLayer, type Piece, type SavedLayer } from "./layers";
import { NO_ADJUSTMENT, adjust, recolor, skinColors, type Adjustment } from "./color";
import { ColorPicker } from "./ColorPicker";
import { outerOnly } from "./cosmetics";
import { SkinStage } from "./SkinStage";
import { PALETTE, SIZE, faceOutlines, floodFill, hexToRgba, isOverlay, makeDefaultSkin, mirrorPixel, rgbaToHex } from "./skinTexture";

type Tool = "pencil" | "texture" | "darken" | "lighten" | "eraser" | "fill" | "picker" | "rotate";
type Surface = "base" | "overlay";

const ZOOM = 7;
const HISTORY = 60;
/** How much one pass of the darken or lighten brush changes a pixel. */
const SHADE_STEP = 0.14;

/** A layer to start from: one the skin was saved with, read back into a canvas. */
export interface StartLayer {
  name: string;
  visible: boolean;
  opacity: number;
  canvas: HTMLCanvasElement;
}

interface SkinEditorProps {
  name: string;
  model: SkinModel;
  /** The picture to start from (copied, never changed). */
  source: HTMLCanvasElement;
  /** The layers the skin was saved with, if any; otherwise the picture is the only layer. */
  layers?: StartLayer[] | null;
  isNew: boolean;
  /** `layers` is null when there's only one: then the skin is all there is to keep. */
  onSave: (png: string, name: string, model: SkinModel, asNew: boolean, layers: SavedLayer[] | null) => Promise<void>;
  /** Keeps the skin's outer layer (as it looks now) as a cosmetic to wear over any skin. */
  onKeepOuterLayer?: (png: string, name: string) => Promise<void>;
  onCancel: () => void;
}

interface Snapshot {
  layers: { id: string; name: string; visible: boolean; opacity: number; data: ImageData }[];
  active: string;
}

/** Paint a skin on the 3D model or on its flat texture, in layers. */
export function SkinEditor({ name: initialName, model: initialModel, source, layers: startLayers, isNew, onSave, onKeepOuterLayer, onCancel }: SkinEditorProps) {
  // The skin as everything shows it: the visible layers stacked.
  const doc = useMemo(() => blankCanvas(), []);
  const [layers, setLayerState] = useState<PaintLayer[]>(() =>
    startLayers && startLayers.length > 0 ? startLayers.map((layer) => makeLayer(layer.name, layer.canvas, layer)) : [makeLayer("Skin", source)],
  );
  const layersRef = useRef(layers);
  const [activeId, setActiveIdState] = useState(() => layers[layers.length - 1].id);
  const activeRef = useRef(activeId);
  const active = layers.find((layer) => layer.id === activeId) ?? layers[layers.length - 1];

  const [name, setName] = useState(initialName);
  const [model, setModel] = useState<SkinModel>(initialModel);
  const [tool, setTool] = useState<Tool>("pencil");
  const [color, setColor] = useState("#e5484d");
  const [recent, setRecent] = useState<string[]>([]);
  // Recolouring: the skin's colour being changed, and how near a shade counts as it.
  const [recolorFrom, setRecolorFrom] = useState<string | null>(null);
  const [tolerance, setTolerance] = useState(0.12);
  // Adjusting a layer's colours: its pixels before, and the sliders.
  const [adjusting, setAdjusting] = useState<{ id: string; base: ImageData } | null>(null);
  const [adjustment, setAdjustment] = useState<Adjustment>(NO_ADJUSTMENT);
  const [mirror, setMirror] = useState(false);
  const [surface, setSurface] = useState<Surface>("base");
  const [showOverlay, setShowOverlay] = useState(true);
  const [grid, setGrid] = useState(true);
  const [version, setVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const history = useRef<Snapshot[]>([]);
  const future = useRef<Snapshot[]>([]);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const viewer = useRef<SkinViewer | null>(null);
  const slim = model === "slim";

  /** Re-stacks the layers into the skin and redraws everything that shows it. */
  const changed = useCallback(() => {
    composite(layersRef.current, doc);
    setVersion((count) => count + 1);
  }, [doc]);
  const setLayers = useCallback(
    (next: PaintLayer[]) => {
      layersRef.current = next;
      setLayerState(next);
      changed();
    },
    [changed],
  );
  const setActive = (id: string) => {
    activeRef.current = id;
    setActiveIdState(id);
  };
  useEffect(() => changed(), [changed]);

  // ── History: every layer's pixels and settings ──
  const capture = useCallback(
    (): Snapshot => ({
      layers: layersRef.current.map((layer) => ({ id: layer.id, name: layer.name, visible: layer.visible, opacity: layer.opacity, data: pen(layer.canvas).getImageData(0, 0, SIZE, SIZE) })),
      active: activeRef.current,
    }),
    [],
  );
  const restore = useCallback(
    (snapshot: Snapshot) => {
      const known = new Map(layersRef.current.map((layer) => [layer.id, layer]));
      const next = snapshot.layers.map((entry) => {
        const canvas = known.get(entry.id)?.canvas ?? blankCanvas();
        pen(canvas).putImageData(entry.data, 0, 0);
        return { id: entry.id, name: entry.name, visible: entry.visible, opacity: entry.opacity, canvas };
      });
      setActive(snapshot.active);
      setLayers(next);
    },
    [setLayers],
  );
  const snapshot = useCallback(() => {
    history.current.push(capture());
    if (history.current.length > HISTORY) history.current.shift();
    future.current = [];
    setCanUndo(true);
    setCanRedo(false);
  }, [capture]);
  const undo = useCallback(() => {
    const previous = history.current.pop();
    if (!previous) return;
    future.current.push(capture());
    restore(previous);
    setCanUndo(history.current.length > 0);
    setCanRedo(true);
  }, [capture, restore]);
  const redo = useCallback(() => {
    const next = future.current.pop();
    if (!next) return;
    history.current.push(capture());
    restore(next);
    setCanUndo(true);
    setCanRedo(future.current.length > 0);
  }, [capture, restore]);

  const remember = (hex: string) => setRecent((current) => [hex, ...current.filter((known) => known !== hex)].slice(0, 10));

  // ── Colour: the skin's own, recolouring, adjusting a layer ──
  const inSkin = useMemo(
    () => skinColors(pen(doc).getImageData(0, 0, SIZE, SIZE), 28).map((entry) => entry.hex),
    // Recounted whenever the skin changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [doc, version],
  );
  /** Changes the picked skin colour (and shades near it) to the current colour, on every visible layer. */
  const applyRecolor = () => {
    if (!recolorFrom || recolorFrom === color) return;
    snapshot();
    for (const layer of layersRef.current) {
      if (!layer.visible) continue;
      const context = pen(layer.canvas);
      const image = context.getImageData(0, 0, SIZE, SIZE);
      if (recolor(image, recolorFrom, color, tolerance) > 0) context.putImageData(image, 0, 0);
    }
    changed();
    remember(color);
    setRecolorFrom(color);
  };
  const startAdjusting = () => {
    setAdjusting({ id: active.id, base: pen(active.canvas).getImageData(0, 0, SIZE, SIZE) });
    setAdjustment(NO_ADJUSTMENT);
  };
  const previewAdjustment = (next: Adjustment) => {
    setAdjustment(next);
    const layer = adjusting && layersRef.current.find((candidate) => candidate.id === adjusting.id);
    if (!adjusting || !layer) return;
    const context = pen(layer.canvas);
    const out = context.createImageData(SIZE, SIZE);
    adjust(adjusting.base, out, next);
    context.putImageData(out, 0, 0);
    changed();
  };
  const finishAdjusting = (keep: boolean) => {
    const layer = adjusting && layersRef.current.find((candidate) => candidate.id === adjusting.id);
    if (adjusting && layer) {
      const context = pen(layer.canvas);
      const adjusted = context.getImageData(0, 0, SIZE, SIZE);
      // History keeps the layer as it was before.
      context.putImageData(adjusting.base, 0, 0);
      if (keep) {
        snapshot();
        context.putImageData(adjusted, 0, 0);
      }
      changed();
    }
    setAdjusting(null);
    setAdjustment(NO_ADJUSTMENT);
  };

  /** The skin's colour at a pixel (what you see, all layers together), or null where it's clear. */
  const seen = (x: number, y: number): string | null => {
    const pixel = pen(doc).getImageData(x, y, 1, 1).data;
    return pixel[3] > 0 ? rgbaToHex(pixel) : null;
  };

  /** Applies the current tool at one texture pixel (and its mirror), on the active layer. */
  const apply = (x: number, y: number) => {
    if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) return;
    const context = pen(active.canvas);
    const targets: [number, number][] = [[x, y]];
    if (mirror) {
      const twin = mirrorPixel(x, y, slim);
      if (twin && (twin[0] !== x || twin[1] !== y)) targets.push(twin);
    }
    if (tool === "picker") {
      const hex = seen(x, y);
      if (hex) {
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
      if (tool === "eraser") {
        context.clearRect(tx, ty, 1, 1);
        continue;
      }
      let paint: string | null = color;
      if (tool === "texture") paint = jitter(color);
      else if (tool === "darken" || tool === "lighten") {
        const under = seen(tx, ty);
        paint = under && tint(under, tool === "darken" ? -SHADE_STEP : SHADE_STEP);
      }
      if (!paint) continue;
      context.fillStyle = paint;
      context.fillRect(tx, ty, 1, 1);
    }
  };
  const paints = tool !== "picker" && tool !== "fill" && tool !== "rotate";

  // Copy the skin into the 3D model's texture whenever it changes.
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
    viewer.current?.playerObject.skin.setOuterLayerVisible(showOverlay || surface === "overlay");
  }, [showOverlay, surface]);

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
    const targets: Object3D[] = parts.map((part) => (surface === "overlay" ? part.outerLayer : part.innerLayer));
    const hit = raycaster.intersectObjects(targets, false)[0];
    if (!hit?.uv) return null;
    const x = Math.min(SIZE - 1, Math.floor(hit.uv.x * SIZE));
    const y = Math.min(SIZE - 1, Math.floor((1 - hit.uv.y) * SIZE));
    // A stray hit on the other surface's pixels is ignored.
    if (isOverlay(x, y, slim) !== (surface === "overlay")) return null;
    return [x, y];
  };
  const onModelDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || tool === "rotate") return;
    const pixel = hitPixel(event);
    if (!pixel) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    if (tool !== "picker") snapshot();
    stroke.current = { painting: paints, last: pixel.join() };
    apply(pixel[0], pixel[1]);
    if (tool === "pencil" || tool === "texture") remember(color);
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
    flatStroke.current = { painting: paints, last: pixel };
    apply(pixel[0], pixel[1]);
    if (tool === "pencil" || tool === "texture") remember(color);
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

  // ── Layers ──
  const indexOf = (id: string) => layersRef.current.findIndex((layer) => layer.id === id);
  /** Puts a new layer just above the active one and makes it active. */
  const addLayer = (layer: PaintLayer) => {
    snapshot();
    const next = [...layersRef.current];
    next.splice(indexOf(activeRef.current) + 1, 0, layer);
    setActive(layer.id);
    setLayers(next);
  };
  const updateLayer = (id: string, patch: Partial<PaintLayer>, remembered = true) => {
    if (remembered) snapshot();
    setLayers(layersRef.current.map((layer) => (layer.id === id ? { ...layer, ...patch } : layer)));
  };
  const moveLayer = (id: string, by: number) => {
    const from = indexOf(id);
    const to = from + by;
    if (from < 0 || to < 0 || to >= layersRef.current.length) return;
    snapshot();
    const next = [...layersRef.current];
    [next[from], next[to]] = [next[to], next[from]];
    setLayers(next);
  };
  const removeLayer = (id: string) => {
    if (layersRef.current.length < 2) return;
    snapshot();
    const index = indexOf(id);
    const next = layersRef.current.filter((layer) => layer.id !== id);
    if (activeRef.current === id) setActive(next[Math.max(0, index - 1)].id);
    setLayers(next);
  };
  const duplicateLayer = (id: string) => {
    const layer = layersRef.current[indexOf(id)];
    if (layer) addLayer(makeLayer(`${layer.name} copy`, layer.canvas, layer));
  };
  /** Paints a layer (as it shows) onto the one below, then drops it. */
  const mergeDown = (id: string) => {
    const index = indexOf(id);
    if (index < 1) return;
    snapshot();
    const [below, above] = [layersRef.current[index - 1], layersRef.current[index]];
    if (above.visible) {
      const g = pen(below.canvas);
      g.globalAlpha = above.opacity;
      g.drawImage(above.canvas, 0, 0);
      g.globalAlpha = 1;
    }
    setActive(below.id);
    setLayers(layersRef.current.filter((layer) => layer.id !== id));
  };
  /** A layer made from the skin as it is now (shading, texture, outlines). */
  const addEffect = (label: string, make: (skin: ImageData, slim: boolean) => HTMLCanvasElement, opacity = 1) => {
    composite(layersRef.current, doc);
    const skin = pen(doc).getImageData(0, 0, SIZE, SIZE);
    const layer = makeLayer(label, make(skin, slim), { opacity });
    snapshot();
    const next = [...layersRef.current, layer];
    setActive(layer.id);
    setLayers(next);
  };
  const addPiece = (piece: Piece, label: string) => {
    addLayer(makeLayer(label, pieceLayer(piece, color, slim)));
    remember(color);
  };

  // Shortcuts: B T D L E G I R tools, M mirror, ⌘Z undo, ⇧⌘Z redo.
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
      const keys: Record<string, Tool> = { b: "pencil", t: "texture", d: "darken", l: "lighten", e: "eraser", g: "fill", i: "picker", r: "rotate" };
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
      composite(layersRef.current, doc);
      const kept = layersRef.current.length > 1 ? saveLayers(layersRef.current) : null;
      await onSave(doc.toDataURL("image/png"), name.trim() || "Untitled skin", model, asNew, kept);
    } finally {
      setSaving(false);
    }
  };

  const startOver = () => {
    snapshot();
    const fresh = makeLayer("Skin", makeDefaultSkin(slim));
    setActive(fresh.id);
    setLayers([fresh]);
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
          {onKeepOuterLayer && (
            <Button variant="secondary" icon={Shirt} disabled={saving} onClick={() => void onKeepOuterLayer(outerOnly(doc), `${name.trim() || "My"} outfit`)} title="Keep the outer layer as a cosmetic, to wear over any skin">
              Keep outer layer
            </Button>
          )}
          <Button onClick={() => void save(false)} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </Button>
        </div>
      </div>

      <div className="grid items-start gap-4 lg:grid-cols-[52px_minmax(0,1fr)_auto]">
        <div role="toolbar" aria-label="Tools" aria-orientation="vertical" className="flex flex-row flex-wrap gap-1.5 rounded-2xl border border-cream/8 bg-ink-800 p-1.5 lg:flex-col">
          <ToolButton label="Pencil (B)" active={tool === "pencil"} onClick={() => setTool("pencil")} icon={<Pencil size={17} aria-hidden="true" />} />
          <ToolButton label="Texture brush: your colour, each pixel a little lighter or darker (T)" active={tool === "texture"} onClick={() => setTool("texture")} icon={<Brush size={17} aria-hidden="true" />} />
          <ToolButton label="Darken what's there (D)" active={tool === "darken"} onClick={() => setTool("darken")} icon={<Moon size={17} aria-hidden="true" />} />
          <ToolButton label="Lighten what's there (L)" active={tool === "lighten"} onClick={() => setTool("lighten")} icon={<Sun size={17} aria-hidden="true" />} />
          <ToolButton label="Eraser (E)" active={tool === "eraser"} onClick={() => setTool("eraser")} icon={<Eraser size={17} aria-hidden="true" />} />
          <ToolButton label="Fill one face (G)" active={tool === "fill"} onClick={() => setTool("fill")} icon={<PaintBucket size={17} aria-hidden="true" />} />
          <ToolButton label="Pick a colour (I)" active={tool === "picker"} onClick={() => setTool("picker")} icon={<Pipette size={17} aria-hidden="true" />} />
          <ToolButton label="Turn the model (R, or right-drag)" active={tool === "rotate"} onClick={() => setTool("rotate")} icon={<Rotate3d size={17} aria-hidden="true" />} />
          <span className="mx-1 my-0.5 h-px bg-cream/10 lg:mx-1.5" aria-hidden="true" />
          <ToolButton label="Mirror left and right (M)" active={mirror} onClick={() => setMirror((on) => !on)} icon={<FlipHorizontal2 size={17} aria-hidden="true" />} />
          <ToolButton label="Show the outer layer" active={showOverlay} onClick={() => setShowOverlay((on) => !on)} icon={<Layers size={17} aria-hidden="true" />} />
          <ToolButton label="Grid on the texture" active={grid} onClick={() => setGrid((on) => !on)} icon={<Grid3x3 size={17} aria-hidden="true" />} />
        </div>

        <div className="relative h-[600px] overflow-hidden rounded-[22px] border border-cream/8 bg-[radial-gradient(circle_at_50%_38%,rgba(244,238,228,0.09),transparent_62%)] bg-ink-850">
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
              value={surface}
              options={[
                { value: "base", label: "Inner", hint: "The skin itself" },
                { value: "overlay", label: "Outer", hint: "The layer just above it: hats, hoods, jackets" },
              ]}
              onChange={setSurface}
              className="pointer-events-auto"
            />
          </div>
          <p className="pointer-events-none absolute inset-x-0 bottom-3 text-center text-xs text-mute">
            {tool === "rotate" ? "Drag to turn the model." : `Painting on "${active.name}". Right-drag turns the model; scroll zooms.`}
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
                <span className="mr-1 inline-block size-2 rounded-sm border border-cream/50 align-middle" /> inner
                <span className="ml-3 mr-1 inline-block size-2 rounded-sm border border-sky/70 align-middle" /> outer
              </span>
              <span className="tabular-nums">{hover ?? "64 × 64"}</span>
            </p>
          </div>

          <LayersPanel
            layers={layers}
            activeId={active.id}
            version={version}
            onSelect={setActive}
            onAdd={() => addLayer(makeLayer(`Layer ${layers.length + 1}`))}
            onEffect={addEffect}
            onPiece={addPiece}
            color={color}
            onToggle={(layer) => updateLayer(layer.id, { visible: !layer.visible })}
            onRename={(layer, next) => updateLayer(layer.id, { name: next })}
            onOpacityStart={snapshot}
            onOpacity={(layer, opacity) => updateLayer(layer.id, { opacity }, false)}
            onMove={moveLayer}
            onDuplicate={duplicateLayer}
            onMerge={mergeDown}
            onRemove={removeLayer}
          />

          <div className="flex flex-col gap-4 rounded-[22px] border border-cream/8 bg-ink-800 p-4">
            <div className="flex items-center gap-2">
              <h3 className="text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">Colour</h3>
              <button type="button" onClick={startOver} className="ml-auto rounded-full px-3 py-1.5 text-xs font-bold text-cream-300 hover:bg-cream/8 hover:text-cream">
                Start from Wisp's skin
              </button>
            </div>
            <ColorPicker color={color} onChange={setColor} />
            <div>
              <div className="mb-1.5 flex items-center gap-2">
                <p className="text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">In this skin</p>
                <span className="text-[11px] text-mute-2">{recolorFrom ? "Pick a new colour above, then change it" : "Click one to change it everywhere"}</span>
              </div>
              <div className="flex flex-wrap gap-1.5">
                {inSkin.map((hex) => (
                  <button
                    key={hex}
                    type="button"
                    aria-label={`Change ${hex} everywhere`}
                    aria-pressed={hex === recolorFrom}
                    onClick={() => setRecolorFrom((current) => (current === hex ? null : hex))}
                    className={cn("size-7 rounded-lg ring-1 ring-cream/10 transition-transform hover:scale-110", hex === recolorFrom && "scale-110 ring-2 ring-ember")}
                    style={{ background: hex }}
                  />
                ))}
              </div>
              {recolorFrom && (
                <div className="mt-3 flex flex-col gap-2.5 rounded-xl bg-ink-900/60 p-3">
                  <p className="flex items-center gap-2 text-xs text-cream-300">
                    <span className="size-4 rounded ring-1 ring-cream/20" style={{ background: recolorFrom }} aria-hidden="true" />
                    to
                    <span className="size-4 rounded ring-1 ring-cream/20" style={{ background: color }} aria-hidden="true" />
                    <span className="text-mute">keeping its shading</span>
                  </p>
                  <Slider label="Similar shades too" value={Math.round(tolerance * 100)} min={0} max={40} step={1} onChange={(value) => setTolerance(value / 100)} format={(value) => (value === 0 ? "Only this one" : `${value}%`)} />
                  <div className="flex gap-2">
                    <Button className="h-9 px-4 text-xs" disabled={recolorFrom === color} onClick={applyRecolor}>
                      Change it
                    </Button>
                    <Button variant="secondary" className="h-9 px-4 text-xs" onClick={() => setRecolorFrom(null)}>
                      Done
                    </Button>
                  </div>
                </div>
              )}
            </div>
            <Swatches title="Palette" colors={PALETTE} current={color} onPick={setColor} />
            {recent.length > 0 && <Swatches title="Recent" colors={recent} current={color} onPick={setColor} />}
            <div className="border-t border-cream/7 pt-3">
              {adjusting ? (
                <div className="flex flex-col gap-2.5">
                  <p className="text-xs font-bold text-cream-300">Adjusting “{layersRef.current.find((layer) => layer.id === adjusting.id)?.name ?? "layer"}”</p>
                  <Slider label="Hue" value={adjustment.hue} min={-180} max={180} step={2} onChange={(hue) => previewAdjustment({ ...adjustment, hue })} format={(value) => `${value > 0 ? "+" : ""}${value}°`} />
                  <Slider label="Colour strength" value={Math.round(adjustment.saturation * 100)} min={-100} max={100} step={2} onChange={(value) => previewAdjustment({ ...adjustment, saturation: value / 100 })} format={(value) => `${value > 0 ? "+" : ""}${value}%`} />
                  <Slider label="Brightness" value={Math.round(adjustment.brightness * 100)} min={-100} max={100} step={2} onChange={(value) => previewAdjustment({ ...adjustment, brightness: value / 100 })} format={(value) => `${value > 0 ? "+" : ""}${value}%`} />
                  <div className="flex gap-2">
                    <Button className="h-9 px-4 text-xs" onClick={() => finishAdjusting(true)}>
                      Keep
                    </Button>
                    <Button variant="secondary" className="h-9 px-4 text-xs" onClick={() => finishAdjusting(false)}>
                      Undo it
                    </Button>
                  </div>
                </div>
              ) : (
                <button type="button" onClick={startAdjusting} className="flex items-center gap-2 text-xs font-bold text-cream-300 hover:text-cream">
                  <Sun size={14} aria-hidden="true" />
                  Adjust hue, strength and brightness of “{active.name}”
                </button>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

interface LayersPanelProps {
  layers: PaintLayer[];
  activeId: string;
  version: number;
  color: string;
  onSelect: (id: string) => void;
  onAdd: () => void;
  onEffect: (label: string, make: (skin: ImageData, slim: boolean) => HTMLCanvasElement, opacity?: number) => void;
  onPiece: (piece: Piece, label: string) => void;
  onToggle: (layer: PaintLayer) => void;
  onRename: (layer: PaintLayer, name: string) => void;
  onOpacityStart: () => void;
  onOpacity: (layer: PaintLayer, opacity: number) => void;
  onMove: (id: string, by: number) => void;
  onDuplicate: (id: string) => void;
  onMerge: (id: string) => void;
  onRemove: (id: string) => void;
}

/** The layer stack, top first, with what you can do to the selected one. */
function LayersPanel(props: LayersPanelProps) {
  const { layers, activeId, version, color, onSelect, onAdd, onEffect, onPiece } = props;
  const [menu, setMenu] = useState(false);
  const [renaming, setRenaming] = useState<string | null>(null);
  const addRef = useRef<HTMLButtonElement>(null);
  const index = layers.findIndex((layer) => layer.id === activeId);
  const active = layers[index];
  const top = [...layers].reverse();

  return (
    <section aria-label="Layers" className="flex flex-col gap-3 rounded-[22px] border border-cream/8 bg-ink-800 p-4">
      <div className="flex items-center gap-2">
        <h3 className="flex items-center gap-2 text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">
          <Layers size={13} aria-hidden="true" />
          Layers
        </h3>
        <span className="text-xs tabular-nums text-mute-2">{layers.length}</span>
        <div className="ml-auto flex items-center gap-1">
          <button
            ref={addRef}
            type="button"
            aria-expanded={menu}
            onClick={() => setMenu((open) => !open)}
            className="flex h-8 items-center gap-1.5 rounded-full border border-cream/15 px-3 text-xs font-bold text-cream-300 transition-colors hover:border-cream/35 hover:text-cream"
          >
            <Sparkles size={12} aria-hidden="true" />
            Add details
          </button>
          <button type="button" aria-label="New empty layer" title="New empty layer" onClick={onAdd} className="grid size-8 place-items-center rounded-full text-cream-300 transition-colors hover:bg-cream/8 hover:text-cream">
            <Plus size={16} aria-hidden="true" />
          </button>
        </div>
        <Popover open={menu} anchorRef={addRef} placement="bottom-end" onClose={() => setMenu(false)} label="Add details" className="w-[250px]">
          <div role="menu" className="flex flex-col">
            <p className="px-3 pb-1 pt-1.5 text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">Depth</p>
            <MenuItem icon={<Moon size={15} aria-hidden="true" />} label="Shading" hint="Light from above, shade below" onSelect={() => (setMenu(false), onEffect("Shading", shadingLayer))} />
            <MenuItem icon={<Brush size={15} aria-hidden="true" />} label="Texture" hint="Grain, so colours aren't flat" onSelect={() => (setMenu(false), onEffect("Texture", textureLayer))} />
            <MenuItem icon={<Grid3x3 size={15} aria-hidden="true" />} label="Outlines" hint="A darker edge on every face" onSelect={() => (setMenu(false), onEffect("Outlines", outlineLayer, 0.8))} />
            <div className="mx-2 my-1 h-px bg-cream/8" />
            <p className="flex items-center gap-2 px-3 pb-1 pt-1 text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">
              Clothes
              <span className="inline-block size-3 rounded-sm ring-1 ring-cream/20" style={{ background: color }} aria-hidden="true" />
            </p>
            {PIECES.map((piece) => (
              <MenuItem key={piece.id} icon={<Shirt size={15} aria-hidden="true" />} label={piece.label} onSelect={() => (setMenu(false), onPiece(piece.id, piece.label))} />
            ))}
          </div>
        </Popover>
      </div>

      <ul className="flex flex-col gap-1">
        {top.map((layer) => {
          const selected = layer.id === activeId;
          return (
            <li key={layer.id}>
              <div
                className={cn(
                  "group flex items-center gap-2 rounded-xl border px-1.5 py-1.5 transition-colors",
                  selected ? "border-cream/35 bg-cream/8" : "border-transparent hover:bg-cream/5",
                )}
              >
                <button
                  type="button"
                  aria-label={layer.visible ? `Hide ${layer.name}` : `Show ${layer.name}`}
                  aria-pressed={layer.visible}
                  onClick={() => props.onToggle(layer)}
                  className="grid size-7 shrink-0 place-items-center rounded-lg text-cream-300 hover:bg-cream/8 hover:text-cream"
                >
                  {layer.visible ? <Eye size={14} aria-hidden="true" /> : <EyeOff size={14} className="text-mute" aria-hidden="true" />}
                </button>
                <LayerThumb canvas={layer.canvas} version={version} dim={!layer.visible} />
                {renaming === layer.id ? (
                  <input
                    autoFocus
                    defaultValue={layer.name}
                    maxLength={40}
                    onBlur={(event) => {
                      setRenaming(null);
                      const next = event.target.value.trim();
                      if (next && next !== layer.name) props.onRename(layer, next);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") event.currentTarget.blur();
                      if (event.key === "Escape") setRenaming(null);
                    }}
                    className="min-w-0 flex-1 rounded-md bg-ink-900 px-1.5 py-0.5 text-sm font-bold text-cream outline-none ring-1 ring-cream/30"
                  />
                ) : (
                  <button
                    type="button"
                    onClick={() => onSelect(layer.id)}
                    onDoubleClick={() => setRenaming(layer.id)}
                    title="Click to paint on it, double-click to rename"
                    className={cn("min-w-0 flex-1 truncate text-left text-sm font-bold", !layer.visible && "text-mute")}
                  >
                    {layer.name}
                  </button>
                )}
                {layer.opacity < 1 && <span className="text-[11px] tabular-nums text-mute-2">{Math.round(layer.opacity * 100)}%</span>}
              </div>
            </li>
          );
        })}
      </ul>

      {active && (
        <div className="flex flex-col gap-2.5 border-t border-cream/8 pt-3">
          <label className="flex items-center gap-3 text-xs font-bold text-cream-300">
            Opacity
            <input
              type="range"
              min={0}
              max={100}
              value={Math.round(active.opacity * 100)}
              onPointerDown={props.onOpacityStart}
              onKeyDown={props.onOpacityStart}
              onChange={(event) => props.onOpacity(active, Number(event.target.value) / 100)}
              className="flex-1 accent-[#f4eee4]"
            />
            <span className="w-9 text-right tabular-nums text-mute">{Math.round(active.opacity * 100)}%</span>
          </label>
          <div className="flex flex-wrap gap-1">
            <SmallButton label="Move up" disabled={index >= layers.length - 1} onClick={() => props.onMove(active.id, 1)}>
              <ArrowUp size={13} aria-hidden="true" />
            </SmallButton>
            <SmallButton label="Move down" disabled={index <= 0} onClick={() => props.onMove(active.id, -1)}>
              <ArrowDown size={13} aria-hidden="true" />
            </SmallButton>
            <SmallButton label="Duplicate" onClick={() => props.onDuplicate(active.id)}>
              <Copy size={13} aria-hidden="true" />
            </SmallButton>
            <SmallButton label="Merge down" disabled={index <= 0} onClick={() => props.onMerge(active.id)}>
              <Merge size={13} aria-hidden="true" />
            </SmallButton>
            <SmallButton label="Rename" onClick={() => setRenaming(active.id)}>
              <Pencil size={13} aria-hidden="true" />
            </SmallButton>
            <SmallButton label="Delete" disabled={layers.length < 2} onClick={() => props.onRemove(active.id)}>
              <Trash2 size={13} aria-hidden="true" />
            </SmallButton>
          </div>
        </div>
      )}
    </section>
  );
}

/** A layer's picture, small, redrawn when the skin changes. */
function LayerThumb({ canvas, version, dim }: { canvas: HTMLCanvasElement; version: number; dim: boolean }) {
  const thumb = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const g = thumb.current?.getContext("2d");
    if (!g) return;
    g.clearRect(0, 0, SIZE, SIZE);
    g.drawImage(canvas, 0, 0);
  }, [canvas, version]);
  return (
    <canvas
      ref={thumb}
      width={SIZE}
      height={SIZE}
      aria-hidden="true"
      className={cn("size-9 shrink-0 rounded-md bg-[repeating-conic-gradient(#242120_0%_25%,#1c1a18_0%_50%)] bg-[length:8px_8px] [image-rendering:pixelated]", dim && "opacity-40")}
    />
  );
}

function SmallButton({ label, onClick, disabled = false, children }: { label: string; onClick: () => void; disabled?: boolean; children: ReactNode }) {
  return (
    <button
      type="button"
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="flex h-7 items-center gap-1 rounded-full border border-cream/12 px-2.5 text-[11px] font-bold text-cream-300 transition-colors enabled:hover:border-cream/35 enabled:hover:text-cream disabled:opacity-35"
    >
      {children}
      {label}
    </button>
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
