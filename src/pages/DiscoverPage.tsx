import { ArrowDownUp, Download, Heart, History, KeyRound, LayoutGrid, List, LoaderCircle, PackagePlus, RotateCcw, User, X } from "lucide-react";
import { m } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { Ghost } from "../components/brand/Ghost";
import { SOURCE_NAMES, SourceIcon } from "../components/brand/SourceIcons";
import { FilterPanel } from "../components/discover/FilterPanel";
import { ServerBrowser } from "../components/servers/ServerBrowser";
import { readDiscoverMode, saveDiscoverMode } from "../components/servers/address";
import { Segmented } from "../components/ui/Segmented";
import { ANY, DEFAULT_FILTERS, NOUNS, SOURCE_TABS, loaderChoices, type Filters, type Tab } from "../components/discover/filters";
import { compatibleLoaders } from "../components/install/compat";
import { Button } from "../components/ui/Button";
import { EmptyState } from "../components/ui/EmptyState";
import { ErrorState } from "../components/ui/ErrorState";
import { Page } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { Pagination } from "../components/ui/Pagination";
import { Select, type SelectOption } from "../components/ui/Select";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { getCategories, getCurseforgeCategories, getCurseforgeStatus, getLoaderTags, getVersions, searchProjects } from "../data/api";
import type { Category, Project, ProjectType, SearchSort, Source } from "../data/types";
import { cn } from "../lib/cn";
import { formatCount, hexColor, loaderName, timeAgo } from "../lib/format";
import { useDebounced } from "../lib/hooks";
import { categoryLabel } from "../lib/markdown";
import { fadeUp } from "../lib/motion";
import { useResource } from "../lib/useResource";
import { useApp } from "../state/AppContext";
import { requestSettingsSection } from "../state/settingsSection";

const SORTS: SelectOption<SearchSort>[] = [
  { value: "relevance", label: "Most relevant" },
  { value: "downloads", label: "Most downloaded" },
  { value: "follows", label: "Most followed" },
  { value: "newest", label: "Newest" },
  { value: "updated", label: "Recently updated" },
];

const PAGE_SIZE = 24;
const INSTALLABLE: ProjectType[] = ["modpack", "mod", "resourcepack", "shader", "datapack", "world"];
const GRID = "grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-4";

type View = "grid" | "list";
const VIEW_KEY = "wisp.discoverView";

function readView(): View {
  try {
    return localStorage.getItem(VIEW_KEY) === "list" ? "list" : "grid";
  } catch {
    return "grid";
  }
}

// Remembered while the app is open, so coming back to Discover finds things as you left them.
let rememberedFilters = DEFAULT_FILTERS;
let rememberedPaging = { key: "", page: 1 };

interface Results {
  source: Source;
  tab: Tab;
  hits: Project[];
  total: number;
}

// CurseForge's categories per kind of project, fetched once per run.
const curseforgeCategories = new Map<Tab, Category[]>();

/** Browse Modrinth and CurseForge: results on the left, the filter rail on the right. The search box is the top bar's. */
/** Discover: content from Modrinth and CurseForge, or public servers to join. */
export function DiscoverPage({ query, onClearQuery }: { query: string; onClearQuery: () => void }) {
  const { importFiles } = useApp();
  const [mode, setModeState] = useState(readDiscoverMode);
  const setMode = (next: "content" | "servers") => {
    setModeState(next);
    saveDiscoverMode(next);
  };
  return (
    <Page>
      <PageHeader
        title="Discover"
        subtitle={
          mode === "servers"
            ? "Public servers to join, with who's online right now. Type in the search bar above to narrow them down."
            : "Modpacks, mods, resource packs, shaders, data packs and worlds from Modrinth and CurseForge. Type in the search bar above to find something."
        }
        actions={
          <div className="flex items-center gap-2.5">
            {mode === "content" && (
              <Button variant="secondary" icon={PackagePlus} onClick={() => importFiles()} title="Modpacks (.mrpack or CurseForge zip), mods (.jar) and resource or shader packs you already have">
                Add from file
              </Button>
            )}
            <Segmented
              label="Discover"
              value={mode}
              onChange={setMode}
              options={[
                { value: "content", label: "Mods & more" },
                { value: "servers", label: "Servers" },
              ]}
            />
          </div>
        }
      />
      {mode === "servers" ? <ServerBrowser query={query} onClearQuery={onClearQuery} /> : <ContentBrowser query={query} onClearQuery={onClearQuery} />}
    </Page>
  );
}

function ContentBrowser({ query, onClearQuery }: { query: string; onClearQuery: () => void }) {
  const { comingSoon, now, home, openProject, requestInstall, settings, navigate } = useApp();
  const [filters, setFilterState] = useState(rememberedFilters);
  const setFilters = (patch: Partial<Filters>) =>
    setFilterState((current) => {
      const next = { ...current, ...patch };
      // Worlds are only on CurseForge: switching away lands on modpacks.
      if (!SOURCE_TABS[next.source].includes(next.tab)) next.tab = "modpack";
      return next;
    });
  const [view, setViewState] = useState<View>(readView);
  const setView = (next: View) => {
    setViewState(next);
    try {
      localStorage.setItem(VIEW_KEY, next);
    } catch {
      // Only a convenience; the grid comes back next time.
    }
  };
  const [versions] = useResource(getVersions);
  const [categories, retryCategories] = useResource(getCategories);
  const [loaderTags] = useResource(getLoaderTags);
  const [curseforge] = useResource(getCurseforgeStatus);
  const curseforgeReady = (curseforge.status === "ready" && curseforge.data.builtIn) || Boolean(settings?.curseforgeApiKey.trim());
  const blocked = filters.source === "curseforge" && !curseforgeReady;

  // CurseForge's categories differ per kind; fetch them when that kind is first shown.
  const [curseCategories, setCurseCategories] = useState<Category[] | null>(() => curseforgeCategories.get(filters.tab) ?? null);
  const [curseCategoriesFailed, setCurseCategoriesFailed] = useState(false);
  const [curseAttempt, setCurseAttempt] = useState(0);
  useEffect(() => {
    if (filters.source !== "curseforge" || !curseforgeReady) return;
    const known = curseforgeCategories.get(filters.tab);
    if (known) {
      setCurseCategories(known);
      setCurseCategoriesFailed(false);
      return;
    }
    let active = true;
    setCurseCategories(null);
    setCurseCategoriesFailed(false);
    getCurseforgeCategories(filters.tab).then(
      (found) => {
        curseforgeCategories.set(filters.tab, found);
        if (active) setCurseCategories(found);
      },
      () => active && setCurseCategoriesFailed(true),
    );
    return () => {
      active = false;
    };
  }, [filters.source, filters.tab, curseforgeReady, curseAttempt]);

  const instance = filters.tab === "modpack" ? null : (home?.instances.find((candidate) => candidate.id === filters.instanceId) ?? null);
  const term = useDebounced(query.trim(), 300);
  // An instance pins the version and the loaders that fit it; otherwise the rail's choices.
  const choices = loaderChoices(filters.source, filters.tab);
  const loaders = instance
    ? (compatibleLoaders(instance.loader, filters.tab) ?? []).filter((loader) => choices.includes(loader))
    : filters.loaders.filter((loader) => choices.includes(loader));
  const gameVersion = instance ? instance.gameVersion : filters.version === ANY ? null : filters.version;
  const environment =
    filters.source === "modrinth" && (filters.tab === "mod" || filters.tab === "modpack") ? (filters.environment === "any" ? null : filters.environment) : null;
  // Modrinth's data packs use the mod categories.
  const modrinthType = filters.tab === "datapack" ? "mod" : filters.tab;
  const tabCategories =
    filters.source === "curseforge"
      ? blocked
        ? []
        : curseCategories
      : categories.status === "ready"
        ? categories.data.filter((category) => category.projectType === modrinthType)
        : null;
  const categoriesFailed = filters.source === "curseforge" ? !blocked && curseCategoriesFailed : categories.status === "error";
  const retryTabCategories = () => (filters.source === "curseforge" ? setCurseAttempt((count) => count + 1) : retryCategories());
  const chosenCategories = filters.categories.filter((name) => tabCategories?.some((category) => category.name === name));
  const categoryName = (name: string) => {
    const found = tabCategories?.find((category) => category.name === name);
    return found?.label ?? categoryLabel(name);
  };
  const searchKey = JSON.stringify([filters.source, filters.tab, filters.sort, loaders, chosenCategories, gameVersion, environment, term]);

  // Any change to the search starts again from page 1.
  const [paging, setPaging] = useState(rememberedPaging);
  const page = paging.key === searchKey ? paging.page : 1;
  useEffect(() => {
    rememberedFilters = filters;
    rememberedPaging = { key: searchKey, page };
  }, [filters, searchKey, page]);

  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
  const [results, setResults] = useState<Results | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [error, setError] = useState("");
  useEffect(() => {
    if (blocked) return;
    let active = true;
    setStatus("loading");
    const [source, tab, sort, chosenLoaders, chosen, version, side, text] = JSON.parse(searchKey) as [
      Source,
      Tab,
      SearchSort,
      string[],
      string[],
      string | null,
      "client" | "server" | null,
      string,
    ];
    searchProjects({
      source,
      query: text,
      projectType: tab,
      sort,
      loaders: chosenLoaders,
      categories: chosen,
      gameVersion: version,
      environment: side,
      offset: (page - 1) * PAGE_SIZE,
      limit: PAGE_SIZE,
    }).then(
      (found) => {
        if (!active) return;
        setResults({ source, tab, hits: found.hits, total: found.total });
        setStatus("ready");
      },
      (failure: unknown) => {
        if (!active) return;
        setError(failure instanceof Error ? failure.message : String(failure));
        setStatus("error");
      },
    );
    return () => {
      active = false;
    };
  }, [searchKey, page, attempt, blocked]);

  const resultsRef = useRef<HTMLElement>(null);
  const goToPage = (next: number) => {
    setPaging({ key: searchKey, page: next });
    resultsRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const shown = results && results.tab === filters.tab && results.source === filters.source ? results : null;
  const refreshing = status === "loading" && shown !== null;
  const pages = shown ? Math.max(1, Math.ceil(shown.total / PAGE_SIZE)) : 1;
  const [one, many] = NOUNS[filters.tab];

  // Every active filter as a removable chip, in plain words.
  const chips: { key: string; label: string; clear: () => void }[] = [];
  if (term) chips.push({ key: "query", label: `“${term}”`, clear: onClearQuery });
  if (instance) chips.push({ key: "instance", label: `Runs on ${instance.name}`, clear: () => setFilters({ instanceId: null }) });
  if (!instance) {
    if (gameVersion) chips.push({ key: "version", label: `Minecraft ${gameVersion}`, clear: () => setFilters({ version: ANY }) });
    for (const loader of loaders) chips.push({ key: `loader-${loader}`, label: loaderName(loader), clear: () => setFilters({ loaders: filters.loaders.filter((item) => item !== loader) }) });
  }
  if (environment) chips.push({ key: "environment", label: environment === "client" ? "Runs in your game" : "Runs on servers", clear: () => setFilters({ environment: "any" }) });
  for (const name of chosenCategories) chips.push({ key: `category-${name}`, label: categoryName(name), clear: () => setFilters({ categories: filters.categories.filter((item) => item !== name) }) });
  const activeFilters = chips.length - (term ? 1 : 0);
  const resetFilters = () => setFilters({ loaders: [], categories: [], version: ANY, environment: "any", instanceId: null });
  const setUpCurseforge = () => {
    requestSettingsSection("sources");
    navigate("settings");
  };
  const clearAll = () => {
    onClearQuery();
    resetFilters();
  };
  const install = (project: Project) => {
    // Modrinth lists data packs as mods; the tab says how to install them.
    const kind: ProjectType = filters.tab === "datapack" ? "datapack" : project.projectType;
    if (INSTALLABLE.includes(kind)) requestInstall({ projectId: project.id, title: project.title, projectType: kind, iconUrl: project.iconUrl });
    else comingSoon(`Installing ${project.title}`);
  };
  const heading = filters.tab === "modpack" ? "Modpacks" : NOUNS[filters.tab][1].replace(/^./, (first) => first.toUpperCase());

  return (
    <>

      <m.div variants={fadeUp} className="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_288px]">
        <section ref={resultsRef} aria-label="Results" aria-busy={status === "loading"} className="flex min-w-0 scroll-mt-28 flex-col gap-4">
          <div className="flex min-h-10 flex-wrap items-center gap-x-3 gap-y-2">
            <div className="flex min-w-0 items-baseline gap-2" aria-live="polite">
              <h2 className="flex items-center gap-2 font-display text-[22px] font-bold tracking-[-0.01em]">
                <SourceIcon source={filters.source} size={18} colored />
                <span className="sr-only">{SOURCE_NAMES[filters.source]} </span>
                {heading}
              </h2>
              <p className="truncate text-sm text-mute">
                {blocked ? (
                  "CurseForge isn't connected yet"
                ) : shown ? (
                  <>
                    <span className="font-bold tabular-nums text-cream-300">{shown.total.toLocaleString()}</span> {shown.total === 1 ? "result" : "results"}
                    {pages > 1 && <span className="tabular-nums text-mute-2"> · page {page.toLocaleString()} of {pages.toLocaleString()}</span>}
                  </>
                ) : (
                  "Searching…"
                )}
              </p>
              {refreshing && <LoaderCircle size={14} className="shrink-0 animate-spin self-center text-mute" aria-label="Loading" />}
            </div>
            <div className="ml-auto flex items-center gap-2.5">
              <span className="flex items-center gap-1.5 text-[13px] font-semibold text-mute">
                <ArrowDownUp size={14} aria-hidden="true" />
                Sort by
              </span>
              <Select label="Sort by" value={filters.sort} options={SORTS} onChange={(sort) => setFilters({ sort })} className="w-[184px]" />
              <ViewSwitch value={view} onChange={setView} />
            </div>
          </div>

          {chips.length > 0 && (
            <div className="-mt-1 flex flex-wrap items-center gap-2">
              {chips.map((chip) => (
                <button
                  key={chip.key}
                  type="button"
                  onClick={chip.clear}
                  aria-label={`Remove filter: ${chip.label}`}
                  className="flex h-8 items-center gap-1.5 rounded-full bg-cream/8 pl-3 pr-2 text-[13px] font-bold text-cream transition-colors hover:bg-cream/14"
                >
                  {chip.label}
                  <X size={13} aria-hidden="true" />
                </button>
              ))}
              {chips.length > 1 && (
                <button
                  type="button"
                  onClick={clearAll}
                  className="flex h-8 items-center gap-1.5 rounded-full px-2.5 text-[13px] font-bold text-cream-300 transition-colors hover:bg-cream/8 hover:text-cream"
                >
                  <RotateCcw size={13} aria-hidden="true" />
                  Clear all
                </button>
              )}
            </div>
          )}

          {blocked ? (
            <EmptyState
              art={<KeyRound size={40} className="text-amber-soft" aria-hidden="true" />}
              title="CurseForge needs an API key"
              body="CurseForge only answers apps that send a key. They're free: sign in at console.curseforge.com, create a key, and paste it into Settings → Mod sources. Modrinth needs nothing."
              actions={
                <div className="flex flex-wrap justify-center gap-2">
                  <button type="button" onClick={setUpCurseforge} className="rounded-full bg-cream px-5 py-2.5 text-sm font-bold text-ink-900">
                    Add a key
                  </button>
                  <button
                    type="button"
                    onClick={() => setFilters({ source: "modrinth" })}
                    className="flex items-center gap-2 rounded-full border border-cream/15 px-5 py-2.5 text-sm font-bold text-cream-300 hover:border-cream/35 hover:text-cream"
                  >
                    <SourceIcon source="modrinth" size={15} colored />
                    Browse Modrinth
                  </button>
                </div>
              }
            />
          ) : status === "error" ? (
            <ErrorState
              title={filters.source === "curseforge" ? "Couldn't search CurseForge" : "Couldn't reach Modrinth"}
              body={error.includes("API key") ? error : "Check your internet connection and try again."}
              onRetry={() => setAttempt((count) => count + 1)}
            />
          ) : !shown ? (
            <ResultsLoading view={view} />
          ) : shown.hits.length === 0 && !refreshing ? (
            <EmptyState
              art={<Ghost size={64} floating className="drop-shadow-[0_0_18px_rgba(244,238,228,0.25)]" />}
              title={term ? `No ${many} match “${term}”` : `No ${many} match these filters`}
              body="Try fewer filters or a different word."
              actions={
                chips.length > 0 ? (
                  <button type="button" onClick={clearAll} className="rounded-full bg-cream px-5 py-2.5 text-sm font-bold text-ink-900">
                    Clear filters
                  </button>
                ) : undefined
              }
            />
          ) : (
            <>
              <ul className={cn(view === "grid" ? GRID : "flex flex-col gap-2.5", "transition-opacity duration-200", refreshing && "pointer-events-none opacity-45")}>
                {shown.hits.map((project, index) => (
                  <li key={project.id} className="min-w-0 animate-card-in" style={{ animationDelay: `${Math.min(index, 12) * 25}ms` }}>
                    {view === "grid" ? (
                      <ProjectCard project={project} source={shown.source} one={one} now={now} onOpen={() => openProject(project.id)} onInstall={() => install(project)} />
                    ) : (
                      <ProjectRow project={project} source={shown.source} now={now} onOpen={() => openProject(project.id)} onInstall={() => install(project)} />
                    )}
                  </li>
                ))}
              </ul>
              <Pagination page={page} pages={pages} onChange={goToPage} />
            </>
          )}
        </section>

        <FilterPanel
          filters={filters}
          onChange={setFilters}
          onReset={resetFilters}
          activeCount={activeFilters}
          instances={home?.instances ?? []}
          instance={instance}
          versions={versions.status === "ready" ? versions.data : null}
          categories={categoriesFailed ? [] : tabCategories}
          categoriesFailed={categoriesFailed}
          onRetryCategories={retryTabCategories}
          loaderTags={loaderTags.status === "ready" ? loaderTags.data : null}
          loaders={loaders}
          chosenCategories={chosenCategories}
          curseforgeReady={curseforgeReady}
          onSetUpCurseforge={setUpCurseforge}
        />
      </m.div>
    </>
  );
}

/** Grid or list, as two icon buttons with their names for screen readers and on hover. */
function ViewSwitch({ value, onChange }: { value: View; onChange: (view: View) => void }) {
  const options: { value: View; label: string; icon: typeof LayoutGrid }[] = [
    { value: "grid", label: "Show as a grid", icon: LayoutGrid },
    { value: "list", label: "Show as a list", icon: List },
  ];
  return (
    <div role="radiogroup" aria-label="Layout" className="flex gap-0.5 rounded-xl border border-cream/12 bg-ink-800 p-0.5">
      {options.map(({ value: option, label, icon: Icon }) => (
        <button
          key={option}
          type="button"
          role="radio"
          aria-checked={value === option}
          aria-label={label}
          title={label}
          onClick={() => onChange(option)}
          className={cn(
            "grid size-[34px] place-items-center rounded-[10px] transition-colors duration-150",
            value === option ? "bg-cream text-ink-900" : "text-mute hover:bg-cream/8 hover:text-cream",
          )}
        >
          <Icon size={16} aria-hidden="true" />
        </button>
      ))}
    </div>
  );
}

/** Where a project comes from, as a small badge on its picture. */
function SourceBadge({ source }: { source: Source }) {
  return (
    <span
      title={`From ${SOURCE_NAMES[source]}`}
      className="absolute right-2.5 top-2.5 grid size-8 place-items-center rounded-full bg-ink-900/75 ring-1 ring-cream/10 backdrop-blur-sm"
    >
      <SourceIcon source={source} size={16} colored />
      <span className="sr-only">From {SOURCE_NAMES[source]}</span>
    </span>
  );
}

function ProjectIcon({ project, className }: { project: Project; className?: string }) {
  return project.iconUrl ? (
    <img src={project.iconUrl} alt="" loading="lazy" decoding="async" className={cn("bg-ink-700 object-cover", className)} />
  ) : (
    <span className={cn("grid place-items-center bg-ink-700 font-display font-bold text-cream-300", className)}>{project.title.charAt(0)}</span>
  );
}

interface ProjectViewProps {
  project: Project;
  source: Source;
  now: number;
  onOpen: () => void;
  onInstall: () => void;
}

function ProjectCard({ project, source, one, now, onOpen, onInstall }: ProjectViewProps & { one: string }) {
  const updated = Date.parse(project.updated);
  const tint = project.color !== null ? hexColor(project.color) : undefined;
  const installLabel = project.projectType === "modpack" ? "Install" : "Add";

  return (
    <article className="wisp-card lift lit group relative flex h-full flex-col">
      {/* The whole card opens the project's page; the install button sits above it. */}
      <button type="button" onClick={onOpen} aria-label={`Open ${project.title}`} className="absolute inset-0 z-0 rounded-[22px_22px_0_22px]" />
      <div className="pointer-events-none relative h-32 overflow-hidden rounded-t-[22px] bg-ink-700" style={tint ? { backgroundColor: tint } : undefined}>
        {project.imageUrl ? (
          <img src={project.imageUrl} alt="" loading="lazy" decoding="async" className="size-full object-cover transition-transform duration-700 ease-out-expo group-hover:scale-[1.04]" />
        ) : (
          <div className="absolute inset-0 bg-[radial-gradient(circle_at_30%_20%,rgba(255,255,255,0.22),transparent_60%)]" />
        )}
        <div className="absolute inset-0 bg-linear-to-t from-ink-900/80 via-ink-900/10 to-transparent" />
        <SourceBadge source={source} />
      </div>

      <div className="pointer-events-none relative flex flex-1 flex-col px-4 pb-3">
        <ProjectIcon project={project} className="relative -mt-7 mb-2.5 size-14 rounded-2xl text-xl shadow-lg ring-[3px] ring-ink-800" />
        <div className="flex items-start gap-2">
          <h3 className="line-clamp-1 min-w-0 flex-1 text-[15px] font-bold leading-snug group-hover:underline group-hover:decoration-cream/40 group-hover:underline-offset-4">
            {project.title}
          </h3>
          <span className="mt-px flex shrink-0 items-center gap-1 text-xs font-semibold tabular-nums text-mute" title={`${project.downloads.toLocaleString()} downloads`}>
            <Download size={12} aria-hidden="true" />
            {formatCount(project.downloads)}
            <span className="sr-only"> downloads</span>
          </span>
        </div>
        <p className="mt-2 line-clamp-2 text-[13px] leading-snug text-cream-400">{project.description}</p>

        <div className="mt-auto flex flex-wrap items-center gap-x-3 gap-y-1 pt-3 text-xs text-mute">
          <span className="flex min-w-0 items-center gap-1">
            <User size={12} className="shrink-0" aria-hidden="true" />
            <span className="truncate">{project.author}</span>
          </span>
          {!Number.isNaN(updated) && (
            <span className="flex items-center gap-1" title="Last updated">
              <History size={12} aria-hidden="true" />
              {timeAgo(updated, now)}
            </span>
          )}
        </div>
        {project.loaders.length > 0 && <p className="mt-1 truncate text-xs font-semibold text-mute-2">{project.loaders.map(loaderName).join(" · ")}</p>}
      </div>
      <div className="relative z-10 px-4 pb-4">
        <button
          type="button"
          onClick={onInstall}
          aria-label={`${installLabel} ${project.title} (${one})`}
          className="tail relative flex h-9 w-full items-center justify-center gap-2 rounded-full bg-cream/10 text-[13px] font-bold transition-colors [--tail:18px] hover:bg-cream hover:text-ink-900"
        >
          <Download size={14} aria-hidden="true" />
          {installLabel}
        </button>
      </div>
    </article>
  );
}

/** The compact layout: one row per project. */
function ProjectRow({ project, source, now, onOpen, onInstall }: ProjectViewProps) {
  const updated = Date.parse(project.updated);
  const installLabel = project.projectType === "modpack" ? "Install" : "Add";
  return (
    <article className="wisp-surface lit group relative flex items-center gap-4 rounded-[18px] p-3 pr-4 transition-colors duration-200 hover:bg-ink-750">
      <button type="button" onClick={onOpen} aria-label={`Open ${project.title}`} className="absolute inset-0 z-0 rounded-[18px]" />
      <ProjectIcon project={project} className="pointer-events-none relative size-14 shrink-0 rounded-[14px] text-xl" />
      <div className="pointer-events-none relative flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-baseline gap-2">
          <h3 className="truncate text-[15px] font-bold group-hover:underline group-hover:decoration-cream/40 group-hover:underline-offset-4">{project.title}</h3>
          <span className="shrink-0 truncate text-xs text-mute">by {project.author}</span>
        </div>
        <p className="line-clamp-1 text-[13px] text-cream-400">{project.description}</p>
        <div className="mt-0.5 flex flex-wrap items-center gap-x-3.5 gap-y-1 text-xs text-mute">
          <span className="flex items-center gap-1" title={`${project.downloads.toLocaleString()} downloads`}>
            <Download size={12} aria-hidden="true" />
            {formatCount(project.downloads)}
          </span>
          <span className="flex items-center gap-1" title={`${project.follows.toLocaleString()} followers`}>
            <Heart size={12} aria-hidden="true" />
            {formatCount(project.follows)}
          </span>
          {!Number.isNaN(updated) && (
            <span className="flex items-center gap-1" title="Last updated">
              <History size={12} aria-hidden="true" />
              {timeAgo(updated, now)}
            </span>
          )}
          {project.loaders.length > 0 && <span className="truncate font-semibold text-mute-2">{project.loaders.map(loaderName).join(" · ")}</span>}
        </div>
      </div>
      <span className="pointer-events-none relative hidden shrink-0 items-center gap-1.5 text-xs font-semibold text-mute sm:flex" title={`From ${SOURCE_NAMES[source]}`}>
        <SourceIcon source={source} size={14} colored />
        {SOURCE_NAMES[source]}
      </span>
      <button
        type="button"
        onClick={onInstall}
        className="relative z-10 flex h-9 shrink-0 items-center gap-2 rounded-full bg-cream/10 px-4 text-[13px] font-bold transition-colors hover:bg-cream hover:text-ink-900"
      >
        <Download size={14} aria-hidden="true" />
        {installLabel}
      </button>
    </article>
  );
}

function ResultsLoading({ view }: { view: View }) {
  if (view === "list") {
    return (
      <Loading label="Loading results" className="flex flex-col gap-2.5">
        {Array.from({ length: 8 }, (_, index) => (
          <div key={index} className="wisp-surface flex items-center gap-4 rounded-[18px] p-3">
            <Skeleton className="size-14 shrink-0 rounded-[14px]" />
            <div className="flex flex-1 flex-col gap-2">
              <Skeleton className="h-4 w-1/3 rounded-full" />
              <Skeleton className="h-3 w-3/4 rounded-full" />
              <Skeleton className="h-3 w-1/4 rounded-full" />
            </div>
            <Skeleton className="h-9 w-20 rounded-full" />
          </div>
        ))}
      </Loading>
    );
  }
  return (
    <Loading label="Loading results" className={GRID}>
      {Array.from({ length: 8 }, (_, index) => (
        <div key={index} className="wisp-card">
          <Skeleton className="h-32 w-full rounded-t-[22px]" />
          <div className="flex flex-col gap-2 px-4 pb-4">
            <Skeleton className="-mt-7 mb-1 size-14 rounded-2xl ring-[3px] ring-ink-800" />
            <Skeleton className="h-4 w-3/4 rounded-full" />
            <Skeleton className="mt-1 h-3 w-full rounded-full" />
            <Skeleton className="h-3 w-4/5 rounded-full" />
            <Skeleton className="mt-2 h-3 w-2/5 rounded-full" />
            <Skeleton className="mt-3 h-9 w-full rounded-full" />
          </div>
        </div>
      ))}
    </Loading>
  );
}
