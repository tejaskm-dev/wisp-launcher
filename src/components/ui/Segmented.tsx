import { cn } from "../../lib/cn";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
  disabled?: boolean;
  /** Shown as a tooltip, e.g. why an option is disabled. */
  hint?: string;
}

interface SegmentedProps<T extends string> {
  label: string;
  value: T;
  options: SegmentOption<T>[];
  onChange: (value: T) => void;
  className?: string;
}

/** A row of mutually exclusive choices; the chosen one is a little cream ghost, tail and all. */
export function Segmented<T extends string>({ label, value, options, onChange, className }: SegmentedProps<T>) {
  return (
    <div role="radiogroup" aria-label={label} className={cn("inline-flex flex-wrap gap-1 rounded-full border border-cream/10 bg-ink-900/60 p-1", className)}>
      {options.map((option) => {
        const active = option.value === value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={active}
            disabled={option.disabled}
            title={option.hint}
            onClick={() => onChange(option.value)}
            className={cn(
              "relative rounded-full px-3.5 py-1.5 text-[13px] font-bold transition-colors duration-200 disabled:cursor-not-allowed disabled:opacity-40",
              active
                ? "tail bg-cream text-ink-900 shadow-[0_6px_18px_-8px_rgba(244,238,228,0.6)] [--tail:16px]"
                : "text-cream-300 enabled:hover:bg-cream/8 enabled:hover:text-cream",
            )}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
