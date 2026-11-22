import { ArrowDownUp, ArrowUpCircle, Download, FilePlus2, Heart, History, LayoutGrid, List, LoaderCircle, Package, Plus, RotateCcw, Search, Trash2, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  addServerFiles,
  getCategories,
  getProject,
  getProjectVersions,
  getServerContent,
  getServerContentUpdates,
  installServerContent,
  removeServerContent,
  searchServerContent,
  setServerContentEnabled,
  updateServerContent,
} from "../../data/api";
import type { Category, HostedView, Project, ProjectDetails, ProjectVersion, SearchSort, ServerContentUpdate, ServerContentView } from "../../data/types";
import { cn } from "../../lib/cn";
import { formatCount, hexColor, timeAgo } from "../../lib/format";
import { useDebounced } from "../../lib/hooks";
import { categoryLabel, renderMarkdown } from "../../lib/markdown";
import { useResource } from "../../lib/useResource";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { EmptyState } from "../ui/EmptyState";
import { Panel } from "../ui/Page";
import { Pagination } from "../ui/Pagination";
import { Segmented, type SegmentOption } from "../ui/Segmented";
import { Select, type SelectOption } from "../ui/Select";
import { Skeleton } from "../ui/Skeleton";
import { Toggle } from "../ui/Toggle";

const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure));
const PAGE_SIZE = 24;
const SORTS: SelectOption<SearchSort>[] = [
  { value: "relevance", label: "Most relevant" },
  { value: "downloads", label: "Most downloaded" },
  { value: "follows", label: "Most followed" },
  { value: "newest", label: "Newest" },
  { value: "updated", label: "Recently updated" },
];
type View = "grid" | "list";
type Section = "installed" | "browse";

/** The loaders Modrinth lists a server's plugins (or mods) under, for its versions list. */
function loadersFor(server: HostedView): string[] {
  switch (server.software) {
    case "paper":
      return ["paper", "spigot", "bukkit"];
    case "purpur":
      return ["purpur", "paper", "spigot", "bukkit"];
    case "folia":
      return ["folia"];
    case "spigot":
      return ["spigot", "bukkit"];
    default:
      return server.loader === "quilt" ? ["quilt", "fabric"] : [server.loader];
  }
}

/**
 * A server's plugins (Paper and its kin) or mods: what's in it, kept up to date, and a library of
 * everything on Modrinth that runs on it (by category, sorted, page by page, each with its own
 * page). Files you have go in too: picked, or dropped on the page. A server made from an
 * instance takes its mods through the instance, so players get the same.
 */
export function ServerContentTab({ server }: { server: HostedView }) {
  const { notify, openInstance } = useApp();
  const [content, setContent] = useState<ServerContentView | null>(null);
  const [updates, setUpdates] = useState<ServerContentUpdate[]>([]);
  const [section, setSection] = useState<Section | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const load = useCallback(() => {
    getServerContent(server.id).then(setContent, () => setContent({ kind: null, items: [], instanceName: null, instanceId: null }));
    getServerContentUpdates(server.id).then(setUpdates, () => setUpdates([]));
  }, [server.id]);
  useEffect(load, [load]);

  const addFiles = useCallback(
    async (files: string[] | null) => {
      setBusy("files");
      try {
        const added = await addServerFiles(server.id, files);
        if (added.length > 0) notify({ tone: "success", title: added.length === 1 ? `Added ${added[0]}` : `Added ${added.length} files`, body: server.state === "running" ? "Restart the server to load them." : "They load when the server starts." });
        load();
      } catch (failure: unknown) {
        notify({ tone: "info", title: "Couldn't add that", body: message(failure) });
      } finally {
        setBusy(null);
      }
    },
    [server.id, server.state, notify, load],
  );

  if (!content) return <Skeleton className="h-48 w-full rounded-[24px]" />;
  const noun = content.kind === "plugins" ? "plugin" : "mod";
  const fromInstance = content.instanceId !== null;
  const shown: Section = section ?? (content.items.length === 0 && !fromInstance ? "browse" : "installed");
  const sections: SegmentOption<Section>[] = [
    { value: "installed", label: `Installed · ${content.items.length}` },
    { value: "browse", label: "Browse" },
  ];

  const act = async (label: string, work: () => Promise<ServerContentView>, failTitle: string) => {
    setBusy(label);
    try {
      setContent(await work());
      getServerContentUpdates(server.id).then(setUpdates, () => {});
    } catch (failure: unknown) {
      notify({ tone: "info", title: failTitle, body: message(failure) });
    } finally {
      setBusy(null);
    }
  };
  const updateFor = (file: string) => updates.find((update) => update.file === file);

  return (
    <div className="relative flex flex-col gap-5">
      <div className="flex flex-wrap items-center gap-3">
        {!fromInstance && <Segmented label="Show" value={shown} options={sections} onChange={setSection} />}
        <div className="ml-auto flex flex-wrap items-center gap-2">
          {updates.length > 0 && !fromInstance && (
            <Button variant="secondary" icon={ArrowUpCircle} className="h-10 px-4 text-[13px]" disabled={busy !== null} onClick={() => void act("all", () => updateServerContent(server.id), "Couldn't update everything")}>
              {busy === "all" ? "Updating…" : `Update all (${updates.length})`}
            </Button>
          )}
          <Button variant="secondary" icon={FilePlus2} className="h-10 px-4 text-[13px]" disabled={busy !== null} onClick={() => void addFiles(null)}>
            {busy === "files" ? "Adding…" : "Add files"}
          </Button>
        </div>
      </div>

      {shown === "installed" || fromInstance ? (
        <Panel
          title={content.kind === "plugins" ? "Plugins" : "Mods"}
          icon={<Package size={19} aria-hidden="true" />}
          description={
            fromInstance
              ? `They come from ${content.instanceName}: what you add there (or drop here), players get too, and the server follows when it next starts. Only the ones that run on a server are listed.`
              : server.state === "running"
                ? "Changes take effect when the server restarts. Drop .jar files here to add them."
                : "What the server loads when it starts. Drop .jar files here to add them."
          }
        >
          {fromInstance && content.instanceId && (
            <Button variant="secondary" className="mt-1 h-9 self-start px-4 text-xs" onClick={() => openInstance(content.instanceId as string)}>
              Open {content.instanceName}
            </Button>
          )}
          {content.items.length === 0 ? (
            <p className="mt-2 text-sm text-mute">No {noun}s yet.</p>
          ) : (
            <ul className="mt-1 flex flex-col divide-y divide-cream/7">
              {content.items.map((item) => {
                const update = updateFor(item.file);
                return (
                  <li key={item.file} className="flex items-center gap-3 py-2.5">
                    <div className={cn("min-w-0 flex-1", !item.enabled && "opacity-55")}>
                      <p className="truncate text-sm font-bold">{item.name}</p>
                      <p className="truncate text-xs text-mute">{item.version ? `${item.version} · ${item.file}` : item.file}</p>
                    </div>
                    {update && !fromInstance && (
                      <button
                        type="button"
                        disabled={busy !== null}
                        onClick={() => void act(item.file, () => updateServerContent(server.id, [item.file]), `Couldn't update ${item.name}`)}
                        className="flex h-8 shrink-0 items-center gap-1.5 rounded-full bg-moss/12 px-3 text-xs font-bold text-moss-soft transition-colors hover:bg-moss/20 disabled:opacity-50"
                        title={`Update to ${update.version}`}
                      >
                        {busy === item.file ? <LoaderCircle size={12} className="animate-spin" aria-hidden="true" /> : <ArrowUpCircle size={12} aria-hidden="true" />}
                        {update.version}
                      </button>
                    )}
                    {!fromInstance && (
                      <>
                        <Toggle
                          compact
                          checked={item.enabled}
                          label={`${item.enabled ? "Turn off" : "Turn on"} ${item.name}`}
                          disabled={busy !== null}
                          onChange={(enabled) => void act(item.file, () => setServerContentEnabled(server.id, item.file, enabled), `Couldn't change ${item.name}`)}
                        />
                        <button
                          type="button"
                          aria-label={`Remove ${item.name}`}
                          disabled={busy !== null}
                          onClick={() => void act(item.file, () => removeServerContent(server.id, item.file), `Couldn't remove ${item.name}`)}
                          className="grid size-8 place-items-center rounded-full text-mute transition-colors hover:bg-coral/10 hover:text-coral disabled:opacity-40"
                        >
                          <Trash2 size={14} aria-hidden="true" />
                        </button>
                      </>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </Panel>
      ) : (
        content.kind && <Library server={server} noun={noun} installed={content} onInstalled={(next) => (setContent(next), getServerContentUpdates(server.id).then(setUpdates, () => {}))} />
      )}
    </div>
  );
}

/** Everything on Modrinth that runs on this server, the way Discover shows it. */
function Library({ server, noun, installed, onInstalled }: { server: HostedView; noun: string; installed: ServerContentView; onInstalled: (content: ServerContentView) => void }) {
  const { notify, now } = useApp();
  const [query, setQuery] = useState("");
  const term = useDebounced(query.trim(), 300);
  const [sort, setSort] = useState<SearchSort>("downloads");
  const [chosen, setChosen] = useState<string[]>([]);
  const [view, setView] = useState<View>("grid");
  const [page, setPage] = useState(1);
  const [results, setResults] = useState<{ hits: Project[]; total: number } | null>(null);
  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
  const [attempt, setAttempt] = useState(0);
  const [installing, setInstalling] = useState<string | null>(null);
  const [opened, setOpened] = useState<Project | null>(null);
  const [categories] = useResource(getCategories);
  const top = useRef<HTMLDivElement>(null);
  // Modrinth files plugins under the mod categories.
  const shownCategories = useMemo(() => (categories.status === "ready" ? categories.data.filter((category) => category.projectType === "mod" && category.header === "categories") : []), [categories]);

  const key = JSON.stringify([term, sort, chosen]);
  const [pageKey, setPageKey] = useState(key);
  const current = pageKey === key ? page : 1;
  useEffect(() => {
    let active = true;
    setStatus("loading");
    searchServerContent(server.id, term, (current - 1) * PAGE_SIZE, sort, chosen).then(
      (found) => {
        if (!active) return;
        setResults({ hits: found.hits, total: found.total });
        setStatus("ready");
      },
      () => active && setStatus("error"),
    );
    return () => {
      active = false;
    };
    // `key` carries the term, sort and categories.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [server.id, key, current, attempt]);

  const have = new Set(installed.items.map((item) => item.projectId).filter(Boolean));
  const install = async (project: Project) => {
    setInstalling(project.id);
    try {
      onInstalled(await installServerContent(server.id, project.id));
      notify({ tone: "success", title: `Added ${project.title}`, body: server.state === "running" ? "Restart the server to load it. What it needs came along." : "It loads when the server starts. What it needs came along." });
    } catch (failure: unknown) {
      notify({ tone: "info", title: `Couldn't add ${project.title}`, body: message(failure) });
    } finally {
      setInstalling(null);
    }
  };
  const goTo = (next: number) => {
    setPageKey(key);
    setPage(next);
    top.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  const toggleCategory = (name: string) => setChosen((list) => (list.includes(name) ? list.filter((item) => item !== name) : [...list, name]));
  const pages = results ? Math.max(1, Math.ceil(results.total / PAGE_SIZE)) : 1;
  const refreshing = status === "loading" && results !== null;

  return (
    <div ref={top} className="grid scroll-mt-28 items-start gap-5 lg:grid-cols-[minmax(0,1fr)_240px]">
      <section className="flex min-w-0 flex-col gap-4" aria-label={`${noun}s`} aria-busy={status === "loading"}>
        <div className="flex flex-wrap items-center gap-2.5">
          <label className="flex h-10 min-w-[220px] flex-1 items-center gap-2 rounded-xl border border-cream/10 bg-ink-900 px-3 focus-within:border-cream/30">
            <Search size={15} className="shrink-0 text-mute" aria-hidden="true" />
            <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={`Search ${noun}s for ${server.gameVersion}`} aria-label={`Search ${noun}s`} className="min-w-0 flex-1 bg-transparent text-sm text-cream outline-none placeholder:text-mute-2" />
            {query && (
              <button type="button" onClick={() => setQuery("")} aria-label="Clear the search" className="text-mute hover:text-cream">
                <X size={14} aria-hidden="true" />
              </button>
            )}
          </label>
          <span className="flex items-center gap-1.5 text-[13px] font-semibold text-mute">
            <ArrowDownUp size={14} aria-hidden="true" />
            <span className="sr-only">Sort by</span>
          </span>
          <Select label="Sort by" value={sort} options={SORTS} onChange={setSort} className="w-[176px]" />
          <div className="flex rounded-xl border border-cream/10 p-0.5" role="group" aria-label="Layout">
            {(["grid", "list"] as const).map((option) => (
              <button
                key={option}
                type="button"
                aria-pressed={view === option}
                aria-label={option === "grid" ? "Grid" : "List"}
                onClick={() => setView(option)}
                className={cn("grid size-8 place-items-center rounded-lg transition-colors", view === option ? "bg-cream/12 text-cream" : "text-mute hover:text-cream")}
              >
                {option === "grid" ? <LayoutGrid size={15} aria-hidden="true" /> : <List size={15} aria-hidden="true" />}
              </button>
            ))}
          </div>
        </div>
        <p className="-mt-1 flex items-center gap-2 text-sm text-mute" aria-live="polite">
          {results ? (
            <>
              <span className="font-bold tabular-nums text-cream-300">{results.total.toLocaleString()}</span> {noun}s for {server.gameVersion}
              {pages > 1 && (
                <span className="tabular-nums text-mute-2">
                  {" "}
                  · page {current} of {pages.toLocaleString()}
                </span>
              )}
            </>
          ) : (
            "Searching…"
          )}
          {refreshing && <LoaderCircle size={14} className="animate-spin" aria-label="Loading" />}
        </p>

        {status === "error" && !results ? (
          <EmptyState
            title="Couldn't reach Modrinth"
            body="Check your internet connection and try again."
            actions={
              <Button variant="secondary" icon={RotateCcw} onClick={() => setAttempt((count) => count + 1)}>
                Try again
              </Button>
            }
          />
        ) : !results ? (
          <div className={view === "grid" ? "grid gap-3 sm:grid-cols-2 xl:grid-cols-3" : "flex flex-col gap-2.5"}>
            {Array.from({ length: 6 }, (_, index) => (
              <Skeleton key={index} className={view === "grid" ? "h-60 rounded-[22px]" : "h-20 rounded-[18px]"} />
            ))}
          </div>
        ) : results.hits.length === 0 ? (
          <EmptyState title={`No ${noun}s found`} body={term ? `Nothing matches “${term}” for this server's version.` : "Try fewer categories."} />
        ) : (
          <>
            <ul className={cn(view === "grid" ? "grid gap-3 sm:grid-cols-2 xl:grid-cols-3" : "flex flex-col gap-2.5", "transition-opacity duration-200", refreshing && "pointer-events-none opacity-45")}>
              {results.hits.map((project) => (
                <li key={project.id} className="min-w-0">
                  {view === "grid" ? (
                    <Card project={project} now={now} added={have.has(project.id)} installing={installing === project.id} disabled={installing !== null} onOpen={() => setOpened(project)} onAdd={() => void install(project)} />
                  ) : (
                    <Row project={project} now={now} added={have.has(project.id)} installing={installing === project.id} disabled={installing !== null} onOpen={() => setOpened(project)} onAdd={() => void install(project)} />
                  )}
                </li>
              ))}
            </ul>
            <Pagination page={current} pages={pages} onChange={goTo} />
          </>
        )}
      </section>

      <aside className="wisp-surface flex flex-col gap-3 rounded-[24px] p-4 lg:sticky lg:top-24">
        <div className="flex items-center justify-between">
          <h3 className="font-display text-base font-bold">Categories</h3>
          {chosen.length > 0 && (
            <button type="button" onClick={() => setChosen([])} className="text-xs font-bold text-mute hover:text-cream">
              Clear
            </button>
          )}
        </div>
        {categories.status === "loading" ? (
          <Skeleton className="h-64 rounded-xl" />
        ) : (
          <div className="flex flex-wrap gap-1.5 lg:flex-col lg:flex-nowrap lg:gap-0.5">
            {shownCategories.map((category: Category) => {
              const on = chosen.includes(category.name);
              return (
                <button
                  key={category.name}
                  type="button"
                  aria-pressed={on}
                  onClick={() => toggleCategory(category.name)}
                  className={cn(
                    "flex items-center gap-2 rounded-xl px-2.5 py-1.5 text-left text-[13px] font-semibold transition-colors",
                    on ? "bg-cream text-ink-900" : "text-cream-300 hover:bg-cream/8 hover:text-cream",
                  )}
                >
                  {/* Modrinth's own category icons are SVG markup. */}
                  <span aria-hidden="true" className="size-4 shrink-0 [&_svg]:size-4" dangerouslySetInnerHTML={{ __html: category.icon }} />
                  {category.label ?? categoryLabel(category.name)}
                </button>
              );
            })}
          </div>
        )}
        <p className="border-t border-cream/7 pt-3 text-xs leading-relaxed text-mute">Only {noun}s made for this server's software and Minecraft {server.gameVersion} are listed.</p>
      </aside>

      {opened && <ProjectSheet server={server} project={opened} added={have.has(opened.id)} installing={installing === opened.id} onClose={() => setOpened(null)} onAdd={() => void install(opened)} />}
    </div>
  );
}

interface ItemProps {
  project: Project;
  now: number;
  added: boolean;
  installing: boolean;
  disabled: boolean;
  onOpen: () => void;
  onAdd: () => void;
}

function Icon({ project, className }: { project: Project; className?: string }) {
  return project.iconUrl ? (
    <img src={project.iconUrl} alt="" loading="lazy" decoding="async" className={cn("bg-ink-700 object-cover", className)} />
  ) : (
    <span className={cn("grid place-items-center bg-ink-700 font-display font-bold text-cream-300", className)}>{project.title.charAt(0)}</span>
  );
}

function AddButton({ project, added, installing, disabled, onAdd, wide = false }: Pick<ItemProps, "project" | "added" | "installing" | "disabled" | "onAdd"> & { wide?: boolean }) {
  if (added) return <span className={cn("flex h-9 shrink-0 items-center justify-center rounded-full px-4 text-[13px] font-bold text-moss-soft", wide && "w-full bg-moss/10")}>Added</span>;
  return (
    <button
      type="button"
      onClick={onAdd}
      disabled={disabled}
      aria-label={`Add ${project.title}`}
      className={cn("relative z-10 flex h-9 shrink-0 items-center justify-center gap-2 rounded-full bg-cream/10 px-4 text-[13px] font-bold transition-colors hover:bg-cream hover:text-ink-900 disabled:opacity-50", wide && "w-full")}
    >
      {installing ? <LoaderCircle size={14} className="animate-spin" aria-hidden="true" /> : <Plus size={14} aria-hidden="true" />}
      {installing ? "Adding…" : "Add"}
    </button>
  );
}

function Card({ project, now, added, installing, disabled, onOpen, onAdd }: ItemProps) {
  const updated = Date.parse(project.updated);
  const tint = project.color !== null ? hexColor(project.color) : undefined;
  return (
    <article className="wisp-card lift lit group relative flex h-full flex-col">
      <button type="button" onClick={onOpen} aria-label={`Open ${project.title}`} className="absolute inset-0 z-0 rounded-[22px_22px_0_22px]" />
      <div className="pointer-events-none relative h-24 overflow-hidden rounded-t-[22px] bg-ink-700" style={tint ? { backgroundColor: tint } : undefined}>
        {project.imageUrl ? <img src={project.imageUrl} alt="" loading="lazy" decoding="async" className="size-full object-cover" /> : <div className="absolute inset-0 bg-[radial-gradient(circle_at_30%_20%,rgba(255,255,255,0.22),transparent_60%)]" />}
        <div className="absolute inset-0 bg-linear-to-t from-ink-900/80 via-ink-900/10 to-transparent" />
      </div>
      <div className="pointer-events-none relative flex flex-1 flex-col px-4 pb-3">
        <Icon project={project} className="relative -mt-6 mb-2 size-12 rounded-2xl text-lg shadow-lg ring-[3px] ring-ink-800" />
        <div className="flex items-start gap-2">
          <h3 className="line-clamp-1 min-w-0 flex-1 text-[15px] font-bold leading-snug group-hover:underline group-hover:decoration-cream/40 group-hover:underline-offset-4">{project.title}</h3>
          <span className="mt-px flex shrink-0 items-center gap-1 text-xs font-semibold tabular-nums text-mute" title={`${project.downloads.toLocaleString()} downloads`}>
            <Download size={12} aria-hidden="true" />
            {formatCount(project.downloads)}
          </span>
        </div>
        <p className="mt-1.5 line-clamp-2 text-[13px] leading-snug text-cream-400">{project.description}</p>
        <p className="mt-auto flex items-center gap-3 pt-2.5 text-xs text-mute">
          <span className="truncate">by {project.author}</span>
          {!Number.isNaN(updated) && (
            <span className="flex shrink-0 items-center gap-1">
              <History size={12} aria-hidden="true" />
              {timeAgo(updated, now)}
            </span>
          )}
        </p>
      </div>
      <div className="relative z-10 px-4 pb-4">
        <AddButton project={project} added={added} installing={installing} disabled={disabled} onAdd={onAdd} wide />
      </div>
    </article>
  );
}

function Row({ project, now, added, installing, disabled, onOpen, onAdd }: ItemProps) {
  const updated = Date.parse(project.updated);
  return (
    <article className="wisp-surface lit group relative flex items-center gap-4 rounded-[18px] p-3 pr-4 transition-colors duration-200 hover:bg-ink-750">
      <button type="button" onClick={onOpen} aria-label={`Open ${project.title}`} className="absolute inset-0 z-0 rounded-[18px]" />
      <Icon project={project} className="pointer-events-none relative size-12 shrink-0 rounded-[14px] text-lg" />
      <div className="pointer-events-none relative flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-baseline gap-2">
          <h3 className="truncate text-[15px] font-bold group-hover:underline group-hover:decoration-cream/40 group-hover:underline-offset-4">{project.title}</h3>
          <span className="shrink-0 truncate text-xs text-mute">by {project.author}</span>
        </div>
        <p className="line-clamp-1 text-[13px] text-cream-400">{project.description}</p>
        <div className="mt-0.5 flex flex-wrap items-center gap-x-3.5 text-xs text-mute">
          <span className="flex items-center gap-1">
            <Download size={12} aria-hidden="true" />
            {formatCount(project.downloads)}
          </span>
          <span className="flex items-center gap-1">
            <Heart size={12} aria-hidden="true" />
            {formatCount(project.follows)}
          </span>
          {!Number.isNaN(updated) && (
            <span className="flex items-center gap-1">
              <History size={12} aria-hidden="true" />
              {timeAgo(updated, now)}
            </span>
          )}
        </div>
      </div>
      <AddButton project={project} added={added} installing={installing} disabled={disabled} onAdd={onAdd} />
    </article>
  );
}

/** One project's page, in a sheet: what it is, pictures, and its builds for this server. */
function ProjectSheet({ server, project, added, installing, onClose, onAdd }: { server: HostedView; project: Project; added: boolean; installing: boolean; onClose: () => void; onAdd: () => void }) {
  const { now } = useApp();
  const [details, setDetails] = useState<ProjectDetails | null>(null);
  const [versions, setVersions] = useState<ProjectVersion[] | null>(null);
  useEffect(() => {
    let active = true;
    getProject(project.id).then(
      (found) => active && setDetails(found),
      () => {},
    );
    getProjectVersions(project.id, loadersFor(server), [server.gameVersion]).then(
      (found) => active && setVersions(found.slice(0, 5)),
      () => active && setVersions([]),
    );
    return () => {
      active = false;
    };
  }, [project.id, server]);
  const html = useMemo(() => (details ? renderMarkdown(details.body).html : ""), [details]);
  return (
    <Dialog
      open
      wide
      onClose={onClose}
      title={project.title}
      description={`${project.description} · by ${project.author}`}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Close
          </Button>
          {added ? (
            <span className="flex h-11 items-center px-5 text-sm font-bold text-moss-soft">In the server</span>
          ) : (
            <Button icon={installing ? undefined : Plus} disabled={installing} onClick={onAdd}>
              {installing ? "Adding…" : `Add to ${server.name}`}
            </Button>
          )}
        </>
      }
    >
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-mute">
          <span className="flex items-center gap-1">
            <Download size={12} aria-hidden="true" />
            {project.downloads.toLocaleString()} downloads
          </span>
          <span className="flex items-center gap-1">
            <Heart size={12} aria-hidden="true" />
            {project.follows.toLocaleString()} followers
          </span>
          {details?.license && <span>{details.license.name}</span>}
        </div>
        {details && details.gallery.length > 0 && (
          <div className="flex gap-2 overflow-x-auto pb-1">
            {details.gallery.slice(0, 6).map((image) => (
              <img key={image.url} src={image.url} alt={image.title ?? ""} loading="lazy" className="h-28 shrink-0 rounded-xl bg-ink-700 object-cover" />
            ))}
          </div>
        )}
        <section>
          <h3 className="mb-2 text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">Builds for this server</h3>
          {versions === null ? (
            <Skeleton className="h-16 rounded-xl" />
          ) : versions.length === 0 ? (
            <p className="text-sm text-mute">None made for this server's software and Minecraft {server.gameVersion}.</p>
          ) : (
            <ul className="flex flex-col divide-y divide-cream/7 rounded-xl bg-ink-900/50 px-3">
              {versions.map((version) => (
                <li key={version.id} className="flex items-center gap-3 py-2 text-sm">
                  <span className="min-w-0 flex-1 truncate font-semibold">{version.versionNumber}</span>
                  {version.versionType !== "release" && <span className="rounded-full bg-amber/12 px-2 py-0.5 text-[11px] font-bold capitalize text-amber-soft">{version.versionType}</span>}
                  <span className="shrink-0 text-xs text-mute">{timeAgo(Date.parse(version.datePublished), now)}</span>
                </li>
              ))}
            </ul>
          )}
        </section>
        {details ? (
          // Sanitised in renderMarkdown: no scripts, frames, forms or styles survive.
          <div className="prose-wisp min-w-0 rounded-2xl border border-cream/8 bg-ink-900/40 p-5 text-sm" dangerouslySetInnerHTML={{ __html: html || "<p>No description.</p>" }} />
        ) : (
          <Skeleton className="h-48 rounded-2xl" />
        )}
      </div>
    </Dialog>
  );
}
