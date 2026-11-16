// Development-only stand-in for the Rust commands, so `pnpm dev` in a plain browser behaves like the
// app: the same public APIs are fetched directly, and settings and instances live in this browser's
// storage. The desktop app never loads this file; there everything comes from crates/wisp-core.

import { DEFAULT_SETTINGS } from "./defaults";
import type {
  Account,
  Category,
  HomeSnapshot,
  Instance,
  JournalEntry,
  JournalQuery,
  Skin,
  SkinLibrary,
  SkinModel,
  Loader,
  LoaderTag,
  LoaderVersion,
  NewInstance,
  PatchNote,
  Project,
  ProjectDetails,
  ProjectSummary,
  ProjectVersion,
  SearchQuery,
  SearchResults,
  Settings,
  VersionList,
} from "./types";

const LOADERS = [
  "fabric", "forge", "neoforge", "quilt", "iris", "optifine", "canvas", "vanilla", "minecraft", "datapack",
  "babric", "bta-babric", "java-agent", "legacy-fabric", "liteloader", "modloader", "nilloader", "ornithe", "rift",
];
const MODRINTH = "https://api.modrinth.com/v2";

// Add ?state=loading or ?state=error to the URL to see the skeletons and error states.
const forcedState = new URLSearchParams(window.location.search).get("state");

function simulate<T>(load: () => Promise<T>): Promise<T> {
  if (forcedState === "loading") return new Promise<T>(() => {});
  if (forcedState === "error") return Promise.reject(new Error("Simulated failure (?state=error)"));
  return load();
}

function readStorage<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function writeStorage(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage can be unavailable (private mode); the preview just won't remember.
  }
}

export async function getSettings(): Promise<Settings> {
  const stored = readStorage<Partial<Settings>>("wisp.preview.settings", {});
  const d = DEFAULT_SETTINGS;
  return {
    ...d,
    ...stored,
    home: { ...d.home, ...stored.home },
    gameWindow: { ...d.gameWindow, ...stored.gameWindow },
    hooks: { ...d.hooks, ...stored.hooks },
    newInstances: { ...d.newInstances, ...stored.newInstances },
    javaPaths: { ...d.javaPaths, ...stored.javaPaths },
    sync: { ...d.sync, ...stored.sync },
    journal: { ...d.journal, ...stored.journal },
  };
}

export async function setSettings(next: Settings): Promise<Settings> {
  writeStorage("wisp.preview.settings", next);
  return next;
}

const listInstances = () => readStorage<Instance[]>("wisp.preview.instances", []);
const readAccount = () => readStorage<Account | null>("wisp.preview.account", null);

export function home(): Promise<HomeSnapshot> {
  if (forcedState === "loading") return new Promise<HomeSnapshot>(() => {});
  const instances = [...listInstances()].sort((a, b) => (b.lastPlayed ?? b.created) - (a.lastPlayed ?? a.created));
  return Promise.resolve({ account: readAccount(), device: { name: "This computer", os: "" }, instances, worlds: [], sync: { state: "off" } });
}

export async function addOfflineAccount(name: string): Promise<Account> {
  const trimmed = name.trim();
  if (!/^\w{3,16}$/.test(trimmed)) throw new Error("Use 3 to 16 letters, numbers or underscores.");
  const account: Account = { id: `preview-${trimmed.toLowerCase()}`, name: trimmed, uuid: "", kind: "offline" };
  writeStorage("wisp.preview.account", account);
  return account;
}

export async function removeAccount(_id: string): Promise<void> {
  writeStorage("wisp.preview.account", null);
}

export async function launchInstance(): Promise<void> {
  throw new Error("Launching needs the desktop app. Run pnpm tauri dev.");
}

export async function createInstance(request: NewInstance): Promise<Instance> {
  const name = request.name.trim();
  if (!name) throw new Error("Give the instance a name.");
  let loaderVersion = request.loaderVersion;
  if (request.loader !== "vanilla" && !loaderVersion) {
    const versions = await loaderVersions(request.loader, request.gameVersion);
    loaderVersion = (versions.find((version) => version.recommended) ?? versions[0])?.version ?? null;
    if (!loaderVersion) throw new Error(`That loader doesn't support Minecraft ${request.gameVersion} yet.`);
  }
  const existing = listInstances();
  const base = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "instance";
  let id = base;
  for (let n = 2; existing.some((instance) => instance.id === id); n += 1) id = `${base}-${n}`;
  const instance: Instance = {
    id,
    name,
    gameVersion: request.gameVersion,
    loader: request.loader,
    loaderVersion,
    created: Date.now(),
    lastPlayed: null,
    icon: request.icon,
    modpack: null,
  };
  writeStorage("wisp.preview.instances", [...existing, instance]);
  return instance;
}

export async function versions(): Promise<VersionList> {
  const response = await fetch("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json");
  if (!response.ok) throw new Error(`Mojang returned ${response.status}`);
  const manifest = (await response.json()) as {
    latest: { release: string; snapshot: string };
    versions: { id: string; type: string; releaseTime: string }[];
  };
  return {
    latestRelease: manifest.latest.release,
    latestSnapshot: manifest.latest.snapshot,
    versions: manifest.versions.map((version) => ({ id: version.id, kind: version.type, released: version.releaseTime })),
  };
}

interface ModrinthHit {
  project_id: string;
  slug: string;
  title: string;
  author: string;
  description?: string;
  project_type: Project["projectType"];
  downloads: number;
  follows?: number;
  date_modified?: string;
  categories?: string[];
  display_categories?: string[];
  versions?: string[];
  icon_url?: string | null;
  featured_gallery?: string | null;
  gallery?: string[];
  color?: number | null;
}

export function search(query: SearchQuery): Promise<SearchResults> {
  if (query.source === "curseforge") return Promise.reject(new Error("CurseForge needs an API key. Add one in Settings → Mod sources."));
  return simulate(async () => {
    const facets = [[`project_type:${query.projectType}`]];
    const loaders = query.loaders.filter((loader) => LOADERS.includes(loader));
    if (loaders.length) facets.push(loaders.map((loader) => `categories:${loader}`));
    for (const category of query.categories) facets.push([`categories:${category}`]);
    if (query.gameVersion) facets.push([`versions:${query.gameVersion}`]);
    if (query.environment === "client") facets.push(["client_side:required", "client_side:optional"]);
    if (query.environment === "server") facets.push(["server_side:required", "server_side:optional"]);
    const params = new URLSearchParams({
      query: query.query.trim(),
      facets: JSON.stringify(facets),
      index: query.sort,
      offset: String(query.offset),
      limit: String(query.limit),
    });
    const response = await fetch(`https://api.modrinth.com/v2/search?${params}`);
    if (!response.ok) throw new Error(`Modrinth returned ${response.status}`);
    const body = (await response.json()) as { hits: ModrinthHit[]; total_hits: number; offset: number };
    return { hits: body.hits.map(toProject), total: body.total_hits, offset: body.offset };
  });
}

function toProject(hit: ModrinthHit): Project {
  return {
    id: hit.project_id,
    slug: hit.slug,
    title: hit.title,
    author: hit.author,
    description: hit.description ?? "",
    projectType: hit.project_type,
    downloads: hit.downloads,
    follows: hit.follows ?? 0,
    updated: hit.date_modified ?? "",
    loaders: (hit.categories ?? []).filter((category) => LOADERS.includes(category)),
    gameVersion: hit.versions?.at(-1) ?? null,
    tags: hit.display_categories ?? [],
    imageUrl: hit.featured_gallery ?? hit.gallery?.find((url) => url.includes("_350.")) ?? null,
    iconUrl: hit.icon_url ?? null,
    color: hit.color ?? null,
  };
}

export function featuredModpacks(): Promise<Project[]> {
  return search({ source: "modrinth", query: "", projectType: "modpack", sort: "follows", loaders: [], categories: [], gameVersion: null, environment: null, offset: 0, limit: 3 }).then(
    (results) => results.hits,
  );
}

interface PatchNoteEntry {
  id: string;
  title: string;
  version: string;
  type: string;
  date?: string;
  shortText?: string;
  image?: { url: string };
}

export function javaNews(): Promise<PatchNote[]> {
  return simulate(async () => {
    const response = await fetch("https://launchercontent.mojang.com/v2/javaPatchNotes.json");
    if (!response.ok) throw new Error(`Mojang returned ${response.status}`);
    const { entries } = (await response.json()) as { entries: PatchNoteEntry[] };
    const newestFirst = (a: PatchNoteEntry, b: PatchNoteEntry) => (b.date ?? "").localeCompare(a.date ?? "");
    const sorted = [...entries].sort(newestFirst);
    return [sorted.find((entry) => entry.type === "release"), sorted.find((entry) => entry.type === "snapshot")]
      .filter((entry): entry is PatchNoteEntry => entry !== undefined)
      .sort(newestFirst)
      .map((entry) => ({
        id: entry.id,
        title: entry.title,
        version: entry.version,
        kind: entry.type,
        date: entry.date ?? "",
        summary: (entry.shortText ?? "").replace(/\r\n/g, "\n").split("\n\n")[0].trim(),
        imageUrl: entry.image ? `https://launchercontent.mojang.com${entry.image.url}` : null,
      }));
  });
}

export async function dataFolder(): Promise<string> {
  return "~/Library/Application Support/app.wisp.launcher (browser preview)";
}

export async function openFolder(): Promise<void> {
  // Nothing to open from a browser tab.
}

// ─── loaders ──────────────────────────────────────────────────────────────────────────────────────

const META: Partial<Record<Loader, string>> = {
  fabric: "https://meta.fabricmc.net/v2/versions",
  quilt: "https://meta.quiltmc.org/v3/versions",
};

export async function loaderGameVersions(loader: Loader): Promise<string[] | null> {
  const meta = META[loader];
  if (loader === "vanilla") return null;
  // Forge's and NeoForge's metadata can't be read from a browser page (no CORS); the app reads it.
  if (!meta) return [];
  const response = await fetch(`${meta}/game`);
  return response.ok ? ((await response.json()) as { version: string }[]).map((entry) => entry.version) : [];
}

export async function loaderVersions(loader: Loader, gameVersion: string): Promise<LoaderVersion[]> {
  const meta = META[loader];
  if (!meta) return [];
  const response = await fetch(`${meta}/loader/${encodeURIComponent(gameVersion)}`);
  if (!response.ok) return [];
  const entries = (await response.json()) as { loader: { version: string; stable?: boolean } }[];
  const versions = entries.map((entry) => ({
    version: entry.loader.version,
    stable: entry.loader.stable ?? !/beta|alpha|pre|rc/i.test(entry.loader.version),
    recommended: false,
  }));
  const pick = versions.findIndex((version) => version.stable);
  if (versions.length) versions[pick >= 0 ? pick : 0].recommended = true;
  return versions;
}

// ─── instances ────────────────────────────────────────────────────────────────────────────────────

export async function updateInstance(id: string, patch: { name?: string; icon?: string }): Promise<Instance> {
  const instances = listInstances();
  const instance = instances.find((candidate) => candidate.id === id);
  if (!instance) throw new Error("That instance no longer exists.");
  if (patch.name !== undefined) instance.name = patch.name.trim() || instance.name;
  if (patch.icon !== undefined) instance.icon = patch.icon || null;
  writeStorage("wisp.preview.instances", instances);
  return instance;
}

export async function deleteInstance(id: string): Promise<void> {
  writeStorage("wisp.preview.instances", listInstances().filter((instance) => instance.id !== id));
}

export async function needsDesktop(what: string): Promise<never> {
  throw new Error(`${what} needs the desktop app. Run pnpm tauri dev.`);
}

// ─── Modrinth projects ────────────────────────────────────────────────────────────────────────────

async function modrinth<T>(path: string): Promise<T> {
  const response = await fetch(`${MODRINTH}${path}`);
  if (!response.ok) throw new Error(`Modrinth returned ${response.status}`);
  return (await response.json()) as T;
}

interface RawProject {
  id: string;
  slug: string;
  title: string;
  description: string;
  body: string;
  project_type: ProjectDetails["projectType"];
  downloads: number;
  followers: number;
  published: string;
  updated: string;
  categories: string[];
  additional_categories?: string[];
  loaders: string[];
  game_versions: string[];
  client_side: string;
  server_side: string;
  license: { id: string; name: string; url: string | null } | null;
  icon_url: string | null;
  color: number | null;
  gallery: { url: string; raw_url?: string; title: string | null; description: string | null; featured: boolean; ordering: number }[];
  source_url: string | null;
  issues_url: string | null;
  wiki_url: string | null;
  discord_url: string | null;
  donation_urls?: { platform: string; url: string }[];
}

export function project(id: string): Promise<ProjectDetails> {
  return simulate(async () => {
    const raw = await modrinth<RawProject>(`/project/${encodeURIComponent(id)}`);
    const members = await modrinth<{ user: { username: string }; role: string }[]>(`/project/${encodeURIComponent(id)}/members`).catch(() => []);
    const links: { label: string; url: string }[] = [];
    const named: [string, string | null][] = [
      ["Source", raw.source_url],
      ["Issues", raw.issues_url],
      ["Wiki", raw.wiki_url],
      ["Discord", raw.discord_url],
    ];
    for (const [label, url] of named) if (url && /^https?:\/\//.test(url)) links.push({ label, url });
    for (const donation of raw.donation_urls ?? []) links.push({ label: `Donate (${donation.platform})`, url: donation.url });
    return {
      id: raw.id,
      slug: raw.slug,
      title: raw.title,
      description: raw.description,
      body: raw.body,
      projectType: raw.project_type,
      downloads: raw.downloads,
      followers: raw.followers,
      published: raw.published,
      updated: raw.updated,
      categories: [...raw.categories, ...(raw.additional_categories ?? [])].filter((category) => !LOADERS.includes(category)),
      loaders: raw.loaders,
      gameVersions: raw.game_versions,
      clientSide: raw.client_side,
      serverSide: raw.server_side,
      license: raw.license,
      iconUrl: raw.icon_url,
      color: raw.color,
      gallery: [...raw.gallery]
        .sort((a, b) => Number(b.featured) - Number(a.featured) || a.ordering - b.ordering)
        .map((image) => ({ url: image.url, rawUrl: image.raw_url ?? image.url, title: image.title, description: image.description, featured: image.featured })),
      links,
      authors: [...members].sort((a, b) => Number(b.role === "Owner") - Number(a.role === "Owner")).map((member) => member.user.username),
    };
  });
}

interface RawVersion {
  id: string;
  project_id: string;
  name: string;
  version_number: string;
  version_type: string;
  loaders: string[];
  game_versions: string[];
  date_published: string;
  downloads: number;
  changelog: string | null;
  files: { url: string; filename: string; primary: boolean; size: number; hashes: { sha1?: string } }[];
  dependencies: { project_id: string | null; version_id: string | null; dependency_type: string }[];
}

export function projectVersions(id: string, loaders: string[], gameVersions: string[]): Promise<ProjectVersion[]> {
  return simulate(async () => {
    const params = new URLSearchParams();
    if (loaders.length) params.set("loaders", JSON.stringify(loaders));
    if (gameVersions.length) params.set("game_versions", JSON.stringify(gameVersions));
    const query = params.toString();
    const raw = await modrinth<RawVersion[]>(`/project/${encodeURIComponent(id)}/version${query ? `?${query}` : ""}`);
    return raw.map((version) => ({
      id: version.id,
      projectId: version.project_id,
      name: version.name,
      versionNumber: version.version_number,
      versionType: version.version_type,
      loaders: version.loaders,
      gameVersions: version.game_versions,
      datePublished: version.date_published,
      downloads: version.downloads,
      changelog: version.changelog ?? "",
      files: version.files.map((file) => ({ url: file.url, filename: file.filename, primary: file.primary, size: file.size, sha1: file.hashes.sha1 ?? null })),
      dependencies: version.dependencies.map((dependency) => ({
        projectId: dependency.project_id,
        versionId: dependency.version_id,
        dependencyType: dependency.dependency_type,
      })),
    }));
  });
}

export async function projects(ids: string[]): Promise<ProjectSummary[]> {
  if (!ids.length) return [];
  const raw = await modrinth<{ id: string; slug: string; title: string; project_type: ProjectSummary["projectType"]; icon_url: string | null }[]>(
    `/projects?ids=${encodeURIComponent(JSON.stringify(ids))}`,
  );
  return raw.map((project) => ({ id: project.id, slug: project.slug, title: project.title, projectType: project.project_type, iconUrl: project.icon_url }));
}

export async function categories(): Promise<Category[]> {
  const raw = await modrinth<{ name: string; project_type: string; header: string; icon: string }[]>("/tag/category");
  return raw.map((category) => ({ name: category.name, projectType: category.project_type, header: category.header, icon: category.icon }));
}

export async function loaderTags(): Promise<LoaderTag[]> {
  const raw = await modrinth<{ name: string; icon: string; supported_project_types: string[] }[]>("/tag/loader");
  return raw.map((tag) => ({ name: tag.name, icon: tag.icon, projectTypes: tag.supported_project_types }));
}

// ── Journal: the preview keeps notes (nothing else happens in a browser) ──

const readJournal = () => readStorage<JournalEntry[]>("wisp.preview.journal", []);

export async function journal(query: JournalQuery): Promise<JournalEntry[]> {
  return simulate(async () =>
    readJournal()
      .filter((entry) => !query.instanceId || entry.instanceId === query.instanceId)
      .filter((entry) => !query.world || entry.world === query.world)
      .filter((entry) => !query.kinds?.length || query.kinds.includes(entry.event.kind))
      .filter((entry) => !query.before || entry.at < query.before)
      .sort((a, b) => b.at - a.at)
      .slice(0, query.limit ?? undefined),
  );
}

export async function addNote(instanceId: string | null, world: string | null, text: string): Promise<JournalEntry> {
  const trimmed = text.trim().slice(0, 2000);
  if (!trimmed) throw new Error("a note can't be empty");
  const entry: JournalEntry = { id: `${Date.now().toString(16)}${Math.random().toString(16).slice(2, 10)}`, at: Date.now(), instanceId, world, event: { kind: "note", text: trimmed } };
  writeStorage("wisp.preview.journal", [...readJournal(), entry]);
  return entry;
}

export async function editNote(id: string, text: string): Promise<void> {
  writeStorage(
    "wisp.preview.journal",
    readJournal().map((entry) => (entry.id === id && entry.event.kind === "note" ? { ...entry, event: { ...entry.event, text: text.trim() } } : entry)),
  );
}

export async function deleteNote(id: string): Promise<void> {
  writeStorage("wisp.preview.journal", readJournal().filter((entry) => entry.id !== id));
}

// ── Skins: kept in local storage as data URLs ──

const readSkins = () => readStorage<SkinLibrary>("wisp.preview.skins", { skins: [], active: null });

function loadPicture(dataUrl: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const picture = new Image();
    picture.onload = () => resolve(picture);
    picture.onerror = () => reject(new Error("that isn't a Minecraft skin: it isn't a PNG picture"));
    picture.src = dataUrl;
  });
}

/** Mirrors skins::guess_model: slim skins leave the strip beside each arm see-through. */
function guessModel(picture: HTMLImageElement): SkinModel {
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context || picture.height !== 64) return "classic";
  context.drawImage(picture, 0, 0);
  const strips: [number, number, number, number][] = [[50, 16, 2, 4], [54, 20, 2, 12], [42, 48, 2, 4], [46, 52, 2, 12]];
  return strips.some(([x, y, w, h]) => {
    const data = context.getImageData(x, y, w, h).data;
    for (let index = 3; index < data.length; index += 4) if (data[index] !== 255) return true;
    return false;
  })
    ? "slim"
    : "classic";
}

const asDataUrl = (png: string) => (png.startsWith("data:") ? png : `data:image/png;base64,${png}`);

export async function skins(): Promise<SkinLibrary> {
  return simulate(async () => readSkins());
}

export async function addSkin(name: string, model: SkinModel | null, png: string, source: string | null): Promise<Skin> {
  const dataUrl = asDataUrl(png);
  const picture = await loadPicture(dataUrl);
  if (picture.width !== 64 || (picture.height !== 64 && picture.height !== 32)) {
    throw new Error(`that isn't a Minecraft skin: it's ${picture.width}×${picture.height} pixels; skins are 64×64`);
  }
  const now = Date.now();
  const skin: Skin = { id: `${now.toString(16)}${Math.random().toString(16).slice(2, 10)}`, name: name.trim().slice(0, 48) || "Untitled skin", model: model ?? guessModel(picture), added: now, updated: now, source };
  writeStorage(`wisp.preview.skin.${skin.id}`, null);
  try {
    localStorage.setItem(`wisp.preview.skin.${skin.id}`, dataUrl);
  } catch {
    throw new Error("The browser preview ran out of storage for skins.");
  }
  const library = readSkins();
  writeStorage("wisp.preview.skins", { skins: [skin, ...library.skins], active: library.active ?? skin.id });
  return skin;
}

export async function updateSkin(id: string, patch: { name?: string; model?: SkinModel; png?: string }): Promise<Skin> {
  const library = readSkins();
  const found = library.skins.find((skin) => skin.id === id);
  if (!found) throw new Error("that skin is gone");
  if (patch.png) localStorage.setItem(`wisp.preview.skin.${id}`, asDataUrl(patch.png));
  const updated: Skin = { ...found, name: patch.name?.trim().slice(0, 48) || found.name, model: patch.model ?? found.model, updated: Date.now() };
  writeStorage("wisp.preview.skins", { ...library, skins: library.skins.map((skin) => (skin.id === id ? updated : skin)) });
  return updated;
}

export async function deleteSkin(id: string): Promise<void> {
  const library = readSkins();
  writeStorage("wisp.preview.skins", { skins: library.skins.filter((skin) => skin.id !== id), active: library.active === id ? null : library.active });
  try {
    localStorage.removeItem(`wisp.preview.skin.${id}`);
  } catch {
    // Nothing to clean up.
  }
}

export async function skinDataUrl(id: string): Promise<string> {
  const stored = localStorage.getItem(`wisp.preview.skin.${id}`);
  if (!stored) throw new Error("that skin is gone");
  return stored;
}

export async function setActiveSkin(id: string | null): Promise<void> {
  writeStorage("wisp.preview.skins", { ...readSkins(), active: id });
}
