import { m } from "motion/react";
import { useId, type ReactNode } from "react";
import { cn } from "../../lib/cn";
import { Ghost } from "../brand/Ghost";

interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: ReactNode;
  disabled?: boolean;
}

/**
 * A labelled on/off switch row. The knob is Wisp's ghost: asleep while the setting is off, awake
 * (and blinking now and then) once it's on.
 */
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
        className={cn(
          "relative h-7 w-[52px] shrink-0 rounded-full transition-colors duration-300 disabled:cursor-not-allowed",
          checked ? "bg-moss shadow-[inset_0_1px_0_rgba(255,255,255,0.18)]" : "bg-cream/14 enabled:hover:bg-cream/22",
        )}
      >
        <m.span
          aria-hidden="true"
          className="absolute left-[3px] top-[2px]"
          initial={false}
          animate={{ x: checked ? 23 : 0, rotate: checked ? 0 : -6 }}
          transition={{ type: "spring", stiffness: 520, damping: 30 }}
        >
          <Ghost size={24} sleeping={!checked} className="drop-shadow-[0_2px_3px_rgba(0,0,0,0.45)]" />
        </m.span>
      </button>
    </div>
  );
}
