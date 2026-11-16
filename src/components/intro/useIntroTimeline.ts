import { useCallback, useEffect, useState } from "react";
import type { IntroMode } from "../../data/types";

/**
 * orb:  a wisp of light gathers in the dark.
 * show: it bursts, the ghost and wordmark appear over the moonlit art.
 * dock: the logo flies to the sidebar and the art settles into the home hero as the app appears.
 * done: the intro is gone.
 */
export type IntroPhase = "orb" | "show" | "dock" | "done";

type PlayingMode = Exclude<IntroMode, "off">;

/** How long each phase lasts, in milliseconds. */
const DURATION: Record<PlayingMode, Record<Exclude<IntroPhase, "done">, number>> = {
  full: { orb: 700, show: 1700, dock: 850 },
  quick: { orb: 0, show: 650, dock: 850 },
};

const NEXT = { orb: "show", show: "dock", dock: "done" } as const;

const prefersReducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/**
 * Steps the startup intro through its phases. `mode` is null until settings have loaded (a few
 * milliseconds); the intro holds on its first frame until then.
 */
export function useIntroTimeline(mode: IntroMode | null) {
  const [phase, setPhase] = useState<IntroPhase>(() => (prefersReducedMotion() ? "done" : "orb"));
  /** True once the intro has visibly played, so the hero knows the floating art landed on it. */
  const [played, setPlayed] = useState(false);
  // Safety net: if settings somehow never arrive, play the default intro rather than hold forever.
  const [gaveUpWaiting, setGaveUpWaiting] = useState(false);
  useEffect(() => {
    if (mode !== null) return;
    const id = window.setTimeout(() => setGaveUpWaiting(true), 1500);
    return () => window.clearTimeout(id);
  }, [mode]);
  const requested = mode ?? (gaveUpWaiting ? "full" : null);
  const effective: IntroMode | null = requested === null ? null : prefersReducedMotion() ? "off" : requested;

  useEffect(() => {
    if (phase === "done" || effective === null) return;
    if (effective === "off") {
      setPhase("done");
      return;
    }
    const id = window.setTimeout(() => {
      const next = NEXT[phase];
      if (next !== "done") setPlayed(true);
      setPhase(next);
    }, DURATION[effective][phase]);
    return () => window.clearTimeout(id);
  }, [phase, effective]);

  /** Any key or click: jump straight to docking. */
  const skip = useCallback(() => {
    setPhase((current) => (current === "orb" || current === "show" ? "dock" : current));
    setPlayed(true);
  }, []);

  /** Plays the intro again (the preview button in Settings). */
  const replay = useCallback(() => {
    setPlayed(false);
    setPhase("orb");
  }, []);

  return { phase, played, mode: effective, skip, replay };
}
