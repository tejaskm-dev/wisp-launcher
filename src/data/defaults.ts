import type { HomeSnapshot, Settings } from "./types";

/** Mirrors `Settings::default()` in crates/wisp-core/src/settings.rs. */
export const DEFAULT_SETTINGS: Settings = {
  intro: "full",
  sidebarCollapsed: false,
  startPage: "home",
  whilePlaying: "hide",
  motion: "system",
  home: { worlds: true, quickActions: true, modpacks: true, news: true },
  memoryMb: 4096,
  gameWindow: { mode: "default", width: 1280, height: 720 },
  jvmArgs: "",
  javaPaths: {},
  curseforgeApiKey: "",
  hooks: { preLaunch: "", wrapper: "", postExit: "" },
  newInstances: { copyOptionsFrom: null, skipWelcome: false, skipTutorial: false, autoJumpOff: false },
  concurrentDownloads: 16,
  externalImages: true,
  sync: { worlds: true, options: true, servers: true, resourcePacks: true, screenshots: false },
  journal: { autoRestorePoints: true, keepRestorePoints: 10, askForNote: true },
};

/** Used if reading the local library fails, so every screen still renders its empty states. */
export const EMPTY_HOME: HomeSnapshot = {
  account: null,
  device: { name: "This computer", os: "" },
  instances: [],
  servers: [],
  worlds: [],
  sync: { state: "off" },
};
