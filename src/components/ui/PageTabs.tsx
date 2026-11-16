import { m } from "motion/react";
import type { PageId } from "../../data/types";
import { cn } from "../../lib/cn";
import { fadeUp } from "../../lib/motion";
import { useApp } from "../../state/AppContext";

export interface PageTab {
  id: PageId;
  label: string;
  count?: number;
}

/** Pages that share a sidebar entry (Worlds and Servers), as tabs under the header. */
export function PageTabs({ tabs, current, label }: { tabs: PageTab[]; current: PageId; label: string }) {
  const { navigate } = useApp();
  return (
    <m.nav variants={fadeUp} aria-label={label} className="-mt-1 flex">
      <div className="inline-flex gap-1 rounded-full border border-cream/10 bg-ink-900/60 p-1">
        {tabs.map((tab) => {
          const active = tab.id === current;
          return (
            <a
              key={tab.id}
              href={`#${tab.id}`}
              aria-current={active ? "page" : undefined}
              onClick={(event) => {
                event.preventDefault();
                if (!active) navigate(tab.id);
              }}
              className={cn(
                "flex items-center gap-2 rounded-full px-4 py-1.5 text-[13px] font-bold transition-colors duration-200",
                active ? "bg-cream text-ink-900 shadow-[0_6px_18px_-8px_rgba(244,238,228,0.6)]" : "text-cream-300 hover:bg-cream/8 hover:text-cream",
              )}
            >
              {tab.label}
              {tab.count !== undefined && tab.count > 0 && <span className={cn("tabular-nums", active ? "text-ink-900/55" : "text-mute-2")}>{tab.count}</span>}
            </a>
          );
        })}
      </div>
    </m.nav>
  );
}
