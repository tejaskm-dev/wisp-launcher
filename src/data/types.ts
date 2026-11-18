// Mirrors the types in crates/wisp-core. Keep both sides in step until the TypeScript side is generated.

export type PageId =
  | "home"
  | "instances"
  | "worlds"
  | "discover"
  | "screenshots"
  | "journal"
  | "servers"
  | "skins"
  | "friends"
  | "sync"
  | "settings"
  | "project"
  | "world";

export interface Account {
  id: string;
  /** Minecraft username. */
  name: string;
  uuid: string;
  /** Offline accounts play singleplayer and offline-mode servers only. */
  kind: "offline";
}

export interface DeviceInfo {
  name: string;
  os: string;
}

export type Loader = "vanilla" | "fabric" | "quilt" | "forge" | "neoforge";

export interface Instance {
  id: string;
  name: string;
  gameVersion: string;
  loader: Loader;
  loaderVersion: string | null;
  /** Unix time in milliseconds. */
  created: number;
  lastPlayed: number | null;
  /** "wisp:<n>" for a built-in icon, or an image URL (a modpack's icon). */
  icon: string | null;
  modpack: ModpackOrigin | null;
  /** Memory for this instance in MB, instead of the one in Settings (Wisp's fixes set it). */
  memoryMb?: number | null;
  /** A newer Java than Minecraft asks for, when a mod needs one. */
  javaMajor?: number | null;
}

export interface ModpackOrigin {
  projectId: string;
  versionId: string;
  title: string;
  versionNumber: string;
}

export interface NewInstance {
  name: string;
  gameVersion: string;
  loader: Loader;
  /** A specific loader version; null uses the recommended one. */
  loaderVersion: string | null;
  icon: string | null;
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
  recommended: boolean;
}

/** Where a world's copy on this computer stands against the cloud. */
export type WorldSync =
  | { kind: "local" }
  | { kind: "synced"; at: number }
  | { kind: "uploading"; progress: number; totalBytes: number }
  | { kind: "cloud-newer" }
  | { kind: "conflict" }
  /** Being played on another computer right now. */
  | { kind: "in-use"; device: string }
  /** A cloud app is still bringing the newest copy. */
  | { kind: "waiting" };

export type GameMode = "survival" | "creative" | "adventure" | "spectator" | "unknown";

/** A world in an instance (crates/wisp-core/src/home.rs). */
export interface WorldSummary {
  /** "<instance id>/<folder>". */
  id: string;
  instanceId: string;
  instanceName: string;
  /** The world's folder in the instance's saves/. */
  folder: string;
  name: string;
  /** icon.png, relative to Wisp's data folder. */
  icon: string | null;
  /** The latest screenshot taken in it, relative to Wisp's data folder. */
  cover: string | null;
  /** "Fabric 26.3". */
  loader: string;
  modCount: number;
  /** Unix time in milliseconds. */
  lastPlayed: number;
  lastDevice: string | null;
  lastDeviceIsThis: boolean;
  /** The newest note left about this world. */
  note: string | null;
  noteAt: number | null;
  sync: WorldSync;
  gameMode: GameMode;
  hardcore: boolean;
  cheats: boolean;
  difficulty: string | null;
  /** The Minecraft version that last saved it. */
  version: string | null;
  /** Seconds spent in it. */
  playSeconds: number | null;
}

export interface WorldExtras {
  sizeBytes: number;
  restorePoints: RestorePoint[];
}

export type SnapshotReason = "auto" | "manual" | "before-restore" | "sync";

/** A restore point: a copy of a world to go back to (crates/wisp-core/src/snapshots.rs). */
export interface RestorePoint {
  /** A SHA-1, like a commit hash. */
  id: string;
  instanceId: string;
  /** The world's folder. */
  world: string;
  /** The world's name when it was taken. */
  name: string;
  createdAt: number;
  reason: SnapshotReason;
  label: string | null;
  files: number;
  totalBytes: number;
  /** What it added to the store (files no earlier restore point had). */
  addedBytes: number;
}

/** A screenshot from an instance (src-tauri/src/library.rs). */
export interface Screenshot {
  /** "<instance id>/<file>". */
  id: string;
  instanceId: string;
  instanceName: string;
  file: string;
  /** Relative to Wisp's data folder. */
  path: string;
  takenAt: number;
  sizeBytes: number;
  width: number | null;
  height: number | null;
  /** The world it was taken in, when the journal knows. */
  world: string | null;
  worldFolder: string | null;
}

/** Something that happened, as the journal records it (crates/wisp-core/src/journal.rs). */
export type JournalEvent =
  | {
      kind: "session";
      startedAt: number;
      endedAt: number;
      crashed: boolean;
      worlds: string[];
      newWorlds: string[];
      screenshots: string[];
      /** Servers joined, as addresses. */
      servers?: string[];
      device: string;
      label: string;
    }
  | { kind: "note"; text: string; session?: string | null }
  | { kind: "restore-point"; snapshot: string; reason: SnapshotReason; files: number; totalBytes: number; addedBytes: number; label?: string | null }
  | { kind: "restored"; snapshot: string }
  | { kind: "content-added"; projectId: string; title: string; projectType: string; version: string; dependencies: number }
  | { kind: "content-removed"; projectId: string; title: string; projectType: string }
  | { kind: "instance-created"; name: string; label: string }
  | { kind: "modpack-installed"; title: string; version: string }
  | { kind: "world-imported"; name: string }
  | { kind: "world-exported"; name: string }
  | { kind: "world-deleted"; name: string }
  /** Wisp fixed what stopped (or would have stopped) the game. */
  | { kind: "fixed"; found: string[]; fixed: string[]; changes: FixChange[]; afterCrash: boolean }
  | { kind: "fix-undone"; fix: string };

/** A change Wisp made to fix something; enough to put it back. */
export type FixChange =
  | { kind: "disabled"; file: string; name: string }
  | { kind: "added"; projectId: string; title: string; version: string; files: string[] }
  | { kind: "replaced"; previous: { title: string; versionNumber: string }; backup: string; current: { title: string; versionNumber: string } }
  | { kind: "loader-changed"; from: string | null; to: string }
  | { kind: "config-moved"; from: string; to: string }
  | { kind: "memory-changed"; from: number | null; to: number }
  | { kind: "java-changed"; from: number | null; to: number };

/** Something Wisp can do with a click, left to the player because it can affect worlds. */
export interface FixOffer {
  label: string;
  detail: string;
  action: { kind: "disable"; file: string; name: string };
}

/** What Wisp found wrong with an instance, what it fixed, and what's left. */
export interface FixReport {
  found: string[];
  fixed: string[];
  changes: FixChange[];
  offers: FixOffer[];
  unsolved: string[];
  /** The journal entry that records the changes, for Undo. */
  entry?: string | null;
}

export interface FixedEvent {
  instanceId: string;
  report: FixReport;
}

export type JournalKind = JournalEvent["kind"];

export interface JournalEntry {
  id: string;
  at: number;
  instanceId?: string | null;
  /** The world (its folder) it's about. */
  world?: string | null;
  event: JournalEvent;
}

export interface JournalQuery {
  instanceId?: string | null;
  world?: string | null;
  kinds?: JournalKind[];
  before?: number | null;
  limit?: number | null;
}

export type SkinModel = "classic" | "slim";

export interface Skin {
  id: string;
  name: string;
  model: SkinModel;
  added: number;
  updated: number;
  /** "file", "player:<name>", "editor". */
  source: string | null;
}

export interface SkinLibrary {
  skins: Skin[];
  active: string | null;
}

/** A player's current skin from Mojang. */
/** One of the skin editor's layers, as kept beside a skin. */
export interface SkinLayerData {
  name: string;
  visible: boolean;
  opacity: number;
  /** A 64×64 PNG: base64, or a data URL when sent from the page. */
  png: string;
}

export interface PlayerSkin {
  name: string;
  model: SkinModel;
  /** Base64 PNG. */
  png: string;
}

/** What a friend is up to (crates/wisp-core/src/social.rs). */
export type FriendStatus =
  | { state: "online" }
  | { state: "playing"; label: string | null; world: string | null; since: number }
  | { state: "offline" };

export interface SocialProfile {
  name: string;
  /** Their skin's face, as a tiny PNG data URL. */
  face: string | null;
}

export interface Friend {
  /** Their friend code (npub…). */
  code: string;
  profile: SocialProfile;
  since: number;
  lastSeen: number | null;
  status: FriendStatus;
}

export interface FriendRequest {
  code: string;
  profile: SocialProfile;
  note: string | null;
  at: number;
}

export interface Party {
  id: string;
  name: string;
  /** The leader's friend code. */
  leader: string;
  members: string[];
  created: number;
}

export interface PartyMember {
  code: string;
  name: string;
  face: string | null;
  status: FriendStatus;
  me: boolean;
  leader: boolean;
}

export interface PartyView extends Party {
  membersShown: PartyMember[];
  leading: boolean;
}

export interface PartyInvite {
  party: Party;
  from: string;
  at: number;
}

/** Friends, as the Friends page shows them (src-tauri/src/social.rs). */
export interface SocialView {
  enabled: boolean;
  relaysConnected: number;
  relays: string[];
  /** Your own friend code. */
  code: string;
  profile: SocialProfile;
  shareActivity: boolean;
  friends: Friend[];
  incoming: FriendRequest[];
  outgoing: FriendRequest[];
  party: PartyView | null;
  invites: PartyInvite[];
}

export interface SocialNotice {
  title: string;
  body: string | null;
}

export interface SkinImport {
  added: Skin[];
  failed: string[];
}

export type SyncStatus =
  | { state: "off" }
  | { state: "on"; label: string; kind: FolderKind; phase: SyncPhase; lastSynced: number | null; message: string | null; conflicts: number };

// ── Cloud sync (crates/wisp-core/src/sync) ──

/** Which app keeps the sync folder in step with the cloud. */
export type FolderKind = "icloud" | "onedrive" | "google-drive" | "dropbox" | "box" | "nextcloud" | "syncthing" | "pcloud" | "folder";

export type SyncPhase = "off" | "idle" | "working" | "waiting" | "unreachable" | "failed";

/** What's already synced in a cloud folder from other computers. */
export interface RemoteSummary {
  devices: string[];
  instances: number;
  worlds: number;
  lastSeen: number | null;
}

/** A cloud folder found on this computer. */
export interface CloudFolder {
  kind: FolderKind;
  label: string;
  path: string;
  existing: RemoteSummary | null;
}

export interface SyncFolder {
  path: string;
  kind: FolderKind;
  label: string;
  since: number;
}

export interface SyncDevice {
  id: string;
  name: string;
  os: string;
  seen: number;
  /** Names of the instances it's playing right now. */
  playing: string[];
  this: boolean;
}

/** One version to pick when a world changed on two computers. */
export interface SyncChoice {
  commit: string;
  device: string;
  at: number;
}

export type SyncItemState =
  | { kind: "synced"; at: number; device: string }
  | { kind: "pending" }
  | { kind: "cloud-newer"; at: number; device: string }
  | { kind: "conflict"; choices: SyncChoice[]; hereAt: number | null }
  | { kind: "in-use"; device: string }
  | { kind: "waiting"; files: number }
  | { kind: "cloud-only"; at: number; device: string; bytes: number; needsInstance: boolean }
  | { kind: "removed"; device: string }
  | { kind: "failed"; message: string };

export interface SyncItem {
  key: string;
  kind: "world" | "instance";
  name: string;
  /** The instance on this computer, when there is one. */
  instanceId: string | null;
  /** A world's folder. */
  world: string | null;
  state: SyncItemState;
}

export interface SyncOverview {
  phase: SyncPhase;
  folder: SyncFolder | null;
  message: string | null;
  lastSynced: number | null;
  devices: SyncDevice[];
  items: SyncItem[];
  usage: number | null;
}

/** Something moving through sync right now. */
export interface SyncActivity {
  key: string;
  name: string;
  direction: "up" | "down";
  done: number;
  total: number;
}

export type SyncResolution = { keep: "here" } | { keep: "cloud"; commit: string } | { keep: "both"; commit: string };

/** What the player should decide before an instance starts. */
export interface LaunchCheck {
  conflicts: SyncItem[];
  /** Another computer playing this instance right now. */
  inUse: string | null;
  /** Files still on their way from the cloud. */
  waiting: number;
}

export interface HomeSnapshot {
  account: Account | null;
  device: DeviceInfo;
  /** Most recently used first. */
  instances: Instance[];
  worlds: WorldSummary[];
  /** Every server in every instance's list, most recently played first. */
  servers: ServerSummary[];
  sync: SyncStatus;
}

/** A server from the instances' server lists; one saved in several shows up once. */
export interface ServerSummary {
  address: string;
  name: string;
  /** PNG data URL, as the game last saw it. */
  icon: string | null;
  /** Instances whose list has it; the first is the one Play uses. */
  instanceIds: string[];
  lastPlayed: number | null;
}

export interface SavedServer {
  name: string;
  address: string;
  icon: string | null;
}

/** A run of a server's message of the day in one style. */
export interface MotdSpan {
  text: string;
  color?: string;
  bold?: boolean;
  italic?: boolean;
  underlined?: boolean;
  strikethrough?: boolean;
  obfuscated?: boolean;
}

/** What a server says about itself on the server list. */
export interface ServerStatus {
  address: string;
  online: boolean;
  motd: MotdSpan[];
  version: string | null;
  protocol: number | null;
  players: number | null;
  maxPlayers: number | null;
  sample: string[];
  icon: string | null;
  pingMs: number | null;
  error: string | null;
}

/** A public server on Discover; its status comes live from the server. */
export interface PublicServer {
  name: string;
  address: string;
  description: string;
  tags: string[];
  languages: string[];
}

export type IntroMode = "full" | "quick" | "off";

export interface SyncPrefs {
  worlds: boolean;
  /** Instances themselves: version, loader, mods and mod configs. */
  instances: boolean;
  options: boolean;
  servers: boolean;
  resourcePacks: boolean;
  screenshots: boolean;
}

export type StartPage = "home" | "instances" | "discover" | "last";
export type WhilePlaying = "keep-open" | "minimize" | "hide";
export type MotionPreference = "system" | "reduced" | "full";
export type WindowMode = "default" | "custom" | "fullscreen";

export interface JournalPrefs {
  /** Take a restore point of each world played, when the game closes. */
  autoRestorePoints: boolean;
  /** Automatic restore points kept per world. */
  keepRestorePoints: number;
  /** Offer to leave a note when the game closes. */
  askForNote: boolean;
}

export interface HomeSections {
  worlds: boolean;
  quickActions: boolean;
  modpacks: boolean;
  news: boolean;
}

export interface GameWindow {
  mode: WindowMode;
  width: number;
  height: number;
}

export interface LaunchHooks {
  preLaunch: string;
  wrapper: string;
  postExit: string;
}

export interface NewInstanceOptions {
  /** An instance whose settings and keybinds (options.txt) new instances copy. */
  copyOptionsFrom: string | null;
  skipWelcome: boolean;
  skipTutorial: boolean;
  autoJumpOff: boolean;
}

/** Mirrors `Settings` in crates/wisp-core/src/settings.rs. */
export interface Settings {
  intro: IntroMode;
  sidebarCollapsed: boolean;
  startPage: StartPage;
  whilePlaying: WhilePlaying;
  motion: MotionPreference;
  home: HomeSections;
  memoryMb: number;
  gameWindow: GameWindow;
  jvmArgs: string;
  /** Custom Java executables by major version: "8", "17", "21", "25". */
  javaPaths: Record<string, string>;
  /** A CurseForge API key, for copies of Wisp built without one. */
  curseforgeApiKey: string;
  hooks: LaunchHooks;
  newInstances: NewInstanceOptions;
  concurrentDownloads: number;
  externalImages: boolean;
  sync: SyncPrefs;
  journal: JournalPrefs;
}

/** A Java runtime Wisp installed (crates/wisp-core/src/launch/java.rs). */
export interface InstalledRuntime {
  component: string;
  platform: string;
  major: number | null;
  version: string;
  sizeBytes: number;
}

/** What `java -version` said about a Java someone picked. */
export interface JavaProbe {
  version: string;
  major: number;
  description: string;
}

export interface StorageUsage {
  instances: number;
  gameFiles: number;
  assets: number;
  java: number;
  cache: number;
}

/** A mod or shader loader as Modrinth lists it (for filters). */
export interface LoaderTag {
  name: string;
  /** SVG markup from Modrinth: sanitise before showing. */
  icon: string;
  projectTypes: string[];
}

export interface GameVersion {
  id: string;
  kind: "release" | "snapshot" | "old_beta" | "old_alpha" | (string & {});
  /** RFC 3339 */
  released: string;
}

export interface VersionList {
  latestRelease: string;
  latestSnapshot: string;
  /** Newest first. */
  versions: GameVersion[];
}

export type ProjectType = "modpack" | "mod" | "resourcepack" | "shader" | "datapack" | "plugin" | "world" | "other";
/** Where projects come from. CurseForge project ids look like "curseforge:238222". */
export type Source = "modrinth" | "curseforge";
export type SearchSort = "relevance" | "downloads" | "follows" | "newest" | "updated";

export interface SearchQuery {
  source: Source;
  query: string;
  projectType: ProjectType;
  sort: SearchSort;
  /** Any of these loaders. */
  loaders: string[];
  /** All of these categories. */
  categories: string[];
  /** Only projects that support this Minecraft version, e.g. "26.3". */
  gameVersion: string | null;
  /** Only projects that run on this side. */
  environment: "client" | "server" | null;
  offset: number;
  limit: number;
}

export interface Project {
  id: string;
  slug: string;
  title: string;
  author: string;
  description: string;
  projectType: ProjectType;
  downloads: number;
  follows: number;
  /** RFC 3339 */
  updated: string;
  loaders: string[];
  gameVersion: string | null;
  tags: string[];
  imageUrl: string | null;
  iconUrl: string | null;
  /** 0xRRGGBB */
  color: number | null;
}

export interface SearchResults {
  hits: Project[];
  total: number;
  offset: number;
}

export interface ProjectDetails {
  id: string;
  slug: string;
  title: string;
  description: string;
  /** Markdown, with some HTML. */
  body: string;
  projectType: ProjectType;
  downloads: number;
  followers: number;
  published: string;
  updated: string;
  categories: string[];
  loaders: string[];
  /** Oldest first. */
  gameVersions: string[];
  clientSide: string;
  serverSide: string;
  license: { id: string; name: string; url: string | null } | null;
  iconUrl: string | null;
  color: number | null;
  gallery: { url: string; rawUrl: string; title: string | null; description: string | null; featured: boolean }[];
  links: { label: string; url: string }[];
  authors: string[];
}

export interface ProjectVersion {
  id: string;
  projectId: string;
  name: string;
  versionNumber: string;
  versionType: "release" | "beta" | "alpha" | (string & {});
  loaders: string[];
  gameVersions: string[];
  datePublished: string;
  downloads: number;
  changelog: string;
  files: { url: string; filename: string; primary: boolean; size: number; sha1: string | null }[];
  dependencies: { projectId: string | null; versionId: string | null; dependencyType: string }[];
}

export interface ProjectSummary {
  id: string;
  slug: string;
  title: string;
  projectType: ProjectType;
  iconUrl: string | null;
}

export interface Category {
  /** Modrinth's slug, or CurseForge's numeric id. */
  name: string;
  projectType: string;
  header: string;
  /** An SVG icon (Modrinth). */
  icon: string;
  /** Words to show when `name` is an id (CurseForge). */
  label?: string | null;
  /** A picture icon (CurseForge). */
  iconUrl?: string | null;
}

/** A world in an instance, for picking where a data pack goes. */
export interface WorldEntry {
  folder: string;
  lastPlayed: number | null;
}

export interface CurseForgeStatus {
  /** A key is set (in Settings or built in), so CurseForge can be searched. */
  available: boolean;
  builtIn: boolean;
}

/** What a file you bring in turned out to be (from what's inside it, not just its name). */
export type ImportKind = "modpack" | "mod" | "resource-pack" | "shader-pack" | "data-pack" | "world" | "unknown";

/** What a modpack file makes. */
export interface PackSummary {
  name: string | null;
  version: string | null;
  gameVersion: string;
  loader: Loader;
  from: "modrinth" | "curse-forge";
}

/** A file looked over before it's brought in. */
export interface InspectedFile {
  path: string;
  fileName: string;
  kind: ImportKind;
  /** From the file's own metadata, when it has some. */
  name: string | null;
  version: string | null;
  /** For a mod: the loaders it's made for. */
  loaders: string[];
  /** For a modpack: what it makes. */
  pack: PackSummary | null;
  /** Why a modpack can't be read, when it can't. */
  problem: string | null;
}

/** What "Speed up" did to an instance. */
export interface SpedUp {
  /** The instance as it is now (a vanilla one moves to Fabric). */
  instance: Instance;
  added: InstalledContent[];
  /** Already in the instance. */
  had: string[];
  /** No build for this version and loader, or clashing with something already there. */
  skipped: string[];
  movedToFabric: boolean;
}

/** How adding one file to an instance went. */
export interface AddedFile {
  path: string;
  added: { content: InstalledContent; note: string | null } | null;
  error: string | null;
}

export interface InstalledContent {
  projectId: string;
  versionId: string;
  title: string;
  versionNumber: string;
  projectType: ProjectType;
  /** Path inside the game folder. */
  file: string;
  iconUrl: string | null;
  /** False when it came along as a dependency. */
  explicit: boolean;
  /** For a data pack, the world (folder) it's in. */
  world?: string | null;
}

export type InstallUpdate =
  | { phase: "preparing" }
  | { phase: "downloading"; doneBytes: number; totalBytes: number; doneFiles: number; totalFiles: number }
  | { phase: "unpacking" }
  | { phase: "done"; instanceId: string; skipped?: string[] }
  | { phase: "failed"; message: string };

export interface InstallEvent {
  jobId: string;
  title: string;
  update: InstallUpdate;
}

export interface PatchNote {
  id: string;
  title: string;
  version: string;
  kind: "release" | "snapshot" | (string & {});
  /** RFC 3339 */
  date: string;
  summary: string;
  imageUrl: string | null;
}

/** What the launcher is doing for one instance (crates/wisp-core/src/launch/mod.rs). */
export type LaunchUpdate =
  | { phase: "checking" }
  /** Before the launcher starts: bringing the newest copy from the sync folder (the window's own step). */
  | { phase: "syncing"; folder: string }
  | { phase: "downloading"; doneBytes: number; totalBytes: number; doneFiles: number; totalFiles: number }
  | { phase: "installing"; loader: string }
  | { phase: "starting" }
  | { phase: "running"; startedAt: number }
  /** The game closed and Wisp is taking a restore point of a world that was played. */
  | { phase: "saving"; world: string }
  /** Wisp found something that would stop the game (or just did) and is fixing it. */
  | { phase: "fixing"; message: string }
  | { phase: "exited"; code: number | null; crashed: boolean; logTail: string[]; help?: FixReport }
  /** `fix` names something Wisp can do about it: install Rosetta for Intel-only versions. */
  | { phase: "failed"; message: string; fix?: "install-rosetta" };

export interface LaunchEvent {
  instanceId: string;
  /** The world (its folder) the game opened straight into, if any. */
  world: string | null;
  /** The server the game joined straight away, if any. */
  server?: string | null;
  update: LaunchUpdate;
}
