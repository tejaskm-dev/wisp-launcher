// The editor's layers: 64×64 canvases stacked bottom first into the skin, the way paint programs
// do it. Also the layers Wisp can make for you so a skin isn't flat: shading, texture, outlines,
// and pieces of clothing in your colour.

import { SIZE, boxes, faces, hexToRgba, type Box, type FaceName, type PartName, type Rect } from "./skinTexture";

export interface PaintLayer {
  id: string;
  name: string;
  canvas: HTMLCanvasElement;
  visible: boolean;
  /** 0 to 1. */
  opacity: number;
}

/** A layer as it's saved beside a skin: its picture as a PNG data URL. */
export interface SavedLayer {
  name: string;
  visible: boolean;
  opacity: number;
  png: string;
}

let made = 0;
const newId = () => `layer-${Date.now().toString(36)}-${(made++).toString(36)}`;

export function blankCanvas(): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = SIZE;
  canvas.height = SIZE;
  return canvas;
}

export function pen(canvas: HTMLCanvasElement): CanvasRenderingContext2D {
  return canvas.getContext("2d", { willReadFrequently: true }) as CanvasRenderingContext2D;
}

export function makeLayer(name: string, source?: CanvasImageSource | null, settings: Partial<Pick<PaintLayer, "visible" | "opacity">> = {}): PaintLayer {
  const canvas = blankCanvas();
  if (source) pen(canvas).drawImage(source, 0, 0);
  return { id: newId(), name, canvas, visible: settings.visible ?? true, opacity: settings.opacity ?? 1 };
}

/** Stacks the visible layers (bottom first) into `target`: the skin as it'll be saved. */
export function composite(layers: PaintLayer[], target: HTMLCanvasElement) {
  const g = pen(target);
  g.clearRect(0, 0, SIZE, SIZE);
  for (const layer of layers) {
    if (!layer.visible || layer.opacity <= 0) continue;
    g.globalAlpha = layer.opacity;
    g.drawImage(layer.canvas, 0, 0);
  }
  g.globalAlpha = 1;
}

export function saveLayers(layers: PaintLayer[]): SavedLayer[] {
  return layers.map((layer) => ({ name: layer.name, visible: layer.visible, opacity: layer.opacity, png: layer.canvas.toDataURL("image/png") }));
}

// ── Colour ──

/** A colour moved towards white (amount > 0) or black (amount < 0). */
export function tint(hex: string, amount: number): string {
  const [r, g, b] = hexToRgba(hex);
  const toward = amount >= 0 ? 255 : 0;
  const k = Math.min(1, Math.abs(amount));
  const mix = (channel: number) => Math.round(channel + (toward - channel) * k);
  return `#${[mix(r), mix(g), mix(b)].map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

/** The colour with a little random light or shade: what the texture brush paints. */
export function jitter(hex: string, strength = 0.11): string {
  return tint(hex, (Math.random() * 2 - 1) * strength);
}

// ── Layers that add depth ──

type Visit = (x: number, y: number, face: FaceName, at: { x: number; y: number }, rect: Rect, box: Box) => void;

function eachFacePixel(slim: boolean, visit: Visit) {
  for (const box of boxes(slim)) {
    const all = faces(box);
    for (const name of Object.keys(all) as FaceName[]) {
      const rect = all[name];
      for (let y = 0; y < rect.h; y += 1) for (let x = 0; x < rect.w; x += 1) visit(rect.x + x, rect.y + y, name, { x, y }, rect, box);
    }
  }
}

/** Light or shade (-1 to 1) as a pixel of a layer: white or black at that strength. */
function light(image: ImageData, x: number, y: number, amount: number) {
  const i = (y * SIZE + x) * 4;
  const value = amount >= 0 ? 255 : 0;
  image.data.set([value, value, value, Math.round(Math.min(1, Math.abs(amount)) * 255)], i);
}

const painted = (skin: ImageData, x: number, y: number) => skin.data[(y * SIZE + x) * 4 + 3] > 0;

function layerFrom(fill: (image: ImageData) => void): HTMLCanvasElement {
  const canvas = blankCanvas();
  const g = pen(canvas);
  const image = g.createImageData(SIZE, SIZE);
  fill(image);
  g.putImageData(image, 0, 0);
  return canvas;
}

/** Soft light from above: tops lighter, lower rows and undersides darker, face edges a touch darker. */
export function shadingLayer(skin: ImageData, slim: boolean): HTMLCanvasElement {
  return layerFrom((image) =>
    eachFacePixel(slim, (x, y, face, at, rect) => {
      if (!painted(skin, x, y)) return;
      let amount: number;
      if (face === "top") amount = 0.1;
      else if (face === "bottom") amount = -0.24;
      else {
        const down = rect.h > 1 ? at.y / (rect.h - 1) : 0;
        amount = 0.06 - 0.22 * down;
        if (face === "back") amount -= 0.04;
        if (face === "left" || face === "right") amount -= 0.03;
      }
      if (at.x === 0 || at.x === rect.w - 1) amount -= 0.05;
      light(image, x, y, amount);
    }),
  );
}

/** Fine grain: every pixel a little lighter or darker, so flat colours look like cloth and skin. */
export function textureLayer(skin: ImageData, slim: boolean): HTMLCanvasElement {
  return layerFrom((image) =>
    eachFacePixel(slim, (x, y) => {
      if (painted(skin, x, y)) light(image, x, y, (Math.random() * 2 - 1) * 0.09);
    }),
  );
}

/** A darker ring round every face, for a drawn, outlined look. */
export function outlineLayer(skin: ImageData, slim: boolean): HTMLCanvasElement {
  return layerFrom((image) =>
    eachFacePixel(slim, (x, y, face, at, rect) => {
      const edge = at.x === 0 || at.y === 0 || at.x === rect.w - 1 || at.y === rect.h - 1;
      if (edge && face !== "top" && face !== "bottom" && painted(skin, x, y)) light(image, x, y, -0.3);
    }),
  );
}

// ── Clothes ──

export type Piece = "hoodie" | "jacket" | "scarf" | "belt" | "boots" | "gloves" | "cap" | "headphones";

export const PIECES: { id: Piece; label: string }[] = [
  { id: "hoodie", label: "Hoodie" },
  { id: "jacket", label: "Open jacket" },
  { id: "scarf", label: "Scarf" },
  { id: "belt", label: "Belt" },
  { id: "boots", label: "Boots" },
  { id: "gloves", label: "Gloves" },
  { id: "cap", label: "Cap" },
  { id: "headphones", label: "Headphones" },
];

const SIDES: FaceName[] = ["front", "back", "left", "right"];

/** A piece of clothing in `color`, on a layer of its own (trims a shade darker). */
export function pieceLayer(piece: Piece, color: string, slim: boolean): HTMLCanvasElement {
  const canvas = blankCanvas();
  const g = pen(canvas);
  const box = (part: PartName, overlay = false) => boxes(slim).find((candidate) => candidate.part === part && candidate.overlay === overlay) as Box;
  const fill = (rect: Rect, hex: string) => {
    if (rect.w <= 0 || rect.h <= 0) return;
    g.fillStyle = hex;
    g.fillRect(rect.x, rect.y, rect.w, rect.h);
  };
  /** Rows `from` to `to` (exclusive) of a box's four sides. */
  const rows = (target: Box, from: number, to: number, hex: string) => {
    for (const side of SIDES) {
      const rect = faces(target)[side];
      fill({ x: rect.x, y: rect.y + from, w: rect.w, h: Math.min(to, rect.h) - from }, hex);
    }
  };
  const face = (target: Box, name: FaceName, hex: string) => fill(faces(target)[name], hex);
  const trim = tint(color, -0.28);
  const limbs: PartName[] = ["rightArm", "leftArm"];
  const legs: PartName[] = ["rightLeg", "leftLeg"];

  switch (piece) {
    case "hoodie": {
      const body = box("body");
      rows(body, 0, 12, color);
      face(body, "top", color);
      face(body, "bottom", trim);
      rows(body, 10, 12, trim);
      // A front pocket and the drawstrings.
      const front = faces(body).front;
      fill({ x: front.x + 2, y: front.y + 7, w: 4, h: 2 }, trim);
      fill({ x: front.x + 3, y: front.y + 1, w: 1, h: 2 }, "#ece6da");
      fill({ x: front.x + 4, y: front.y + 1, w: 1, h: 2 }, "#ece6da");
      for (const part of limbs) {
        rows(box(part), 0, 12, color);
        rows(box(part), 10, 12, trim);
        face(box(part), "top", color);
      }
      // The hood, on the head's outer layer, open at the face.
      const hood = box("head", true);
      for (const name of ["top", "back", "left", "right"] as FaceName[]) face(hood, name, color);
      const hoodFront = faces(hood).front;
      fill({ x: hoodFront.x, y: hoodFront.y, w: hoodFront.w, h: 1 }, color);
      fill({ x: hoodFront.x, y: hoodFront.y + 1, w: 1, h: 4 }, color);
      fill({ x: hoodFront.x + hoodFront.w - 1, y: hoodFront.y + 1, w: 1, h: 4 }, color);
      break;
    }
    case "jacket": {
      const body = box("body", true);
      rows(body, 0, 12, color);
      // Open down the middle of the front.
      const front = faces(body).front;
      g.clearRect(front.x + 3, front.y + 1, 2, front.h - 1);
      rows(body, 0, 1, trim);
      for (const part of limbs) {
        rows(box(part, true), 0, 11, color);
        rows(box(part, true), 10, 11, trim);
      }
      break;
    }
    case "scarf": {
      const body = box("body", true);
      rows(body, 0, 2, color);
      const front = faces(body).front;
      fill({ x: front.x + 1, y: front.y + 2, w: 2, h: 5 }, color);
      fill({ x: front.x + 1, y: front.y + 6, w: 2, h: 1 }, trim);
      fill({ x: front.x, y: front.y + 1, w: front.w, h: 1 }, trim);
      break;
    }
    case "belt": {
      const body = box("body");
      rows(body, 9, 10, color);
      const front = faces(body).front;
      fill({ x: front.x + 3, y: front.y + 9, w: 2, h: 1 }, "#d8b44c");
      break;
    }
    case "boots":
      for (const part of legs) {
        rows(box(part), 8, 12, color);
        rows(box(part), 8, 9, tint(color, 0.18));
        face(box(part), "bottom", trim);
      }
      break;
    case "gloves":
      for (const part of limbs) {
        rows(box(part), 9, 12, color);
        rows(box(part), 9, 10, trim);
        face(box(part), "bottom", color);
      }
      break;
    case "cap": {
      const head = box("head");
      face(head, "top", color);
      rows(head, 0, 2, color);
      const brim = faces(box("head", true)).front;
      fill({ x: brim.x, y: brim.y + 2, w: brim.w, h: 1 }, trim);
      break;
    }
    case "headphones": {
      const outer = box("head", true);
      const band = tint(color, -0.45);
      const top = faces(outer).top;
      fill({ x: top.x, y: top.y + 3, w: top.w, h: 2 }, band);
      for (const side of ["left", "right"] as FaceName[]) {
        const rect = faces(outer)[side];
        fill({ x: rect.x + 3, y: rect.y, w: 2, h: 3 }, band);
        fill({ x: rect.x + 2, y: rect.y + 3, w: 4, h: 4 }, color);
        fill({ x: rect.x + 3, y: rect.y + 4, w: 2, h: 2 }, trim);
      }
      break;
    }
  }
  return canvas;
}
