import { m } from "motion/react";
import type { ReactNode } from "react";
import { fadeUp } from "../../lib/motion";

interface EmptyStateProps {
  art?: ReactNode;
  title: string;
  body: ReactNode;
  actions?: ReactNode;
}

/** A page-level "nothing here yet" panel. */
export function EmptyState({ art, title, body, actions }: EmptyStateProps) {
  return (
    <m.div
      variants={fadeUp}
      className="group flex flex-col items-center gap-4 rounded-[26px] border border-dashed border-cream/15 bg-cream/2 px-8 py-14 text-center"
    >
      {art}
      <h2 className="font-display text-2xl font-bold tracking-[-0.01em]">{title}</h2>
      <p className="max-w-md text-[15px] leading-relaxed text-mute">{body}</p>
      {actions && <div className="mt-2 flex flex-wrap justify-center gap-2.5">{actions}</div>}
    </m.div>
  );
}
