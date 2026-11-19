import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openUrl } from "@tauri-apps/plugin-opener";
import type {
  Account,
  CloudFolder,
  LaunchCheck,
  SyncActivity,
  SyncOverview,
  SyncResolution,
  Category,
  CurseForgeStatus,
  FixedEvent,
  FixOffer,
  HomeSnapshot,
  Instance,
  InstallEvent,
  AddedFile,
  InspectedFile,
  InstalledContent,
  SpedUp,
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
  PublicServer,
  RestorePoint,
  SavedServer,
  Screenshot,
  SearchQuery,
  SearchResults,
  ServerStatus,
  Settings,
  Skin,
  SkinImport,
  SkinLayerData,
  SkinLibrary,
  SkinModel,
  SocialNotice,
  SocialView,
  StorageUsage,
  VersionList,
  WorldEntry,
  WorldExtras,
  InstanceItem,
  ItemUpdate,
  RemovedItem,
  InstanceStats,
  InstanceMemory,
  LocalSettings,
  MovePlan,
  MovedInstance,
  UpdatedAll,
  LogFile,
  PackUpdates,
  PackUpdated,
  ExportOptions,
  ExportedFile,
  ChatMessage,
  ConsoleLine,
  Conversation,
  HostedView,
  HostingChange,
  JoinProgress,
  NewHostedServer,
  PlayerList,
  ServerChanges,
  ServerPlayers,
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
/** With a world (its folder) or a server (its address), the game goes straight there. */
export const launchInstance = (id: string, world: string | null = null, server: string | null = null) =>
  call<void>("launch_instance", { id, world, server }, (p) => p.launchInstance());
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
/** Installs a modpack file (a .mrpack, or a CurseForge zip) as a new instance; returns the job id. */
export const installModpackFile = (path: string, title: string) =>
  call<string>("install_modpack_file", { path, title }, (p) => p.needsDesktop("Installing modpack files"));
/** Adds the performance mods to an instance (moving a vanilla one to Fabric first). */
export const speedUpInstance = (instanceId: string) => call<SpedUp>("speed_up_instance", { instanceId }, (p) => p.needsDesktop("Speeding up instances"));
/** A file picker for modpacks, mods and packs: the files picked, none if cancelled. */
export const pickImportFiles = () => call<string[]>("pick_import_files", undefined, (p) => p.needsDesktop("Adding files"));
/** What each file is, before anything is done with it. */
export const inspectImportFiles = (paths: string[]) => call<InspectedFile[]>("inspect_import_files", { paths }, () => Promise.resolve(paths.map(guessFile)));

/** The browser preview can't open files: it goes by the name alone. */
function guessFile(path: string): InspectedFile {
  const fileName = path.split(/[\\/]/).pop() ?? path;
  const extension = fileName.split(".").pop()?.toLowerCase();
  const kind = extension === "mrpack" ? "modpack" : extension === "jar" ? "mod" : "unknown";
  const pack = kind === "modpack" ? { name: fileName.replace(/\.mrpack$/i, ""), version: null, gameVersion: "1.21.1", loader: "fabric" as const, from: "modrinth" as const } : null;
  return { path, fileName, kind, name: null, version: null, loaders: kind === "mod" ? ["fabric"] : [], pack, problem: null };
}
/** Adds mods, resource packs and shader packs to an instance. */
export const addContentFiles = (instanceId: string, files: string[]) =>
  call<AddedFile[]>("add_content_files", { instanceId, files }, (p) => p.needsDesktop("Adding files"));
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
/** The editor's layers for a skin (each PNG as base64), if it was saved with them. */
export const getSkinLayers = (id: string) => call<SkinLayerData[] | null>("get_skin_layers", { id }, (p) => p.skinLayers(id));
/** Keeps the editor's layers beside a skin; an empty list forgets them. */
export const setSkinLayers = (id: string, layers: SkinLayerData[]) => call<void>("set_skin_layers", { id, layers }, (p) => p.setSkinLayers(id, layers));
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
export const declineServerInvite = (from: string, serverId: string) => call<SocialView>("decline_server_invite", { from, serverId }, friendsNeedDesktop);
/** A conversation's messages, oldest first; reading it marks it read. */
export const getChat = (conversation: Conversation) => call<ChatMessage[]>("get_chat", { conversation }, () => Promise.resolve([]));
export const sendChat = (conversation: Conversation, text: string) => call<ChatMessage[]>("send_chat", { conversation, text }, friendsNeedDesktop);
/** Joins a friend's server: matches their instance, connects, and starts the game. Resolves with the instance playing it. */
export const joinFriendServer = (code: string, serverId: string) => call<string>("join_friend_server", { code, serverId }, friendsNeedDesktop);

/** Calls `handler` with each chat message's conversation as it arrives. */
export function onChat(handler: (conversation: Conversation) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<Conversation>("chat", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

/** Calls `handler` as joining a friend's server goes along. */
export function onJoinProgress(handler: (progress: JoinProgress) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<JoinProgress>("join", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

// ── Servers hosted on this computer (src-tauri/src/hosting.rs) ──

const hostingNeedsDesktop = () => Promise.reject(new Error("Hosting a server needs the desktop app."));
export const getHostedServers = () => call<HostedView[]>("get_hosted_servers", undefined, (p) => p.hostedServers());
export const getHostedServer = (id: string) => call<HostedView>("get_hosted_server", { id }, (p) => p.hostedServer(id));
export const createHostedServer = (request: NewHostedServer) => call<HostedView>("create_hosted_server", { request }, hostingNeedsDesktop);
export const updateHostedServer = (id: string, changes: ServerChanges) => call<HostedView>("update_hosted_server", { id, changes }, hostingNeedsDesktop);
export const deleteHostedServer = (id: string) => call<void>("delete_hosted_server", { id }, hostingNeedsDesktop);
export const openServerFolder = (id: string) => call<void>("open_server_folder", { id }, hostingNeedsDesktop);
export const startHostedServer = (id: string) => call<HostedView>("start_hosted_server", { id }, hostingNeedsDesktop);
export const stopHostedServer = (id: string) => call<HostedView>("stop_hosted_server", { id }, hostingNeedsDesktop);
export const restartHostedServer = (id: string) => call<HostedView>("restart_hosted_server", { id }, hostingNeedsDesktop);
export const agreeServerEula = (id: string) => call<HostedView>("agree_server_eula", { id }, hostingNeedsDesktop);
export const sendServerCommand = (id: string, command: string) => call<void>("send_server_command", { id, command }, hostingNeedsDesktop);
export const getServerConsole = (id: string) => call<ConsoleLine[]>("get_server_console", { id }, (p) => p.serverConsole());
export const getServerProperties = (id: string) => call<[string, string][]>("get_server_properties", { id }, (p) => p.serverProperties());
export const setServerProperties = (id: string, values: [string, string][]) => call<[string, string][]>("set_server_properties", { id, values }, hostingNeedsDesktop);
export const getServerPlayers = (id: string) => call<ServerPlayers>("get_server_players", { id }, (p) => p.serverPlayers());
export const setServerPlayer = (id: string, list: PlayerList, name: string, add: boolean) => call<ServerPlayers>("set_server_player", { id, list, name, add }, hostingNeedsDesktop);
export const setServerWorld = (id: string, source: { instanceId: string; folder: string }) => call<HostedView>("set_server_world", { id, source }, hostingNeedsDesktop);
export const getServerBackups = (id: string) => call<RestorePoint[]>("get_server_backups", { id }, () => Promise.resolve([]));
export const backUpServer = (id: string) => call<RestorePoint>("back_up_server", { id }, hostingNeedsDesktop);
export const restoreServerBackup = (id: string, backup: string) => call<void>("restore_server_backup", { id, backup }, hostingNeedsDesktop);
/** Starts the game straight into one of your servers. Resolves with the instance playing it. */
export const playHostedServer = (id: string) => call<string>("play_hosted_server", { id }, hostingNeedsDesktop);
export const inviteToServer = (id: string, code: string) => call<HostedView>("invite_to_server", { id, code }, hostingNeedsDesktop);

/** Calls `handler` whenever a hosted server changes, with what changed. */
export function onHostingChange(handler: (change: HostingChange) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<HostingChange>("hosting", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

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

// ── Servers ──

/** Each server's status, in the order asked (from memory if asked in the last minute, unless `refresh`). */
export const getServerStatuses = (addresses: string[], refresh = false) =>
  call<ServerStatus[]>("get_server_statuses", { addresses, refresh }, (p) => p.serverStatuses(addresses));
export const getPublicServers = () => call<PublicServer[]>("get_public_servers", undefined, (p) => p.publicServers());
export const addServer = (instanceId: string, name: string, address: string) =>
  call<SavedServer>("add_server", { instanceId, name, address }, (p) => p.addServer(instanceId, name, address));
export const removeServer = (instanceId: string, address: string) => call<void>("remove_server", { instanceId, address }, (p) => p.removeServer(instanceId, address));

// ── Settings shared between instances ──

/** Ids of the instances that keep their own settings instead of sharing them. */
export const getInstancesKeepingSettings = () => call<string[]>("get_instances_keeping_settings", undefined, () => Promise.resolve([]));
export const setInstanceKeepsSettings = (id: string, keep: boolean) =>
  call<void>("set_instance_keeps_settings", { id, keep }, (p) => p.needsDesktop("Keeping an instance's settings apart"));
/** Gives every other instance all of this one's settings, from their next start. Returns how many settings. */
export const shareInstanceSettings = (id: string) => call<number>("share_instance_settings", { id }, (p) => p.needsDesktop("Sharing an instance's settings"));

// ── Fixes ──

export const undoFix = (entryId: string) => call<void>("undo_fix", { entryId }, (p) => p.needsDesktop("Undoing a fix"));
/** Does a fix Wisp offered; returns its journal entry, for Undo. */
export const applyFix = (instanceId: string, action: FixOffer["action"]) => call<string | null>("apply_fix", { instanceId, action }, (p) => p.needsDesktop("Fixing"));

/** Calls `handler` when Wisp fixes (or finds) problems with an instance. */
export function onFixed(handler: (event: FixedEvent) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<FixedEvent>("fixed", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

/** Calls `handler` for every launch update; returns a function that stops listening. */
export function onLaunchEvent(handler: (event: LaunchEvent) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<LaunchEvent>("launch", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

/**
 * A tester build: made for a closed group of people who own Minecraft, with offline accounts on.
 * Never released. It updates from a tester channel of its own, never from the public one (which
 * would take offline accounts away).
 */
export const testerBuild = import.meta.env.VITE_WISP_TESTER === "1";
/** This tester build's number, when it has one (tester builds made before tester updates don't). */
export const testerBuildNumber = testerBuild ? (import.meta.env.VITE_WISP_TESTER_BUILD ?? null) : null;

/** This tester build's code, to invite someone with; null outside tester builds. */
export const getTesterCode = () => call<string | null>("get_tester_code", undefined, (p) => p.testerCode());

/** Whether text could be a tester code: WISP- (or not) and 24 letters and digits, dashes anywhere. */
export function looksLikeTesterCode(text: string): boolean {
  let symbols = text.toUpperCase().replace(/[^0-9A-Z]/g, "");
  if (symbols.length === 28 && symbols.startsWith("WISP")) symbols = symbols.slice(4);
  return /^[0-9A-TV-Z]{24}$/.test(symbols);
}

/**
 * Switches this copy of Wisp to tester builds with a tester code: the code is checked against the
 * tester feed, then the newest tester build downloads and installs in this one's place. Wisp has to
 * restart after; on Windows the installer does that itself.
 */
export function joinTesterBuilds(code: string, onProgress: (done: number, total: number | null) => void): Promise<void> {
  if (!isDesktop) return browserPreview().then((p) => p.joinTesterBuilds(code, onProgress));
  const channel = new Channel<{ done: number; total: number | null }>();
  channel.onmessage = (progress) => onProgress(progress.done, progress.total);
  return invoke<void>("join_tester_builds", { code, onProgress: channel });
}

/** How a version reads to people: "0.1.9", or "tester build 145" for a tester build's "0.1.8+145". */
export function versionLabel(version: string): string {
  const build = /\+(\d+)$/.exec(version);
  return testerBuild && build ? `tester build ${build[1]}` : version;
}
/** Developer and tester builds can make offline accounts; public releases need Microsoft sign-in. */
export const canAddOfflineAccounts = import.meta.env.DEV || testerBuild;

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

// ── Cloud sync ──

const SYNC_OFF: SyncOverview = { phase: "off", folder: null, message: null, lastSynced: null, devices: [], items: [], usage: null };

export const getSync = () => call<SyncOverview>("get_sync", undefined, (p) => p.sync());
/** The cloud folders on this computer (iCloud Drive, OneDrive...), the likeliest first. */
export const getSyncFolders = () => call<CloudFolder[]>("get_sync_folders", undefined, (p) => p.syncFolders());
/** Lets the player pick any folder; null when they cancelled. */
export const pickSyncFolder = () => call<string | null>("pick_sync_folder", undefined, () => Promise.resolve(null));
/** Turns sync on with a cloud folder (Wisp's folder goes inside it) and syncs. */
export const startSync = (path: string) => call<SyncOverview>("start_sync", { path }, (p) => p.needsDesktop("Cloud sync"));
export const stopSync = () => call<SyncOverview>("stop_sync", undefined, () => Promise.resolve(SYNC_OFF));
export const syncNow = () => call<SyncOverview>("sync_now", undefined, () => Promise.resolve(SYNC_OFF));
export const resolveSync = (key: string, resolution: SyncResolution) => call<SyncOverview>("resolve_sync", { key, resolution }, () => Promise.resolve(SYNC_OFF));
export const bringSyncItem = (key: string) => call<SyncOverview>("bring_sync_item", { key }, () => Promise.resolve(SYNC_OFF));
export const removeSyncItem = (key: string) => call<SyncOverview>("remove_sync_item", { key }, () => Promise.resolve(SYNC_OFF));
export const openSyncFolder = () => call<void>("open_sync_folder", undefined, () => Promise.resolve());
/** Just before Play: brings the newest copy and says what to decide first. */
export const checkLaunch = (instanceId: string) =>
  call<LaunchCheck>("check_launch", { instanceId }, (p) => p.launchCheck());

/** Whenever sync finishes something (with where everything stands now). */
export function onSyncChange(handler: (overview: SyncOverview) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<SyncOverview>("sync", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

/** What's moving right now, as it moves (null when it's done). */
export function onSyncActivity(handler: (activity: SyncActivity | null) => void): () => void {
  if (!isDesktop) return () => {};
  const stop = listen<SyncActivity | null>("sync-activity", (event) => handler(event.payload));
  return () => void stop.then((unlisten) => unlisten());
}

// ── An instance's own page (src-tauri/src/instance_page.rs) ──

/** Everything in an instance, named (Modrinth's names where it knows the files). */
export const getInstanceItems = (id: string) => call<InstanceItem[]>("get_instance_items", { id }, (p) => p.instanceItems(id));
export const getInstanceUpdates = (id: string) => call<ItemUpdate[]>("get_instance_updates", { id }, (p) => p.instanceUpdates(id));
/** Turns a mod or pack on or off; resolves to where its file is now. */
export const setItemEnabled = (id: string, file: string, enabled: boolean) =>
  call<string>("set_item_enabled", { id, file, enabled }, (p) => p.needsDesktop("Turning mods on and off"));
export const removeItem = (id: string, file: string, name: string) => call<RemovedItem>("remove_item", { id, file, name }, (p) => p.needsDesktop("Removing mods"));
export const putBackItem = (id: string, removedId: string) => call<string>("put_back_item", { id, removedId }, (p) => p.needsDesktop("Putting mods back"));
export const installItemVersion = (id: string, file: string, versionId: string) =>
  call<InstalledContent[]>("install_item_version", { id, file, versionId }, (p) => p.needsDesktop("Changing versions"));
export const updateAllItems = (id: string) => call<UpdatedAll>("update_all_items", { id }, (p) => p.needsDesktop("Updating mods"));
export const getInstanceStats = (id: string) => call<InstanceStats>("get_instance_stats", { id }, () => Promise.resolve({ playtimeMs: 0, sessions: 0 }));
export const getLocalSettings = (id: string) => call<LocalSettings>("get_local_settings", { id }, () => Promise.resolve({}));
export const setLocalSettings = (id: string, settings: LocalSettings) =>
  call<void>("set_local_settings", { id, settings }, (p) => p.needsDesktop("Instance settings"));
export const setInstanceMemory = (id: string, memoryMb: number | null) =>
  call<Instance>("set_instance_memory", { id, memoryMb }, (p) => p.setInstanceMemory(id, memoryMb));
/** What an instance's next start gets for memory, and where that comes from. */
export const getInstanceMemory = (id: string) => call<InstanceMemory>("get_instance_memory", { id }, (p) => p.instanceMemory(id));
export const setInstanceJava = (id: string, javaMajor: number | null) =>
  call<Instance>("set_instance_java", { id, javaMajor }, (p) => p.needsDesktop("Instance settings"));
export const planInstanceMove = (id: string, gameVersion: string, loader: Loader) =>
  call<MovePlan>("plan_instance_move", { id, gameVersion, loader }, (p) => p.needsDesktop("Changing versions"));
export const moveInstance = (id: string, gameVersion: string, loader: Loader, loaderVersion: string | null) =>
  call<MovedInstance>("move_instance", { id, gameVersion, loader, loaderVersion }, (p) => p.needsDesktop("Changing versions"));
export const duplicateInstance = (id: string, name: string, withWorlds: boolean) =>
  call<Instance>("duplicate_instance", { id, name, withWorlds }, (p) => p.needsDesktop("Copying instances"));
/** Resolves to null when the save dialog was closed. */
export const exportInstance = (id: string, options: ExportOptions) =>
  call<ExportedFile | null>("export_instance", { id, options }, (p) => p.needsDesktop("Exporting modpacks"));
export const getInstanceLogs = (id: string) => call<LogFile[]>("get_instance_logs", { id }, () => Promise.resolve([]));
export const readInstanceLog = (id: string, name: string) => call<string>("read_instance_log", { id, name }, (p) => p.needsDesktop("Reading logs"));
export const checkPackUpdates = (id: string) => call<PackUpdates>("check_pack_updates", { id }, () => Promise.resolve({ sameGame: null, newerGame: null }));
export const updateInstancePack = (id: string, versionId: string, gameVersions: string[]) =>
  call<PackUpdated>("update_instance_pack", { id, versionId, gameVersions }, (p) => p.needsDesktop("Updating modpacks"));
