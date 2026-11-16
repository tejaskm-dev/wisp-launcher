import { Check } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { cn } from "../../lib/cn";
import { useLatest } from "../../lib/hooks";

export type Placement = "top-start" | "bottom-start" | "bottom-end" | "right-end";

interface PopoverProps {
  open: boolean;
  /** The button that opens it; the popover is placed against it and focus returns to it. */
  anchorRef: RefObject<HTMLElement | null>;
  placement: Placement;
  onClose: () => void;
  label: string;
  children: ReactNode;
  className?: string;
}

type Position = { left?: number; right?: number; top?: number; bottom?: number };

const GAP = 8;

function place(rect: DOMRect, placement: Placement): Position {
  switch (placement) {
    case "top-start":
      return { left: rect.left, bottom: window.innerHeight - rect.top + GAP };
    case "bottom-start":
      return { left: rect.left, top: rect.bottom + GAP };
    case "bottom-end":
      return { right: window.innerWidth - rect.right, top: rect.bottom + GAP };
    case "right-end":
      return { left: rect.right + GAP + 4, bottom: window.innerHeight - rect.bottom };
  }
}

const ORIGIN: Record<Placement, string> = {
  "top-start": "bottom left",
  "bottom-start": "top left",
  "bottom-end": "top right",
  "right-end": "bottom left",
};

const FOCUSABLE = "button:not(:disabled), [href], input:not(:disabled)";

/**
 * A floating panel (menus, pickers). Rendered into <body> so no scroll container clips it.
 * Closes on Escape, an outside click or scrolling; arrow keys move between its buttons.
 */
export function Popover({ open, anchorRef, placement, onClose, label, children, className }: PopoverProps) {
  const [position, setPosition] = useState<Position | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const close = useLatest(onClose);

  useLayoutEffect(() => {
    const anchor = anchorRef.current;
    if (!open || !anchor) return;
    const update = () => setPosition(place(anchor.getBoundingClientRect(), placement));
    update();
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [open, anchorRef, placement]);

  useEffect(() => {
    if (!open) return;
    const panel = () => panelRef.current;
    const inside = (target: EventTarget | null) =>
      target instanceof Node && (panel()?.contains(target) || anchorRef.current?.contains(target));

    const onPointerDown = (event: PointerEvent) => {
      if (!inside(event.target)) close.current();
    };
    const onScroll = (event: Event) => {
      if (!inside(event.target)) close.current();
    };
    // Capture phase, so Escape closes only this popover and not a dialog underneath it.
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        close.current();
        anchorRef.current?.focus();
        return;
      }
      const current = panel();
      if (!current || (event.key !== "ArrowDown" && event.key !== "ArrowUp")) return;
      const items = [...current.querySelectorAll<HTMLElement>(FOCUSABLE)];
      if (items.length === 0) return;
      event.preventDefault();
      const index = items.indexOf(document.activeElement as HTMLElement);
      const step = event.key === "ArrowDown" ? 1 : -1;
      items[(index + step + items.length) % items.length].focus();
    };

    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("scroll", onScroll, true);
    window.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  }, [open, anchorRef, close]);

  // Move focus into the panel: a [data-autofocus] element (a filter box), else the checked option,
  // else the first button.
  useEffect(() => {
    if (!open || !position) return;
    const current = panelRef.current;
    const target =
      current?.querySelector<HTMLElement>("[data-autofocus]") ??
      current?.querySelector<HTMLElement>("[aria-checked='true']") ??
      current?.querySelector<HTMLElement>(FOCUSABLE);
    target?.focus({ preventScroll: true });
  }, [open, position]);

  const fromSide = placement === "right-end";
  const fromAbove = placement === "top-start";

  return createPortal(
    <AnimatePresence>
      {open && position && (
        <m.div
          ref={panelRef}
          role="dialog"
          aria-label={label}
          style={{ position: "fixed", ...position, transformOrigin: ORIGIN[placement] }}
          initial={{ opacity: 0, scale: 0.95, x: fromSide ? -6 : 0, y: fromSide ? 0 : fromAbove ? 6 : -6 }}
          animate={{ opacity: 1, scale: 1, x: 0, y: 0 }}
          exit={{ opacity: 0, scale: 0.97, transition: { duration: 0.12 } }}
          transition={{ type: "spring", stiffness: 560, damping: 36 }}
          className={cn(
            "z-[70] rounded-[18px] border border-cream/10 bg-ink-700 p-1.5 shadow-[0_24px_60px_-12px_rgba(0,0,0,0.85)]",
            className,
          )}
        >
          {children}
        </m.div>
      )}
    </AnimatePresence>,
    document.body,
  );
}

/** A row inside a popover menu. */
export function MenuItem({
  icon,
  label,
  hint,
  shortcut,
  disabled,
  checked,
  onSelect,
}: {
  icon?: ReactNode;
  label: string;
  hint?: string;
  shortcut?: string;
  disabled?: boolean;
  /** Set for single-choice menus. */
  checked?: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role={checked === undefined ? "menuitem" : "menuitemradio"}
      aria-checked={checked}
      disabled={disabled}
      onClick={onSelect}
      className="group flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left outline-none transition-colors enabled:hover:bg-cream/8 enabled:focus-visible:bg-cream/8 disabled:cursor-not-allowed"
    >
      {icon && <span className={cn("grid size-5 shrink-0 place-items-center text-cream-400", disabled && "opacity-50")}>{icon}</span>}
      <span className="flex min-w-0 flex-1 flex-col">
        <span className={cn("text-sm font-semibold", disabled && "text-mute")}>{label}</span>
        {hint && <span className="text-xs leading-snug text-mute-2">{hint}</span>}
      </span>
      {shortcut && <kbd className="shrink-0 font-sans text-xs font-semibold text-mute-2">{shortcut}</kbd>}
      {checked && <Check size={15} strokeWidth={2.6} className="shrink-0 text-ember" aria-hidden="true" />}
    </button>
  );
}
