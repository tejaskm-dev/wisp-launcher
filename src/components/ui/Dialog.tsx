import { X } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect, useId, type ReactNode } from "react";

interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  /** Wider, for reading (a project's description). */
  wide?: boolean;
}

export function Dialog({ open, onClose, title, description, children, footer, wide = false }: DialogProps) {
  const titleId = useId();

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, onClose]);

  return (
    <AnimatePresence>
      {open && (
        <m.div className="fixed inset-0 z-[60] grid place-items-center p-6" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
          <div aria-hidden="true" onClick={onClose} className="absolute inset-0 bg-ink-950/75" />
          <m.div
            role="dialog"
            aria-modal="true"
            aria-labelledby={titleId}
            initial={{ opacity: 0, y: 18, scale: 0.97 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 10, scale: 0.98 }}
            transition={{ type: "spring", stiffness: 380, damping: 30 }}
            className={`tail relative flex max-h-[calc(100dvh-3rem)] w-full ${wide ? "max-w-[780px]" : "max-w-[580px]"} flex-col rounded-[26px] bg-ink-800 shadow-[inset_0_1px_0_rgba(244,238,228,0.06)] [--tail:46px] [filter:drop-shadow(0_40px_70px_rgba(0,0,0,0.75))]`}
          >
            <header className="flex items-start gap-4 px-6 pt-6">
              <div className="min-w-0 flex-1">
                <h2 id={titleId} className="font-display text-2xl font-bold tracking-[-0.01em]">
                  {title}
                </h2>
                {description && <p className="mt-1 text-sm text-mute">{description}</p>}
              </div>
              <button
                type="button"
                aria-label="Close"
                onClick={onClose}
                className="grid size-9 place-items-center rounded-xl text-mute transition-colors hover:bg-cream/8 hover:text-cream"
              >
                <X size={18} aria-hidden="true" />
              </button>
            </header>
            <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">{children}</div>
            {footer && <footer className="flex items-center justify-end gap-2.5 border-t border-cream/8 px-6 py-4">{footer}</footer>}
          </m.div>
        </m.div>
      )}
    </AnimatePresence>
  );
}
