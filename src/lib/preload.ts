/**
 * The Skins page's code, most of it three.js for the 3D stage. The app loads it when the page
 * opens; pointing at the account menu (the way there) loads it a moment early.
 */
export const loadSkinsPage = () => import("../pages/SkinsPage");

/** Starts loading the Skins page ahead of time. A failure is ignored: opening the page tries again. */
export function preloadSkinsPage() {
  loadSkinsPage().catch(() => {});
}
