import { RotateCw, WifiOff } from "lucide-react";
import { m } from "motion/react";

/** Shown in place of a feed that failed to load. */
export function ErrorState({ title, body, onRetry }: { title: string; body: string; onRetry: () => void }) {
  return (
    <m.div
      role="alert"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      className="wisp-surface flex flex-wrap items-center gap-x-4 gap-y-3 rounded-[20px] p-4"
    >
      <span className="grid size-11 shrink-0 place-items-center rounded-2xl bg-coral/10 text-coral">
        <WifiOff size={20} aria-hidden="true" />
      </span>
      <div className="min-w-[160px] flex-1">
        <p className="text-[15px] font-bold">{title}</p>
        <p className="mt-0.5 text-[13px] text-mute">{body}</p>
      </div>
      <button
        type="button"
        onClick={onRetry}
        className="group flex h-9 items-center gap-2 rounded-full border border-cream/20 px-4 text-[13px] font-bold transition-colors hover:border-cream/40 hover:bg-cream/8"
      >
        <RotateCw size={14} className="transition-transform duration-500 group-hover:rotate-180" aria-hidden="true" />
        Try again
      </button>
    </m.div>
  );
}
