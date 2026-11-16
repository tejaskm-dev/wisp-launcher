import { AnimatePresence, LazyMotion, MotionConfig, domAnimation } from "motion/react";
import { lazy, Suspense, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import heroArt from "./assets/art/hero-moon-shrine.jpg";
import type { HeroIntro } from "./components/hero/Hero";
import { StartupIntro } from "./components/intro/StartupIntro";
import { useIntroTimeline } from "./components/intro/useIntroTimeline";
import { NAV_ITEMS } from "./components/sidebar/nav";
import type { InstallTarget } from "./components/install/InstallDialog";
import { InstallPanel } from "./components/install/InstallPanel";
import { NotePrompt, type NoteRequest } from "./components/journal/NotePrompt";
import { LaunchPanel } from "./components/launch/LaunchPanel";
import { Sidebar } from "./components/sidebar/Sidebar";
import { Toasts } from "./components/Toasts";
import { TopBar } from "./components/TopBar";
import {
  canAddOfflineAccounts,
  getHome,
  getInstalls,
  getJournal,
  getLaunches,
  getSettings,
  importWorld,
  importWorldPaths,
  installModpack,
  launchInstance,
  onInstallEvent,
  onJournalChange,
  onLaunchEvent,
  onSocialChange,
  saveSettings,
  stopInstance,
} from "./data/api";
import { DEFAULT_SETTINGS, EMPTY_HOME } from "./data/defaults";
import type { HomeSnapshot, InstallEvent, LaunchUpdate, PageId, Settings, StartPage } from "./data/types";
import { useIdleWhenUnfocused, useLatest, useNow } from "./lib/hooks";
import { HomePage } from "./pages/HomePage";
import { AppContext, type AppContextValue, type SettingsPatch } from "./state/AppContext";
import { useToasts } from "./state/useToasts";

// Home ships in the main bundle; every other page loads from disk the first time it's opened.
const InstancesPage = lazy(() => import("./pages/InstancesPage").then((module) => ({ default: module.InstancesPage })));
const WorldsPage = lazy(() => import("./pages/WorldsPage").then((module) => ({ default: module.WorldsPage })));
const DiscoverPage = lazy(() => import("./pages/DiscoverPage").then((module) => ({ default: module.DiscoverPage })));
const ScreenshotsPage = lazy(() => import("./pages/ScreenshotsPage").then((module) => ({ default: module.ScreenshotsPage })));
const SyncPage = lazy(() => import("./pages/SyncPage").then((module) => ({ default: module.SyncPage })));
const SettingsPage = lazy(() => import("./pages/SettingsPage").then((module) => ({ default: module.SettingsPage })));
const ProjectPage = lazy(() => import("./pages/ProjectPage").then((module) => ({ default: module.ProjectPage })));
const WorldPage = lazy(() => import("./pages/WorldPage").then((module) => ({ default: module.WorldPage })));
const JournalPage = lazy(() => import("./pages/JournalPage").then((module) => ({ default: module.JournalPage })));
const SkinsPage = lazy(() => import("./pages/SkinsPage").then((module) => ({ default: module.SkinsPage })));
const FriendsPage = lazy(() => import("./pages/FriendsPage").then((module) => ({ default: module.FriendsPage })));
const ImportWorldDialog = lazy(() =>
  import("./components/worlds/ImportWorldDialog").then((module) => ({ default: module.ImportWorldDialog })),
);
const InstallDialog = lazy(() => import("./components/install/InstallDialog").then((module) => ({ default: module.InstallDialog })));
const NewInstanceDialog = lazy(() =>
  import("./components/instances/NewInstanceDialog").then((module) => ({ default: module.NewInstanceDialog })),
);
const AddAccountDialog = lazy(() =>
  import("./components/accounts/AddAccountDialog").then((module) => ({ default: module.AddAccountDialog })),
);

const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure));

function without<T>(record: Record<string, T>, key: string): Record<string, T> {
  const next = { ...record };
  delete next[key];
  return next;
}

/** Settings apply instantly; the save waits for a pause, so dragging a slider writes once. */
const SAVE_DELAY_MS = 300;

const LAST_PAGE_KEY = "wisp.lastPage";
const RESTORABLE: PageId[] = ["home", "instances", "worlds", "discover", "screenshots", "journal", "skins", "friends", "sync", "settings"];

/** Where Wisp was when it last closed, read once before this run starts recording its own pages. */
const LAST_PAGE: PageId = (() => {
  try {
    const last = localStorage.getItem(LAST_PAGE_KEY);
    return RESTORABLE.find((page) => page === last) ?? "home";
  } catch {
    return "home";
  }
})();

/** The page Wisp opens on, per Settings → General. */
function startingPage(start: StartPage): PageId {
  return start === "last" ? LAST_PAGE : start;
}

// The start page setting, mirrored here so the very first frame is already the right page (the
// settings themselves arrive a moment later from the launcher).
const START_PAGE_KEY = "wisp.startPage";
const INITIAL_PAGE: PageId = (() => {
  try {
    const start = localStorage.getItem(START_PAGE_KEY);
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
  /** Once you've been elsewhere, Home makes its own entrance instead of the intro's hand-off. */
  const [leftHome, setLeftHome] = useState(INITIAL_PAGE !== "home");
  const navigate = useCallback((next: PageId) => {
    setPage(next);
    if (next !== "home") setLeftHome(true);
  }, []);

  // A Modrinth project's page, and the page to go back to from it.
  const [projectRoute, setProjectRoute] = useState<{ id: string; from: PageId } | null>(null);
  const pageNow = useLatest(page);
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

  const mainRef = useRef<HTMLElement>(null);
  useEffect(() => {
    mainRef.current?.scrollTo({ top: 0 });
  }, [page, projectRoute?.id, worldRoute?.id]);

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
  const intro = useIntroTimeline(settings ? settings.intro : null);
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
  const requestedWorlds = useRef<Record<string, string>>({});
  const homeNow = useLatest(home);

  useEffect(() => {
    let active = true;
    getLaunches().then((current) => {
      if (!active) return;
      setLaunches(Object.fromEntries(current.map((event) => [event.instanceId, event.update])));
      setLaunchWorlds(Object.fromEntries(current.map((event) => [event.instanceId, event.world])));
    }, () => {});
    const stopListening = onLaunchEvent(({ instanceId, world, update }) => {
      // A normal quit clears the card; a crash or failure stays until dismissed.
      setLaunches((current) =>
        update.phase === "exited" && !update.crashed ? without(current, instanceId) : { ...current, [instanceId]: update },
      );
      if (update.phase === "running") {
        const asked = requestedWorlds.current[instanceId];
        setLaunchWorlds((current) => ({ ...current, [instanceId]: world }));
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
        if (update.phase === "exited" && !update.crashed) setLaunchWorlds((current) => without(current, instanceId));
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
  const playAfterAccount = useRef<{ instanceId: string; world: string | null } | null>(null);

  const startLaunch = useCallback(
    (instanceId: string, world: string | null = null) => {
      setLaunches((current) => ({ ...current, [instanceId]: { phase: "checking" } }));
      setLaunchWorlds((current) => ({ ...current, [instanceId]: world }));
      if (world) requestedWorlds.current[instanceId] = world;
      else delete requestedWorlds.current[instanceId];
      launchInstance(instanceId, world).catch((failure: unknown) => {
        setLaunches((current) => without(current, instanceId));
        setLaunchWorlds((current) => without(current, instanceId));
        notify({ tone: "info", title: "Couldn't start the game", body: message(failure) });
      });
    },
    [notify],
  );

  const addAccount = useCallback(() => {
    if (canAddOfflineAccounts) setAccountDialog((current) => (current.open ? current : { open: true, session: current.session + 1 }));
    else notify({ tone: "resting", title: "Microsoft sign-in is coming soon", body: "You'll sign in with the Microsoft account that owns Minecraft." });
  }, [notify]);

  const play = useCallback(
    (instanceId: string, world: string | null = null) => {
      if (!homeNow.current?.account) {
        playAfterAccount.current = { instanceId, world };
        addAccount();
        return;
      }
      startLaunch(instanceId, world);
    },
    [homeNow, addAccount, startLaunch],
  );

  const stop = useCallback(
    (instanceId: string) => {
      stopInstance(instanceId).catch((failure: unknown) => notify({ tone: "info", title: "Couldn't stop the game", body: message(failure) }));
    },
    [notify],
  );

  const dismissLaunch = useCallback((instanceId: string) => setLaunches((current) => without(current, instanceId)), []);

  const onAccountAdded = useCallback(() => {
    // Read it now: the dialog's close (which clears it) runs before the refresh finishes.
    const waiting = playAfterAccount.current;
    playAfterAccount.current = null;
    void refreshHome().then(() => {
      if (waiting) startLaunch(waiting.instanceId, waiting.world);
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
  const dismissInstall = useCallback((jobId: string) => setInstalls((current) => without(current, jobId)), []);

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
      play,
      stop,
      dismissLaunch,
      addAccount,
      openProject,
      openWorld,
      importWorlds,
      requestInstall,
      installs,
      startModpackInstall,
      dismissInstall,
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
      play,
      stop,
      dismissLaunch,
      addAccount,
      openProject,
      openWorld,
      importWorlds,
      requestInstall,
      installs,
      startModpackInstall,
      dismissInstall,
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

              <AnimatePresence mode="wait" initial={false}>
                {page === "home" && <HomePage key="home" revealed={revealed} heroIntro={heroIntro} />}
                {page === "instances" && <Lazy key="instances" page={<InstancesPage />} />}
                {page === "worlds" && <Lazy key="worlds" page={<WorldsPage />} />}
                {page === "discover" && <Lazy key="discover" page={<DiscoverPage query={query} onClearQuery={() => setQuery("")} />} />}
                {page === "screenshots" && <Lazy key="screenshots" page={<ScreenshotsPage />} />}
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
              </AnimatePresence>
            </main>
          </div>

          {dialog.session > 0 && (
            <Suspense fallback={null}>
              <NewInstanceDialog key={dialog.session} open={dialog.open} onClose={closeNewInstance} />
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
          {installDialog.session > 0 && installDialog.target && (
            <Suspense fallback={null}>
              <InstallDialog key={installDialog.session} target={installDialog.target} open={installDialog.open} onClose={closeInstallDialog} />
            </Suspense>
          )}
          <Toasts toasts={toasts} onDismiss={dismiss}>
            <AnimatePresence>{noteRequest && <NotePrompt key={noteRequest.session ?? noteRequest.world} request={noteRequest} onDone={() => setNoteRequest(null)} />}</AnimatePresence>
            <InstallPanel />
            <LaunchPanel />
          </Toasts>
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

/** A lazily loaded page. Its own Suspense boundary keeps the page it replaces on screen meanwhile. */
function Lazy({ page }: { page: ReactNode }) {
  return <Suspense fallback={null}>{page}</Suspense>;
}
