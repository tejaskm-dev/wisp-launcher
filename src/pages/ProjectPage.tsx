import { ArrowLeft, ChevronDown, Download, ExternalLink, Heart, History, ImageOff, Plus, Scale } from "lucide-react";
import { m } from "motion/react";
import { useMemo, useState, type MouseEvent, type ReactNode } from "react";
import { loaderList } from "../components/install/compat";
import { Chips } from "../components/ui/Chips";
import { Dialog } from "../components/ui/Dialog";
import { ErrorState } from "../components/ui/ErrorState";
import { Page } from "../components/ui/Page";
import { Segmented } from "../components/ui/Segmented";
import { Select, type SelectOption } from "../components/ui/Select";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { Toggle } from "../components/ui/Toggle";
import { getProject, getProjectVersions, openExternal, projectUrl } from "../data/api";
import type { PageId, ProjectDetails, ProjectType, ProjectVersion } from "../data/types";
import { cn } from "../lib/cn";
import { formatBytes, formatCount, formatDay, hexColor, loaderName, timeAgo } from "../lib/format";
import { categoryLabel, renderMarkdown } from "../lib/markdown";
import { fadeUp } from "../lib/motion";
import { useResource } from "../lib/useResource";
import { useApp } from "../state/AppContext";

type Tab = "overview" | "gallery" | "versions";

const BACK_LABELS: Partial<Record<PageId, string>> = {
  discover: "Discover",
  home: "Home",
  instances: "Instances",
};

const TYPE_LABELS: Partial<Record<ProjectType, string>> = {
  modpack: "Modpack",
  mod: "Mod",
  resourcepack: "Resource pack",
  shader: "Shader",
};

const INSTALLABLE: ProjectType[] = ["modpack", "mod", "resourcepack", "shader", "datapack", "world"];

/** How to install a project or one of its versions: Modrinth lists data packs as mods. */
function installKind(projectType: ProjectType, loaders: string[]): ProjectType {
  return loaders.length > 0 && loaders.every((loader) => loader === "datapack") ? "datapack" : projectType;
}

const INSTALL_LABEL: Partial<Record<ProjectType, string>> = { modpack: "Install modpack", datapack: "Add to a world", world: "Add to an instance" };
const RELEASE_VERSION = /^\d+\.\d+(\.\d+)?$/;
const VERSIONS_PER_PAGE = 30;

/** Opens links from rendered Markdown in the browser, never inside Wisp. */
function openLinksExternally(event: MouseEvent<HTMLElement>) {
  const link = (event.target as HTMLElement).closest("a");
  if (!link) return;
  event.preventDefault();
  const href = link.getAttribute("href") ?? "";
  if (/^https?:\/\//i.test(href)) void openExternal(href);
}

/** "26.1 – 26.3" for the releases a project supports. */
function versionRange(gameVersions: string[]): string {
  const releases = gameVersions.filter((version) => RELEASE_VERSION.test(version));
  const list = releases.length ? releases : gameVersions;
  if (list.length === 0) return "Any";
  return list.length === 1 ? list[0] : `${list[0]} – ${list[list.length - 1]}`;
}

export function ProjectPage({ projectId, backTo, onBack }: { projectId: string; backTo: PageId; onBack: () => void }) {
  const [project, retry] = useResource(() => getProject(projectId));
  const [tab, setTab] = useState<Tab>("overview");

  return (
    <Page className="gap-6">
      <m.button
        variants={fadeUp}
        type="button"
        onClick={onBack}
        className="group flex w-fit items-center gap-2 rounded-full py-1 pr-3 text-sm font-bold text-cream-300 transition-colors hover:text-cream"
      >
        <ArrowLeft size={16} className="transition-transform group-hover:-translate-x-0.5" aria-hidden="true" />
        Back to {BACK_LABELS[backTo] ?? "Discover"}
      </m.button>

      {project.status === "loading" ? (
        <ProjectLoading />
      ) : project.status === "error" ? (
        <ErrorState title="Couldn't load this project" body="Check your connection and try again." onRetry={retry} />
      ) : (
        <ProjectBody project={project.data} tab={tab} onTab={setTab} />
      )}
    </Page>
  );
}

function ProjectBody({ project, tab, onTab }: { project: ProjectDetails; tab: Tab; onTab: (tab: Tab) => void }) {
  const { now, requestInstall } = useApp();
  const banner = project.gallery.find((image) => image.featured) ?? project.gallery[0];
  const tint = project.color !== null ? hexColor(project.color) : undefined;
  const kind = installKind(project.projectType, project.loaders);
  const installable = INSTALLABLE.includes(kind);
  const install = (as: ProjectType = kind) => requestInstall({ projectId: project.id, title: project.title, projectType: as, iconUrl: project.iconUrl });
  // Some projects ship as both a mod and a data pack (Terralith, say).
  const alsoDatapack = kind !== "datapack" && project.loaders.includes("datapack");
  const web = projectUrl(project);

  return (
    <>
      <m.header variants={fadeUp} className="overflow-hidden rounded-[26px] border border-cream/8 bg-ink-800">
        <div className="relative h-44 overflow-hidden bg-ink-700" style={tint ? { backgroundColor: tint } : undefined}>
          {banner && <img src={banner.rawUrl} alt="" decoding="async" className="size-full object-cover" />}
          <div className="absolute inset-0 bg-linear-to-t from-ink-800 via-ink-800/30 to-transparent" />
        </div>
        <div className="relative flex flex-wrap items-end gap-x-6 gap-y-4 px-6 pb-6">
          {project.iconUrl ? (
            <img src={project.iconUrl} alt="" className="-mt-12 size-24 shrink-0 rounded-[22px] bg-ink-700 object-cover shadow-xl ring-4 ring-ink-800" />
          ) : (
            <span className="-mt-12 grid size-24 shrink-0 place-items-center rounded-[22px] bg-ink-700 font-display text-4xl font-bold ring-4 ring-ink-800">
              {project.title.charAt(0)}
            </span>
          )}
          <div className="min-w-0 flex-1">
            <p className="text-xs font-bold uppercase tracking-[0.14em] text-ember-soft">{TYPE_LABELS[project.projectType] ?? "Project"}</p>
            <h1 className="mt-1 font-display text-[34px] font-extrabold leading-tight tracking-[-0.02em]">{project.title}</h1>
            {project.authors.length > 0 && <p className="text-sm text-mute">by {project.authors.slice(0, 3).join(", ")}</p>}
          </div>
          <div className="flex flex-wrap items-center gap-2.5">
            {installable && (
              <button
                type="button"
                onClick={() => install()}
                className="flex h-12 items-center gap-2 rounded-full bg-cream px-6 text-[15px] font-bold text-ink-900 shadow-[0_12px_36px_-10px_rgba(244,238,228,0.45)] transition-transform hover:scale-[1.03] active:scale-[0.97]"
              >
                <Plus size={17} strokeWidth={2.4} aria-hidden="true" />
                {INSTALL_LABEL[kind] ?? "Add to instance"}
              </button>
            )}
            {alsoDatapack && (
              <button
                type="button"
                onClick={() => install("datapack")}
                className="flex h-12 items-center gap-2 rounded-full border border-cream/20 px-5 text-sm font-bold transition-colors hover:border-cream/40 hover:bg-cream/8"
              >
                As a data pack
              </button>
            )}
            <button
              type="button"
              onClick={() => void openExternal(web.url)}
              className="flex h-12 items-center gap-2 rounded-full border border-cream/20 px-5 text-sm font-bold transition-colors hover:border-cream/40 hover:bg-cream/8"
            >
              {web.site}
              <ExternalLink size={14} aria-hidden="true" />
            </button>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-x-6 gap-y-2 border-t border-cream/8 px-6 py-3.5 text-sm text-mute">
          <p className="min-w-0 flex-1 basis-80 text-cream-300">{project.description}</p>
          <Stat icon={<Download size={15} aria-hidden="true" />} label="Downloads" value={formatCount(project.downloads)} />
          <Stat icon={<Heart size={15} aria-hidden="true" />} label="Followers" value={formatCount(project.followers)} />
          <Stat icon={<History size={15} aria-hidden="true" />} label="Updated" value={timeAgo(Date.parse(project.updated), now)} />
        </div>
      </m.header>

      <m.div variants={fadeUp}>
        <Segmented
          label="Project sections"
          value={tab}
          onChange={onTab}
          options={[
            { value: "overview", label: "Overview" },
            { value: "gallery", label: `Gallery${project.gallery.length ? ` (${project.gallery.length})` : ""}`, disabled: project.gallery.length === 0 },
            { value: "versions", label: "Versions" },
          ]}
        />
      </m.div>

      {tab === "overview" && <Overview project={project} />}
      {tab === "gallery" && <Gallery project={project} />}
      {tab === "versions" && <Versions project={project} />}
    </>
  );
}

function Stat({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <span className="flex items-center gap-1.5" title={label}>
      {icon}
      <span className="sr-only">{label}:</span>
      <span className="font-semibold text-cream-300">{value}</span>
    </span>
  );
}

function Overview({ project }: { project: ProjectDetails }) {
  const { settings, navigate } = useApp();
  const externalImages = settings?.externalImages ?? true;
  const { html, blockedImages } = useMemo(() => renderMarkdown(project.body, { externalImages }), [project.body, externalImages]);
  const side = (value: string) => ({ required: "Required", optional: "Optional", unsupported: "Not needed" })[value] ?? "Unknown";
  const showsLoaders = project.projectType !== "resourcepack";

  return (
    <m.div variants={fadeUp} initial="hidden" animate="show" className="grid items-start gap-6 lg:grid-cols-[minmax(0,1fr)_280px]">
      <div className="flex min-w-0 flex-col gap-3">
        {blockedImages > 0 && (
          <p className="flex flex-wrap items-center gap-x-2 gap-y-1 rounded-2xl border border-cream/8 bg-ink-800/70 px-4 py-3 text-[13px] text-mute">
            <ImageOff size={15} aria-hidden="true" className="shrink-0" />
            {blockedImages === 1 ? "1 image from another website is hidden." : `${blockedImages} images from other websites are hidden.`}
            <button type="button" onClick={() => navigate("settings")} className="font-bold text-cream-300 underline decoration-cream/30 underline-offset-4 hover:text-cream">
              Privacy settings
            </button>
          </p>
        )}
        <article
          onClick={openLinksExternally}
          className="prose-wisp min-w-0 rounded-[24px] border border-cream/8 bg-ink-800 p-6"
          // Sanitised in renderMarkdown: no scripts, frames, forms or styles survive.
          dangerouslySetInnerHTML={{ __html: html || "<p>No description.</p>" }}
        />
      </div>
      <aside className="flex flex-col gap-4 lg:sticky lg:top-24">
        <SideCard title="Works with">
          {showsLoaders && project.loaders.length > 0 && (
            <div className="flex flex-wrap gap-1.5">
              {project.loaders
                .filter((loader) => loader !== "minecraft")
                .map((loader) => (
                  <span key={loader} className="rounded-full bg-cream/8 px-2.5 py-1 text-xs font-bold text-cream-300">
                    {loaderName(loader)}
                  </span>
                ))}
            </div>
          )}
          <p className="text-sm">
            <span className="text-mute">Minecraft </span>
            <span className="font-semibold">{versionRange(project.gameVersions)}</span>
          </p>
        </SideCard>
        {(project.projectType === "mod" || project.projectType === "modpack") && project.clientSide !== "" && (
          <SideCard title="Needed on">
            <p className="text-sm">
              <span className="text-mute">Your game: </span>
              <span className="font-semibold">{side(project.clientSide)}</span>
            </p>
            <p className="text-sm">
              <span className="text-mute">Servers: </span>
              <span className="font-semibold">{side(project.serverSide)}</span>
            </p>
          </SideCard>
        )}
        {project.categories.length > 0 && (
          <SideCard title="Categories">
            <div className="flex flex-wrap gap-1.5">
              {project.categories.map((category) => (
                <span key={category} className="rounded-full border border-cream/12 px-2.5 py-1 text-xs font-semibold text-cream-300">
                  {categoryLabel(category)}
                </span>
              ))}
            </div>
          </SideCard>
        )}
        {project.links.length > 0 && (
          <SideCard title="Links">
            <div className="flex flex-col">
              {project.links.map((link) => (
                <button
                  key={link.url}
                  type="button"
                  onClick={() => void openExternal(link.url)}
                  className="flex items-center justify-between gap-2 rounded-lg py-1.5 text-left text-sm font-semibold text-cream-300 transition-colors hover:text-cream"
                >
                  {link.label}
                  <ExternalLink size={13} className="shrink-0 text-mute" aria-hidden="true" />
                </button>
              ))}
            </div>
          </SideCard>
        )}
        <SideCard title="Details">
          {project.license && (
            <p className="flex items-center gap-2 text-sm">
              <Scale size={14} className="text-mute" aria-hidden="true" />
              <span className="font-semibold">{project.license.name || project.license.id.replace(/^LicenseRef-/, "").replace(/-/g, " ")}</span>
            </p>
          )}
          <p className="text-sm">
            <span className="text-mute">Published </span>
            <span className="font-semibold">{formatDay(project.published)}</span>
          </p>
          {project.authors.length > 0 && (
            <p className="text-sm">
              <span className="text-mute">Team </span>
              <span className="font-semibold">{project.authors.join(", ")}</span>
            </p>
          )}
        </SideCard>
      </aside>
    </m.div>
  );
}

function SideCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2.5 rounded-[20px] border border-cream/8 bg-ink-800 p-4">
      <h2 className="text-xs font-bold uppercase tracking-[0.12em] text-mute-2">{title}</h2>
      {children}
    </section>
  );
}

function Gallery({ project }: { project: ProjectDetails }) {
  const [open, setOpen] = useState<number | null>(null);
  const image = open === null ? null : project.gallery[open];
  return (
    <m.div variants={fadeUp} initial="hidden" animate="show">
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(260px,1fr))] gap-4">
        {project.gallery.map((entry, index) => (
          <li key={entry.url}>
            <button
              type="button"
              onClick={() => setOpen(index)}
              className="group flex w-full flex-col overflow-hidden rounded-[18px] border border-cream/8 bg-ink-800 text-left transition-colors hover:border-cream/25"
            >
              <img src={entry.url} alt={entry.title ?? ""} loading="lazy" decoding="async" className="aspect-video w-full object-cover" />
              {entry.title && <span className="truncate px-3.5 py-2.5 text-sm font-semibold">{entry.title}</span>}
            </button>
          </li>
        ))}
      </ul>
      <Dialog open={image !== null} onClose={() => setOpen(null)} title={image?.title || project.title} description={image?.description ?? undefined}>
        {image && <img src={image.rawUrl} alt={image.title ?? ""} className="w-full rounded-2xl" />}
      </Dialog>
    </m.div>
  );
}

const ALL = "all";

function Versions({ project }: { project: ProjectDetails }) {
  const { requestInstall } = useApp();
  const [versions, retry] = useResource(() => getProjectVersions(project.id));
  const [loader, setLoader] = useState(ALL);
  const [gameVersion, setGameVersion] = useState(ALL);
  const [showUnstable, setShowUnstable] = useState(false);
  const [limit, setLimit] = useState(VERSIONS_PER_PAGE);
  const [expanded, setExpanded] = useState<string | null>(null);

  const list = versions.status === "ready" ? versions.data : [];
  const loaders = useMemo(() => [...new Set(list.flatMap((version) => version.loaders))].filter((name) => name !== "minecraft"), [list]);
  const gameVersions = useMemo(() => {
    const seen = new Set<string>();
    for (const version of list) for (const game of version.gameVersions) seen.add(game);
    // Newest first, releases only, so the menu stays short.
    return [...seen].filter((game) => RELEASE_VERSION.test(game)).sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
  }, [list]);
  const filtered = list.filter(
    (version) =>
      (loader === ALL || version.loaders.includes(loader)) &&
      (gameVersion === ALL || version.gameVersions.includes(gameVersion)) &&
      (showUnstable || version.versionType === "release"),
  );
  const unstableHidden = !showUnstable && list.some((version) => version.versionType !== "release");

  if (versions.status === "loading") {
    return (
      <Loading label="Loading versions" className="flex flex-col gap-2">
        {[0, 1, 2, 3, 4].map((index) => (
          <Skeleton key={index} className="h-16 w-full rounded-2xl" />
        ))}
      </Loading>
    );
  }
  if (versions.status === "error") return <ErrorState title="Couldn't load versions" body="Check your connection and try again." onRetry={retry} />;

  const gameOptions: SelectOption<string>[] = [{ value: ALL, label: "Any version" }, ...gameVersions.map((game) => ({ value: game, label: game }))];
  return (
    <m.div variants={fadeUp} initial="hidden" animate="show" className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-x-6 gap-y-3 rounded-[20px] border border-cream/8 bg-ink-800/60 px-4 py-3">
        {loaders.length > 1 && (
          <div className="flex items-center gap-3">
            <span className="text-[13px] font-semibold text-mute">Loader</span>
            <Chips label="Loader" value={loader} options={[{ value: ALL, label: "All" }, ...loaders.map((name) => ({ value: name, label: loaderName(name) }))]} onChange={setLoader} />
          </div>
        )}
        <div className="flex items-center gap-3">
          <span className="text-[13px] font-semibold text-mute">Minecraft</span>
          <Select label="Minecraft version" value={gameVersion} options={gameOptions} onChange={setGameVersion} searchable className="w-40" />
        </div>
        <div className="ml-auto w-64">
          <Toggle label="Betas and alphas" checked={showUnstable} onChange={setShowUnstable} />
        </div>
      </div>

      <p className="text-sm text-mute">
        <span className="font-bold text-cream">{filtered.length}</span> of {list.length} versions
        {unstableHidden && " · betas and alphas hidden"}
      </p>

      {filtered.length === 0 ? (
        <p className="rounded-2xl border border-dashed border-cream/15 px-4 py-8 text-center text-sm text-mute">No versions match. Try another loader or Minecraft version{unstableHidden ? ", or show betas" : ""}.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {filtered.slice(0, limit).map((version) => (
            <VersionRow
              key={version.id}
              version={version}
              projectType={installKind(project.projectType, version.loaders)}
              expanded={expanded === version.id}
              onToggle={() => setExpanded((current) => (current === version.id ? null : version.id))}
              onInstall={() =>
                requestInstall({
                  projectId: project.id,
                  title: project.title,
                  projectType: installKind(project.projectType, version.loaders),
                  iconUrl: project.iconUrl,
                  version: { id: version.id, label: version.versionNumber, loaders: version.loaders, gameVersions: version.gameVersions },
                })
              }
            />
          ))}
        </ul>
      )}
      {filtered.length > limit && (
        <button type="button" onClick={() => setLimit((count) => count + VERSIONS_PER_PAGE)} className="mx-auto rounded-full border border-cream/20 px-5 py-2.5 text-sm font-bold hover:bg-cream/8">
          Show {Math.min(VERSIONS_PER_PAGE, filtered.length - limit)} more
        </button>
      )}
    </m.div>
  );
}

const TYPE_BADGE: Record<string, string> = {
  release: "bg-moss/15 text-moss-soft",
  beta: "bg-amber/15 text-amber-soft",
  alpha: "bg-coral/15 text-coral",
};

function VersionRow({
  version,
  projectType,
  expanded,
  onToggle,
  onInstall,
}: {
  version: ProjectVersion;
  projectType: ProjectType;
  expanded: boolean;
  onToggle: () => void;
  onInstall: () => void;
}) {
  const { settings } = useApp();
  const externalImages = settings?.externalImages ?? true;
  const html = useMemo(() => (expanded ? renderMarkdown(version.changelog, { externalImages }).html : ""), [expanded, version.changelog, externalImages]);
  const file = version.files.find((candidate) => candidate.primary) ?? version.files[0];
  const games = version.gameVersions.filter((game) => RELEASE_VERSION.test(game));
  const installable = INSTALLABLE.includes(projectType);

  return (
    <li className="rounded-[18px] border border-cream/8 bg-ink-800">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 p-3.5">
        <span className={cn("rounded-full px-2.5 py-1 text-[11px] font-bold capitalize", TYPE_BADGE[version.versionType] ?? "bg-cream/10 text-cream-300")}>{version.versionType}</span>
        <div className="min-w-0 flex-1 basis-56">
          <p className="truncate text-sm font-bold">{version.name || version.versionNumber}</p>
          <p className="truncate text-xs text-mute">
            {[loaderList(version.loaders), games.length ? (games.length > 3 ? `${games[0]} – ${games[games.length - 1]}` : games.join(", ")) : version.gameVersions.slice(0, 3).join(", ")]
              .filter(Boolean)
              .join(" · ")}
          </p>
        </div>
        <span className="text-xs text-mute">{formatDay(version.datePublished)}</span>
        <span className="flex items-center gap-1 text-xs text-mute">
          <Download size={12} aria-hidden="true" />
          {formatCount(version.downloads)}
        </span>
        {file && <span className="text-xs text-mute-2">{formatBytes(file.size)}</span>}
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={onToggle}
            aria-expanded={expanded}
            className="flex h-8 items-center gap-1 rounded-full px-3 text-xs font-bold text-cream-300 transition-colors hover:bg-cream/8 hover:text-cream"
          >
            Changelog
            <ChevronDown size={13} className={cn("transition-transform", expanded && "rotate-180")} aria-hidden="true" />
          </button>
          {installable && (
            <button type="button" onClick={onInstall} className="h-8 rounded-full bg-cream/10 px-3.5 text-xs font-bold transition-colors hover:bg-cream hover:text-ink-900">
              {projectType === "modpack" ? "Install" : "Add"}
            </button>
          )}
        </div>
      </div>
      {expanded && (
        <div
          onClick={openLinksExternally}
          className="prose-wisp border-t border-cream/8 px-4 py-3.5 text-sm"
          // Sanitised in renderMarkdown.
          dangerouslySetInnerHTML={{ __html: html || "<p>No changelog for this version.</p>" }}
        />
      )}
    </li>
  );
}

function ProjectLoading() {
  return (
    <Loading label="Loading project" className="flex flex-col gap-6">
      <div className="overflow-hidden rounded-[26px] border border-cream/8 bg-ink-800">
        <Skeleton className="h-44 w-full" />
        <div className="flex items-end gap-6 px-6 pb-6">
          <Skeleton className="-mt-12 size-24 rounded-[22px] ring-4 ring-ink-800" />
          <div className="flex flex-1 flex-col gap-2.5">
            <Skeleton className="h-3 w-20 rounded-full" />
            <Skeleton className="h-8 w-64 rounded-xl" />
            <Skeleton className="h-3.5 w-40 rounded-full" />
          </div>
          <Skeleton className="h-12 w-44 rounded-full" />
        </div>
      </div>
      <Skeleton className="h-9 w-72 rounded-full" />
      <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_280px]">
        <Skeleton className="h-96 w-full rounded-[24px]" />
        <Skeleton className="h-64 w-full rounded-[20px]" />
      </div>
    </Loading>
  );
}
