import { useEffect, useState } from "react";
import { getSkinDataUrl } from "../../data/api";
import type { Skin } from "../../data/types";

// Skin pictures as data URLs, fetched once per version of each skin.
const cache = new Map<string, Promise<string>>();

/** A skin's picture as a data URL (same-origin, so canvases can read it). */
export function skinSource(skin: Pick<Skin, "id" | "updated">): Promise<string> {
  const key = `${skin.id}:${skin.updated}`;
  const known = cache.get(key);
  if (known) return known;
  const loading = getSkinDataUrl(skin.id).catch((failure: unknown) => {
    cache.delete(key);
    throw failure;
  });
  cache.set(key, loading);
  return loading;
}

/** The data URL for a skin once it has loaded; null before then (or for no skin). */
export function useSkinSource(skin: Pick<Skin, "id" | "updated"> | null): string | null {
  const [source, setSource] = useState<string | null>(null);
  const key = skin ? `${skin.id}:${skin.updated}` : null;
  useEffect(() => {
    if (!skin) {
      setSource(null);
      return;
    }
    let active = true;
    skinSource(skin).then(
      (found) => active && setSource(found),
      () => active && setSource(null),
    );
    return () => {
      active = false;
    };
    // The key covers the skin's identity and version.
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps
  return source;
}
