import { cn } from "../../lib/cn";

// The sidebar logo and the startup intro's flying logo share these, so the intro can land on the
// sidebar pixel for pixel. Change them together.

export const LOGO_ROW = "relative flex items-center gap-2.5 px-1.5";
export const LOGO_GHOST_SIZE = 40;
/** From the logo's left edge to the ghost's centre: the row's padding plus half the ghost. */
export const LOGO_GHOST_CENTER = 6 + LOGO_GHOST_SIZE / 2;
export const LOGO_SPARKLE = "absolute -top-1.5 left-[46px] size-3.5 text-ember";
export const WORDMARK = "whitespace-nowrap font-display text-[30px] font-extrabold leading-none tracking-[-0.03em]";
export const WORDMARK_LETTERS = ["w", "i", "s", "p"] as const;

/** "wisp", one box per letter so it lines up exactly with the intro's animated letters. */
export function Wordmark({ className }: { className?: string }) {
  return (
    <span className={cn(WORDMARK, className)}>
      {WORDMARK_LETTERS.map((letter, index) => (
        <span key={index} className="inline-block">
          {letter}
        </span>
      ))}
    </span>
  );
}
