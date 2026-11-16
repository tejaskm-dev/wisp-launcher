// The 64×64 skin texture: where each body part's faces are, mirroring between the left and right
// sides, flood fill that stays on one face, a flat "paper doll" preview, and Wisp's own default
// skin, drawn here from code.

export const SIZE = 64;

export type FaceName = "top" | "bottom" | "right" | "front" | "left" | "back";
export type PartName = "head" | "body" | "rightArm" | "leftArm" | "rightLeg" | "leftLeg";

export interface Box {
  part: PartName;
  overlay: boolean;
  u: number;
  v: number;
  w: number;
  h: number;
  d: number;
}

/** Every box on the texture, base layer then overlay. Arms are 3 wide on slim skins. */
export function boxes(slim: boolean): Box[] {
  const arm = slim ? 3 : 4;
  return [
    { part: "head", overlay: false, u: 0, v: 0, w: 8, h: 8, d: 8 },
    { part: "body", overlay: false, u: 16, v: 16, w: 8, h: 12, d: 4 },
    { part: "rightArm", overlay: false, u: 40, v: 16, w: arm, h: 12, d: 4 },
    { part: "leftArm", overlay: false, u: 32, v: 48, w: arm, h: 12, d: 4 },
    { part: "rightLeg", overlay: false, u: 0, v: 16, w: 4, h: 12, d: 4 },
    { part: "leftLeg", overlay: false, u: 16, v: 48, w: 4, h: 12, d: 4 },
    { part: "head", overlay: true, u: 32, v: 0, w: 8, h: 8, d: 8 },
    { part: "body", overlay: true, u: 16, v: 32, w: 8, h: 12, d: 4 },
    { part: "rightArm", overlay: true, u: 40, v: 32, w: arm, h: 12, d: 4 },
    { part: "leftArm", overlay: true, u: 48, v: 48, w: arm, h: 12, d: 4 },
    { part: "rightLeg", overlay: true, u: 0, v: 32, w: 4, h: 12, d: 4 },
    { part: "leftLeg", overlay: true, u: 0, v: 48, w: 4, h: 12, d: 4 },
  ];
}

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function faces(box: Box): Record<FaceName, Rect> {
  const { u, v, w, h, d } = box;
  return {
    top: { x: u + d, y: v, w, h: d },
    bottom: { x: u + d + w, y: v, w, h: d },
    right: { x: u, y: v + d, w: d, h },
    front: { x: u + d, y: v + d, w, h },
    left: { x: u + d + w, y: v + d, w: d, h },
    back: { x: u + 2 * d + w, y: v + d, w, h },
  };
}

const inside = (rect: Rect, x: number, y: number) => x >= rect.x && x < rect.x + rect.w && y >= rect.y && y < rect.y + rect.h;

/** The box and face a texture pixel belongs to, if any. */
export function locate(x: number, y: number, slim: boolean): { box: Box; face: FaceName; rect: Rect } | null {
  for (const box of boxes(slim)) {
    const all = faces(box);
    for (const face of Object.keys(all) as FaceName[]) {
      if (inside(all[face], x, y)) return { box, face, rect: all[face] };
    }
  }
  return null;
}

const MIRROR_PART: Record<PartName, PartName> = { head: "head", body: "body", rightArm: "leftArm", leftArm: "rightArm", rightLeg: "leftLeg", leftLeg: "rightLeg" };
const MIRROR_FACE: Record<FaceName, FaceName> = { top: "top", bottom: "bottom", front: "front", back: "back", right: "left", left: "right" };

/** The pixel that mirrors this one across the body (right arm ↔ left arm, a face's left ↔ right). */
export function mirrorPixel(x: number, y: number, slim: boolean): [number, number] | null {
  const found = locate(x, y, slim);
  if (!found) return null;
  const target = boxes(slim).find((box) => box.part === MIRROR_PART[found.box.part] && box.overlay === found.box.overlay);
  if (!target) return null;
  const rect = faces(target)[MIRROR_FACE[found.face]];
  const localX = x - found.rect.x;
  const localY = y - found.rect.y;
  return [rect.x + (rect.w - 1 - localX), rect.y + localY];
}

/** Whether a pixel is part of the overlay (outer) layer. */
export function isOverlay(x: number, y: number, slim: boolean): boolean {
  return locate(x, y, slim)?.box.overlay ?? false;
}

export type RGBA = [number, number, number, number];

export function hexToRgba(hex: string): RGBA {
  const value = Number.parseInt(hex.replace("#", "").slice(0, 6), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255, 255];
}

export function rgbaToHex([r, g, b]: RGBA | Uint8ClampedArray): string {
  return `#${[r, g, b].map((channel) => channel.toString(16).padStart(2, "0")).join("")}`;
}

/** Fills the run of same-coloured pixels around (x, y), without leaving that face. */
export function floodFill(data: ImageData, x: number, y: number, color: RGBA, slim: boolean): number[] {
  const found = locate(x, y, slim);
  const bound = found?.rect ?? { x: 0, y: 0, w: SIZE, h: SIZE };
  const index = (px: number, py: number) => (py * SIZE + px) * 4;
  const start = index(x, y);
  const from = [data.data[start], data.data[start + 1], data.data[start + 2], data.data[start + 3]];
  const same = (i: number) =>
    from[3] === 0 && data.data[i + 3] === 0 ? true : data.data[i] === from[0] && data.data[i + 1] === from[1] && data.data[i + 2] === from[2] && data.data[i + 3] === from[3];
  if (from.every((channel, i) => channel === color[i])) return [];
  const changed: number[] = [];
  const stack: [number, number][] = [[x, y]];
  const seen = new Set<number>();
  while (stack.length > 0) {
    const [px, py] = stack.pop() as [number, number];
    if (!inside(bound, px, py)) continue;
    const i = index(px, py);
    if (seen.has(i) || !same(i)) continue;
    seen.add(i);
    changed.push(px, py);
    stack.push([px + 1, py], [px - 1, py], [px, py + 1], [px, py - 1]);
  }
  for (let k = 0; k < changed.length; k += 2) {
    const i = index(changed[k], changed[k + 1]);
    data.data.set(color, i);
  }
  return changed;
}

/** Every face's outline, for drawing guides on the flat texture. */
export function faceOutlines(slim: boolean): (Rect & { overlay: boolean; label: string })[] {
  return boxes(slim).flatMap((box) => Object.entries(faces(box)).map(([face, rect]) => ({ ...rect, overlay: box.overlay, label: `${box.part} ${face}` })));
}

/**
 * The skin seen from the front, laid flat: head, body, arms and legs with their overlays, 16×32
 * pixels. Drawn with crisp pixels into `canvas`.
 */
export function drawPaperDoll(source: CanvasImageSource, canvas: HTMLCanvasElement, slim: boolean) {
  const context = canvas.getContext("2d");
  if (!context) return;
  const arm = slim ? 3 : 4;
  canvas.width = 16;
  canvas.height = 32;
  context.imageSmoothingEnabled = false;
  context.clearRect(0, 0, 16, 32);
  const blit = (sx: number, sy: number, w: number, h: number, dx: number, dy: number) => context.drawImage(source, sx, sy, w, h, dx, dy, w, h);
  const offset = 4 - arm;
  // Base layer.
  blit(8, 8, 8, 8, 4, 0); // head
  blit(20, 20, 8, 12, 4, 8); // body
  blit(44, 20, arm, 12, offset, 8); // right arm (viewer's left)
  blit(36, 52, arm, 12, 12, 8); // left arm
  blit(4, 20, 4, 12, 4, 20); // right leg
  blit(20, 52, 4, 12, 8, 20); // left leg
  // Overlay.
  blit(40, 8, 8, 8, 4, 0);
  blit(20, 36, 8, 12, 4, 8);
  blit(44, 36, arm, 12, offset, 8);
  blit(52, 52, arm, 12, 12, 8);
  blit(4, 36, 4, 12, 4, 20);
  blit(4, 52, 4, 12, 8, 20);
}

/** Just the face (with its hat layer), 8×8, for avatars. */
export function drawFace(source: CanvasImageSource, canvas: HTMLCanvasElement) {
  const context = canvas.getContext("2d");
  if (!context) return;
  canvas.width = 8;
  canvas.height = 8;
  context.imageSmoothingEnabled = false;
  context.clearRect(0, 0, 8, 8);
  context.drawImage(source, 8, 8, 8, 8, 0, 0, 8, 8);
  context.drawImage(source, 40, 8, 8, 8, 0, 0, 8, 8);
}

// ── Wisp's default skin: someone in a dark hoodie with a red sparkle, cream face, ghost-white hair ──

function paintBox(context: CanvasRenderingContext2D, box: Box, colors: Partial<Record<FaceName, string>> & { all: string }) {
  const all = faces(box);
  for (const face of Object.keys(all) as FaceName[]) {
    const rect = all[face];
    context.fillStyle = colors[face] ?? colors.all;
    context.fillRect(rect.x, rect.y, rect.w, rect.h);
  }
}

function shade(context: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, color: string) {
  context.fillStyle = color;
  context.fillRect(x, y, w, h);
}

/** Draws Wisp's default skin. Original art, made of plain rectangles. */
export function makeDefaultSkin(slim = false): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = SIZE;
  canvas.height = SIZE;
  const context = canvas.getContext("2d") as CanvasRenderingContext2D;
  const parts = boxes(slim);
  const box = (part: PartName, overlay = false) => parts.find((candidate) => candidate.part === part && candidate.overlay === overlay) as Box;
  const skinTone = "#e8c9a8";
  const skinShadow = "#d4b090";
  const hair = "#f4eee4";
  const hairShadow = "#d9d0c2";
  const hoodie = "#26221f";
  const hoodieDark = "#1a1715";
  const hoodieLight = "#332d29";
  const pants = "#3a3f52";
  const pantsDark = "#2c3040";
  const shoe = "#f4eee4";
  const ember = "#e5484d";

  // Head: skin, with hair on top, back and sides.
  paintBox(context, box("head"), { all: skinTone, top: hair, back: hair });
  const head = faces(box("head"));
  shade(context, head.front.x, head.front.y, 8, 2, hair); // fringe
  shade(context, head.front.x, head.front.y + 2, 1, 2, hair);
  shade(context, head.front.x + 7, head.front.y + 2, 1, 1, hair);
  shade(context, head.front.x + 1, head.front.y + 2, 2, 1, hairShadow);
  for (const side of [head.right, head.left]) {
    shade(context, side.x, side.y, 8, 3, hair);
    shade(context, side.x, side.y + 3, 3, 2, hair);
  }
  shade(context, head.right.x + 5, head.right.y + 3, 3, 1, hairShadow);
  // Eyes (dark, with a cream glint), cheeks, mouth.
  shade(context, head.front.x + 1, head.front.y + 4, 2, 2, "#ffffff");
  shade(context, head.front.x + 5, head.front.y + 4, 2, 2, "#ffffff");
  shade(context, head.front.x + 2, head.front.y + 4, 1, 2, "#2a2a3a");
  shade(context, head.front.x + 5, head.front.y + 4, 1, 2, "#2a2a3a");
  shade(context, head.front.x + 1, head.front.y + 6, 1, 1, "#e8a7a0");
  shade(context, head.front.x + 6, head.front.y + 6, 1, 1, "#e8a7a0");
  shade(context, head.front.x + 3, head.front.y + 7, 2, 1, skinShadow);
  // Hood (overlay) around the head, open at the front.
  const hood = faces(box("head", true));
  for (const face of [hood.top, hood.back, hood.right, hood.left]) shade(context, face.x, face.y, face.w, face.h, hoodie);
  context.clearRect(hood.right.x + 6, hood.right.y + 3, 2, 5);
  context.clearRect(hood.left.x, hood.left.y + 3, 2, 5);
  shade(context, hood.front.x, hood.front.y, 8, 1, hoodie);
  shade(context, hood.front.x, hood.front.y + 1, 1, 3, hoodie);
  shade(context, hood.front.x + 7, hood.front.y + 1, 1, 3, hoodie);

  // Body: hoodie with a pocket, drawstrings and the sparkle.
  paintBox(context, box("body"), { all: hoodie, top: hoodieLight, bottom: hoodieDark });
  const body = faces(box("body"));
  shade(context, body.front.x + 3, body.front.y, 1, 3, hair);
  shade(context, body.front.x + 4, body.front.y, 1, 4, hair);
  shade(context, body.front.x + 1, body.front.y + 8, 6, 3, hoodieLight);
  shade(context, body.front.x + 1, body.front.y + 11, 6, 1, hoodieDark);
  shade(context, body.front.x + 5, body.front.y + 3, 1, 1, ember);
  shade(context, body.front.x + 4, body.front.y + 4, 3, 1, ember);
  shade(context, body.front.x + 5, body.front.y + 5, 1, 1, ember);
  shade(context, body.back.x + 1, body.back.y + 10, 6, 2, hoodieDark);

  // Arms: sleeves with skin-tone hands.
  for (const part of ["rightArm", "leftArm"] as const) {
    const arm = faces(box(part));
    paintBox(context, box(part), { all: hoodie, top: hoodieLight });
    for (const face of [arm.front, arm.back, arm.right, arm.left]) {
      shade(context, face.x, face.y + face.h - 3, face.w, 3, skinTone);
      shade(context, face.x, face.y + face.h - 4, face.w, 1, hoodieDark);
    }
    shade(context, arm.bottom.x, arm.bottom.y, arm.bottom.w, arm.bottom.h, skinShadow);
  }
  // Legs: trousers and cream shoes.
  for (const part of ["rightLeg", "leftLeg"] as const) {
    const leg = faces(box(part));
    paintBox(context, box(part), { all: pants, bottom: shoe });
    for (const face of [leg.front, leg.back, leg.right, leg.left]) {
      shade(context, face.x, face.y + face.h - 2, face.w, 2, shoe);
      shade(context, face.x, face.y + face.h - 3, face.w, 1, pantsDark);
    }
  }
  return canvas;
}

/** A colour palette for the editor: skin tones, hair, and Minecraft-ish brights. */
export const PALETTE = [
  "#f4eee4", "#e8c9a8", "#d4b090", "#b98a6a", "#8d5e44", "#5c3b2a",
  "#26221f", "#3a3f52", "#6b6f80", "#a0a4b3", "#ffffff", "#000000",
  "#e5484d", "#f07a5a", "#f0a04b", "#f4d35e", "#4cc38a", "#2f9e6e",
  "#3d8bfd", "#9db8ff", "#7a5af8", "#c9a7ff", "#e879b9", "#f2979a",
];
