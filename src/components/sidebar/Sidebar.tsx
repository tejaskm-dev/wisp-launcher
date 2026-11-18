import { testerBuild } from "../../data/api";
import { LoaderCircle, PanelLeftClose, PanelLeftOpen, Play, Plus, Settings } from "lucide-react";
import { m, type Variants } from "motion/react";
import { useEffect, useState, type FocusEvent, type PointerEvent } from "react";
import { cn } from "../../lib/cn";
import { useScrollEdges } from "../../lib/hooks";
import { instanceLabel, timeAgo } from "../../lib/format";
import { easeOutExpo } from "../../lib/motion";
import { isMac, modKey } from "../../lib/platform";
import { useApp } from "../../state/AppContext";
import { Ghost } from "../brand/Ghost";
import { LOGO_GHOST_SIZE, LOGO_ROW, LOGO_SPARKLE, Wordmark } from "../brand/Logo";
import { Sparkle } from "../brand/Sparkle";
import { InstanceAvatar } from "../instances/InstanceAvatar";
import { InstanceLabel } from "../instances/VersionTag";
import { describeLaunch } from "../launch/describe";
import { Loading, Skeleton } from "../ui/Skeleton";
import { AccountButton } from "./AccountButton";
import { NAV_ITEMS, navParent } from "./nav";
import { RailTooltip, type RailTip, type TipFor } from "./RailTooltip";

const FULL_WIDTH = 248;
/** Wide enough for the 52px controls plus the 16px padding either side. */
const RAIL_WIDTH = 84;
/** Row height (46px) plus the 4px gap: how far the active pill moves per item. */
const ITEM_STEP = 50;
const RECENT_LIMIT = 3;

const navList: Variants = {
  hidden: {},
  show: { transition: { staggerChildren: 0.035, delayChildren: 0.08 } },
};

const navItem: Variants = {
  hidden: { opacity: 0, x: -10 },
  show: { opacity: 1, x: 0, transition: { duration: 0.45, ease: easeOutExpo } },
};

/** Labels fade out (but keep their place) when the sidebar collapses to icons. */
const fade = (collapsed: boolean) => cn("transition-opacity duration-200", collapsed && "opacity-0");

interface SidebarProps {
  /** False while the startup intro covers the screen. */
  revealed: boolean;
  /** Hidden until the intro's flying logo lands on this one. */
  logoVisible: boolean;
}

export function Sidebar({ revealed, logoVisible }: SidebarProps) {
  const { page, navigate, home, settings, updateSettings } = useApp();
  const collapsed = settings?.sidebarCollapsed ?? false;
  const [tip, setTip] = useState<RailTip | null>(null);
  const middle = useScrollEdges<HTMLDivElement>("y");
  const middleMask =
    middle.edges.start && middle.edges.end
      ? "linear-gradient(to bottom, transparent, black 28px, black calc(100% - 44px), transparent)"
      : middle.edges.end
        ? "linear-gradient(to bottom, black calc(100% - 44px), transparent)"
        : middle.edges.start
          ? "linear-gradient(to bottom, transparent, black 28px)"
          : undefined;

  // A tooltip left over from before the sidebar changed size would point at the wrong place.
  useEffect(() => setTip(null), [collapsed]);

  const showTip = (element: HTMLElement, label: string, hint?: string) => {
    const rect = element.getBoundingClientRect();
    setTip({ label, hint, left: RAIL_WIDTH + 6, centerY: rect.top + rect.height / 2 });
  };
  const hideTip = () => setTip(null);
  const tipFor: TipFor = (label, hint) =>
    collapsed
      ? {
          onPointerEnter: (event: PointerEvent<HTMLElement>) => showTip(event.currentTarget, label, hint),
          onPointerLeave: hideTip,
          onFocus: (event: FocusEvent<HTMLElement>) => showTip(event.currentTarget, label, hint),
          onBlur: hideTip,
        }
      : {};

  // A project's page belongs to Discover, a world's to Worlds.
  const activeIndex = NAV_ITEMS.findIndex((item) => item.id === navParent(page));
  const toggle = () => updateSettings((current) => ({ sidebarCollapsed: !current.sidebarCollapsed }));

  return (
    <>
      <m.aside
        aria-label="Sidebar"
        initial={false}
        animate={{ opacity: revealed ? 1 : 0 }}
        transition={{ duration: 0.45, ease: "easeOut" }}
        style={{ width: collapsed ? RAIL_WIDTH : FULL_WIDTH }}
        className={cn(
          "relative flex shrink-0 flex-col overflow-hidden border-r border-cream/5 bg-ink-950",
          // Only animate the width once the app is showing, not when settings first load.
          revealed && "transition-[width] duration-300 ease-out-expo",
          isMac ? "pt-13" : "pt-7",
        )}
      >
        {/* Lets the window be dragged from the top strip (macOS draws its traffic lights here). */}
        <div data-tauri-drag-region className="absolute inset-x-0 top-0 h-11" />

        <div className="px-4">
          {/* data-logo: the startup intro measures this and lands its flying logo here. */}
          <a
            href="#home"
            aria-label="Wisp home"
            data-logo
            onClick={(event) => {
              event.preventDefault();
              navigate("home");
            }}
            style={{ opacity: logoVisible ? 1 : 0 }}
            className={cn(LOGO_ROW, "group w-fit rounded-xl")}
          >
            <Ghost size={LOGO_GHOST_SIZE} className="transition-transform duration-500 ease-out-expo group-hover:-rotate-8 group-hover:scale-105" />
            <Wordmark className={fade(collapsed)} />
            <Sparkle className={LOGO_SPARKLE} />
          </a>
          {testerBuild && !collapsed && (
            <p className="mt-1.5 w-fit rounded-full bg-ember/15 px-2.5 py-0.5 text-[11px] font-bold text-ember-soft" title="Offline accounts are on, and it doesn't update itself. For people who own Minecraft.">
              Tester build
            </p>
          )}
        </div>

        {/* The middle scrolls on short windows; the account row stays put. Its scrollbar is hidden,
            so an edge with more beyond it fades out instead. */}
        <div
          ref={middle.ref}
          style={{ maskImage: middleMask, WebkitMaskImage: middleMask }}
          className="no-scrollbar mt-6 flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto overflow-x-hidden px-4 pb-6"
        >
          <m.nav aria-label="Main" variants={navList} initial="hidden" animate={revealed ? "show" : "hidden"} className="relative flex flex-col gap-1">
            {activeIndex >= 0 && (
              <m.span
                aria-hidden="true"
                className="tail absolute inset-x-0 top-0 h-[46px] rounded-full bg-cream shadow-[0_8px_24px_-8px_rgba(244,238,228,0.45)] [--tail:26px]"
                initial={false}
                animate={{ y: activeIndex * ITEM_STEP }}
                transition={{ type: "spring", stiffness: 480, damping: 36 }}
              />
            )}
            {NAV_ITEMS.map((item, index) => {
              const active = item.id === navParent(page);
              const Icon = item.icon;
              const count = item.id === "instances" ? home?.instances.length : item.id === "worlds" ? home?.worlds.length : undefined;
              const shortcut = `${modKey}${index + 1}`;
              return (
                <m.a
                  key={item.id}
                  variants={navItem}
                  href={`#${item.id}`}
                  aria-current={active ? "page" : undefined}
                  aria-keyshortcuts={`${isMac ? "Meta" : "Control"}+${index + 1}`}
                  onClick={(event) => {
                    event.preventDefault();
                    navigate(item.id);
                  }}
                  {...tipFor(item.label, shortcut)}
                  className={cn(
                    "group relative z-10 flex h-[46px] items-center gap-3.5 rounded-full px-4 text-[15px] font-semibold transition-colors duration-200",
                    active ? "text-ink-900" : "lit text-cream-400 hover:bg-cream/5 hover:text-cream",
                  )}
                >
                  <span className="relative shrink-0">
                    <Icon size={20} strokeWidth={active ? 2.1 : 1.8} aria-hidden="true" />
                  </span>
                  <span className={cn("min-w-0 flex-1 overflow-hidden whitespace-nowrap", fade(collapsed))}>{item.label}</span>
                  {/* Right side: the count, swapped for the shortcut on hover. */}
                  <span className={cn("grid shrink-0 justify-items-end text-[13px]", fade(collapsed))}>
                    <span
                      className={cn(
                        "col-start-1 row-start-1 tabular-nums transition-opacity duration-150 group-hover:opacity-0",
                        active ? "text-ink-900/60" : "text-mute-2",
                      )}
                    >
                      {count !== undefined && count > 0 ? count : null}
                    </span>
                    <kbd
                      className={cn(
                        "col-start-1 row-start-1 font-sans font-semibold opacity-0 transition-opacity duration-150 group-hover:opacity-100",
                        active ? "text-ink-900/55" : "text-mute-2",
                      )}
                    >
                      {shortcut}
                    </kbd>
                  </span>
                </m.a>
              );
            })}
          </m.nav>

          <RecentInstances collapsed={collapsed} tipFor={tipFor} revealed={revealed} />
        </div>

        <div className="relative flex flex-col gap-3 border-t border-cream/5 px-4 pb-4 pt-3">
          {/* Wisp's ghost keeps watch over the account row. */}
          <Ghost
            size={38}
            floating
            className={cn(
              "pointer-events-none absolute -top-7 right-5 z-10 drop-shadow-[0_0_14px_rgba(244,238,228,0.28)] transition-opacity duration-200",
              collapsed && "opacity-0",
            )}
          />
          <div className={cn("flex gap-1", collapsed ? "flex-col items-center" : "items-center")}>
            <AccountButton collapsed={collapsed} />
            <button
              type="button"
              aria-label="Settings"
              aria-current={page === "settings" ? "page" : undefined}
              aria-keyshortcuts={isMac ? "Meta+," : "Control+,"}
              title={collapsed ? undefined : `Settings (${modKey},)`}
              onClick={() => navigate("settings")}
              {...tipFor("Settings", `${modKey},`)}
              className={cn(
                "group/gear relative grid size-10 shrink-0 place-items-center rounded-xl transition-colors duration-200",
                page === "settings" ? "tail bg-cream text-ink-900 [--tail:18px]" : "text-mute hover:bg-cream/6 hover:text-cream",
              )}
            >
              <Settings size={18} className="transition-transform duration-500 ease-out-expo group-hover/gear:rotate-45" aria-hidden="true" />
            </button>
            <button
              type="button"
              aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
              aria-keyshortcuts={isMac ? "Meta+B" : "Control+B"}
              title={collapsed ? undefined : `Collapse sidebar (${modKey}B)`}
              onClick={toggle}
              {...tipFor("Expand sidebar", `${modKey}B`)}
              className="grid size-10 shrink-0 place-items-center rounded-xl text-mute transition-colors duration-200 hover:bg-cream/6 hover:text-cream"
            >
              {collapsed ? <PanelLeftOpen size={18} aria-hidden="true" /> : <PanelLeftClose size={18} aria-hidden="true" />}
            </button>
          </div>
        </div>
      </m.aside>

      <RailTooltip tip={collapsed ? tip : null} />
    </>
  );
}

/** Quick-launch list of the most recently used instances. */
function RecentInstances({ collapsed, tipFor, revealed }: { collapsed: boolean; tipFor: TipFor; revealed: boolean }) {
  const { home, now, openNewInstance, launches, play } = useApp();
  const recent = home ? home.instances.slice(0, RECENT_LIMIT) : null;

  return (
    <m.section
      aria-labelledby="recent-title"
      initial={false}
      animate={{ opacity: revealed ? 1 : 0 }}
      transition={{ duration: 0.5, delay: revealed ? 0.3 : 0 }}
      className="flex flex-col gap-1"
    >
      <div className="relative flex h-6 items-center px-3">
        <h2 id="recent-title" className={cn("text-[11px] font-bold uppercase tracking-[0.16em] text-mute-2", fade(collapsed))}>
          Recent
        </h2>
        <span aria-hidden="true" className={cn("absolute left-3 top-1/2 h-px w-7 bg-cream/12 transition-opacity duration-200", !collapsed && "opacity-0")} />
      </div>

      {recent === null ? (
        <Loading label="Loading recent instances" className="flex flex-col gap-1">
          {[0, 1].map((index) => (
            <div key={index} className="flex items-center gap-3 p-1.5">
              <Skeleton className="size-10 shrink-0 rounded-xl" />
              <div className="flex flex-1 flex-col gap-2">
                <Skeleton className="h-3.5 w-24 rounded-full" />
                <Skeleton className="h-3 w-16 rounded-full" />
              </div>
            </div>
          ))}
        </Loading>
      ) : recent.length === 0 ? (
        <button
          type="button"
          onClick={openNewInstance}
          {...tipFor("New instance")}
          className="group flex items-center gap-3 rounded-2xl border border-dashed border-cream/15 p-1.5 text-left transition-colors duration-200 hover:border-cream/35 hover:bg-cream/4"
        >
          <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-cream/6 text-cream-300 transition-transform duration-300 ease-out-expo group-hover:rotate-90">
            <Plus size={18} aria-hidden="true" />
          </span>
          <span className={cn("flex min-w-0 flex-col whitespace-nowrap", fade(collapsed))}>
            <span className="text-sm font-bold">New instance</span>
            <span className="text-xs text-mute">Pick a version to play</span>
          </span>
        </button>
      ) : (
        <ul className="flex flex-col gap-1">
          {recent.map((instance) => {
            const view = describeLaunch(launches[instance.id], now);
            const active = view.kind === "busy" || view.kind === "running";
            const status =
              view.kind === "idle" ? (
                <>
                  <InstanceLabel instance={instance} />
                  {instance.lastPlayed !== null ? ` · ${timeAgo(instance.lastPlayed, now)}` : ""}
                </>
              ) : view.kind === "problem" ? (
                "Didn't start · try again"
              ) : (
                view.label
              );
            return (
              <li key={instance.id}>
                <button
                  type="button"
                  aria-label={active ? `${instance.name}: ${view.label}` : `Play ${instance.name}`}
                  aria-disabled={active}
                  onClick={active ? undefined : () => play(instance.id)}
                  {...tipFor(active ? `${instance.name}: ${view.label}` : `Play ${instance.name}`, instanceLabel(instance))}
                  className="lit group flex w-full items-center gap-3 rounded-2xl p-1.5 text-left transition-colors duration-200 hover:bg-cream/5"
                >
                  <span className="relative shrink-0">
                    <InstanceAvatar instance={instance} className="size-10 text-base" />
                    {view.kind === "busy" ? (
                      <span className="ghost-shape absolute inset-0 grid place-items-center bg-ink-950/65 text-cream">
                        <LoaderCircle size={16} className="animate-spin" aria-hidden="true" />
                      </span>
                    ) : view.kind === "running" ? (
                      <span aria-hidden="true" className="absolute -right-0.5 -top-0.5 size-3 rounded-full border-2 border-ink-950 bg-moss" />
                    ) : (
                      <span className="ghost-shape absolute inset-0 grid place-items-center bg-ink-950/60 text-cream opacity-0 transition-opacity duration-200 group-hover:opacity-100 group-focus-visible:opacity-100">
                        <Play size={15} fill="currentColor" strokeWidth={0} aria-hidden="true" />
                      </span>
                    )}
                  </span>
                  <span className={cn("flex min-w-0 flex-1 flex-col whitespace-nowrap", fade(collapsed))}>
                    <span className="truncate text-sm font-bold">{instance.name}</span>
                    <span
                      className={cn(
                        "truncate text-xs",
                        view.kind === "running" ? "font-semibold text-moss-soft" : view.kind === "problem" ? "text-coral" : "text-mute",
                      )}
                    >
                      {status}
                    </span>
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </m.section>
  );
}
