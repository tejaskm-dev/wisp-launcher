/** The app version from package.json, filled in by Vite at build time. */
declare const __APP_VERSION__: string;

interface ImportMetaEnv {
  /** "1" in tester builds (the release workflow's tester option): offline accounts on. */
  readonly VITE_WISP_TESTER?: string;
  /** A tester build's number (the workflow run that made it), when it takes tester updates. */
  readonly VITE_WISP_TESTER_BUILD?: string;
}
