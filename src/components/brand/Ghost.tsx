import { useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import { cn } from "../../lib/cn";

const BODY_PATH =
  "M30 6C16 6 8 17 8 30C8 40 12 47 18 51C22 54 24 58 30 58C36 58 40 55 46 56C51 57 55 60 58 58C54 55 52 50 52 44C53 38 54 34 54 29C54 15 44 6 30 6Z";

interface GhostProps {
  size?: number;
  className?: string;
  /** Gentle idle bob. */
  floating?: boolean;
  /** Closed eyes, for "resting while you play". */
  sleeping?: boolean;
  /** Controls the blink directly (the startup intro times it). Left out, the ghost blinks on its own. */
  blinking?: boolean;
}

/**
 * Blinks every few seconds. A one-off transition driven by a timer, not an infinite CSS
 * animation: SVG internals can't run on the GPU, so a looping animation would repaint every frame.
 */
function useBlink(enabled: boolean) {
  const [blinking, setBlinking] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    let timer = 0;
    const schedule = () => {
      timer = window.setTimeout(() => {
        if (document.hidden || !document.hasFocus()) return schedule();
        setBlinking(true);
        timer = window.setTimeout(() => {
          setBlinking(false);
          schedule();
        }, 150);
      }, 2600 + Math.random() * 4400);
    };
    schedule();
    return () => window.clearTimeout(timer);
  }, [enabled]);

  return blinking;
}

/** The Wisp mascot. It blinks on its own and can float or sleep. */
export function Ghost({ size = 40, className, floating = false, sleeping = false, blinking: blinkingProp }: GhostProps) {
  const reduceMotion = useReducedMotion();
  const autoBlink = useBlink(!sleeping && !reduceMotion && blinkingProp === undefined);
  const blinking = blinkingProp ?? autoBlink;

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      aria-hidden="true"
      className={cn("shrink-0 overflow-visible", floating && "animate-ghost-float", className)}
    >
      <path d={BODY_PATH} fill="#f4eee4" />
      {sleeping ? (
        <g fill="none" stroke="#141210" strokeWidth={2.6} strokeLinecap="round">
          <path d="M19.5 29q3.5 3 7 0" />
          <path d="M32.5 29q3.5 3 7 0" />
        </g>
      ) : (
        <g
          fill="#141210"
          style={{
            transformBox: "fill-box",
            transformOrigin: "center",
            transform: blinking ? "scaleY(0.12)" : "none",
            transition: "transform 70ms ease-out",
          }}
        >
          <ellipse cx={23} cy={28} rx={3.4} ry={5.2} />
          <ellipse cx={36} cy={28} rx={3.4} ry={5.2} />
        </g>
      )}
    </svg>
  );
}
