import { ChevronLeft, ChevronRight } from "lucide-react";
import { cn } from "../../lib/cn";

/** Page numbers to show: always the first and last, the current page and its neighbours. */
function pageList(page: number, pages: number): (number | "gap")[] {
  const wanted = new Set([1, pages, page - 1, page, page + 1]);
  if (page <= 3) [2, 3, 4].forEach((n) => wanted.add(n));
  if (page >= pages - 2) [pages - 1, pages - 2, pages - 3].forEach((n) => wanted.add(n));
  const sorted = [...wanted].filter((n) => n >= 1 && n <= pages).sort((a, b) => a - b);
  const result: (number | "gap")[] = [];
  sorted.forEach((n, index) => {
    if (index > 0 && n - sorted[index - 1] > 1) result.push("gap");
    result.push(n);
  });
  return result;
}

export function Pagination({ page, pages, onChange }: { page: number; pages: number; onChange: (page: number) => void }) {
  if (pages <= 1) return null;
  const step = "flex h-10 items-center gap-1.5 rounded-xl px-3.5 text-sm font-bold transition-colors disabled:opacity-35";

  return (
    <nav aria-label="Pages" className="flex flex-wrap items-center justify-center gap-1.5 pt-2">
      <button type="button" onClick={() => onChange(page - 1)} disabled={page <= 1} className={cn(step, "enabled:hover:bg-cream/8")}>
        <ChevronLeft size={16} aria-hidden="true" />
        Previous
      </button>
      {pageList(page, pages).map((entry, index) =>
        entry === "gap" ? (
          <span key={`gap-${index}`} aria-hidden="true" className="px-1 text-mute-2">
            …
          </span>
        ) : (
          <button
            key={entry}
            type="button"
            onClick={() => onChange(entry)}
            aria-label={`Page ${entry}`}
            aria-current={entry === page ? "page" : undefined}
            className={cn(
              "h-10 min-w-10 rounded-xl px-2 text-sm font-bold tabular-nums transition-colors",
              entry === page ? "bg-cream text-ink-900" : "text-cream-300 hover:bg-cream/8 hover:text-cream",
            )}
          >
            {entry.toLocaleString()}
          </button>
        ),
      )}
      <button type="button" onClick={() => onChange(page + 1)} disabled={page >= pages} className={cn(step, "enabled:hover:bg-cream/8")}>
        Next
        <ChevronRight size={16} aria-hidden="true" />
      </button>
    </nav>
  );
}
