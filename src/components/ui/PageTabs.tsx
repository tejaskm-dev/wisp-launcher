import { m } from "motion/react";
import { useContext, useLayoutEffect, useRef } from "react";
import type { PageId } from "../../data/types";
import { cn } from "../../lib/cn";
import { fadeUp } from "../../lib/motion";
import { useApp } from "../../state/AppContext";
import { PageArrivalContext } from "./Page";

export interface PageTab {
  id: PageId;
  label: string;
  count?: number;
}

/** Where each tab bar's pill last sat, so switching tabs slides it over from there. */
const lastPill = new Map<string, { left: number; width: number }>();

/**
 * Pages that share a sidebar entry (Worlds and Servers), as tabs under the header. The chosen tab
 * wears a little cream ghost, tail and all, which slides across when you switch.
 */
export function PageTabs({ tabs, current, label }: { tabs: PageTab[]; current: PageId; label: string }) {
  const { navigate } = useApp();
  const arrival = useContext(PageArrivalContext);
  const bar = useRef<HTMLDivElement>(null);
  const pill = useRef<HTMLSpanElement>(null);

  // Each tab is its own page, so the bar is new after a switch: the pill starts where the last
  // bar's was and slides to its tab.
  useLayoutEffect(() => {
    const active = bar.current?.querySelector<HTMLElement>("[aria-current=page]");
    const element = pill.current;
    if (!active || !element) return;
    const to = { left: active.offsetLeft, width: active.offsetWidth };
    const from = arrival === "tab" ? lastPill.get(label) : undefined;
    lastPill.set(label, to);
    element.style.transition = "none";
    element.style.width = `${(from ?? to).width}px`;
    element.style.transform = `translateX(${(from ?? to).left}px)`;
    if (!from || (from.left === to.left && from.width === to.width)) return;
    element.getBoundingClientRect();
    element.style.transition = "";
    element.style.width = `${to.width}px`;
    element.style.transform = `translateX(${to.left}px)`;
  });

  return (
    <m.nav variants={fadeUp} aria-label={label} data-page-tabs className="-mt-1 flex">
      <div ref={bar} className="relative inline-flex gap-1 rounded-full border border-cream/10 bg-ink-900/60 p-1">
        <span
          ref={pill}
          aria-hidden="true"
          className="tail absolute inset-y-1 left-0 rounded-full bg-cream shadow-[0_6px_18px_-8px_rgba(244,238,228,0.6)] transition-[transform,width] duration-300 ease-out-expo [--tail:16px]"
        />
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
                "relative flex items-center gap-2 rounded-full px-4 py-1.5 text-[13px] font-bold transition-colors duration-300",
                active ? "text-ink-900" : "text-cream-300 hover:bg-cream/8 hover:text-cream",
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
