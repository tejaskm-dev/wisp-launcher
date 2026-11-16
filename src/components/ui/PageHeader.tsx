import { m } from "motion/react";
import type { ReactNode } from "react";
import { fadeUp } from "../../lib/motion";
import { Sparkle } from "../brand/Sparkle";

interface PageHeaderProps {
  title: string;
  subtitle?: string;
  actions?: ReactNode;
}

export function PageHeader({ title, subtitle, actions }: PageHeaderProps) {
  return (
    <m.header variants={fadeUp} className="flex flex-wrap items-end gap-x-6 gap-y-4">
      <div className="min-w-0 flex-1">
        <h1 className="flex items-start gap-2 font-display text-[40px] font-extrabold leading-none tracking-[-0.035em]">
          {title}
          <Sparkle twinkle className="mt-0.5 size-4 text-ember" />
        </h1>
        {subtitle && <p className="mt-2.5 max-w-xl text-[15px] leading-relaxed text-mute">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2.5">{actions}</div>}
    </m.header>
  );
}
