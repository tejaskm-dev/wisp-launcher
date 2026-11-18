import { invoke } from "@tauri-apps/api/core";
import type { Update } from "@tauri-apps/plugin-updater";
import { useSyncExternalStore } from "react";
import { isDesktop } from "../data/api";

/**
 * Wisp updating itself: release builds check for a new version now and then, offer it, and
 * install it on a yes (then restart). Developer builds don't update themselves.
 */
export type UpdateState =
  | { phase: "off" }
  | { phase: "idle"; checkedAt: number | null }
  | { phase: "checking" }
  | { phase: "available"; version: string; notes: string; later: boolean }
  | { phase: "downloading"; version: string; done: number; total: number | null }
  | { phase: "restarting"; version: string }
  | { phase: "failed"; message: string; version: string | null };

let state: UpdateState = { phase: "idle", checkedAt: null };
let pending: Update | null = null;
const listeners = new Set<() => void>();

function set(next: UpdateState) {
  state = next;
  for (const listener of listeners) listener();
}

export function useUpdate(): UpdateState {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => state,
  );
}

let enabled: Promise<boolean> | null = null;
function updatesOn() {
  enabled ??= isDesktop ? invoke<boolean>("updates_on").catch(() => false) : Promise.resolve(false);
  return enabled;
}

const failure = (error: unknown) => (error instanceof Error ? error.message : String(error));

/** In the browser preview, `?update=available` shows the update window (the desktop app never does this). */
function previewUpdate(): boolean {
  if (isDesktop || typeof location === "undefined" || new URLSearchParams(location.search).get("update") !== "available") return false;
  set({ phase: "available", version: "0.1.9", notes: "### What's new\n\n- **Instance pages**: every instance has its own page.\n- **Fixed**: something that was broken.", later: false });
  return true;
}

/** Asks whether there's a newer Wisp. Quiet about it unless one turns up. */
export async function checkForUpdate() {
  if (previewUpdate()) return;
  if (!(await updatesOn())) {
    set({ phase: "off" });
    return;
  }
  if (state.phase === "checking" || state.phase === "downloading" || state.phase === "restarting") return;
  set({ phase: "checking" });
  try {
    // Loaded only when it's needed, so it stays out of the startup bundle.
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    pending = update;
    if (update) set({ phase: "available", version: update.version, notes: update.body?.trim() ?? "", later: false });
    else set({ phase: "idle", checkedAt: Date.now() });
  } catch (error) {
    set({ phase: "failed", message: failure(error), version: null });
  }
}

/** Downloads and installs the update found, then restarts Wisp on it. */
export async function installUpdate() {
  const update = pending;
  if (!update) return;
  const version = update.version;
  let done = 0;
  let total: number | null = null;
  set({ phase: "downloading", version, done, total });
  try {
    await update.downloadAndInstall((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? null;
      else if (event.event === "Progress") done += event.data.chunkLength;
      set({ phase: "downloading", version, done, total });
    });
    set({ phase: "restarting", version });
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  } catch (error) {
    set({ phase: "failed", message: failure(error), version });
  }
}

/** Puts the offer away until Wisp next starts (or the next check finds it again). */
export function updateLater() {
  if (state.phase === "available") set({ ...state, later: true });
  else if (state.phase === "failed") set({ phase: "idle", checkedAt: null });
}
