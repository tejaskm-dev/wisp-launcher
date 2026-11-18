import { AnimatePresence, m, useReducedMotion } from "motion/react";
import { useId, useState, type ReactNode } from "react";
import { cn } from "../../lib/cn";
import { Ghost } from "../brand/Ghost";
import { Sparkle } from "../brand/Sparkle";

interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: ReactNode;
  disabled?: boolean;
}

/** How far the ghost travels across the track when it's switched on. */
const ACROSS = 27;

/**
 * A labelled on/off switch row in Wisp's own shape. The track is the ghost's silhouette, its tail
 * flicking out at the corner (and wagging when you point at it, like the buttons), and the knob
 * is the ghost itself. Off, it dozes on the left in an unlit track; switched on, it hops across,
 * wakes up, lights the track and leaves a spark behind, the way Wisp marks what's chosen.
 */
export function Toggle({ checked, onChange, label, description, disabled = false }: ToggleProps) {
  const id = useId();
  const calm = useReducedMotion();
  // Counts switch-ons, so each one gets its own spark.
  const [sparks, setSparks] = useState(0);

  const flip = () => {
    if (!checked) setSparks((count) => count + 1);
    onChange(!checked);
  };

  return (
    <div className={cn("flex items-center justify-between gap-6 py-3.5", disabled && "opacity-50")}>
      <div className="min-w-0">
        <label htmlFor={id} className="text-[15px] font-semibold">
          {label}
        </label>
        {description && <p className="mt-0.5 text-[13px] leading-relaxed text-mute">{description}</p>}
      </div>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={flip}
        className={cn(
          "tail wag relative mr-1 h-[30px] w-[58px] shrink-0 rounded-full transition-[background-color,filter] duration-300 [--tail:18px] disabled:cursor-not-allowed",
          checked
            ? "bg-moss [filter:drop-shadow(0_0_9px_rgba(76,195,138,0.38))]"
            : "ghost-ring bg-ink-950 [--ring:rgba(244,238,228,0.15)] enabled:hover:[--ring:rgba(244,238,228,0.32)]",
        )}
      >
        <m.span
          aria-hidden="true"
          className="absolute left-[3px] top-[2px] block"
          initial={false}
          animate={
            calm
              ? { x: checked ? ACROSS : 0, opacity: checked ? 1 : 0.78 }
              : {
                  x: checked ? ACROSS : 0,
                  // A hop up and across when it wakes; a slump back down when it dozes off.
                  y: checked ? [0, -8, -1] : [-1, -3, 1],
                  rotate: checked ? [-6, 10, 0] : [0, -12, -7],
                  opacity: checked ? 1 : 0.78,
                }
          }
          transition={{
            x: { type: "spring", stiffness: 420, damping: 30 },
            y: { duration: 0.45, ease: [0.16, 1, 0.3, 1] },
            rotate: { duration: 0.45, ease: [0.16, 1, 0.3, 1] },
            opacity: { duration: 0.25 },
          }}
        >
          <Ghost size={26} sleeping={!checked} className={checked ? "drop-shadow-[0_2px_3px_rgba(10,9,8,0.5)]" : undefined} />
        </m.span>
        <AnimatePresence>
          {checked && sparks > 0 && !calm && (
            <m.span
              key={sparks}
              aria-hidden="true"
              className="pointer-events-none absolute -right-1.5 -top-2.5 block text-ember"
              initial={{ opacity: 0, scale: 0.2, rotate: 0, y: 4 }}
              animate={{ opacity: [0, 1, 0], scale: [0.2, 1.15, 0.7], rotate: 90, y: -6 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.85, ease: [0.16, 1, 0.3, 1] }}
            >
              <Sparkle className="size-3" />
            </m.span>
          )}
        </AnimatePresence>
      </button>
    </div>
  );
}
