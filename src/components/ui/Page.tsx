import { m } from "motion/react";
import type { ReactNode } from "react";
import { cn } from "../../lib/cn";
import { stagger } from "../../lib/motion";

/** A page's root: staggers its sections in on arrival and fades out on the way to another page. */
export function Page({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <m.div
      variants={stagger}
      initial="hidden"
      animate="show"
      exit={{ opacity: 0, transition: { duration: 0.15 } }}
      className={cn("flex flex-col gap-7 px-7 pb-12", className)}
    >
      {children}
    </m.div>
  );
}

/** A titled card that groups related settings or details on a page. */
export function Panel({
  title,
  description,
  icon,
  action,
  children,
  className,
}: {
  title: string;
  description?: ReactNode;
  icon?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("wisp-surface rounded-[24px] p-6", className)}>
      <header className="mb-2 flex items-start gap-3.5">
        {icon && <span className="grid size-10 shrink-0 place-items-center rounded-[13px] bg-cream/7 text-cream-300">{icon}</span>}
        <div className="min-w-0 flex-1">
          <h2 className="font-display text-[19px] font-bold tracking-[-0.01em]">{title}</h2>
          {description && <p className="mt-0.5 text-[13px] leading-relaxed text-mute">{description}</p>}
        </div>
        {action}
      </header>
      {children}
    </section>
  );
}
