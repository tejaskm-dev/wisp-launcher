import { m } from "motion/react";
import { useId, type ReactNode } from "react";
import { cn } from "../../lib/cn";

interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: ReactNode;
  disabled?: boolean;
}

/** A labelled on/off switch row. */
export function Toggle({ checked, onChange, label, description, disabled = false }: ToggleProps) {
  const id = useId();
  return (
    <div className={cn("flex items-center justify-between gap-6 py-3.5", disabled && "opacity-50")}>
      <div className="min-w-0">
        <label htmlFor={id} className="text-[15px] font-semibold">
          {label}
        </label>
        {description && <p className="mt-0.5 text-[13px] leading-relaxed text-mute">{description}</p>}
      </div>
      <button
        id={id}
        type="button"
        role="switch"
        aria-checked={checked}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={cn("relative h-7 w-12 shrink-0 rounded-full transition-colors duration-200", checked ? "bg-cream" : "bg-cream/15 hover:bg-cream/25")}
      >
        <m.span
          aria-hidden="true"
          className={cn("absolute left-1 top-1 size-5 rounded-full shadow-sm transition-colors duration-200", checked ? "bg-ink-900" : "bg-cream")}
          initial={false}
          animate={{ x: checked ? 20 : 0 }}
          transition={{ type: "spring", stiffness: 520, damping: 32 }}
        />
      </button>
    </div>
  );
}
