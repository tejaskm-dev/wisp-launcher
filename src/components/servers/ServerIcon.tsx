import { cn } from "../../lib/cn";

/** A server's icon, or the first letter of its name when it has none. */
export function ServerIcon({ icon, name, className }: { icon: string | null | undefined; name: string; className?: string }) {
  if (icon) return <img src={icon} alt="" draggable={false} className={cn("shrink-0 rounded-xl bg-ink-900 object-cover", className)} />;
  return (
    <span aria-hidden="true" className={cn("grid shrink-0 place-items-center rounded-xl bg-cream/8 font-display font-bold text-cream-300", className)}>
      {name.trim().slice(0, 1).toUpperCase() || "?"}
    </span>
  );
}
