import { cn } from "../../lib/cn";

/** Four-point star used as the brand accent. Colour comes from `text-*`. */
export function Sparkle({ className, twinkle = false }: { className?: string; twinkle?: boolean }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className={cn("shrink-0 fill-current", twinkle && "animate-twinkle", className)}>
      <path d="M12 0C12.8 7.2 16.8 11.2 24 12C16.8 12.8 12.8 16.8 12 24C11.2 16.8 7.2 12.8 0 12C7.2 11.2 11.2 7.2 12 0Z" />
    </svg>
  );
}
