import { Box, ChevronRight, Compass, Download, Link2, Zap, type LucideIcon } from "lucide-react";
import { m } from "motion/react";
import { cn } from "../lib/cn";
import { fadeUp, rise } from "../lib/motion";
import { SectionHeader } from "./ui/SectionHeader";

export type QuickActionId = "import" | "instance" | "modpacks" | "link";

const ACTIONS: { id: QuickActionId; label: string; hint: string; icon: LucideIcon; tone: string }[] = [
  { id: "import", label: "Import from another launcher", hint: "Official, Prism, Modrinth, CurseForge", icon: Download, tone: "bg-cream/8 text-cream" },
  { id: "instance", label: "New instance", hint: "Pick a version and mod loader", icon: Box, tone: "bg-sky/10 text-sky" },
  { id: "modpacks", label: "Browse modpacks", hint: "From Modrinth", icon: Compass, tone: "bg-amber/10 text-amber-soft" },
  { id: "link", label: "Link another computer", hint: "Your worlds follow you there", icon: Link2, tone: "bg-moss/10 text-moss-soft" },
];

export function QuickActions({ onAction }: { onAction: (id: QuickActionId) => void }) {
  return (
    <m.section variants={rise} aria-labelledby="actions-title" className="flex flex-col gap-3.5">
      <SectionHeader id="actions-title" title="Quick actions" icon={<Zap size={18} strokeWidth={2} className="text-ember" aria-hidden="true" />} />
      <ul className="flex flex-col gap-2">
        {ACTIONS.map((action) => {
          const Icon = action.icon;
          return (
            <m.li key={action.id} variants={fadeUp}>
              <button
                type="button"
                onClick={() => onAction(action.id)}
                className="group flex w-full items-center gap-3.5 rounded-2xl border border-cream/7 bg-ink-800 px-3.5 py-2.5 text-left transition-colors duration-200 hover:border-cream/15 hover:bg-ink-700"
              >
                <span className={cn("grid size-9 shrink-0 place-items-center rounded-xl transition-transform duration-300 ease-out-expo group-hover:scale-110", action.tone)}>
                  <Icon size={18} strokeWidth={1.9} aria-hidden="true" />
                </span>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="truncate text-[15px] font-semibold">{action.label}</span>
                  <span className="truncate text-xs text-mute">{action.hint}</span>
                </span>
                <ChevronRight size={16} className="shrink-0 text-mute-2 transition-transform duration-300 group-hover:translate-x-1 group-hover:text-cream" aria-hidden="true" />
              </button>
            </m.li>
          );
        })}
      </ul>
    </m.section>
  );
}
