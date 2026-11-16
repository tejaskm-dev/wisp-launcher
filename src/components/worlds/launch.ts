import type { LaunchUpdate, WorldSummary } from "../../data/types";
import { describeLaunch } from "../launch/describe";
import type { WorldLaunch } from "../WorldsSection";

/**
 * How a world's Play button should look: its own launch when its instance is opening or running
 * this world; otherwise idle, and blocked while the instance is busy with something else.
 */
export function worldLaunch(
  world: Pick<WorldSummary, "instanceId" | "folder">,
  launches: Record<string, LaunchUpdate>,
  launchWorlds: Record<string, string | null>,
  now: number,
): WorldLaunch {
  const update = launches[world.instanceId];
  const view = describeLaunch(update, now);
  if (!update) return { view, blocked: false };
  if (launchWorlds[world.instanceId] === world.folder) return { view, blocked: false };
  const busy = view.kind === "busy" || view.kind === "running";
  return { view: describeLaunch(undefined, now), blocked: busy };
}
