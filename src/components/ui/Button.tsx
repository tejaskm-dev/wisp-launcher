import type { LucideIcon } from "lucide-react";
import type { ButtonHTMLAttributes } from "react";
import { cn } from "../../lib/cn";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary";
  size?: "md" | "lg";
  icon?: LucideIcon;
}

/**
 * Pill button. The primary one is shaped like Wisp's ghost: its tail flicks out at the bottom
 * right and wags when you point at it, while a light sweeps across. The secondary one is an
 * outline a wisp of light follows.
 */
export function Button({ variant = "primary", size = "md", icon: Icon, className, children, ...props }: ButtonProps) {
  const primary = variant === "primary";
  return (
    <button
      type="button"
      {...props}
      className={cn(
        "group relative inline-flex shrink-0 items-center justify-center gap-2.5 rounded-full font-bold transition-[transform,box-shadow,background-color,border-color,opacity] duration-200 ease-out-expo enabled:active:scale-[0.97] disabled:cursor-not-allowed disabled:opacity-45",
        size === "lg" ? "h-14 px-7 text-[17px] [--tail:30px]" : "h-11 px-5 text-sm [--tail:24px]",
        primary
          ? "tail wag bg-cream text-ink-900 shadow-[0_12px_40px_-8px_rgba(244,238,228,0.35)] enabled:hover:scale-[1.03] enabled:hover:shadow-[0_16px_48px_-6px_rgba(244,238,228,0.5)]"
          : "lit border border-cream/20 bg-ink-900/40 text-cream enabled:hover:border-cream/40 enabled:hover:bg-cream/8",
        className,
      )}
    >
      {primary && (
        <span aria-hidden="true" className="pointer-events-none absolute inset-0 overflow-hidden rounded-[inherit]">
          <span className="absolute inset-y-0 left-0 w-1/3 bg-linear-to-r from-transparent via-white/70 to-transparent opacity-0 group-enabled:group-hover:animate-shimmer group-enabled:group-hover:opacity-100" />
        </span>
      )}
      {Icon && <Icon size={size === "lg" ? 18 : 16} strokeWidth={2.2} className="relative" aria-hidden="true" />}
      <span className="relative">{children}</span>
    </button>
  );
}
