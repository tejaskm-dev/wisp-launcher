import { Check, ChevronDown, Search } from "lucide-react";
import { useRef, useState } from "react";
import { cn } from "../../lib/cn";
import { Popover } from "./Popover";

export interface SelectOption<T extends string> {
  value: T;
  label: string;
  /** A short note on the right, e.g. "Latest". */
  note?: string;
}

interface SelectProps<T extends string> {
  /** Names the control for screen readers; show a visible label next to it too. */
  label: string;
  value: T;
  options: SelectOption<T>[];
  onChange: (value: T) => void;
  /** Adds a filter box at the top of the menu, for long lists. */
  searchable?: boolean;
  className?: string;
}

/** A button showing the current choice; opens a menu with a checkmark on the selected option. */
export function Select<T extends string>({ label, value, options, onChange, searchable = false, className }: SelectProps<T>) {
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  const buttonRef = useRef<HTMLButtonElement>(null);
  const current = options.find((option) => option.value === value) ?? options[0];
  const needle = filter.trim().toLowerCase();
  const shown = needle ? options.filter((option) => option.label.toLowerCase().includes(needle)) : options;

  const close = () => {
    setOpen(false);
    setFilter("");
  };

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        aria-label={`${label}: ${current.label}`}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => (open ? close() : setOpen(true))}
        className={cn(
          "flex h-10 items-center justify-between gap-3 rounded-xl border bg-ink-800 pl-3.5 pr-3 text-sm font-semibold transition-colors",
          open ? "border-cream/35 bg-ink-700" : "border-cream/12 hover:border-cream/30",
          className,
        )}
      >
        <span className="truncate">{current.label}</span>
        <ChevronDown size={16} className={cn("shrink-0 text-mute transition-transform duration-200", open && "rotate-180")} aria-hidden="true" />
      </button>
      <Popover open={open} anchorRef={buttonRef} placement="bottom-start" onClose={close} label={label} className="w-[240px]">
        {searchable && (
          <label className="mb-1 flex items-center gap-2 rounded-xl bg-ink-900/60 px-3 text-mute focus-within:text-cream-300">
            <Search size={14} aria-hidden="true" />
            <span className="sr-only">Find</span>
            <input
              data-autofocus
              value={filter}
              onChange={(event) => setFilter(event.target.value)}
              placeholder="Find…"
              spellCheck={false}
              className="h-9 min-w-0 flex-1 bg-transparent text-sm text-cream outline-none placeholder:text-mute"
            />
          </label>
        )}
        <div role="menu" aria-label={label} className="flex max-h-72 flex-col overflow-y-auto">
          {shown.length === 0 && <p className="px-3 py-3 text-sm text-mute">Nothing matches “{filter.trim()}”.</p>}
          {shown.map((option) => {
            const selected = option.value === value;
            return (
              <button
                key={option.value}
                type="button"
                role="menuitemradio"
                aria-checked={selected}
                onClick={() => {
                  onChange(option.value);
                  close();
                  buttonRef.current?.focus();
                }}
                className={cn(
                  "flex w-full items-center gap-2.5 rounded-xl px-3 py-2 text-left text-sm outline-none transition-colors hover:bg-cream/8 focus-visible:bg-cream/8",
                  selected ? "font-bold text-cream" : "font-semibold text-cream-300",
                )}
              >
                <Check size={15} strokeWidth={2.6} className={cn("shrink-0 text-ember", !selected && "invisible")} aria-hidden="true" />
                <span className="min-w-0 flex-1 truncate">{option.label}</span>
                {option.note && <span className="shrink-0 text-xs font-semibold text-mute-2">{option.note}</span>}
              </button>
            );
          })}
        </div>
      </Popover>
    </>
  );
}
