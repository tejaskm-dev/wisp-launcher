import { X } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import type { ReactNode } from "react";
import type { Toast } from "../state/useToasts";
import { Ghost } from "./brand/Ghost";

/** The bottom-right stack: toasts, with anything passed as children (launch progress) beneath. */
export function Toasts({ toasts, onDismiss, children }: { toasts: Toast[]; onDismiss: (id: number) => void; children?: ReactNode }) {
  return (
    <div aria-live="polite" className="pointer-events-none fixed bottom-5 right-5 z-50 flex w-[340px] flex-col gap-2.5">
      <AnimatePresence initial={false}>
        {toasts.map((toast) => (
          <m.div
            key={toast.id}
            initial={{ opacity: 0, y: 16, scale: 0.96 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, x: 28, transition: { duration: 0.2 } }}
            transition={{ type: "spring", stiffness: 380, damping: 30 }}
            className="pointer-events-auto flex items-start gap-3 rounded-2xl border border-cream/10 bg-ink-700/95 p-3.5 shadow-[0_20px_50px_-12px_rgba(0,0,0,0.8)]"
          >
            <Ghost size={30} floating sleeping={toast.tone === "resting"} className="mt-0.5" />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-bold">{toast.title}</p>
              {toast.body && <p className="mt-0.5 whitespace-pre-line text-[13px] leading-snug text-mute">{toast.body}</p>}
              {toast.action && (
                <button
                  type="button"
                  onClick={() => {
                    toast.action?.run();
                    onDismiss(toast.id);
                  }}
                  className="mt-2 rounded-full border border-cream/20 px-3 py-1 text-xs font-bold text-cream-300 transition-colors hover:border-cream/45 hover:text-cream"
                >
                  {toast.action.label}
                </button>
              )}
            </div>
            <button
              type="button"
              aria-label="Dismiss"
              onClick={() => onDismiss(toast.id)}
              className="rounded-lg p-1 text-mute transition-colors hover:bg-cream/5 hover:text-cream"
            >
              <X size={14} aria-hidden="true" />
            </button>
          </m.div>
        ))}
      </AnimatePresence>
      {children}
    </div>
  );
}
