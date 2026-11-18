import { useEffect, useState } from "react";
import { getCurseforgeStatus } from "../data/api";
import { useApp } from "./AppContext";

// Whether this copy of Wisp was built with a CurseForge key: asked once per run.
let builtIn: boolean | null = null;
let asking: Promise<boolean> | null = null;

function askBuiltIn(): Promise<boolean> {
  asking ??= getCurseforgeStatus().then(
    (status) => (builtIn = status.builtIn),
    () => (builtIn = false),
  );
  return asking;
}

/**
 * Whether CurseForge can be used: this copy of Wisp has a key built in, or one was added in
 * Settings. Without a key, Wisp behaves as if CurseForge weren't there at all: no source switch,
 * no CurseForge worlds, no mention of it. Null while that's still being found out.
 */
export function useCurseforge(): boolean | null {
  const { settings } = useApp();
  const [known, setKnown] = useState(builtIn);
  useEffect(() => {
    if (known !== null) return;
    let active = true;
    void askBuiltIn().then((value) => active && setKnown(value));
    return () => {
      active = false;
    };
  }, [known]);
  if (settings?.curseforgeApiKey.trim()) return true;
  return known;
}
