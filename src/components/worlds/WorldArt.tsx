import { Globe } from "lucide-react";
import { fileUrl, thumbUrl } from "../../data/api";
import type { WorldSummary } from "../../data/types";
import { cn } from "../../lib/cn";

/** A soft colour from a world's name, so worlds without pictures still look different. */
export function worldHue(name: string): number {
  let hash = 0;
  for (const char of name) hash = (hash * 31 + char.charCodeAt(0)) | 0;
  return Math.abs(hash) % 360;
}

/**
 * A world's picture: its latest screenshot, else its icon (pixel art, kept crisp over a
 * blurred copy of itself), else a colour from its name with a globe.
 */
export function WorldArt({
  world,
  width = 480,
  className,
  iconClassName,
  zoomOnHover = false,
}: {
  world: Pick<WorldSummary, "name" | "icon" | "cover" | "lastPlayed">;
  /** Thumbnail width to ask for, in pixels. */
  width?: number;
  className?: string;
  iconClassName?: string;
  zoomOnHover?: boolean;
}) {
  const zoom = zoomOnHover && "transition-transform duration-700 ease-out-expo group-hover:scale-[1.07]";
  if (world.cover) {
    return (
      <div className={cn("relative overflow-hidden bg-ink-700", className)}>
        <img src={thumbUrl(world.cover, width, world.lastPlayed)} alt="" loading="lazy" decoding="async" className={cn("size-full object-cover", zoom)} />
      </div>
    );
  }
  if (world.icon) {
    const icon = fileUrl(world.icon, world.lastPlayed);
    return (
      <div className={cn("relative grid place-items-center overflow-hidden bg-ink-700", className)}>
        <img src={icon} alt="" aria-hidden="true" className={cn("absolute inset-0 size-full scale-125 object-cover opacity-70 blur-xl", zoom)} />
        <img
          src={icon}
          alt=""
          loading="lazy"
          className={cn("relative size-16 rounded-xl object-cover shadow-[0_10px_30px_-8px_rgba(0,0,0,0.8)] ring-1 ring-cream/15 [image-rendering:pixelated]", iconClassName)}
        />
      </div>
    );
  }
  const hue = worldHue(world.name);
  return (
    <div
      className={cn("relative grid place-items-center overflow-hidden", className)}
      style={{ background: `radial-gradient(circle at 30% 25%, hsl(${hue} 45% 32%), hsl(${(hue + 40) % 360} 35% 14%) 70%)` }}
    >
      <Globe size={30} strokeWidth={1.4} className="text-cream/45" aria-hidden="true" />
    </div>
  );
}

/** A small square of a world's picture, for pickers and lists. */
export function WorldThumb({ world, className }: { world: Pick<WorldSummary, "name" | "icon" | "cover" | "lastPlayed">; className?: string }) {
  if (world.icon) {
    return <img src={fileUrl(world.icon, world.lastPlayed)} alt="" className={cn("shrink-0 object-cover [image-rendering:pixelated]", className)} />;
  }
  if (world.cover) {
    return <img src={thumbUrl(world.cover, 160, world.lastPlayed)} alt="" className={cn("shrink-0 object-cover", className)} />;
  }
  const hue = worldHue(world.name);
  return (
    <span className={cn("grid shrink-0 place-items-center text-cream/60", className)} style={{ background: `hsl(${hue} 35% 24%)` }}>
      <Globe size={18} aria-hidden="true" />
    </span>
  );
}

const MODES: Record<string, string> = { survival: "Survival", creative: "Creative", adventure: "Adventure", spectator: "Spectator" };

/** "Hardcore", "Creative · Cheats", for badges. */
export function worldMode(world: Pick<WorldSummary, "gameMode" | "hardcore" | "cheats">): string {
  const parts = [world.hardcore ? "Hardcore" : (MODES[world.gameMode] ?? "")];
  if (world.cheats) parts.push("Cheats");
  return parts.filter(Boolean).join(" · ");
}

/** "3 h 20 min", "45 min", "under a minute". */
export function playTime(seconds: number): string {
  const minutes = Math.round(seconds / 60);
  if (minutes < 1) return "under a minute";
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  return minutes % 60 === 0 ? `${hours} h` : `${hours} h ${minutes % 60} min`;
}
