import { PackagePlus } from "lucide-react";
import { AnimatePresence, LazyMotion, MotionConfig, domAnimation, m } from "motion/react";
import { lazy, startTransition, Suspense, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import heroArt from "./assets/art/hero-moon-shrine.jpg";
import type { HeroIntro } from "./components/hero/Hero";
import { StartupIntro } from "./components/intro/StartupIntro";
import { useIntroTimeline } from "./components/intro/useIntroTimeline";
import { NAV_ITEMS } from "./components/sidebar/nav";
import type { InstallTarget } from "./components/install/InstallDialog";
import { InstallPanel } from "./components/install/InstallPanel";
import { NotePrompt, type NoteRequest } from "./components/journal/NotePrompt";
import { LaunchPanel } from "./components/launch/LaunchPanel";
import { SyncGate, type Gate } from "./components/launch/SyncGate";
import { Sidebar } from "./components/sidebar/Sidebar";
import { Toasts } from "./components/Toasts";
import { TopBar } from "./components/TopBar";
import { UpdatePanel } from "./components/UpdatePanel";
import {
  canAddOfflineAccounts,
  checkLaunch,
  getHome,
  getInstalls,
  getJournal,
  getLaunches,
  getSettings,
  importWorld,
  importWorldPaths,
  installModpack,
  installModpackFile,
  onFileDrop,
  launchInstance,
  onInstallEvent,
  onJournalChange,
  onFixed,
  onLaunchEvent,
  onSocialChange,
  onSyncChange,
  resolveSync,
  saveSettings,
  syncNow,
  stopInstance,
  undoFix,
} from "./data/api";
import { DEFAULT_SETTINGS, EMPTY_HOME } from "./data/defaults";
import type { HomeSnapshot, InstallEvent, LaunchUpdate, PageId, Settings, StartPage, SyncItem, SyncResolution } from "./data/types";
import { useIdleWhenUnfocused, useLatest, useNow } from "./lib/hooks";
import { loadSkinsPage } from "./lib/preload";
import { HomePage } from "./pages/HomePage";
import { PageArrivalContext, type PageArrival } from "./components/ui/Page";
import { AppContext, type AppContextValue, type InstanceTab, type SettingsPatch } from "./state/AppContext";
import { useToasts } from "./state/useToasts";

// Home ships in the main bundle; every other page loads from disk the first time it's opened.
const InstancesPage = lazy(() => import("./pages/InstancesPage").then((module) => ({ default: module.InstancesPage })));
// The pages shown as tabs of another: each loads its siblings once it's open, so switching is instant.
const tabPageCode = {
  worlds: () => import("./pages/WorldsPage"),
  servers: () => import("./pages/ServersPage"),
  journal: () => import("./pages/JournalPage"),
  screenshots: () => import("./pages/ScreenshotsPage"),
} satisfies Partial<Record<PageId, () => Promise<unknown>>>;
const WorldsPage = lazy(() => tabPageCode.worlds().then((module) => ({ default: module.WorldsPage })));
const DiscoverPage = lazy(() => import("./pages/DiscoverPage").then((module) => ({ default: module.DiscoverPage })));
const ScreenshotsPage = lazy(() => tabPageCode.screenshots().then((module) => ({ default: module.ScreenshotsPage })));
const SyncPage = lazy(() => import("./pages/SyncPage").then((module) => ({ default: module.SyncPage })));
const SettingsPage = lazy(() => import("./pages/SettingsPage").then((module) => ({ default: module.SettingsPage })));
const ProjectPage = lazy(() => import("./pages/ProjectPage").then((module) => ({ default: module.ProjectPage })));
const WorldPage = lazy(() => import("./pages/WorldPage").then((module) => ({ default: module.WorldPage })));
const InstancePage = lazy(() => import("./pages/InstancePage").then((module) => ({ default: module.InstancePage })));
const JournalPage = lazy(() => tabPageCode.journal().then((module) => ({ default: module.JournalPage })));
const SkinsPage = lazy(() => loadSkinsPage().then((module) => ({ default: module.SkinsPage })));
const ServersPage = lazy(() => tabPageCode.servers().then((module) => ({ default: module.ServersPage })));
const FriendsPage = lazy(() => import("./pages/FriendsPage").then((module) => ({ default: module.FriendsPage })));
const ImportDialog = lazy(() => import("./components/install/ImportDialog").then((module) => ({ default: module.ImportDialog })));
const ImportWorldDialog = lazy(() =>
  import("./components/worlds/ImportWorldDialog").then((module) => ({ default: module.ImportWorldDialog })),
);
const InstallDialog = lazy(() => import("./components/install/InstallDialog").then((module) => ({ default: module.InstallDialog })));
const HostedServerPage = lazy(() => import("./pages/HostedServerPage").then((module) => ({ default: module.HostedServerPage })));
const NewServerDialog = lazy(() => import("./components/hosting/NewServerDialog").then((module) => ({ default: module.NewServerDialog })));
const NewInstanceDialog = lazy(() =>
  import("./components/instances/NewInstanceDialog").then((module) => ({ default: module.NewInstanceDialog })),
);
const AddAccountDialog = lazy(() =>
  import("./components/accounts/AddAccountDialog").then((module) => ({ default: module.AddAccountDialog })),
);

const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure));

/** A toast's worth of a fix report: up to three lines, then how many more there are. */
const brief = (lines: string[]) => (lines.length <= 3 ? lines : [...lines.slice(0, 2), `And ${lines.length - 2} more.`]).join("\n");

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  const next = { ...record };
  delete next[key];
  return next;
}

/** Settings apply instantly; the save waits for a pause, so dragging a slider writes once. */
const SAVE_DELAY_MS = 300;

const LAST_PAGE_KEY = "wisp.lastPage";
const RESTORABLE: PageId[] = ["home", "instances", "worlds", "servers", "discover", "screenshots", "journal", "skins", "friends", "sync", "settings"];

/** Pages that share a sidebar entry and show as tabs of one page. */
/** How long Play waits for files a cloud app is still bringing before it asks, and how often it looks. */
const FILES_WAIT_MS = 90_000;
const FILES_POLL_MS = 3_000;

const TAB_GROUPS: (keyof typeof tabPageCode)[][] = [
  ["worlds", "servers"],
  ["journal", "screenshots"],
];
const tabGroupOf = (page: PageId) => TAB_GROUPS.find((group) => (group as PageId[]).includes(page)) ?? null;
/** Whether going from one page to the other is a switch between tabs of the same page. */
const isTabSwitch = (from: PageId, to: PageId) => from !== to && tabGroupOf(from) !== null && tabGroupOf(from) === tabGroupOf(to);

/** Where Wisp was when it last closed, read once before this run starts recording its own pages. */
const LAST_PAGE: PageId = (() => {
  try {
    const last = localStorage.getItem(LAST_PAGE_KEY);
    return RESTORABLE.find((page) => page === last) ?? "home";
  } catch {
    return "home";
  }
})();

/**
 * Whether this window is Wisp coming back after a game it closed for (Settings → General → When a
 * game starts). Then there's no intro, and it opens where you were.
 */
const BACK_FROM_GAME = (window as { __WISP_FROM_GAME__?: boolean }).__WISP_FROM_GAME__ === true;

/** The page Wisp opens on, per Settings → General (or where you were, back from a game). */
function startingPage(start: StartPage): PageId {
  return start === "last" || BACK_FROM_GAME ? LAST_PAGE : start;
}

// The start page setting, mirrored here so the very first frame is already the right page (the
// settings themselves arrive a moment later from the launcher).
const START_PAGE_KEY = "wisp.startPage";
const INITIAL_PAGE: PageId = (() => {
  try {
    const start = localStorage.getItem(START_PAGE_KEY);
    if (BACK_FROM_GAME) return LAST_PAGE;
    return start === "home" || start === "instances" || start === "discover" || start === "last" ? startingPage(start) : "home";
  } catch {
    return "home";
  }
})();

export default function App() {
  const { toasts, notify, dismiss } = useToasts();
  const now = useNow();
  useIdleWhenUnfocused();

  // ── Library ──
  const [home, setHome] = useState<HomeSnapshot | null>(null);
  const refreshHome = useCallback(async () => {
    try {
      setHome(await getHome());
    } catch {
      // Keep what's on screen. On the first load, fall back to the empty states.
      setHome((current) => current ?? EMPTY_HOME);
    }
  }, []);
  useEffect(() => {
    void refreshHome();
  }, [refreshHome]);

  // ── Settings ──
  const [settings, setSettings] = useState<Settings | null>(null);
  // The latest settings, readable outside renders (saves and functional patches use it).
  const settingsRef = useRef<Settings | null>(null);
  const saveTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    let active = true;
    const adopt = (loaded: Settings) => {
      if (!active || settingsRef.current !== null) return;
      settingsRef.current = loaded;
      setSettings(loaded);
      // Only if nothing else was opened meanwhile.
      const start = startingPage(loaded.startPage);
      if (start !== INITIAL_PAGE) {
        setPage((current) => (current === INITIAL_PAGE ? start : current));
        if (start !== "home") setLeftHome(true);
      }
    };
    getSettings().then(adopt, () => adopt(DEFAULT_SETTINGS));
    return () => {
      active = false;
    };
  }, []);

  const updateSettings = useCallback(
    (patch: SettingsPatch) => {
      const current = settingsRef.current;
      if (!current) return;
      const next = { ...current, ...(typeof patch === "function" ? patch(current) : patch) };
      settingsRef.current = next;
      setSettings(next);

      window.clearTimeout(saveTimer.current);
      saveTimer.current = window.setTimeout(() => {
        const toSave = settingsRef.current;
        if (!toSave) return;
        saveSettings(toSave).then(
          (saved) => {
            // The Rust side may clamp values; take its copy unless something changed meanwhile.
            if (settingsRef.current !== toSave) return;
            settingsRef.current = saved;
            setSettings(saved);
          },
          () => notify({ tone: "info", title: "Couldn't save settings", body: "Your changes apply until Wisp closes." }),
        );
      }, SAVE_DELAY_MS);
    },
    [notify],
  );

  // ── Navigation and search ──
  const [page, setPage] = useState<PageId>(INITIAL_PAGE);
  const pageNow = useLatest(page);
  /** Once you've been elsewhere, Home makes its own entrance instead of the intro's hand-off. */
  const [leftHome, setLeftHome] = useState(INITIAL_PAGE !== "home");
  // Tabs of one page (Worlds and Servers) swap in place: no fade out and back in, and the old tab
  // stays up until the new one is ready rather than the page going blank.
  const [arrival, setArrival] = useState<PageArrival>("enter");
  const navigate = useCallback(
    (next: PageId) => {
      const tab = isTabSwitch(pageNow.current, next);
      setArrival(tab ? "tab" : "enter");
      if (tab) startTransition(() => setPage(next));
      else setPage(next);
      if (next !== "home") setLeftHome(true);
    },
    [pageNow],
  );
  useEffect(() => {
    for (const sibling of tabGroupOf(page) ?? []) void tabPageCode[sibling]().catch(() => {});
  }, [page]);

  // A server hosted on this computer, and the page to go back to from it.
  const [hostedRoute, setHostedRoute] = useState<{ id: string; from: PageId } | null>(null);
  const openHosted = useCallback(
    (id: string) => {
      setHostedRoute((current) => ({ id, from: pageNow.current === "hosted" ? (current?.from ?? "servers") : pageNow.current }));
      navigate("hosted");
    },
    [navigate, pageNow],
  );

  // A Modrinth project's page, and the page to go back to from it.
  const [projectRoute, setProjectRoute] = useState<{ id: string; from: PageId } | null>(null);
  const openProject = useCallback(
    (id: string) => {
      setProjectRoute((current) => ({ id, from: pageNow.current === "project" ? (current?.from ?? "discover") : pageNow.current }));
      navigate("project");
    },
    [navigate, pageNow],
  );

  // A world's page, and the page to go back to from it.
  const [worldRoute, setWorldRoute] = useState<{ id: string; from: PageId } | null>(null);
  const openWorld = useCallback(
    (id: string) => {
      setWorldRoute((current) => ({ id, from: pageNow.current === "world" ? (current?.from ?? "worlds") : pageNow.current }));
      navigate("world");
    },
    [navigate, pageNow],
  );

  // An instance's page (and which of its tabs), and the page to go back to from it.
  const [instanceRoute, setInstanceRoute] = useState<{ id: string; tab: InstanceTab; from: PageId; session: number } | null>(null);
  const openInstance = useCallback(
    (id: string, tab: InstanceTab = "content") => {
      setInstanceRoute((current) => ({
        id,
        tab,
        from: pageNow.current === "instance" ? (current?.from ?? "instances") : pageNow.current,
        session: (current?.session ?? 0) + 1,
      }));
      navigate("instance");
    },
    [navigate, pageNow],
  );
  // The instance Discover was opened for, so installing puts it first.
  const [discoverFor, setDiscoverFor] = useState<string | null>(null);
  const openDiscoverFor = useCallback(
    (instanceId: string) => {
      setDiscoverFor(instanceId);
      navigate("discover");
    },
    [navigate],
  );

  const mainRef = useRef<HTMLElement>(null);
  const arrivalNow = useLatest(arrival);
  useEffect(() => {
    const main = mainRef.current;
    if (!main) return;
    if (arrivalNow.current !== "tab") {
      main.scrollTo({ top: 0 });
      return;
    }
    // Switching tabs keeps your place, unless you'd scrolled past the tabs: then they come back up
    // to just under the top bar.
    const tabs = main.querySelector<HTMLElement>("[data-page-tabs]");
    if (!tabs) return;
    const cover = main.querySelector<HTMLElement>("[data-top-bar]")?.offsetHeight ?? 0;
    const top = tabs.getBoundingClientRect().top - main.getBoundingClientRect().top + main.scrollTop - cover - 8;
    if (main.scrollTop > top) main.scrollTo({ top });
  }, [page, projectRoute?.id, worldRoute?.id, arrivalNow]);

  // Remembered for "Open Wisp on: where you left off". A project page reopens as Discover, a world's as Worlds.
  useEffect(() => {
    const last = page === "project" ? (projectRoute?.from ?? "discover") : page === "world" ? (worldRoute?.from ?? "worlds") : page;
    try {
      localStorage.setItem(LAST_PAGE_KEY, last);
    } catch {
      // Storage can be unavailable; the setting then falls back to Home.
    }
  }, [page, projectRoute?.from, worldRoute?.from]);

  const startPage = settings?.startPage;
  useEffect(() => {
    if (!startPage) return;
    try {
      localStorage.setItem(START_PAGE_KEY, startPage);
    } catch {
      // Without storage the first frame shows Home, then the start page.
    }
  }, [startPage]);

  // Settings → Appearance → Motion, for CSS animations (Motion's own follow MotionConfig below).
  const motion = settings?.motion ?? "system";
  useEffect(() => {
    document.documentElement.dataset.motion = motion;
  }, [motion]);

  const [query, setQuery] = useState("");
  const onQueryChange = useCallback(
    (value: string) => {
      setQuery(value);
      if (value.trim()) navigate("discover");
    },
    [navigate],
  );

  // ── Startup intro ──
  const intro = useIntroTimeline(settings ? (BACK_FROM_GAME ? "off" : settings.intro) : null);
  const revealed = intro.phase === "dock" || intro.phase === "done";
  const heroIntro: HeroIntro = leftHome
    ? null
    : intro.phase !== "done"
      ? intro.mode === "off"
        ? null
        : "flying"
      : intro.played
        ? "landed"
        : null;

  // ── New instance dialog: mounted fresh for each opening, so the form starts clean ──
  const [dialog, setDialog] = useState({ open: false, session: 0 });
  const openNewInstance = useCallback(() => setDialog((current) => (current.open ? current : { open: true, session: current.session + 1 })), []);
  const closeNewInstance = useCallback(() => setDialog((current) => ({ ...current, open: false })), []);

  // ── Host a server dialog, the same way; it can start from an instance and one of its worlds ──
  const [serverDialog, setServerDialog] = useState<{ open: boolean; session: number; instanceId: string | null; world: string | null }>({ open: false, session: 0, instanceId: null, world: null });
  const hostServer = useCallback(
    (from?: { instanceId?: string | null; world?: string | null }) =>
      setServerDialog((current) => (current.open ? current : { open: true, session: current.session + 1, instanceId: from?.instanceId ?? null, world: from?.world ?? null })),
    [],
  );
  const closeServerDialog = useCallback(() => setServerDialog((current) => ({ ...current, open: false })), []);

  const comingSoon = useCallback(
    (feature: string) =>
      notify({ tone: "resting", title: `${feature} is coming soon`, body: "This build has the interface; the launcher core comes next." }),
    [notify],
  );

  // ── Journal ──
  const [journalVersion, setJournalVersion] = useState(0);
  useEffect(
    () =>
      onJournalChange(() => {
        setJournalVersion((version) => version + 1);
        // Notes and screenshots show on world cards.
        void refreshHome();
      }),
    [refreshHome],
  );
  const [noteRequest, setNoteRequest] = useState<NoteRequest | null>(null);

  // Friend requests, accepts and party invites show wherever you are.
  useEffect(() => onSocialChange((notice) => notice && notify({ tone: "info", title: notice.title, body: notice.body ?? undefined })), [notify]);
  const settingsNow = useLatest(settings);

  // ── Launching ──
  const [launches, setLaunches] = useState<Record<string, LaunchUpdate>>({});
  /** The world each launch opens straight into (asked for here; the launcher confirms it). */
  const [launchWorlds, setLaunchWorlds] = useState<Record<string, string | null>>({});
  /** The server each launch joins straight away. */
  const [launchServers, setLaunchServers] = useState<Record<string, string | null>>({});
  const requestedWorlds = useRef<Record<string, string>>({});
  const homeNow = useLatest(home);

  useEffect(() => {
    let active = true;
    getLaunches().then((current) => {
      if (!active) return;
      setLaunches(Object.fromEntries(current.map((event) => [event.instanceId, event.update])));
      setLaunchWorlds(Object.fromEntries(current.map((event) => [event.instanceId, event.world])));
      setLaunchServers(Object.fromEntries(current.map((event) => [event.instanceId, event.server ?? null])));
    }, () => {});
    const stopListening = onLaunchEvent(({ instanceId, world, server, update }) => {
      // A normal quit clears the card; a crash or failure stays until dismissed.
      setLaunches((current) =>
        update.phase === "exited" && !update.crashed ? without(current, instanceId) : { ...current, [instanceId]: update },
      );
      if (update.phase === "running") {
        const asked = requestedWorlds.current[instanceId];
        setLaunchWorlds((current) => ({ ...current, [instanceId]: world }));
        setLaunchServers((current) => ({ ...current, [instanceId]: server ?? null }));
        if (asked && !world) {
          const instance = homeNow.current?.instances.find((candidate) => candidate.id === instanceId);
          const name = homeNow.current?.worlds.find((candidate) => candidate.instanceId === instanceId && candidate.folder === asked)?.name ?? asked;
          notify({
            tone: "info",
            title: `Pick ${name} from Singleplayer`,
            body: `Minecraft ${instance?.gameVersion ?? "this version"} opens on its title screen; going straight into a world needs 1.20 or newer.`,
          });
        }
      }
      if (update.phase === "exited" || update.phase === "failed") {
        delete requestedWorlds.current[instanceId];
        if (update.phase === "exited" && !update.crashed) {
          setLaunchWorlds((current) => without(current, instanceId));
          setLaunchServers((current) => without(current, instanceId));
        }
      }
      if (update.phase === "running" || update.phase === "exited") void refreshHome();
      // Offer a note about the world that was just played.
      if (update.phase === "exited" && !update.crashed && settingsNow.current?.journal.askForNote !== false) {
        getJournal({ instanceId, kinds: ["session"], limit: 1 }).then(async ([entry]) => {
          if (!entry || entry.event.kind !== "session" || entry.event.worlds.length === 0) return;
          const folder = world ?? entry.event.worlds[0];
          const fresh = await getHome().catch(() => homeNow.current);
          const name = fresh?.worlds.find((candidate) => candidate.instanceId === instanceId && candidate.folder === folder)?.name ?? folder;
          setNoteRequest({ instanceId, world: folder, worldName: name, session: entry.id });
        }, () => {});
      }
    });
    return () => {
      active = false;
      stopListening();
    };
  }, [refreshHome, notify, homeNow, settingsNow]);

  // The add-account dialog, and an instance waiting to play once an account exists.
  const [accountDialog, setAccountDialog] = useState({ open: false, session: 0 });
  const playAfterAccount = useRef<{ instanceId: string; world: string | null; server: string | null } | null>(null);

  // With cloud sync on, Play first brings the newest copy here. Files still on their way are
  // waited for (the cloud app is asked for them); it stops to ask only when a world changed on two
  // computers and isn't settled by itself, the instance is open on another one, or the files
  // haven't come after a while.
  const [gate, setGate] = useState<Gate | null>(null);
  const launchesNow = useLatest(launches);

  const startLaunch = useCallback(
    (instanceId: string, world: string | null = null, server: string | null = null, skipSync = false) => {
      const begin = () => {
        setLaunches((current) => ({ ...current, [instanceId]: { phase: "checking" } }));
        setLaunchWorlds((current) => ({ ...current, [instanceId]: world }));
        setLaunchServers((current) => ({ ...current, [instanceId]: server }));
        if (world) requestedWorlds.current[instanceId] = world;
        else delete requestedWorlds.current[instanceId];
        launchInstance(instanceId, world, server).catch((failure: unknown) => {
          setLaunches((current) => without(current, instanceId));
          setLaunchWorlds((current) => without(current, instanceId));
          setLaunchServers((current) => without(current, instanceId));
          notify({ tone: "info", title: "Couldn't start the game", body: message(failure) });
        });
      };
      const sync = homeNow.current?.sync;
      if (skipSync || sync?.state !== "on") return begin();
      setLaunches((current) => ({ ...current, [instanceId]: { phase: "syncing", folder: sync.label } }));
      const startedAt = Date.now();
      const look = () =>
        checkLaunch(instanceId).then(
          (check) => {
            // Cancelled from the launch card while it waited.
            if (launchesNow.current[instanceId]?.phase !== "syncing") return;
            if (check.conflicts.length === 0 && !check.inUse && check.waiting === 0) return begin();
            if (check.conflicts.length === 0 && !check.inUse && Date.now() - startedAt < FILES_WAIT_MS) {
              setLaunches((current) => ({ ...current, [instanceId]: { phase: "syncing", folder: sync.label, waiting: check.waiting } }));
              window.setTimeout(() => void look(), FILES_POLL_MS);
              return;
            }
            setLaunches((current) => without(current, instanceId));
            const instanceName = homeNow.current?.instances.find((candidate) => candidate.id === instanceId)?.name ?? "This instance";
            setGate({ instanceId, instanceName, world, server, folder: sync.label, check });
          },
          // Trouble with sync never stops you playing.
          () => begin(),
        );
      void look();
    },
    [notify, homeNow, launchesNow],
  );

  const resolveAtGate = useCallback(
    async (item: SyncItem, resolution: SyncResolution) => {
      try {
        await resolveSync(item.key, resolution);
      } catch (failure) {
        notify({ tone: "info", title: `Couldn't settle ${item.name}`, body: message(failure) });
        return;
      }
      void refreshHome();
      // Once every world is settled, the game starts by itself.
      setGate((current) => {
        if (!current) return current;
        const conflicts = current.check.conflicts.filter((other) => other.key !== item.key);
        if (conflicts.length === 0 && !current.check.inUse && current.check.waiting === 0) {
          window.setTimeout(() => startLaunch(current.instanceId, current.world, current.server, true), 0);
          return null;
        }
        return { ...current, check: { ...current.check, conflicts } };
      });
    },
    [notify, refreshHome, startLaunch],
  );

  // Sync finished something: badges and the top bar catch up.
  // What sync settled by itself (a world changed on two computers) is said once.
  const settledSeen = useRef(new Set<string>());
  useEffect(
    () =>
      onSyncChange((overview) => {
        void refreshHome();
        for (const line of overview.settled ?? []) {
          const key = `${overview.lastSynced ?? 0}|${line}`;
          if (settledSeen.current.has(key)) continue;
          settledSeen.current.add(key);
          notify({ tone: "info", title: "Wisp settled a world for you", body: line, lifetimeMs: 14_000 });
        }
      }),
    [refreshHome, notify],
  );

  // Coming back to Wisp after a while (say, from playing on another computer): sync straight
  // away rather than at the next scheduled check.
  useEffect(() => {
    const onFocus = () => {
      const sync = homeNow.current?.sync;
      if (sync?.state === "on" && Date.now() - (sync.lastSynced ?? 0) > 2 * 60_000) void syncNow().catch(() => {});
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [homeNow]);

  // What Wisp fixed before starting a game (or after a crash it restarted from).
  useEffect(
    () =>
      onFixed(({ instanceId, report }) => {
        const name = homeNow.current?.instances.find((candidate) => candidate.id === instanceId)?.name ?? "this instance";
        const entry = report.entry;
        if (report.fixed.length > 0) {
          notify({
            tone: "success",
            title: report.fixed.length === 1 ? `Fixed a problem with ${name}` : `Fixed ${report.fixed.length} problems with ${name}`,
            body: brief(report.fixed),
            lifetimeMs: 14_000,
            action: entry
              ? {
                  label: "Undo",
                  run: () =>
                    void undoFix(entry).then(
                      () => notify({ tone: "info", title: "Put back the way it was", body: "The fix is undone. You'll find it in the journal." }),
                      (failure: unknown) => notify({ tone: "info", title: "Couldn't undo it", body: message(failure) }),
                    ),
                }
              : undefined,
          });
        }
        if (report.unsolved.length > 0) notify({ tone: "info", title: `Something may stop ${name}`, body: brief(report.unsolved), lifetimeMs: 14_000 });
        void refreshHome();
      }),
    [notify, homeNow, refreshHome],
  );

  const addAccount = useCallback(() => {
    if (canAddOfflineAccounts) setAccountDialog((current) => (current.open ? current : { open: true, session: current.session + 1 }));
    else notify({ tone: "resting", title: "Microsoft sign-in is coming soon", body: "You'll sign in with the Microsoft account that owns Minecraft." });
  }, [notify]);

  const play = useCallback(
    (instanceId: string, world: string | null = null, server: string | null = null) => {
      if (!homeNow.current?.account) {
        playAfterAccount.current = { instanceId, world, server };
        addAccount();
        return;
      }
      startLaunch(instanceId, world, server);
    },
    [homeNow, addAccount, startLaunch],
  );

  const stop = useCallback(
    (instanceId: string) => {
      // Still syncing (waiting for files): nothing has started yet, so cancelling is just letting go.
      if (launchesNow.current[instanceId]?.phase === "syncing") {
        setLaunches((current) => without(current, instanceId));
        return;
      }
      stopInstance(instanceId).catch((failure: unknown) => notify({ tone: "info", title: "Couldn't stop the game", body: message(failure) }));
    },
    [notify, launchesNow],
  );

  const dismissLaunch = useCallback((instanceId: string) => setLaunches((current) => without(current, instanceId)), []);

  const onAccountAdded = useCallback(() => {
    // Read it now: the dialog's close (which clears it) runs before the refresh finishes.
    const waiting = playAfterAccount.current;
    playAfterAccount.current = null;
    void refreshHome().then(() => {
      if (waiting) startLaunch(waiting.instanceId, waiting.world, waiting.server);
    });
  }, [refreshHome, startLaunch]);

  const closeAccountDialog = useCallback(() => {
    playAfterAccount.current = null;
    setAccountDialog((current) => ({ ...current, open: false }));
  }, []);

  // ── Installing ──
  const [installDialog, setInstallDialog] = useState<{ open: boolean; session: number; target: InstallTarget | null }>({
    open: false,
    session: 0,
    target: null,
  });
  const requestInstall = useCallback(
    (target: InstallTarget) => setInstallDialog((current) => ({ open: true, session: current.session + 1, target })),
    [],
  );
  const closeInstallDialog = useCallback(() => setInstallDialog((current) => ({ ...current, open: false })), []);

  const [installs, setInstalls] = useState<Record<string, InstallEvent>>({});
  useEffect(() => {
    let active = true;
    getInstalls().then(
      (current) => active && setInstalls(Object.fromEntries(current.map((event) => [event.jobId, event]))),
      () => {},
    );
    const stopListening = onInstallEvent((event) => {
      if (event.update.phase === "done") {
        setInstalls((current) => without(current, event.jobId));
        void refreshHome();
        const skipped = event.update.skipped ?? [];
        notify(
          skipped.length
            ? {
                tone: "info",
                title: `${event.title} is installed, but missing ${skipped.length === 1 ? "a file" : `${skipped.length} files`}`,
                body: `Their authors only allow downloads from CurseForge's website: ${skipped.join(", ")}. Download them there and put them in the instance's mods folder.`,
              }
            : { tone: "success", title: `${event.title} is ready`, body: "It's in Instances. Press Play to start." },
        );
      } else {
        setInstalls((current) => ({ ...current, [event.jobId]: event }));
      }
    });
    return () => {
      active = false;
      stopListening();
    };
  }, [refreshHome, notify]);

  const startModpackInstall = useCallback(
    (target: InstallTarget) => {
      setInstalls((current) => ({ ...current, [target.projectId]: { jobId: target.projectId, title: target.title, update: { phase: "preparing" } } }));
      installModpack(target.projectId, target.version?.id ?? null, target.title).catch((failure: unknown) => {
        setInstalls((current) => without(current, target.projectId));
        notify({ tone: "info", title: "Couldn't install the modpack", body: message(failure) });
      });
    },
    [notify],
  );
  const startModpackFileInstall = useCallback(
    (path: string, title: string) => {
      const jobId = `file:${path}`;
      setInstalls((current) => ({ ...current, [jobId]: { jobId, title, update: { phase: "preparing" } } }));
      installModpackFile(path, title).catch((failure: unknown) => {
        setInstalls((current) => without(current, jobId));
        notify({ tone: "info", title: "Couldn't install the modpack", body: message(failure) });
      });
    },
    [notify],
  );
  const dismissInstall = useCallback((jobId: string) => setInstalls((current) => without(current, jobId)), []);

  // ── Bringing in files: modpacks become instances, mods and packs go into one ──
  const [fileImport, setFileImport] = useState<{ open: boolean; session: number; files: string[] | null; instanceId: string | null }>({
    open: false,
    session: 0,
    files: null,
    instanceId: null,
  });
  const importFiles = useCallback(
    (files: string[] | null = null, instanceId: string | null = null) =>
      setFileImport((current) => ({ open: true, session: current.session + 1, files, instanceId })),
    [],
  );
  // Files dropped anywhere go to the import dialog, except on pages that take their own drops
  // (worlds on Worlds, skins on Skins).
  const [droppingFiles, setDroppingFiles] = useState(false);
  useEffect(
    () =>
      onFileDrop((drop) => {
        const own = pageNow.current === "worlds" || pageNow.current === "skins";
        if (drop.type === "leave" || own) {
          setDroppingFiles(false);
          return;
        }
        if (drop.type === "over") {
          setDroppingFiles(drop.paths.length > 0);
          return;
        }
        setDroppingFiles(false);
        if (drop.paths.length > 0) importFiles(drop.paths);
      }),
    [importFiles, pageNow],
  );

  // ── Importing worlds: straight to the file picker with one instance, else ask which ──
  const [worldImport, setWorldImport] = useState<{ open: boolean; session: number; files: string[] | null }>({ open: false, session: 0, files: null });
  const runWorldImport = useCallback(
    (instanceId: string, files: string[] | null) => {
      const job = files ? importWorldPaths(instanceId, files) : importWorld(instanceId);
      job.then(
        (added) => {
          if (added.length === 0) return;
          void refreshHome();
          notify({ tone: "success", title: added.length === 1 ? "World imported" : `${added.length} worlds imported`, body: "They're in Worlds, ready to play." });
        },
        (failure: unknown) => notify({ tone: "info", title: "Couldn't import that", body: message(failure) }),
      );
    },
    [notify, refreshHome],
  );
  const importWorlds = useCallback(
    (files: string[] | null = null) => {
      const instances = homeNow.current?.instances ?? [];
      if (instances.length === 0) {
        notify({ tone: "info", title: "Make an instance first", body: "Worlds live in instances. Make one with the Minecraft version your world is from." });
        openNewInstance();
      } else if (instances.length === 1) {
        runWorldImport(instances[0].id, files);
      } else {
        setWorldImport((current) => ({ open: true, session: current.session + 1, files }));
      }
    },
    [homeNow, notify, openNewInstance, runWorldImport],
  );

  // ── Shortcuts: ⌘1–⌘7 pages, ⌘, settings, ⌘B sidebar (Ctrl on Windows and Linux). ⌘K lives in the top bar. ──
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return;
      if (event.key.toLowerCase() === "b") {
        event.preventDefault();
        updateSettings((current) => ({ sidebarCollapsed: !current.sidebarCollapsed }));
      } else if (event.key === ",") {
        event.preventDefault();
        navigate("settings");
      } else if (/^[1-9]$/.test(event.key)) {
        const item = NAV_ITEMS[Number(event.key) - 1];
        if (!item) return;
        event.preventDefault();
        navigate(item.id);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate, updateSettings]);

  const context = useMemo<AppContextValue>(
    () => ({
      page,
      navigate,
      home,
      refreshHome,
      settings,
      updateSettings,
      notify,
      comingSoon,
      openNewInstance,
      replayIntro: intro.replay,
      launches,
      launchWorlds,
      launchServers,
      play,
      stop,
      dismissLaunch,
      addAccount,
      openProject,
      openWorld,
      openInstance,
      openHosted,
      hostServer,
      discoverFor,
      openDiscoverFor,
      importWorlds,
      requestInstall,
      installs,
      startModpackInstall,
      startModpackFileInstall,
      dismissInstall,
      importFiles,
      journalVersion,
      now,
    }),
    [
      page,
      navigate,
      home,
      refreshHome,
      settings,
      updateSettings,
      notify,
      comingSoon,
      openNewInstance,
      intro.replay,
      launches,
      launchWorlds,
      launchServers,
      play,
      stop,
      dismissLaunch,
      addAccount,
      openProject,
      openWorld,
      openInstance,
      openHosted,
      hostServer,
      discoverFor,
      openDiscoverFor,
      importWorlds,
      requestInstall,
      installs,
      startModpackInstall,
      startModpackFileInstall,
      dismissInstall,
      importFiles,
      journalVersion,
      now,
    ],
  );

  return (
    <MotionConfig reducedMotion={motion === "reduced" ? "always" : motion === "full" ? "never" : "user"}>
      <LazyMotion features={domAnimation} strict>
        <AppContext value={context}>
          <div className="flex h-full overflow-hidden">
            <Sidebar revealed={revealed} logoVisible={intro.phase === "done"} />

            <main ref={mainRef} className="relative min-w-0 flex-1 overflow-y-auto overflow-x-hidden">
              <TopBar
                sync={home?.sync ?? null}
                onOpenSync={() => navigate("sync")}
                revealed={revealed}
                query={query}
                onQueryChange={onQueryChange}
              />

              {/* Tabs of one page (worlds and servers, journal and screenshots) share a key, so moving
                  between them swaps the page in place instead of fading it out and back. */}
              <PageArrivalContext value={arrival}>
                <AnimatePresence mode="wait" initial={false}>
                  {page === "home" && <HomePage key="home" revealed={revealed} heroIntro={heroIntro} />}
                  {page === "instances" && <Lazy key="instances" page={<InstancesPage />} />}
                  {page === "worlds" && <Lazy key="worlds" page={<WorldsPage />} />}
                  {page === "discover" && <Lazy key="discover" page={<DiscoverPage query={query} onClearQuery={() => setQuery("")} />} />}
                  {page === "screenshots" && <Lazy key="journal" page={<ScreenshotsPage />} />}
                  {page === "servers" && <Lazy key="worlds" page={<ServersPage />} />}
                  {page === "journal" && <Lazy key="journal" page={<JournalPage />} />}
                  {page === "skins" && <Lazy key="skins" page={<SkinsPage />} />}
                  {page === "friends" && <Lazy key="friends" page={<FriendsPage />} />}
                  {page === "sync" && <Lazy key="sync" page={<SyncPage />} />}
                  {page === "settings" && <Lazy key="settings" page={<SettingsPage />} />}
                  {page === "project" && projectRoute && (
                    <Lazy
                      key={`project-${projectRoute.id}`}
                      page={<ProjectPage projectId={projectRoute.id} backTo={projectRoute.from} onBack={() => navigate(projectRoute.from)} />}
                    />
                  )}
                  {page === "world" && worldRoute && (
                    <Lazy key={`world-${worldRoute.id}`} page={<WorldPage worldId={worldRoute.id} backTo={worldRoute.from} onBack={() => navigate(worldRoute.from)} />} />
                  )}
                  {page === "hosted" && hostedRoute && (
                    <Lazy key={`hosted-${hostedRoute.id}`} page={<HostedServerPage serverId={hostedRoute.id} backTo={hostedRoute.from} onBack={() => navigate(hostedRoute.from)} />} />
                  )}
                  {page === "instance" && instanceRoute && (
                    <Lazy
                      key={`instance-${instanceRoute.id}-${instanceRoute.session}`}
                      page={<InstancePage instanceId={instanceRoute.id} initialTab={instanceRoute.tab} backTo={instanceRoute.from} onBack={() => navigate(instanceRoute.from)} />}
                    />
                  )}
                </AnimatePresence>
              </PageArrivalContext>
            </main>
          </div>

          {dialog.session > 0 && (
            <Suspense fallback={null}>
              <NewInstanceDialog key={dialog.session} open={dialog.open} onClose={closeNewInstance} />
            </Suspense>
          )}
          {serverDialog.session > 0 && (
            <Suspense fallback={null}>
              <NewServerDialog key={serverDialog.session} open={serverDialog.open} onClose={closeServerDialog} instanceId={serverDialog.instanceId} world={serverDialog.world} />
            </Suspense>
          )}
          {accountDialog.session > 0 && (
            <Suspense fallback={null}>
              <AddAccountDialog key={accountDialog.session} open={accountDialog.open} onClose={closeAccountDialog} onAdded={onAccountAdded} />
            </Suspense>
          )}
          {worldImport.session > 0 && (
            <Suspense fallback={null}>
              <ImportWorldDialog
                key={worldImport.session}
                open={worldImport.open}
                instances={home?.instances ?? []}
                files={worldImport.files}
                onClose={() => setWorldImport((current) => ({ ...current, open: false }))}
                onImport={(instanceId) => {
                  setWorldImport((current) => ({ ...current, open: false }));
                  runWorldImport(instanceId, worldImport.files);
                }}
              />
            </Suspense>
          )}
          {fileImport.session > 0 && (
            <Suspense fallback={null}>
              <ImportDialog
                key={fileImport.session}
                files={fileImport.files}
                instanceId={fileImport.instanceId}
                open={fileImport.open}
                onClose={() => setFileImport((current) => ({ ...current, open: false }))}
              />
            </Suspense>
          )}
          <AnimatePresence>{droppingFiles && <FileDropHint key="drop" />}</AnimatePresence>
          {installDialog.session > 0 && installDialog.target && (
            <Suspense fallback={null}>
              <InstallDialog key={installDialog.session} target={installDialog.target} open={installDialog.open} onClose={closeInstallDialog} />
            </Suspense>
          )}
          <SyncGate
            gate={gate}
            now={now}
            onResolve={resolveAtGate}
            onPlay={() => {
              const current = gate;
              setGate(null);
              if (current) startLaunch(current.instanceId, current.world, current.server, true);
            }}
            onRetry={() => {
              const current = gate;
              setGate(null);
              if (current) startLaunch(current.instanceId, current.world, current.server);
            }}
            onCancel={() => setGate(null)}
          />
          <Toasts toasts={toasts} onDismiss={dismiss}>
            <AnimatePresence>{noteRequest && <NotePrompt key={noteRequest.session ?? noteRequest.world} request={noteRequest} onDone={() => setNoteRequest(null)} />}</AnimatePresence>
            <InstallPanel />
            <LaunchPanel />
          </Toasts>
          {/* A new Wisp is announced in its own window, above everything. */}
          <UpdatePanel />
          {intro.phase !== "done" && (
            <StartupIntro
              phase={intro.phase}
              quick={intro.mode === "quick"}
              art={heroArt}
              railOnly={settings?.sidebarCollapsed ?? false}
              onSkip={intro.skip}
            />
          )}
        </AppContext>
      </LazyMotion>
    </MotionConfig>
  );
}

/** Shown while files are dragged over the window, on pages that don't take their own drops. */
function FileDropHint() {
  return (
    <m.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="pointer-events-none fixed inset-0 z-[70] grid place-items-center bg-ink-950/70 p-10 backdrop-blur-sm"
    >
      <m.div
        initial={{ scale: 0.94, y: 10 }}
        animate={{ scale: 1, y: 0 }}
        className="flex flex-col items-center gap-3 rounded-[28px] border-2 border-dashed border-cream/40 bg-ink-800/90 px-14 py-12 text-center"
      >
        <PackagePlus size={40} className="text-cream" aria-hidden="true" />
        <p className="font-display text-2xl font-bold">Drop to add</p>
        <p className="text-sm text-mute">Modpacks become instances. Mods and packs go into one you pick.</p>
      </m.div>
    </m.div>
  );
}

/** A lazily loaded page. Its own Suspense boundary keeps the page it replaces on screen meanwhile. */
function Lazy({ page }: { page: ReactNode }) {
  return <Suspense fallback={null}>{page}</Suspense>;
}
