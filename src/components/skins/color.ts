// Colour for the skin editor: HSV for the picker, the colours a skin is made of, recolouring that
// keeps a skin's shading, and hue, saturation and brightness for a whole layer.

export interface Hsv {
  /** 0 to 360. */
  h: number;
  /** 0 to 1. */
  s: number;
  /** 0 to 1. */
  v: number;
}

const clamp = (value: number, low = 0, high = 1) => Math.min(high, Math.max(low, value));
const byte = (value: number) => Math.round(clamp(value, 0, 255));
const hex2 = (value: number) => byte(value).toString(16).padStart(2, "0");

export function rgbToHex(r: number, g: number, b: number): string {
  return `#${hex2(r)}${hex2(g)}${hex2(b)}`;
}

export function hexToRgb(hex: string): [number, number, number] {
  const clean = hex.replace("#", "");
  return [parseInt(clean.slice(0, 2), 16) || 0, parseInt(clean.slice(2, 4), 16) || 0, parseInt(clean.slice(4, 6), 16) || 0];
}

export function rgbToHsv(r: number, g: number, b: number): Hsv {
  const [red, green, blue] = [r / 255, g / 255, b / 255];
  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const delta = max - min;
  let h = 0;
  if (delta > 0) {
    if (max === red) h = 60 * (((green - blue) / delta) % 6);
    else if (max === green) h = 60 * ((blue - red) / delta + 2);
    else h = 60 * ((red - green) / delta + 4);
  }
  return { h: (h + 360) % 360, s: max === 0 ? 0 : delta / max, v: max };
}

export function hsvToRgb({ h, s, v }: Hsv): [number, number, number] {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return [(r + m) * 255, (g + m) * 255, (b + m) * 255];
}

export const hexToHsv = (hex: string): Hsv => rgbToHsv(...hexToRgb(hex));
export const hsvToHex = (hsv: Hsv): string => rgbToHex(...hsvToRgb(hsv));

/** The colours a skin is made of, most used first (clear pixels left out). */
export function skinColors(image: ImageData, limit = 24): { hex: string; count: number }[] {
  const counts = new Map<number, number>();
  const { data } = image;
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] < 8) continue;
    const key = (data[i] << 16) | (data[i + 1] << 8) | data[i + 2];
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return [...counts.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, limit)
    .map(([key, count]) => ({ hex: rgbToHex((key >> 16) & 255, (key >> 8) & 255, key & 255), count }));
}

/** A colour in CIE Lab: lightness 0 to 100, then green-red and blue-yellow. */
function lab(r: number, g: number, b: number): [number, number, number] {
  const linear = (channel: number) => {
    const c = channel / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  const [lr, lg, lb] = [linear(r), linear(g), linear(b)];
  const x = (lr * 0.4124 + lg * 0.3576 + lb * 0.1805) / 0.95047;
  const y = lr * 0.2126 + lg * 0.7152 + lb * 0.0722;
  const z = (lr * 0.0193 + lg * 0.1192 + lb * 0.9505) / 1.08883;
  const f = (t: number) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  return [116 * f(y) - 16, 500 * (f(x) - f(y)), 200 * (f(y) - f(z))];
}

/** How different two colours look (0 the same, 1 about as different as black and white). */
function distance(a: [number, number, number], b: [number, number, number]): number {
  const [la, aa, ba] = lab(...a);
  const [lb, ab, bb] = lab(...b);
  return Math.sqrt((la - lb) ** 2 + (aa - ab) ** 2 + (ba - bb) ** 2) / 100;
}

/**
 * Turns every pixel that looks like `from` (within `tolerance`, 0 to 1) into `to`, keeping how
 * much lighter, darker or greyer each was than `from`: a shaded red jumper becomes an equally
 * shaded blue one. Returns how many pixels changed.
 */
export function recolor(image: ImageData, from: string, to: string, tolerance: number): number {
  const source = hexToRgb(from);
  const start = rgbToHsv(...source);
  const target = hexToHsv(to);
  const { data } = image;
  let changed = 0;
  for (let i = 0; i < data.length; i += 4) {
    if (data[i + 3] < 8) continue;
    const pixel: [number, number, number] = [data[i], data[i + 1], data[i + 2]];
    if (distance(pixel, source) > tolerance + 0.0001) continue;
    const here = rgbToHsv(...pixel);
    // Greys have no hue of their own: they take the new one. The rest keep their small offset.
    const offset = start.s < 0.08 || here.s < 0.08 ? 0 : ((here.h - start.h + 540) % 360) - 180;
    const [r, g, b] = hsvToRgb({ h: (target.h + offset + 360) % 360, s: clamp(target.s + (here.s - start.s)), v: clamp(target.v + (here.v - start.v)) });
    data[i] = byte(r);
    data[i + 1] = byte(g);
    data[i + 2] = byte(b);
    changed += 1;
  }
  return changed;
}

export interface Adjustment {
  /** Degrees round the colour wheel, -180 to 180. */
  hue: number;
  /** -1 (grey) to 1 (twice as strong). */
  saturation: number;
  /** -1 (black) to 1 (white). */
  brightness: number;
}

export const NO_ADJUSTMENT: Adjustment = { hue: 0, saturation: 0, brightness: 0 };

/** `base` with its colours turned, made stronger or weaker, and lighter or darker, into `out`. */
export function adjust(base: ImageData, out: ImageData, { hue, saturation, brightness }: Adjustment) {
  const from = base.data;
  const to = out.data;
  for (let i = 0; i < from.length; i += 4) {
    to[i + 3] = from[i + 3];
    if (from[i + 3] === 0) continue;
    const hsv = rgbToHsv(from[i], from[i + 1], from[i + 2]);
    const s = clamp(saturation >= 0 ? hsv.s + (1 - hsv.s) * saturation * hsv.s : hsv.s * (1 + saturation));
    const v = clamp(brightness >= 0 ? hsv.v + (1 - hsv.v) * brightness : hsv.v * (1 + brightness));
    const [r, g, b] = hsvToRgb({ h: (hsv.h + hue + 360) % 360, s, v });
    to[i] = byte(r);
    to[i + 1] = byte(g);
    to[i + 2] = byte(b);
  }
}
