import { Bell, CloudOff, Search, X } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect, useRef, useState } from "react";
import type { SyncStatus } from "../data/types";
import { modKey } from "../lib/platform";
import { Ghost } from "./brand/Ghost";
import { Popover } from "./ui/Popover";

interface TopBarProps {
  /** Null while the library is loading. */
  sync: SyncStatus | null;
  onOpenSync: () => void;
  /** False while the startup intro covers the screen. */
  revealed: boolean;
  /** The Discover search. Typing here from any page opens Discover with the results. */
  query: string;
  onQueryChange: (query: string) => void;
}

export function TopBar({ sync, onOpenSync, revealed, query, onQueryChange }: TopBarProps) {
  const searchRef = useRef<HTMLInputElement>(null);
  const bellRef = useRef<HTMLButtonElement>(null);
  const [inboxOpen, setInboxOpen] = useState(false);

  // ⌘K / Ctrl+K jumps to search from anywhere.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <m.header
      data-tauri-drag-region
      data-top-bar
      initial={false}
      animate={revealed ? { opacity: 1, y: 0 } : { opacity: 0, y: -8 }}
      transition={{ duration: 0.45, ease: "easeOut", delay: revealed ? 0.12 : 0 }}
      className="sticky top-0 z-30 flex items-center gap-4 bg-linear-to-b from-ink-900 from-60% to-ink-900/0 px-7 pb-6 pt-5"
    >
      <div role="search" className="w-full max-w-[560px]">
        <label className="flex h-[46px] items-center gap-3 rounded-full border border-cream/10 bg-ink-800 pl-[18px] pr-2 text-mute transition-colors focus-within:border-cream/30 focus-within:bg-ink-700">
          <Search size={18} aria-hidden="true" />
          <span className="sr-only">Search Modrinth</span>
          <input
            ref={searchRef}
            type="search"
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key !== "Escape") return;
              if (query) onQueryChange("");
              else event.currentTarget.blur();
            }}
            placeholder="Search modpacks, mods and shaders…"
            spellCheck={false}
            className="min-w-0 flex-1 bg-transparent text-[15px] text-cream outline-none placeholder:text-mute [&::-webkit-search-cancel-button]:hidden"
          />
          {query ? (
            <button
              type="button"
              aria-label="Clear search"
              onClick={() => {
                onQueryChange("");
                searchRef.current?.focus();
              }}
              className="grid size-8 place-items-center rounded-full text-mute transition-colors hover:bg-cream/10 hover:text-cream"
            >
              <X size={16} aria-hidden="true" />
            </button>
          ) : (
            <kbd className="rounded-full bg-cream/10 px-2.5 py-1 font-sans text-[13px] font-semibold text-cream-200">{modKey}K</kbd>
          )}
        </label>
      </div>

      <div className="ml-auto flex items-center gap-2">
        <AnimatePresence initial={false}>
          {sync?.state === "off" && (
            <m.button
              type="button"
              onClick={onOpenSync}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -6 }}
              className="flex h-9 items-center gap-2 rounded-full border border-cream/10 px-3.5 text-[13px] font-semibold text-cream-400 transition-colors hover:border-cream/25 hover:text-cream"
            >
              <CloudOff size={14} aria-hidden="true" />
              Sync off
            </m.button>
          )}
        </AnimatePresence>

        <button
          ref={bellRef}
          type="button"
          aria-label="Notifications"
          aria-expanded={inboxOpen}
          onClick={() => setInboxOpen((open) => !open)}
          className="grid size-10 place-items-center rounded-xl text-cream-400 transition-colors hover:bg-cream/5 hover:text-cream aria-expanded:bg-cream/8 aria-expanded:text-cream"
        >
          <Bell size={20} aria-hidden="true" />
        </button>
        <Popover open={inboxOpen} anchorRef={bellRef} placement="bottom-end" onClose={() => setInboxOpen(false)} label="Notifications" className="w-[300px]">
          <div className="flex flex-col items-center gap-1.5 px-5 pb-6 pt-5 text-center">
            <Ghost size={46} sleeping floating className="mb-2" />
            <p className="font-display text-[17px] font-bold">All quiet</p>
            <p className="text-[13px] leading-relaxed text-mute">Downloads, sync and update news will show up here.</p>
          </div>
        </Popover>
      </div>
    </m.header>
  );
}
