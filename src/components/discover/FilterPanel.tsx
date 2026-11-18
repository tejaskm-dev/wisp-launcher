import DOMPurify from "dompurify";
import { ChevronDown, Filter, KeyRound, Lock, Monitor, RotateCw, Server, X } from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";
import type { Category, Instance, LoaderTag, VersionList } from "../../data/types";
import { cn } from "../../lib/cn";
import { instanceLabel, loaderName } from "../../lib/format";
import { categoryLabel } from "../../lib/markdown";
import { SOURCE_NAMES, SourceIcon } from "../brand/SourceIcons";
import { InstanceAvatar } from "../instances/InstanceAvatar";
import { Select, type SelectOption } from "../ui/Select";
import { Loading, Skeleton } from "../ui/Skeleton";
import { ANY, CONTENT_TYPES, HEADER_ORDER, LOADER_CHOICES, SOURCES, SOURCE_TABS, type Filters } from "./filters";
import { Sparkle } from "../brand/Sparkle";

interface FilterPanelProps {
  filters: Filters;
  onChange: (patch: Partial<Filters>) => void;
  /** Clears every filter (not the source, the kind of project or the sort). */
  onReset: () => void;
  activeCount: number;
  instances: Instance[];
  /** The instance results are pinned to, if any: it sets the version and loader. */
  instance: Instance | null;
  versions: VersionList | null;
  /** This kind of project's categories; null while they load. */
  categories: Category[] | null;
  categoriesFailed: boolean;
  onRetryCategories: () => void;
  loaderTags: LoaderTag[] | null;
  /** The loaders and categories in effect. */
  loaders: string[];
  chosenCategories: string[];
  /** CurseForge has an API key to search with. */
  curseforgeReady: boolean;
  onSetUpCurseforge: () => void;
}

/** Discover's filter rail, on the right: where to search, what kind, then everything that narrows it. */
export function FilterPanel(props: FilterPanelProps) {
  const { filters, onChange, onReset, activeCount, instances, instance, versions, categories, categoriesFailed, onRetryCategories, loaderTags } = props;
  const { loaders, chosenCategories, curseforgeReady, onSetUpCurseforge } = props;
  const loaderChoice = LOADER_CHOICES[filters.source][filters.tab];
  const moreChosen = (loaderChoice?.more ?? []).some((loader) => loaders.includes(loader));
  const [showMore, setShowMore] = useState(false);
  const icons = useSanitizedIcons(loaderTags, categories);

  const versionOptions: SelectOption<string>[] = [{ value: ANY, label: "Any version" }];
  for (const version of versions?.versions ?? []) {
    const listed = version.kind === "release" || (filters.snapshots && version.kind === "snapshot") || version.id === filters.version;
    if (listed) versionOptions.push({ value: version.id, label: version.id, note: version.id === versions?.latestRelease ? "Latest" : version.kind === "snapshot" ? "Snapshot" : undefined });
  }
  const instanceOptions: SelectOption<string>[] = [
    { value: ANY, label: "Any instance" },
    ...instances.map((candidate) => ({ value: candidate.id, label: candidate.name, note: instanceLabel(candidate) })),
  ];

  const groups = useMemo(() => {
    const byHeader = new Map<string, Category[]>();
    for (const category of categories ?? []) byHeader.set(category.header, [...(byHeader.get(category.header) ?? []), category]);
    return [...byHeader.entries()].sort(([a], [b]) => HEADER_ORDER.indexOf(a) - HEADER_ORDER.indexOf(b));
  }, [categories]);
  const toggleLoader = (loader: string) =>
    onChange({ loaders: loaders.includes(loader) ? filters.loaders.filter((item) => item !== loader) : [...filters.loaders, loader] });

  return (
    <aside
      aria-label="Filters"
      className="flex flex-col overflow-hidden rounded-[22px] border border-cream/8 bg-ink-800/70 lg:sticky lg:top-24 lg:max-h-[calc(100vh-7.5rem)]"
    >
      <div className="flex items-center gap-2 border-b border-cream/7 px-4 py-3.5">
        <Filter size={15} className="text-mute" aria-hidden="true" />
        <h2 className="text-sm font-bold">Filters</h2>
        {activeCount > 0 && (
          <span className="grid h-[18px] min-w-[18px] place-items-center rounded-full bg-coral/20 px-1.5 text-[11px] font-bold tabular-nums text-coral">
            {activeCount}
            <span className="sr-only"> on</span>
          </span>
        )}
        <button
          type="button"
          onClick={onReset}
          disabled={activeCount === 0}
          className="ml-auto flex h-7 items-center gap-1 rounded-full px-2.5 text-xs font-bold text-cream-300 transition-colors enabled:hover:bg-cream/8 enabled:hover:text-cream disabled:opacity-35"
        >
          <X size={13} aria-hidden="true" />
          Clear
        </button>
      </div>

      <div className="flex flex-col gap-6 overflow-y-auto overscroll-contain px-4 pb-5 pt-4">
        {curseforgeReady && (
        <Section title="Source">
          <div role="radiogroup" aria-label="Where to search" className="grid grid-cols-2 gap-1 rounded-2xl border border-cream/8 bg-ink-900/60 p-1">
            {SOURCES.map(({ value }) => {
              const selected = filters.source === value;
              return (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={selected}
                  onClick={() => onChange({ source: value })}
                  className={cn(
                    "flex h-10 items-center justify-center gap-2 rounded-xl text-[13px] font-bold transition-colors duration-150",
                    selected ? "bg-cream/10 text-cream shadow-[inset_0_0_0_1px_rgba(244,238,228,0.12)]" : "text-mute hover:text-cream",
                  )}
                >
                  <SourceIcon source={value} size={16} colored={selected} />
                  {SOURCE_NAMES[value]}
                </button>
              );
            })}
          </div>
          {filters.source === "curseforge" && !curseforgeReady && (
            <button
              type="button"
              onClick={onSetUpCurseforge}
              className="flex items-start gap-2 rounded-xl bg-amber/10 px-3 py-2.5 text-left text-xs leading-snug text-amber-soft transition-colors hover:bg-amber/15"
            >
              <KeyRound size={14} className="mt-px shrink-0" aria-hidden="true" />
              <span>
                CurseForge only answers apps with an API key. <span className="font-bold underline underline-offset-2">Add one in Settings</span>
              </span>
            </button>
          )}
        </Section>
        )}

        <Section title="Content type">
          <div role="radiogroup" aria-label="What to browse" className="flex flex-wrap gap-1.5">
            {SOURCE_TABS[filters.source].map((value) => {
              const { label, icon: Icon } = CONTENT_TYPES[value];
              return (
                <Pill key={value} role="radio" active={filters.tab === value} label={label} icon={<Icon size={14} aria-hidden="true" />} onClick={() => onChange({ tab: value })} />
              );
            })}
          </div>
          {filters.source === "modrinth" && curseforgeReady && <Hint>Worlds (maps) are only on CurseForge.</Hint>}
        </Section>

        {filters.tab !== "modpack" && instances.length > 0 && (
          <Section title="For an instance">
            {instance ? (
              <div className="flex items-center gap-2.5 rounded-xl border border-cream/10 bg-cream/5 p-2">
                <InstanceAvatar instance={instance} className="size-9 text-sm" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[13px] font-bold">{instance.name}</p>
                  <p className="truncate text-xs text-mute">{instanceLabel(instance)}</p>
                </div>
                <button type="button" aria-label="Stop filtering by this instance" onClick={() => onChange({ instanceId: null })} className="rounded-lg p-1.5 text-mute hover:bg-cream/8 hover:text-cream">
                  <X size={14} aria-hidden="true" />
                </button>
              </div>
            ) : (
              <Select label="For an instance" value={ANY} options={instanceOptions} onChange={(value) => onChange({ instanceId: value === ANY ? null : value })} className="w-full" />
            )}
            <Hint>{instance ? "Only shows what runs on its version and loader." : "Pick one to see only what runs on it."}</Hint>
          </Section>
        )}

        <Section title="Game version">
          {instance ? (
            <Locked by={instance.name} value={`Minecraft ${instance.gameVersion}`} />
          ) : (
            <>
              <Select label="Game version" value={filters.version} options={versionOptions} onChange={(version) => onChange({ version })} searchable className="w-full" />
              <CheckRow label="List snapshots too" checked={filters.snapshots} onChange={(snapshots) => onChange({ snapshots })} />
            </>
          )}
        </Section>

        {loaderChoice && (
          <Section title={filters.tab === "shader" ? "Shader loaders" : "Mod loaders"}>
            {instance ? (
              <Locked by={instance.name} value={loaderName(instance.loader)} />
            ) : (
              <>
                <div className="flex flex-wrap gap-1.5">
                  {[...loaderChoice.main, ...(showMore || moreChosen ? loaderChoice.more : [])].map((loader) => (
                    <Pill key={loader} active={loaders.includes(loader)} label={loaderName(loader)} icon={<SvgIcon markup={icons.loaders[loader]} />} onClick={() => toggleLoader(loader)} />
                  ))}
                  {loaderChoice.more.length > 0 && !moreChosen && (
                    <button
                      type="button"
                      aria-expanded={showMore}
                      onClick={() => setShowMore((open) => !open)}
                      className="flex h-8 items-center gap-1 rounded-full px-2.5 text-[13px] font-bold text-mute transition-colors hover:bg-cream/8 hover:text-cream"
                    >
                      {showMore ? "Fewer" : `${loaderChoice.more.length} more`}
                      <ChevronDown size={13} aria-hidden="true" className={cn("transition-transform duration-200", showMore && "rotate-180")} />
                    </button>
                  )}
                </div>
                <Hint>Any of the ones you pick. Wisp's instances run Fabric, Forge, NeoForge and Quilt; the rest are here to browse.</Hint>
              </>
            )}
          </Section>
        )}

        {filters.source === "modrinth" && (filters.tab === "mod" || filters.tab === "modpack") && (
          <Section title="Runs on">
            <div className="flex flex-wrap gap-1.5">
              <Pill
                active={filters.environment === "client"}
                label="Your game"
                icon={<Monitor size={14} aria-hidden="true" />}
                onClick={() => onChange({ environment: filters.environment === "client" ? "any" : "client" })}
              />
              <Pill
                active={filters.environment === "server"}
                label="Servers"
                icon={<Server size={14} aria-hidden="true" />}
                onClick={() => onChange({ environment: filters.environment === "server" ? "any" : "server" })}
              />
            </div>
          </Section>
        )}

        {categoriesFailed ? (
          <Section title="Categories">
            <div className="flex items-center gap-2 rounded-xl bg-cream/5 px-3 py-2.5 text-xs text-mute">
              <span className="min-w-0 flex-1">Couldn't load the categories.</span>
              <button type="button" onClick={onRetryCategories} className="flex items-center gap-1 rounded-lg px-2 py-1 font-bold text-cream-300 hover:bg-cream/8 hover:text-cream">
                <RotateCw size={12} aria-hidden="true" />
                Retry
              </button>
            </div>
          </Section>
        ) : categories === null ? (
          <Section title="Categories">
            <Loading label="Loading categories" className="flex flex-wrap gap-1.5">
              {["w-20", "w-24", "w-16", "w-28", "w-20", "w-14", "w-24"].map((width, index) => (
                <Skeleton key={index} className={cn("h-8 rounded-full", width)} />
              ))}
            </Loading>
          </Section>
        ) : (
          groups.map(([header, items]) => (
            <Section key={header} title={categoryLabel(header)}>
              <div className="flex flex-wrap gap-1.5">
                {items.map((category) => {
                  const active = chosenCategories.includes(category.name);
                  return (
                    <Pill
                      key={category.name}
                      active={active}
                      label={category.label ?? categoryLabel(category.name)}
                      icon={
                        category.iconUrl ? (
                          <img src={category.iconUrl} alt="" loading="lazy" referrerPolicy="no-referrer" className="size-3.5 shrink-0 object-contain" />
                        ) : (
                          <SvgIcon markup={icons.categories[category.name]} />
                        )
                      }
                      onClick={() => onChange({ categories: active ? filters.categories.filter((name) => name !== category.name) : [...filters.categories, category.name] })}
                    />
                  );
                })}
              </div>
              {header === "categories" && <Hint>Every one you pick.</Hint>}
            </Section>
          ))
        )}
      </div>
    </aside>
  );
}

/** Sanitised SVG logos for loaders and categories (they come from Modrinth as markup). */
function useSanitizedIcons(loaderTags: LoaderTag[] | null, categories: Category[] | null) {
  return useMemo(() => {
    const clean = (markup: string) => DOMPurify.sanitize(markup, { USE_PROFILES: { svg: true } });
    return {
      loaders: Object.fromEntries((loaderTags ?? []).map((tag) => [tag.name, clean(tag.icon)])) as Record<string, string>,
      categories: Object.fromEntries((categories ?? []).filter((category) => category.icon).map((category) => [category.name, clean(category.icon)])) as Record<string, string>,
    };
  }, [loaderTags, categories]);
}

function SvgIcon({ markup }: { markup: string | undefined }) {
  if (!markup) return null;
  return <span aria-hidden="true" className="grid size-3.5 shrink-0 place-items-center [&>svg]:size-3.5" dangerouslySetInnerHTML={{ __html: markup }} />;
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2.5">
      <h3 className="text-[11px] font-bold uppercase tracking-[0.13em] text-mute">{title}</h3>
      {children}
    </section>
  );
}

function Pill({ active, label, icon, onClick, role }: { active: boolean; label: string; icon?: ReactNode; onClick: () => void; role?: "radio" }) {
  return (
    <button
      type="button"
      role={role}
      aria-checked={role === "radio" ? active : undefined}
      aria-pressed={role === "radio" ? undefined : active}
      onClick={onClick}
      className={cn(
        "flex h-8 max-w-full items-center gap-1.5 rounded-full border px-3 text-[13px] font-semibold transition-colors duration-150",
        active ? "border-cream/60 bg-cream text-ink-900" : "lit border-cream/10 bg-cream/[0.04] text-cream-300 hover:border-cream/25 hover:bg-cream/8 hover:text-cream",
      )}
    >
      {icon ?? (active && <Sparkle className="-ml-0.5 size-2.5 shrink-0 text-ember" />)}
      <span className="truncate">{label}</span>
    </button>
  );
}

function CheckRow({ label, checked, onChange }: { label: string; checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="flex items-center gap-2 self-start rounded-lg py-0.5 pr-1.5 text-left text-xs font-semibold text-cream-300 transition-colors hover:text-cream"
    >
      <span aria-hidden="true" className={cn("grid size-4 shrink-0 place-items-center rounded-[5px] border transition-colors", checked ? "border-cream bg-cream text-ink-900" : "border-cream/30")}>
        {checked && <Sparkle className="size-2.5 text-ember" />}
      </span>
      {label}
    </button>
  );
}

/** A filter an instance has set, saying so instead of showing a greyed-out control. */
function Locked({ by, value }: { by: string; value: string }) {
  return (
    <div className="flex items-center gap-2.5 rounded-xl bg-cream/5 px-3 py-2.5">
      <Lock size={13} className="shrink-0 text-mute" aria-hidden="true" />
      <p className="min-w-0 text-[13px] leading-snug">
        <span className="font-bold">{value}</span>
        <span className="block truncate text-xs text-mute">Set by {by}</span>
      </p>
    </div>
  );
}

function Hint({ children }: { children: ReactNode }) {
  return <p className="text-xs leading-snug text-mute-2">{children}</p>;
}
