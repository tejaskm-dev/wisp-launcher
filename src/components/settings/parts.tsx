import { useEffect, useId, useState, type ReactNode } from "react";
import type { Settings } from "../../data/types";
import type { SettingsPatch } from "../../state/AppContext";
import { cn } from "../../lib/cn";

/** What every settings section gets: the settings, and a way to change them (saved as you go). */
export interface SectionProps {
  settings: Settings;
  update: (patch: SettingsPatch) => void;
}

/** A setting's label and explanation on the left, its control on the right (or below). */
export function Row({ label, description, children, stacked = false }: { label: string; description?: ReactNode; children?: ReactNode; stacked?: boolean }) {
  return (
    <div className={cn("py-3.5", stacked ? "flex flex-col gap-3" : "flex flex-wrap items-center justify-between gap-x-6 gap-y-3")}>
      <div className={cn("min-w-0", !stacked && "max-w-md flex-1")}>
        <p className="text-[15px] font-semibold">{label}</p>
        {description && <div className="mt-0.5 text-[13px] leading-relaxed text-mute">{description}</div>}
      </div>
      {children}
    </div>
  );
}

export function Divider() {
  return <div aria-hidden="true" className="h-px bg-cream/7" />;
}

/** A small explanation under a group of settings. */
export function Note({ children }: { children: ReactNode }) {
  return <p className="mt-2 text-xs leading-relaxed text-mute-2">{children}</p>;
}

/**
 * A text box that saves when you leave it or press Enter, so half-typed values (a path, a command)
 * never take effect.
 */
export function TextField({
  label,
  value,
  onCommit,
  placeholder,
  mono = false,
  secret = false,
  className,
}: {
  label: string;
  value: string;
  onCommit: (value: string) => void;
  placeholder?: string;
  mono?: boolean;
  /** Hide what's typed, like a password. */
  secret?: boolean;
  className?: string;
}) {
  const id = useId();
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  const commit = () => {
    if (draft.trim() !== value) onCommit(draft.trim());
  };
  return (
    <>
      <label htmlFor={id} className="sr-only">
        {label}
      </label>
      <input
        id={id}
        type={secret ? "password" : "text"}
        value={draft}
        placeholder={placeholder}
        spellCheck={false}
        autoCapitalize="off"
        autoCorrect="off"
        onChange={(event) => setDraft(event.target.value)}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") event.currentTarget.blur();
          if (event.key === "Escape") {
            setDraft(value);
            event.currentTarget.blur();
          }
        }}
        className={cn(
          "h-10 w-full min-w-0 rounded-xl border border-cream/12 bg-ink-900/60 px-3.5 text-[13px] text-cream placeholder:text-mute-2 transition-colors hover:border-cream/25 focus:border-cream/40 focus:outline-none",
          mono && "font-mono",
          className,
        )}
      />
    </>
  );
}

/** A whole-number box that saves on leaving it, kept within a range. */
export function NumberField({ label, value, min, max, onCommit, suffix }: { label: string; value: number; min: number; max: number; onCommit: (value: number) => void; suffix?: string }) {
  const id = useId();
  const [draft, setDraft] = useState(String(value));
  useEffect(() => setDraft(String(value)), [value]);
  const commit = () => {
    const parsed = Number.parseInt(draft, 10);
    const next = Number.isNaN(parsed) ? value : Math.min(max, Math.max(min, parsed));
    setDraft(String(next));
    if (next !== value) onCommit(next);
  };
  return (
    <div className="flex items-center gap-2">
      <label htmlFor={id} className="sr-only">
        {label}
      </label>
      <input
        id={id}
        inputMode="numeric"
        value={draft}
        onChange={(event) => setDraft(event.target.value.replace(/[^0-9]/g, ""))}
        onBlur={commit}
        onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
        className="h-10 w-[84px] rounded-xl border border-cream/12 bg-ink-900/60 px-3 text-center text-sm font-semibold tabular-nums text-cream transition-colors hover:border-cream/25 focus:border-cream/40 focus:outline-none"
      />
      {suffix && <span className="text-xs font-semibold text-mute">{suffix}</span>}
    </div>
  );
}

/** A labelled group of choices shown as pills (every option visible, as the UX rules ask). */
export function Choice<T extends string>({ label, value, options, onChange }: { label: string; value: T; options: { value: T; label: string }[]; onChange: (value: T) => void }) {
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
