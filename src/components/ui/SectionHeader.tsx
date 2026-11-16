import type { ReactNode } from "react";

interface SectionHeaderProps {
  id: string;
  title: string;
  icon: ReactNode;
  count?: number;
  action?: ReactNode;
}

export function SectionHeader({ id, title, icon, count, action }: SectionHeaderProps) {
  return (
    <header className="flex min-h-9 items-center gap-2.5">
      {icon}
      <h2 id={id} className="font-display text-[21px] font-bold tracking-[-0.01em]">
        {title}
      </h2>
      {count !== undefined && <span className="text-sm tabular-nums text-mute-2">{count}</span>}
      {action && <div className="ml-auto flex items-center gap-1">{action}</div>}
    </header>
  );
}
