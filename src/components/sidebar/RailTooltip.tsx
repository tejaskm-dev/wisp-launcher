import { AnimatePresence, m } from "motion/react";
import type { FocusEvent, PointerEvent } from "react";
import { createPortal } from "react-dom";

export interface RailTip {
  label: string;
  hint?: string;
  /** Screen position: left edge, and the vertical centre of the control it labels. */
  left: number;
  centerY: number;
}

export interface TipHandlers {
  onPointerEnter?: (event: PointerEvent<HTMLElement>) => void;
  onPointerLeave?: () => void;
  onFocus?: (event: FocusEvent<HTMLElement>) => void;
  onBlur?: () => void;
}

/** Hover/focus handlers that show a label beside an icon-only control. */
export type TipFor = (label: string, hint?: string) => TipHandlers;

/**
 * One shared tooltip for the collapsed sidebar. It glides from item to item as the pointer moves
 * down the rail, and lives in <body> so the sidebar's overflow never clips it.
 */
export function RailTooltip({ tip }: { tip: RailTip | null }) {
  return createPortal(
    <AnimatePresence>
      {tip && (
        <m.div
          key="rail-tip"
          role="tooltip"
          className="pointer-events-none fixed left-0 top-0 z-[70] flex -translate-y-1/2 items-center gap-2.5 whitespace-nowrap rounded-xl border border-cream/10 bg-ink-700 py-1.5 pl-3 pr-2 text-[13px] font-semibold shadow-[0_12px_32px_-8px_rgba(0,0,0,0.8)]"
          initial={{ opacity: 0, x: tip.left - 6, y: tip.centerY }}
          animate={{ opacity: 1, x: tip.left, y: tip.centerY }}
          exit={{ opacity: 0, transition: { duration: 0.1 } }}
          transition={{ type: "spring", stiffness: 640, damping: 42 }}
        >
          {tip.label}
          {tip.hint && <kbd className="rounded-md bg-cream/8 px-1.5 py-0.5 font-sans text-xs font-semibold text-mute">{tip.hint}</kbd>}
        </m.div>
      )}
    </AnimatePresence>,
    document.body,
  );
}
