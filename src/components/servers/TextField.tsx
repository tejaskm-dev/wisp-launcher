import type { ReactNode } from "react";

/** A labelled text box, as Wisp's dialogs use them. */
export function TextField({ label, value, onChange, placeholder, icon, autoFocus, maxLength }: { label: string; value: string; onChange: (value: string) => void; placeholder?: string; icon?: ReactNode; autoFocus?: boolean; maxLength?: number }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[13px] font-bold text-cream-300">{label}</span>
      <span className="flex h-11 items-center gap-2 rounded-xl border border-cream/12 bg-ink-900/60 px-3.5 focus-within:border-cream/35">
        {icon}
        <input
          autoFocus={autoFocus}
          value={value}
          maxLength={maxLength}
          spellCheck={false}
          onChange={(event) => onChange(event.target.value)}
          placeholder={placeholder}
          className="min-w-0 flex-1 bg-transparent text-sm text-cream outline-none placeholder:text-mute"
        />
      </span>
    </label>
  );
}
