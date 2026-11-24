// Cosmetics: a skin's outer layer (hat, jacket, sleeves, trousers) kept on its own and worn over
// any skin, replacing that skin's own outer layer. The same parts as wisp_core::skins::OUTER.

import { useEffect, useState } from "react";
import { getCosmeticDataUrl } from "../../data/api";
import { loadSkinImage } from "./SkinDoll";

/** x, y, width, height of each outer-layer part on a 64×64 skin. */
export const OUTER: [number, number, number, number][] = [
  [32, 0, 32, 16],
  [16, 32, 24, 16],
  [40, 32, 16, 16],
  [48, 48, 16, 16],
  [0, 32, 16, 16],
  [0, 48, 16, 16],
];

/** `skin` wearing `cosmetic`: its outer layer swapped for the cosmetic's. */
export function wear(skin: CanvasImageSource, cosmetic: CanvasImageSource | null): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const g = canvas.getContext("2d", { willReadFrequently: true }) as CanvasRenderingContext2D;
  g.imageSmoothingEnabled = false;
  g.drawImage(skin, 0, 0);
  if (cosmetic) {
    for (const [x, y, w, h] of OUTER) {
      g.clearRect(x, y, w, h);
      g.drawImage(cosmetic, x, y, w, h, x, y, w, h);
    }
  }
  return canvas;
}

/** Just the outer layer of a skin, as a PNG data URL (what a cosmetic keeps). */
export function outerOnly(skin: CanvasImageSource): string {
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const g = canvas.getContext("2d") as CanvasRenderingContext2D;
  for (const [x, y, w, h] of OUTER) g.drawImage(skin, x, y, w, h, x, y, w, h);
  return canvas.toDataURL("image/png");
}

const cache = new Map<string, Promise<string>>();

/** A cosmetic's picture as a data URL, fetched once. */
export function cosmeticSource(id: string): Promise<string> {
  const known = cache.get(id);
  if (known) return known;
  const loading = getCosmeticDataUrl(id).catch((failure: unknown) => {
    cache.delete(id);
    throw failure;
  });
  cache.set(id, loading);
  return loading;
}

/** A skin (data URL or canvas) wearing a cosmetic (by id; none for the skin's own outer layer). */
export function useWorn(skin: string | HTMLCanvasElement | null, cosmeticId: string | null): HTMLCanvasElement | string | null {
  const [worn, setWorn] = useState<HTMLCanvasElement | string | null>(cosmeticId ? null : skin);
  useEffect(() => {
    if (!skin || !cosmeticId) {
      setWorn(skin);
      return;
    }
    let active = true;
    const base = typeof skin === "string" ? loadSkinImage(skin) : Promise.resolve(skin);
    Promise.all([base, cosmeticSource(cosmeticId).then(loadSkinImage)]).then(
      ([picture, cosmetic]) => active && setWorn(wear(picture, cosmetic)),
      () => active && setWorn(skin),
    );
    return () => {
      active = false;
    };
  }, [skin, cosmeticId]);
  return worn;
}
