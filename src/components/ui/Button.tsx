import type { LucideIcon } from "lucide-react";
import type { ButtonHTMLAttributes } from "react";
import { cn } from "../../lib/cn";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary";
  size?: "md" | "lg";
  icon?: LucideIcon;
}

/** Pill button. The primary one gets a light sweep on hover, like the Play button. */
export function Button({ variant = "primary", size = "md", icon: Icon, className, children, ...props }: ButtonProps) {
  return (
    <button
      type="button"
      {...props}
      className={cn(
        "group relative inline-flex shrink-0 items-center justify-center gap-2.5 overflow-hidden rounded-full font-bold transition-[transform,box-shadow,background-color,border-color] duration-200 ease-out-expo active:scale-[0.97]",
        size === "lg" ? "h-14 px-7 text-[17px]" : "h-11 px-5 text-sm",
        variant === "primary"
          ? "bg-cream text-ink-900 shadow-[0_12px_40px_-8px_rgba(244,238,228,0.35)] hover:scale-[1.03] hover:shadow-[0_16px_48px_-6px_rgba(244,238,228,0.5)]"
          : "border border-cream/20 bg-ink-900/40 text-cream hover:border-cream/40 hover:bg-cream/8",
        className,
      )}
    >
      {variant === "primary" && (
        <span
          aria-hidden="true"
          className="absolute inset-y-0 left-0 w-1/3 bg-linear-to-r from-transparent via-white/70 to-transparent opacity-0 group-hover:animate-shimmer group-hover:opacity-100"
        />
      )}
      {Icon && <Icon size={size === "lg" ? 18 : 16} strokeWidth={2.2} className="relative" aria-hidden="true" />}
      <span className="relative">{children}</span>
    </button>
  );
}
