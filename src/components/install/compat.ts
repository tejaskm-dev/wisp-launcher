import type { Instance, Loader, ProjectType } from "../../data/types";
import { loaderName } from "../../lib/format";

/** What a project has builds for: from its page, or from one specific version. */
export interface Availability {
  projectType: ProjectType;
  loaders: string[];
  gameVersions: string[];
}

/** Mirrors `content::compatible_loaders` in wisp-core. */
export function compatibleLoaders(loader: Loader, kind: ProjectType): string[] | null {
  switch (kind) {
    case "mod":
      return { vanilla: null, fabric: ["fabric"], quilt: ["quilt", "fabric"], forge: ["forge"], neoforge: ["neoforge"] }[loader];
    case "resourcepack":
      return ["minecraft"];
    case "shader":
      return loader === "vanilla" ? ["vanilla"] : ["iris", "optifine", "vanilla", "canvas"];
    case "datapack":
      return ["datapack"];
    default:
      return null;
  }
}

/** Whether a project (or version) can go into an instance, and if not, why, in a few words. */
export function fits(project: Availability, instance: Instance): { ok: true } | { ok: false; reason: string } {
  // A world opens in its own version or a newer one; Minecraft upgrades it.
  if (project.projectType === "world") return { ok: true };
  if (project.projectType === "datapack") {
    return project.gameVersions.length === 0 || project.gameVersions.includes(instance.gameVersion)
      ? { ok: true }
      : { ok: false, reason: `Not made for ${instance.gameVersion}` };
  }
  const loaders = compatibleLoaders(instance.loader, project.projectType);
  if (!loaders) return { ok: false, reason: project.projectType === "mod" ? "Vanilla can't run mods" : "Can't add this here" };
  if (!project.loaders.some((loader) => loaders.includes(loader))) {
    if (project.projectType === "shader" && instance.loader === "vanilla") return { ok: false, reason: "Needs a loader with Iris" };
    return { ok: false, reason: `No ${loaderName(instance.loader)} build` };
  }
  if (!project.gameVersions.includes(instance.gameVersion)) return { ok: false, reason: `Not made for ${instance.gameVersion}` };
  return { ok: true };
}

/** Builds for Fabric, Forge…, but not "minecraft" (resource packs) or shader loaders, read as a list. */
export function loaderList(loaders: string[]): string {
  return loaders.filter((loader) => loader !== "minecraft").map(loaderName).join(", ");
}
