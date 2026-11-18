import { Blocks, Database, Image as ImageIcon, Sparkles, type LucideIcon } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useState, type ReactNode } from "react";
import { fileUrl } from "../../data/api";
import type { InstanceItem, ItemKind } from "../../data/types";
import { cn } from "../../lib/cn";
import { easeOutExpo } from "../../lib/motion";

export function message(failure: unknown): string {
  return failure instanceof Error ? failure.message : String(failure);
}

/** What each kind of thing is called. */
export const KIND_NAMES: Record<ItemKind, { one: string; many: string; empty: string; discover: string }> = {
  mod: { one: "mod", many: "Mods", empty: "No mods yet", discover: "Find mods" },
  "resource-pack": { one: "resource pack", many: "Resource packs", empty: "No resource packs yet", discover: "Find resource packs" },
  shader: { one: "shader pack", many: "Shaders", empty: "No shader packs yet", discover: "Find shaders" },
  "data-pack": { one: "data pack", many: "Data packs", empty: "No data packs in its worlds", discover: "Find data packs" },
};

export const KIND_ICONS: Record<ItemKind, LucideIcon> = { mod: Blocks, "resource-pack": ImageIcon, shader: Sparkles, "data-pack": Database };

/** "Sodium", "Sodium and Iris", "Sodium, Iris and Lithium", "Sodium, Iris and 3 more". */
export function listNames(names: string[], shown = 2): string {
  if (names.length <= shown + 1) return names.length <= 1 ? (names[0] ?? "") : `${names.slice(0, -1).join(", ")} and ${names.at(-1)}`;
  return `${names.slice(0, shown).join(", ")} and ${names.length - shown} more`;
}

/** A mod's or pack's picture: Modrinth's, the one in its own file, or a block by kind. */
export function ItemIcon({ item, className }: { item: Pick<InstanceItem, "kind" | "iconUrl" | "iconFile" | "name">; className?: string }) {
  const [broken, setBroken] = useState(false);
  const source = broken ? null : (item.iconUrl ?? (item.iconFile ? fileUrl(item.iconFile) : null));
  if (source) {
    return (
      <img
        src={source}
        alt=""
        loading="lazy"
        decoding="async"
        referrerPolicy="no-referrer"
        onError={() => setBroken(true)}
        className={cn("shrink-0 rounded-xl bg-ink-700 object-cover [image-rendering:pixelated]", className)}
      />
    );
  }
  const Icon = KIND_ICONS[item.kind];
  return (
    <span aria-hidden="true" className={cn("grid shrink-0 place-items-center rounded-xl bg-cream/6 text-cream-300", className)}>
      <Icon size={18} />
    </span>
  );
}

/** A section of the instance page: its icon and title, a line on what it's for, and its controls. */
export function Panel({
  icon: Icon,
  title,
  description,
  action,
  children,
  className,
}: {
  icon?: LucideIcon;
  title: string;
  description?: ReactNode;
  action?: ReactNode;
  children?: ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("rounded-[22px] border border-cream/7 bg-ink-800 p-5", className)}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-3">
          {Icon && (
            <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-cream/6 text-cream-300">
              <Icon size={17} aria-hidden="true" />
            </span>
          )}
          <div className="min-w-0">
            <h3 className="font-display text-lg font-bold leading-tight">{title}</h3>
            {description && <p className="mt-0.5 max-w-[68ch] text-[13px] leading-relaxed text-mute">{description}</p>}
          </div>
        </div>
        {action}
      </div>
      {children && <div className="mt-4">{children}</div>}
    </section>
  );
}

/** What a choice opens slides open (and closed) rather than jumping in. */
export function Reveal({ open, children, className }: { open: boolean; children: ReactNode; className?: string }) {
  return (
    <AnimatePresence initial={false}>
      {open && (
        <m.div
          key="reveal"
          initial={{ height: 0, opacity: 0 }}
          animate={{ height: "auto", opacity: 1, transition: { height: { duration: 0.34, ease: easeOutExpo }, opacity: { duration: 0.24, delay: 0.06 } } }}
          exit={{ height: 0, opacity: 0, transition: { height: { duration: 0.26, ease: easeOutExpo }, opacity: { duration: 0.12 } } }}
          className={cn("overflow-hidden", className)}
        >
          {children}
        </m.div>
      )}
    </AnimatePresence>
  );
}
