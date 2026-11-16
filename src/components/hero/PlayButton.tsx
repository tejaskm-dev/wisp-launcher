import { LoaderCircle, Play, RotateCw } from "lucide-react";
import { m } from "motion/react";
import type { LaunchView } from "../launch/describe";

interface PlayButtonProps {
  view: LaunchView;
  /** What Play starts, for screen readers. */
  name: string;
  onPlay: () => void;
  onStop: () => void;
}

/** The main call to action. Shows launch progress inside the button, then a live "Playing" state. */
export function PlayButton({ view, name, onPlay, onStop }: PlayButtonProps) {
  const busy = view.kind === "busy";
  const running = view.kind === "running";
  const ariaLabel = running ? `Stop ${name}` : busy ? `${name}: ${view.label}` : `Play ${name}`;

  const handleClick = () => {
    if (running) onStop();
    else if (!busy) onPlay();
  };

  return (
    <m.button
      type="button"
      onClick={handleClick}
      aria-label={ariaLabel}
      aria-disabled={busy}
      whileHover={busy ? undefined : { scale: 1.035 }}
      whileTap={busy ? undefined : { scale: 0.97 }}
      className="group relative flex h-[60px] min-w-[156px] items-center justify-center overflow-hidden rounded-full bg-cream px-8 font-display text-[21px] font-extrabold text-ink-900 shadow-[0_12px_40px_-6px_rgba(244,238,228,0.35)] transition-shadow duration-300 hover:shadow-[0_16px_56px_-4px_rgba(244,238,228,0.55)]"
    >
      {busy && view.progress !== null && (
        <m.span
          aria-hidden="true"
          className="absolute inset-0 origin-left bg-cream-400/70"
          initial={false}
          animate={{ scaleX: view.progress }}
          transition={{ ease: "linear", duration: 0.15 }}
        />
      )}
      {view.kind === "idle" && (
        <span
          aria-hidden="true"
          className="absolute inset-y-0 left-0 w-1/3 bg-linear-to-r from-transparent via-white/70 to-transparent opacity-0 group-hover:animate-shimmer group-hover:opacity-100"
        />
      )}
      <span className="relative flex items-center gap-3">
        {running ? (
          <span className="relative flex size-2.5" aria-hidden="true">
            <span className="absolute inset-0 animate-ping-soft rounded-full bg-moss" />
            <span className="relative size-2.5 rounded-full bg-moss" />
          </span>
        ) : busy ? (
          <LoaderCircle size={18} className="animate-spin" aria-hidden="true" />
        ) : view.kind === "problem" ? (
          <RotateCw size={17} aria-hidden="true" />
        ) : (
          <Play size={18} fill="currentColor" strokeWidth={0} aria-hidden="true" />
        )}
        <span className="tabular-nums">{view.kind === "idle" ? "Play" : view.label}</span>
        {running && (
          <span className="rounded-full bg-ink-900/10 px-2 py-0.5 font-sans text-xs font-bold transition-colors group-hover:bg-ink-900 group-hover:text-cream">
            Stop
          </span>
        )}
      </span>
    </m.button>
  );
}
