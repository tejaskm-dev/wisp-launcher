import { createContext, useContext } from "react";
import type { InstallTarget } from "../components/install/InstallDialog";
import type { HomeSnapshot, InstallEvent, LaunchUpdate, PageId, Settings } from "../data/types";
import type { Toast } from "./useToasts";

export interface AppContextValue {
  page: PageId;
  navigate: (page: PageId) => void;
  /** Null while the library is loading. */
  home: HomeSnapshot | null;
  /** Reloads the library without showing skeletons (after creating an instance, say). */
  refreshHome: () => Promise<void>;
  /** Null while settings are loading. */
  settings: Settings | null;
  /** Applies at once and saves shortly after. Pass a function to build the patch from the current settings. */
  updateSettings: (patch: SettingsPatch) => void;
  notify: (toast: Omit<Toast, "id">) => void;
  /** For features whose interface exists but whose launcher work isn't built yet. */
  comingSoon: (feature: string) => void;
  openNewInstance: () => void;
  /** Plays the startup animation again (the preview button in Settings). */
  replayIntro: () => void;
  /** The latest launch update per instance id, for instances starting, running or just failed. */
  launches: Record<string, LaunchUpdate>;
  /** The world (its folder) each launch opens straight into, when one was asked for. */
  launchWorlds: Record<string, string | null>;
  /** Starts an instance (asking for an account first if there isn't one), straight into a world if given. */
  play: (instanceId: string, world?: string | null) => void;
  /** Stops a running game, or cancels a launch still downloading. */
  stop: (instanceId: string) => void;
  /** Clears a failed or crashed launch from view. */
  dismissLaunch: (instanceId: string) => void;
  /** Opens the add-account dialog (developer builds) or explains sign-in (release builds). */
  addAccount: () => void;
  /** Opens a Modrinth project's page. */
  openProject: (projectId: string) => void;
  /** Opens a world's page ("<instance id>/<folder>"). */
  openWorld: (worldId: string) => void;
  /** Imports worlds (from dropped files, or a file picker), asking which instance if there's a choice. */
  importWorlds: (files?: string[] | null) => void;
  /** Opens the install dialog for a project (or a specific version of it). */
  requestInstall: (target: InstallTarget) => void;
  /** Modpack installs running in the background, by job id. */
  installs: Record<string, InstallEvent>;
  startModpackInstall: (target: InstallTarget) => void;
  dismissInstall: (jobId: string) => void;
  /** Goes up whenever the journal changes, so pages showing it reload. */
  journalVersion: number;
  now: number;
}

export type SettingsPatch = Partial<Settings> | ((current: Settings) => Partial<Settings>);

export const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const value = useContext(AppContext);
  if (!value) throw new Error("useApp must be used inside AppContext");
  return value;
}
