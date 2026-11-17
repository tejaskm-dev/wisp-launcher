import { m } from "motion/react";
import { cn } from "../../lib/cn";
import { Ghost } from "../brand/Ghost";

const TONES = {
  cream: "bg-cream",
  amber: "bg-amber",
  moss: "bg-moss",
} as const;

/** How many blocks a pixel bar is made of. */
const BLOCKS = 24;

interface ProgressBarProps {
  /** 0 to 1; null shows a sliding bar for work with no measurable progress yet. */
  value: number | null;
  label: string;
  tone?: keyof typeof TONES;
  striped?: boolean;
  /**
   * For game downloads: the bar fills block by block, like a Minecraft progress bar, with Wisp's
   * ghost riding along at the front.
   */
  pixel?: boolean;
  className?: string;
}

/** Thin progress bar. Animates with a GPU-friendly scaleX instead of width. */
export function ProgressBar({ value, label, tone = "cream", striped = false, pixel = false, className }: ProgressBarProps) {
  const percent = value === null ? undefined : Math.round(value * 100);
  if (pixel) {
    const lit = value === null ? 0 : Math.round(value * BLOCKS);
    return (
      <div role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} className={cn("relative pt-4", className)}>
        <div className="flex h-2 gap-[2px]">
          {Array.from({ length: BLOCKS }, (_, index) => (
            <span
              key={index}
              className={cn(
                "flex-1 rounded-[2px] transition-colors duration-200",
                value === null ? "animate-pulse bg-cream/12" : index < lit ? TONES[tone] : "bg-cream/10",
              )}
              style={value === null ? { animationDelay: `${index * 40}ms` } : undefined}
            />
          ))}
        </div>
        {value !== null && (
          // A full-width layer slid along by a transform (a percentage of its own width), so the
          // ghost moves on the GPU rather than by layout.
          <m.span
            aria-hidden="true"
            className="pointer-events-none absolute inset-x-0 top-0"
            initial={false}
            animate={{ x: `${Math.min(1, Math.max(0, value)) * 100}%` }}
            transition={{ type: "spring", stiffness: 160, damping: 24 }}
          >
            <Ghost size={16} floating className="-ml-2" />
          </m.span>
        )}
      </div>
    );
  }
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percent}
      className={cn("relative h-1.5 overflow-hidden rounded-full bg-cream/10", className)}
    >
      {value === null ? (
        <div className={cn("absolute inset-y-0 left-0 w-1/3 animate-progress-slide rounded-full", TONES[tone])} />
      ) : (
        <m.div
          className={cn("h-full w-full origin-left rounded-full", TONES[tone], striped && "animate-stripes bg-stripes")}
          initial={{ scaleX: 0 }}
          animate={{ scaleX: value }}
          transition={{ type: "spring", stiffness: 140, damping: 26 }}
        />
      )}
    </div>
  );
}
