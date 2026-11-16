import { useEffect, useRef } from "react";
import { cn } from "../../lib/cn";
import { drawFace, drawPaperDoll } from "./skinTexture";

const cache = new Map<string, Promise<HTMLImageElement>>();

/** Loads a skin picture once (it's read back for pixels, so it's fetched as CORS-clean). */
export function loadSkinImage(src: string): Promise<HTMLImageElement> {
  const known = cache.get(src);
  if (known) return known;
  const loading = new Promise<HTMLImageElement>((resolve, reject) => {
    const picture = new Image();
    picture.crossOrigin = "anonymous";
    picture.onload = () => resolve(picture);
    picture.onerror = () => {
      cache.delete(src);
      reject(new Error("couldn't load the skin"));
    };
    picture.src = src;
  });
  cache.set(src, loading);
  return loading;
}

/** A skin seen flat from the front, crisp pixels. */
export function SkinDoll({ src, slim, className }: { src: string | HTMLCanvasElement; slim: boolean; className?: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let active = true;
    const target = canvas.current;
    if (!target) return;
    if (typeof src !== "string") {
      drawPaperDoll(src, target, slim);
      return;
    }
    if (!src) return;
    loadSkinImage(src).then((picture) => active && drawPaperDoll(picture, target, slim), () => {});
    return () => {
      active = false;
    };
  }, [src, slim]);
  return <canvas ref={canvas} width={16} height={32} aria-hidden="true" className={cn("[image-rendering:pixelated]", className)} />;
}

/** Just the face with its hat layer, for avatars. */
export function SkinFace({ src, className }: { src: string | HTMLCanvasElement; className?: string }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let active = true;
    const target = canvas.current;
    if (!target || !src) return;
    if (typeof src !== "string") {
      drawFace(src, target);
      return;
    }
    loadSkinImage(src).then((picture) => active && drawFace(picture, target), () => {});
    return () => {
      active = false;
    };
  }, [src]);
  return <canvas ref={canvas} width={8} height={8} aria-hidden="true" className={cn("[image-rendering:pixelated]", className)} />;
}

/** Tells the rest of the app (the account avatar) that the skin library changed. */
export function announceSkinsChanged() {
  window.dispatchEvent(new Event("wisp:skins-changed"));
}
