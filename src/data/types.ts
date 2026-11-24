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
  | "world"
  | "instance"
  | "hosted";

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

/** An instance's memory as its next start will have it (src-tauri/src/instance_page.rs). */
export interface InstanceMemory {
  /** What the game gets, in MB. */
  mb: number;
  /** Set for this instance (by hand, or by a fix after it ran out). */
  own: boolean;
  /** Wisp's setting, in Settings > Game. */
  settingMb: number;
  /** What Wisp picks when the instance has none of its own: the setting, or more for a big modpack. */
  automaticMb: number;
  /** The mods turned on, which the automatic memory goes by. */
  mods: number;
  /** The computer's memory, when it can be read. */
  systemMb: number | null;
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
  | { kind: "modpack-updated"; title: string; from: string; to: string }
  | { kind: "version-changed"; from: string; to: string }
  | { kind: "instance-duplicated"; from: string }
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

/** The template an image tool paints (base64 PNG) and the prompt that goes with it. */
export interface SkinTemplate {
  png: string;
  prompt: string;
  /** The follow-up for the outer layer: the same painting with only what sits on top. */
  outerPrompt: string;
}

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
  /** Outer layers kept to wear over any skin. */
  cosmetics?: Cosmetic[];
  /** The cosmetic worn over the skin, if any. */
  wornCosmetic?: string | null;
}

/** A skin's outer layer kept on its own, to wear over any skin. */
export interface Cosmetic {
  id: string;
  name: string;
  added: number;
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
  /** Their servers you can join right now. */
  hosting: ServerCard[];
  /** Chat messages from them you haven't read. */
  unread: number;
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
  /** Invites to friends' servers. */
  serverInvites: ServerInvite[];
  /** The party's chat, when you're in a party. */
  partyChat: ChatSummary | null;
}

/** A server a friend hosts, as you see it (crates/wisp-core/src/social.rs). */
export interface ServerCard {
  id: string;
  name: string;
  gameVersion: string;
  /** "vanilla", "fabric", "quilt", "forge" or "neoforge". */
  loader: string;
  loaderVersion: string | null;
  /** Up and taking players right now. */
  online: boolean;
  /** On its way up: joining waits for it. */
  starting?: boolean;
  players: number;
  maxPlayers: number;
  /** How many mods joining brings along. */
  mods: number;
  /** A plugin server's software; players join with plain Minecraft. */
  software?: string;
}

export interface ServerInvite {
  server: ServerCard;
  /** Who invited you (their friend code). */
  from: string;
  at: number;
}

export interface ChatMessage {
  id: string;
  /** The sender's friend code. */
  from: string;
  text: string;
  at: number;
}

/** "party:<id>" or "friend:<code>". */
export type Conversation = string;

export interface ChatSummary {
  conversation: Conversation;
  unread: number;
  last: ChatMessage | null;
}

/** What joining a friend's server is doing (src-tauri/src/hosting.rs). */
/** What a server's join code says about it (wisp_core::codes). */
export interface CodeCard {
  name: string;
  gameVersion: string;
  loader: string;
  software?: string | null;
  addresses: string[];
  online: boolean;
  /** On its way up: joining waits for it. */
  starting?: boolean;
  players: number;
  maxPlayers: number;
  mods: number;
  at: number;
}

export interface CodeLookup {
  /** The code, tidied up. */
  code: string;
  server: CodeCard | null;
}

/** How joining a server by its address went (src-tauri/src/hosting.rs). */
export type AddressJoin =
  | { kind: "plain" }
  | { kind: "joined"; instanceId: string }
  | { kind: "needs-consent"; server: string; files: string[] };

export interface JoinProgress {
  code: string;
  serverId: string;
  step: "asking" | "waiting" | "connecting" | "mods" | "launching" | "done" | "failed";
  doneBytes: number;
  totalBytes: number;
  message: string | null;
}

// ── Servers hosted on this computer (src-tauri/src/hosting.rs) ──

export type HostedState = "stopped" | "preparing" | "starting" | "running" | "stopping" | "crashed";

export interface ServerExit {
  code: number | null;
  crashed: boolean;
  /** Why, when the server said: it needs the EULA agreed to, or its port is taken. */
  reason: "eula" | "port" | null;
}

/** How friends reach a server over the link between Wisps. */
export interface LinkReach {
  /** This computer's address as the internet sees it. */
  public: string | null;
  /** The router changes ports per destination: direct links rarely get through it. */
  strictRouter: boolean;
  /** The router opened a way in (UPnP). */
  routerOpened: boolean;
}

export interface HostedServer {
  id: string;
  name: string;
  gameVersion: string;
  loader: Loader;
  loaderVersion: string | null;
  created: number;
  lastStarted: number | null;
  /** Its own memory in MB; absent when Wisp picks. */
  memoryMb?: number;
  /** The instance people play it with. */
  instanceId?: string;
  eula: boolean;
  friendsJoin: boolean;
  invited: string[];
  openPort: boolean;
  backups: boolean;
  icon?: string;
  /** The address people join from anywhere: a playit tunnel's, or your own. */
  address?: string;
  /** A plugin server (Paper and its kin) instead of the loader's own. */
  software?: ServerSoftware;
  startWithWisp?: boolean;
  /** Restarts every so many hours while it runs. */
  restartEveryHours?: number;
  aikarFlags?: boolean;
  jvmArgs?: string;
  /** Its own Java (8, 17, 21, 25); absent lets Wisp pick. */
  javaMajor?: number;
}

export type ServerSoftware = "paper" | "purpur" | "folia" | "spigot";

/** A plugin or mod in a server's own folder. */
export interface ServerItem {
  file: string;
  name: string;
  version: string;
  enabled: boolean;
  projectId: string | null;
}

/** A newer build of something in a server. */
export interface ServerContentUpdate {
  file: string;
  projectId: string;
  versionId: string;
  version: string;
}

export interface ServerContentView {
  /** What the server takes; null for plain Vanilla. */
  kind: "plugins" | "mods" | null;
  items: ServerItem[];
  /** The instance its mods come from, when it has one. */
  instanceName: string | null;
  instanceId: string | null;
}

/** A mod Wisp keeps out of a server: it only runs in the game (it crashed the server as it loaded). */
export interface SetAsideMod {
  file: string;
  name: string;
  sha1: string;
  at: number;
}

export interface HostedView extends HostedServer {
  state: HostedState;
  /** Setting up: downloads and installers. */
  progress: LaunchUpdate | null;
  players: string[];
  maxPlayers: number;
  port: number;
  lanAddress: string | null;
  publicAddress: string | null;
  startedAt: number | null;
  readyAt: number | null;
  exit: ServerExit | null;
  failure: string | null;
  effectiveMemoryMb: number;
  automaticMemory: boolean;
  /** What Wisp would pick. */
  pickedMemoryMb: number;
  modCount: number;
  motd: string;
  onlineMode: boolean;
  whitelist: boolean;
  worldExists: boolean;
  instanceName: string | null;
  /**
   * How players sign in: Microsoft accounts checked by Mojang, accounts checked by Wisp's skin
   * server (developer and tester builds, which play offline accounts), or nobody checked.
   */
  accounts: "mojang" | "wisp" | "offline";
  /** Friends let in this run. */
  friendsIn: string[];
  reach: LinkReach | null;
  setAside: SetAsideMod[];
  /** Its join code: anyone with Wisp joins with it. */
  code: string | null;
}

/** playit.gg on this computer (src-tauri/src/playit.rs). */
export interface PlayitView {
  /** This build runs playit's agent itself (tester builds). */
  builtIn: boolean;
  linked: boolean;
  /** The claim page, while Wisp waits for you to approve it. */
  linking: string | null;
  running: boolean;
  /** Something wrong with the link to show (playit.gg forgot this computer), if anything. */
  problem: string | null;
  /** playit's licence, shown in About by builds that carry its agent. */
  notice: string | null;
}

export type PlayitEvent =
  | { kind: "step"; step: "visit" | "approve" | "approved" }
  | { kind: "linked" }
  | { kind: "failed"; message: string };

export interface AddressCheck {
  reachable: boolean;
  hint: string | null;
}

export interface NewHostedServer {
  name: string;
  gameVersion: string;
  loader: Loader;
  loaderVersion: string | null;
  instanceId: string | null;
  world: { instanceId: string; folder: string } | null;
  properties: [string, string][];
  /** Ticked "I agree to the Minecraft EULA". */
  eula: boolean;
  software: ServerSoftware | null;
}

export interface ServerChanges {
  name?: string;
  friendsJoin?: boolean;
  openPort?: boolean;
  backups?: boolean;
  memoryMb?: number;
  automaticMemory?: boolean;
  /** The address people join from anywhere ("" takes it away). */
  address?: string;
  startWithWisp?: boolean;
  /** 0 turns scheduled restarts off. */
  restartEveryHours?: number;
  aikarFlags?: boolean;
  /** "" takes them away. */
  jvmArgs?: string;
  /** 0 lets Wisp pick. */
  javaMajor?: number;
}

export interface ConsoleLine {
  at: number;
  text: string;
  level: "info" | "warn" | "error" | "input" | "wisp";
}

export type HostingChange =
  | { serverId: string; kind: "changed" }
  | { serverId: string; kind: "progress"; update: LaunchUpdate }
  | { serverId: string; kind: "lines"; lines: ConsoleLine[] }
  | { serverId: string; kind: "players"; players: string[] }
  | { serverId: string; kind: "failed"; message: string }
  | { serverId: string; kind: "fixed"; message: string };

export type PlayerList = "whitelist" | "ops" | "banned";

export interface PlayerEntry {
  uuid: string;
  name: string;
}

export interface ServerPlayers {
  whitelist: PlayerEntry[];
  ops: PlayerEntry[];
  banned: PlayerEntry[];
  online: string[];
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
  /** What sync settled by itself in this run, a sentence each. */
  settled?: string[];
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
  /** Hosted with Wisp: joining from Wisp brings its mods and your skin along. */
  wisp?: boolean;
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
  /** A world changed on two computers is settled without asking: the copy played last wins, the other is kept. */
  settleConflicts: boolean;
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
  /** Settings changed in one instance reach the others on this computer when they next start. */
  shareOptions: boolean;
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
  | { phase: "syncing"; folder: string; waiting?: number }
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

// ── An instance's own page ──

export type ItemKind = "mod" | "resource-pack" | "shader" | "data-pack";
/** Who put a file there: the modpack the instance came from, Wisp, or the player by hand. */
export type ItemOrigin = "pack" | "wisp" | "yours";

/** One mod, resource pack, shader pack or data pack in an instance. */
export interface InstanceItem {
  kind: ItemKind;
  /** Where it is in the game folder: "mods/sodium.jar", with ".disabled" while it's off. */
  file: string;
  enabled: boolean;
  /** An unzipped pack: listed, but it can't be turned off. */
  folder: boolean;
  size: number;
  name: string;
  version: string;
  description: string;
  /** Modrinth's picture of the project. */
  iconUrl: string | null;
  /** A picture taken from the file itself, as a path in the data folder. */
  iconFile: string | null;
  projectId: string | null;
  versionId: string | null;
  origin: ItemOrigin;
  /** The world a data pack is in. */
  world: string | null;
  /** Something other mods need rather than something picked for itself. */
  library: boolean;
  /** The mods that can't run without it. */
  neededBy: string[];
  sha1: string;
}

export interface ItemUpdate {
  file: string;
  projectId: string;
  versionId: string;
  versionNumber: string;
}

/** A file taken out of an instance, waiting in case it's wanted back. */
export interface RemovedItem {
  id: string;
  file: string;
  at: number;
}

export interface InstanceStats {
  playtimeMs: number;
  sessions: number;
}

/** What an instance does differently on this computer (never synced). Unset means Wisp's setting. */
export interface LocalSettings {
  jvmArgs?: string | null;
  gameWindow?: GameWindow | null;
  hooks?: LaunchHooks | null;
  /** "screenshot:<file>", "none", or unset for its newest screenshot. */
  banner?: string | null;
}

export interface MoveItem {
  file: string;
  name: string;
  versionId?: string | null;
  versionNumber?: string | null;
}

/** What moving to another Minecraft version or loader does to an instance's mods. */
export interface MovePlan {
  updates: MoveItem[];
  unchanged: MoveItem[];
  withoutBuild: MoveItem[];
  unknown: MoveItem[];
}

export interface MovedInstance {
  instance: Instance;
  moved: { updated: string[]; turnedOff: string[]; failed: [string, string][] };
  restorePoints: number;
}

export interface UpdatedAll {
  updated: string[];
  failed: [string, string][];
}

export type LogKind = "latest" | "older" | "crash";

export interface LogFile {
  /** Relative to the game folder. */
  name: string;
  kind: LogKind;
  size: number;
  modified: number;
}

export interface PackRelease {
  versionId: string;
  versionNumber: string;
  gameVersions: string[];
  published: string;
}

export interface PackUpdates {
  /** The newest version for the instance's Minecraft version. */
  sameGame: PackRelease | null;
  /** The newest of all, when it's for another Minecraft version. */
  newerGame: PackRelease | null;
}

export interface PackUpdated {
  from: string;
  to: string;
  added: number;
  removed: number;
  movedTo: string | null;
}

export interface ExportOptions {
  version: string;
  configs: boolean;
  options: boolean;
}

export interface ExportedFile {
  path: string;
  downloads: number;
  bundled: number;
  bytes: number;
}
