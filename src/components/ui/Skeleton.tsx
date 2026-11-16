import type { ReactNode } from "react";
import { cn } from "../../lib/cn";

/** A grey placeholder shape. Size and rounding come from `className`. */
export function Skeleton({ className }: { className?: string }) {
  return <div aria-hidden="true" className={cn("skeleton", className)} />;
}

/**
 * Wraps a loading layout: tells screen readers what is loading, and fades in only after a short
 * delay so data that arrives quickly never flashes a skeleton.
 */
export function Loading({ label, className, children }: { label: string; className?: string; children: ReactNode }) {
  return (
    <div role="status" aria-label={label} aria-busy="true" className={cn("animate-skeleton-in", className)}>
      {children}
    </div>
  );
}
