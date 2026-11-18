/** The app version from package.json, filled in by Vite at build time. */
declare const __APP_VERSION__: string;

interface ImportMetaEnv {
  /** "1" in tester builds (the release workflow's tester option): offline accounts on, no updates. */
  readonly VITE_WISP_TESTER?: string;
}
