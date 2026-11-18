import type { Instance, Loader } from "../../data/types";
import { cn } from "../../lib/cn";
import { ICON_COLUMNS, ICON_ROWS, ICON_SHEET, builtinIcon } from "./icons";

// Letter tiles for instances without an icon: a colour per loader.
const LOADER_STYLE: Record<Loader, string> = {
  vanilla: "from-[#6b9b45] to-[#3e6328] text-[#eef7e0]",
  fabric: "from-[#d1ad72] to-[#8d6b3b] text-[#fff6e3]",
  quilt: "from-[#9b7bd0] to-[#5b3f8a] text-[#f3ecff]",
  forge: "from-[#73829b] to-[#3f4859] text-[#eef2f8]",
  neoforge: "from-[#e39147] to-[#a1561d] text-[#fff1e3]",
};

/** One tile of the built-in icon sheet. */
export function SpriteIcon({ index, className }: { index: number; className?: string }) {
  const column = index % ICON_COLUMNS;
  const row = Math.floor(index / ICON_COLUMNS);
  return (
    <span
      aria-hidden="true"
      className={cn("block shrink-0 bg-ink-850", className)}
      style={{
        backgroundImage: `url(${ICON_SHEET})`,
        backgroundSize: `${ICON_COLUMNS * 100}% ${ICON_ROWS * 100}%`,
        backgroundPosition: `${(column / (ICON_COLUMNS - 1)) * 100}% ${(row / (ICON_ROWS - 1)) * 100}%`,
      }}
    />
  );
}

/**
 * An instance's icon: a built-in one, a modpack's image, or a block with its first letter, always
 * in the ghost's outline (a rounded tile with the tail flicking out at the corner).
 */
export function InstanceAvatar({ instance, className }: { instance: Pick<Instance, "name" | "loader"> & { icon?: string | null }; className?: string }) {
  const builtin = builtinIcon(instance.icon);
  if (builtin !== null) {
    return <SpriteIcon index={builtin} className={cn("ghost-shape", className)} />;
  }
  if (instance.icon?.startsWith("https://")) {
    // The ghost's body is the top-left 56/64 of the tile (the rest is where its tail flicks out),
    // so a downloaded picture fits the body rather than being cut off at the right and bottom.
    return (
      <span aria-hidden="true" className={cn("ghost-shape relative block shrink-0 bg-ink-700", className)}>
        <img src={instance.icon} alt="" loading="lazy" decoding="async" className="absolute left-0 top-0 size-[87.5%] object-cover" />
      </span>
    );
  }
  const letter = instance.name.trim().charAt(0).toUpperCase() || "?";
  return (
    <span
      aria-hidden="true"
      className={cn(
        "ghost-shape grid shrink-0 place-items-center bg-linear-to-br pb-[6%] pr-[6%] font-display font-extrabold shadow-[inset_0_-3px_0_rgba(0,0,0,0.25),inset_0_2px_0_rgba(255,255,255,0.18)]",
        LOADER_STYLE[instance.loader],
        className,
      )}
    >
      {letter}
    </span>
  );
}
