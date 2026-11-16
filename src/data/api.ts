import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openUrl } from "@tauri-apps/plugin-opener";
import type {
  Account,
  Category,
  CurseForgeStatus,
  HomeSnapshot,
  Instance,
  InstallEvent,
  InstalledContent,
  InstalledRuntime,
  JavaProbe,
  JournalEntry,
  JournalQuery,
  LaunchEvent,
  Loader,
  LoaderTag,
  LoaderVersion,
  NewInstance,
  PatchNote,
  PlayerSkin,
  Project,
  ProjectDetails,
  ProjectSummary,
  ProjectType,
  ProjectVersion,
  RestorePoint,
  Screenshot,
  SearchQuery,
  SearchResults,
  Settings,
  Skin,
  SkinImport,
  SkinLibrary,
  SkinModel,
  SocialNotice,
  SocialView,
  StorageUsage,
  VersionList,
  WorldEntry,
  WorldExtras,
} from "./types";

/** True inside the desktop app; false when the UI runs in a plain browser via `pnpm dev`. */
export const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type BrowserPreview = typeof import("./browserPreview");

// A plain browser has no Rust commands. During development a small fallback stands in for them;
// production builds only ever run inside the desktop app.
function browserPreview(): Promise<BrowserPreview> {
  if (!import.meta.env.DEV) return Promise.reject(new Error("Wisp only runs inside its desktop app."));
  return import("./browserPreview");
}

function call<T>(command: string, args: Record<string, unknown> | undefined, fallback: (preview: BrowserPreview) => Promise<T>): Promise<T> {
  return isDesktop ? invoke<T>(command, args) : browserPreview().then(fallback);
}

export const getHome = () => call<HomeSnapshot>("get_home", undefined, (p) => p.home());
export const getSettings = () => call<Settings>("get_settings", undefined, (p) => p.getSettings());
export const saveSettings = (next: Settings) => call<Settings>("set_settings", { next }, (p) => p.setSettings(next));
export const getVersions = () => call<VersionList>("get_versions", undefined, (p) => p.versions());
export const createInstance = (request: NewInstance) => call<Instance>("create_instance", { request }, (p) => p.createInstance(request));
export const searchProjects = (query: SearchQuery) => call<SearchResults>("search_projects", { query }, (p) => p.search(query));
export const openInstanceFolder = (id: string) => call<void>("open_instance_folder", { id }, (p) => p.openFolder());
export const getDataFolder = () => call<string>("get_data_folder", undefined, (p) => p.dataFolder());
export const openDataFolder = () => call<void>("open_data_folder", undefined, (p) => p.openFolder());
export const getFeaturedModpacks = () => call<Project[]>("get_featured_modpacks", undefined, (p) => p.featuredModpacks());
export const getJavaNews = () => call<PatchNote[]>("get_java_news", undefined, (p) => p.javaNews());
/** With a world (its folder), the game opens straight into it on versions that can. */
export const launchInstance = (id: string, world: string | null = null) => call<void>("launch_instance", { id, world }, (p) => p.launchInstance());
export const stopInstance = (id: string) => call<void>("stop_instance", { id }, () => Promise.resolve());
export const getLaunches = () => call<LaunchEvent[]>("get_launches", undefined, () => Promise.resolve([]));
export const getSystemMemory = () => call<number | null>("get_system_memory", undefined, () => Promise.resolve(16384));
export const getJavaRuntimes = () => call<InstalledRuntime[]>("get_java_runtimes", undefined, () => Promise.resolve([]));
export const deleteJavaRuntime = (component: string, platform: string) =>
  call<void>("delete_java_runtime", { component, platform }, () => Promise.resolve());
export const testJava = (path: string) =>
  call<JavaProbe>("test_java", { path }, () => Promise.reject(new Error("Testing Java needs the desktop app.")));
export const getStorageUsage = () =>
  call<StorageUsage>("get_storage_usage", undefined, () => Promise.resolve({ instances: 0, gameFiles: 0, assets: 0, java: 0, cache: 0 }));
export const clearCache = () => call<number>("clear_cache", undefined, () => Promise.resolve(0));
/** Installs Apple's Rosetta (macOS asks for a password). False when the prompt was cancelled. */
export const installRosetta = () => call<boolean>("install_rosetta", undefined, () => Promise.resolve(false));
export const addOfflineAccount = (name: string) => call<Account>("add_offline_account", { name }, (p) => p.addOfflineAccount(name));
export const removeAccount = (id: string) => call<void>("remove_account", { id }, (p) => p.removeAccount(id));

export const getLoaderGameVersions = (loader: Loader) =>
  call<string[] | null>("get_loader_game_versions", { loader }, (p) => p.loaderGameVersions(loader));
export const getLoaderVersions = (loader: Loader, gameVersion: string) =>
  call<LoaderVersion[]>("get_loader_versions", { loader, gameVersion }, (p) => p.loaderVersions(loader, gameVersion));
export const updateInstance = (id: string, patch: { name?: string; icon?: string }) =>
  call<Instance>("update_instance", { id, patch }, (p) => p.updateInstance(id, patch));
export const deleteInstance = (id: string) => call<void>("delete_instance", { id }, (p) => p.deleteInstance(id));
export const getProject = (id: string) => call<ProjectDetails>("get_project", { id }, (p) => p.project(id));
export const getProjectVersions = (id: string, loaders: string[] = [], gameVersions: string[] = []) =>
  call<ProjectVersion[]>("get_project_versions", { id, loaders, gameVersions }, (p) => p.projectVersions(id, loaders, gameVersions));
export const getProjects = (ids: string[]) => call<ProjectSummary[]>("get_projects", { ids }, (p) => p.projects(ids));
export const getCategories = () => call<Category[]>("get_categories", undefined, (p) => p.categories());
export const getLoaderTags = () => call<LoaderTag[]>("get_loader_tags", undefined, (p) => p.loaderTags());
export const getInstanceContent = (id: string) =>
  call<InstalledContent[]>("get_instance_content", { id }, () => Promise.resolve([]));
/** `world` adds a data pack to that world (its folder) instead of the instance. */
export const installProject = (instanceId: string, projectId: string, versionId: string | null, world: string | null = null) =>
  call<InstalledContent[]>("install_project", { instanceId, projectId, versionId, world }, (p) => p.needsDesktop("Installing"));
export const removeContent = (instanceId: string, projectId: string, world: string | null = null) =>
  call<void>("remove_content", { instanceId, projectId, world }, () => Promise.resolve());
export const getInstanceWorlds = (id: string) => call<WorldEntry[]>("get_instance_worlds", { id }, () => Promise.resolve([]));
export const getCurseforgeStatus = () =>
  call<CurseForgeStatus>("get_curseforge_status", undefined, () => Promise.resolve({ available: false, builtIn: false }));
export const getCurseforgeCategories = (projectType: ProjectType) =>
  call<Category[]>("get_curseforge_categories", { projectType }, () => Promise.resolve([]));
/** Starts a modpack install in the background; progress arrives through `onInstallEvent`. */
export const installModpack = (projectId: string, versionId: string | null, title: string) =>
  call<string>("install_modpack", { projectId, versionId, title }, (p) => p.needsDesktop("Installing modpacks"));
export const getInstalls = () => call<InstallEvent[]>("get_installs", undefined, () => Promise.resolve([]));

// ── Worlds ──

export const getWorldExtras = (instanceId: string, folder: string) =>
  call<WorldExtras>("get_world_extras", { instanceId, folder }, (p) => p.needsDesktop("World details"));
export const openWorldFolder = (instanceId: string, folder: string) => call<void>("open_world_folder", { instanceId, folder }, (p) => p.openFolder());
/** Opens a file picker for world zips; returns the new worlds' folders (empty if cancelled). */
export const importWorld = (instanceId: string) => call<string[]>("import_world", { instanceId }, (p) => p.needsDesktop("Importing worlds"));
/** Worlds dropped on the window: zips or world folders. */
export const importWorldPaths = (instanceId: string, files: string[]) =>
  call<string[]>("import_world_paths", { instanceId, files }, (p) => p.needsDesktop("Importing worlds"));
/** Saves a world as a zip; returns where, or null if cancelled. */
export const exportWorld = (instanceId: string, folder: string) =>
  call<string | null>("export_world", { instanceId, folder }, (p) => p.needsDesktop("Exporting worlds"));
/** Moves a world to the Trash. */
export const deleteWorld = (instanceId: string, folder: string) => call<void>("delete_world", { instanceId, folder }, (p) => p.needsDesktop("Deleting worlds"));

// ── Screenshots ──

export const getScreenshots = () => call<Screenshot[]>("get_screenshots", undefined, () => Promise.resolve([]));
/** Moves a screenshot to the Trash. */
export const deleteScreenshot = (instanceId: string, file: string) => call<void>("delete_screenshot", { instanceId, file }, () => Promise.resolve());
/** A screenshot's bytes, for the clipboard. */
export const getScreenshotBytes = (instanceId: string, file: string) =>
  call<ArrayBuffer>("get_screenshot_bytes", { instanceId, file }, () => Promise.reject(new Error("Copying needs the desktop app.")));
export const revealScreenshot = (instanceId: string, file: string) => call<void>("reveal_screenshot", { instanceId, file }, () => Promise.resolve());
export const openScreenshotsFolder = (instanceId: string) => call<void>("open_screenshots_folder", { instanceId }, (p) => p.openFolder());

// ── Journal and restore points ──

export const getJournal = (query: JournalQuery = {}) => call<JournalEntry[]>("get_journal", { query }, (p) => p.journal(query));
export const addNote = (instanceId: string | null, world: string | null, text: string, session: string | null = null) =>
  call<JournalEntry>("add_note", { instanceId, world, text, session }, (p) => p.addNote(instanceId, world, text));
export const editNote = (id: string, text: string) => call<void>("edit_note", { id, text }, (p) => p.editNote(id, text));
export const deleteNote = (id: string) => call<void>("delete_note", { id }, (p) => p.deleteNote(id));
export const getRestorePoints = () => call<RestorePoint[]>("get_restore_points", undefined, () => Promise.resolve([]));
export const getRestorePointUsage = () => call<number>("get_restore_point_usage", undefined, () => Promise.resolve(0));
export const createRestorePoint = (instanceId: string, folder: string, label: string | null = null) =>
  call<RestorePoint>("create_restore_point", { instanceId, folder, label }, (p) => p.needsDesktop("Restore points"));
/** Puts a world back the way a restore point has it (the world as it is now is kept first). */
export const restoreWorld = (id: string) => call<RestorePoint>("restore_world", { id }, (p) => p.needsDesktop("Restoring worlds"));
/** Returns the bytes freed. */
export const deleteRestorePoint = (id: string) => call<number>("delete_restore_point", { id }, (p) => p.needsDesktop("Restore points"));

// ── Skins ──

export const getSkins = () => call<SkinLibrary>("get_skins", undefined, (p) => p.skins());
/** A skin's PNG as a data URL (the viewer and editor read its pixels, so it can't come from another origin). */
export const getSkinDataUrl = (id: string) =>
  call<string>("get_skin_png", { id }, (p) => p.skinDataUrl(id)).then((png) => (png.startsWith("data:") ? png : `data:image/png;base64,${png}`));
/** `png` is base64 (or a data URL). With no model, Wisp guesses it from the picture. */
export const addSkin = (name: string, model: SkinModel | null, png: string, source: string | null = null) =>
  call<Skin>("add_skin", { name, model, png, source }, (p) => p.addSkin(name, model, png, source));
export const updateSkin = (id: string, patch: { name?: string; model?: SkinModel; png?: string }) =>
  call<Skin>("update_skin", { id, ...patch }, (p) => p.updateSkin(id, patch));
export const deleteSkin = (id: string) => call<void>("delete_skin", { id }, (p) => p.deleteSkin(id));
export const setActiveSkin = (id: string | null) => call<void>("set_active_skin", { id }, (p) => p.setActiveSkin(id));
export const fetchPlayerSkin = (username: string) =>
  call<PlayerSkin>("fetch_player_skin", { username }, () => Promise.reject(new Error("Looking up players needs the desktop app.")));
export const pickSkinFiles = () => call<SkinImport>("pick_skin_files", undefined, (p) => p.needsDesktop("Importing skin files"));
export const importSkinPaths = (files: string[]) => call<SkinImport>("import_skin_paths", { files }, (p) => p.needsDesktop("Importing skin files"));
export const exportSkin = (id: string) => call<string | null>("export_skin", { id }, (p) => p.needsDesktop("Exporting skins"));

// ── Friends (src-tauri/src/social.rs) ──

const friendsNeedDesktop = () => Promise.reject(new Error("Friends needs the desktop app."));
export const getSocial = () => call<SocialView>("get_social", undefined, friendsNeedDesktop);
export const setSocialEnabled = (enabled: boolean) => call<SocialView>("set_social_enabled", { enabled }, friendsNeedDesktop);
export const setSocialProfile = (name: string, face: string | null) => call<SocialView>("set_social_profile", { name, face }, friendsNeedDesktop);
export const setShareActivity = (share: boolean) => call<SocialView>("set_share_activity", { share }, friendsNeedDesktop);
export const addFriend = (code: string, note: string | null) => call<SocialView>("add_friend", { code, note }, friendsNeedDesktop);
export const acceptFriend = (code: string) => call<SocialView>("accept_friend", { code }, friendsNeedDesktop);
export const declineFriend = (code: string) => call<SocialView>("decline_friend", { code }, friendsNeedDesktop);
/** Removes a friend, or takes back a request. */
export const removeFriend = (code: string) => call<SocialView>("remove_friend", { code }, friendsNeedDesktop);
export const createParty = (name: string) => call<SocialView>("create_party", { name }, friendsNeedDesktop);
export const inviteToParty = (code: string) => call<SocialView>("invite_to_party", { code }, friendsNeedDesktop);
export const joinParty = (partyId: string) => call<SocialView>("join_party", { partyId }, friendsNeedDesktop);
export const declineParty = (partyId: string) => call<SocialView>("decline_party", { partyId }, friendsNeedDesktop);
export const leaveParty = () => call<SocialView>("leave_party", undefined, friendsNeedDesktop);

/** Calls `handler` whenever friends change, with a notice worth showing if there is one. */
export function onSocialChange(handler: (notice: SocialNotice | null) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<SocialNotice | null>("social", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

// ── Pictures from Wisp's data folder, served by the wisp:// protocol (src-tauri/src/protocol.rs) ──

const PROTOCOL = typeof navigator !== "undefined" && /Windows/.test(navigator.userAgent) ? "http://wisp.localhost" : "wisp://localhost";
const encodePath = (path: string) => path.split("/").map(encodeURIComponent).join("/");

/** A picture in the data folder (a world icon, a screenshot), whole. `version` busts the cache when it changes. */
export function fileUrl(path: string, version: number | string = 0): string {
  return `${PROTOCOL}/file/${encodePath(path)}?v=${version}`;
}

/** A JPEG thumbnail of a picture in the data folder, at most `width` pixels wide. */
export function thumbUrl(path: string, width: number, version: number | string = 0): string {
  return `${PROTOCOL}/thumb/${width}/${encodePath(path)}?v=${version}`;
}


/** Calls `handler` whenever the journal changes (a session ended, a note was added…). */
export function onJournalChange(handler: () => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen("journal", () => handler());
  return () => void stop.then((unlisten) => unlisten());
}

export type FileDrop = { type: "over"; paths: string[] } | { type: "drop"; paths: string[] } | { type: "leave" };

/** Files dragged over or dropped on the window (desktop only: the browser preview has no paths). */
export function onFileDrop(handler: (drop: FileDrop) => void): () => void {
  if (!isDesktop) return () => {};
  let known: string[] = [];
  const stop = getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload;
    if (payload.type === "enter") {
      known = payload.paths;
      handler({ type: "over", paths: known });
    } else if (payload.type === "drop") {
      handler({ type: "drop", paths: payload.paths });
    } else if (payload.type === "leave") {
      handler({ type: "leave" });
    }
  });
  return () => void stop.then((unlisten) => unlisten());
}

/** Calls `handler` for every modpack install update; returns a function that stops listening. */
export function onInstallEvent(handler: (event: InstallEvent) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<InstallEvent>("install", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

/** Calls `handler` for every launch update; returns a function that stops listening. */
export function onLaunchEvent(handler: (event: LaunchEvent) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<LaunchEvent>("launch", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

/** Developer builds can make offline accounts for testing; release builds need Microsoft sign-in. */
export const canAddOfflineAccounts = import.meta.env.DEV;

/** Opens a web page in the default browser (never inside Wisp's own window). */
export function openExternal(url: string): Promise<void> {
  if (isDesktop) return openUrl(url);
  window.open(url, "_blank", "noopener");
  return Promise.resolve();
}

export function modrinthUrl(project: Pick<Project, "projectType" | "slug">): string {
  return `https://modrinth.com/${project.projectType}/${project.slug}`;
}

/** Where a project lives on the web: its Modrinth page, or its CurseForge one. */
export function projectUrl(project: Pick<ProjectDetails, "id" | "projectType" | "slug" | "title" | "links">): { site: string; url: string } {
  if (project.id.startsWith("curseforge:")) {
    const page = project.links.find((link) => link.label === "CurseForge")?.url;
    return { site: "CurseForge", url: page ?? `https://www.curseforge.com/minecraft/search?search=${encodeURIComponent(project.title)}` };
  }
  return { site: "Modrinth", url: modrinthUrl(project) };
}
