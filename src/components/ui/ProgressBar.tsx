import { m } from "motion/react";
import { cn } from "../../lib/cn";

const TONES = {
  cream: "bg-cream",
  amber: "bg-amber",
  moss: "bg-moss",
} as const;

interface ProgressBarProps {
  /** 0 to 1; null shows a sliding bar for work with no measurable progress yet. */
  value: number | null;
  label: string;
  tone?: keyof typeof TONES;
  striped?: boolean;
  className?: string;
}

/** Thin progress bar. Animates with a GPU-friendly scaleX instead of width. */
export function ProgressBar({ value, label, tone = "cream", striped = false, className }: ProgressBarProps) {
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value === null ? undefined : Math.round(value * 100)}
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
