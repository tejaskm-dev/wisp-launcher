import type { ReactNode } from "react";
import { cn } from "../../lib/cn";

export interface ChipOption<T extends string> {
  value: T;
  label: string;
}

/** A single-choice row of filter chips: every option visible, the chosen one filled in. */
export function Chips<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: ChipOption<T>[];
  onChange: (value: T) => void;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="flex flex-wrap gap-1.5">
      {options.map((option) => {
        const selected = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(option.value)}
            className={cn(
              "h-9 rounded-full border px-3.5 text-[13px] font-bold transition-colors duration-200",
              selected ? "border-cream bg-cream text-ink-900" : "border-cream/12 text-cream-300 hover:border-cream/30 hover:text-cream",
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}

/** A visible label beside a filter control, so it's always clear what the control changes. */
export function FilterField({ label, children, className }: { label: string; children: ReactNode; className?: string }) {
  return (
    <div className={cn("flex items-center gap-3", className)}>
      <span className="w-[76px] shrink-0 text-[13px] font-semibold text-mute">{label}</span>
      {children}
    </div>
  );
}
