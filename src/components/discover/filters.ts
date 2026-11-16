import { Braces, Earth, Image as ImageIcon, Package, Puzzle, Sparkles, type LucideIcon } from "lucide-react";
import type { SearchSort, Source } from "../../data/types";

/** The kinds of project Discover browses. */
export type Tab = "modpack" | "mod" | "resourcepack" | "shader" | "datapack" | "world";

export const ANY = "any";

export interface Filters {
  source: Source;
  tab: Tab;
  sort: SearchSort;
  loaders: string[];
  /** Modrinth category slugs, or CurseForge category ids. */
  categories: string[];
  version: string;
  /** List snapshots in the version menu too. */
  snapshots: boolean;
  environment: "any" | "client" | "server";
  /** Filter to what fits this instance: its version and loader. */
  instanceId: string | null;
}

export const DEFAULT_FILTERS: Filters = {
  source: "modrinth",
  tab: "modpack",
  sort: "relevance",
  loaders: [],
  categories: [],
  version: ANY,
  snapshots: false,
  environment: "any",
  instanceId: null,
};

export const SOURCES: { value: Source; label: string }[] = [
  { value: "modrinth", label: "Modrinth" },
  { value: "curseforge", label: "CurseForge" },
];

export const CONTENT_TYPES: Record<Tab, { label: string; icon: LucideIcon }> = {
  modpack: { label: "Modpacks", icon: Package },
  mod: { label: "Mods", icon: Puzzle },
  resourcepack: { label: "Resource packs", icon: ImageIcon },
  shader: { label: "Shaders", icon: Sparkles },
  datapack: { label: "Data packs", icon: Braces },
  world: { label: "Worlds", icon: Earth },
};

/** What each source has. Only CurseForge lists worlds. */
export const SOURCE_TABS: Record<Source, Tab[]> = {
  modrinth: ["modpack", "mod", "resourcepack", "shader", "datapack"],
  curseforge: ["modpack", "mod", "resourcepack", "shader", "datapack", "world"],
};

/** Loaders worth filtering by, per source and kind: the ones Wisp runs first, the rest after. */
export const LOADER_CHOICES: Record<Source, Partial<Record<Tab, { main: string[]; more: string[] }>>> = {
  modrinth: {
    modpack: { main: ["fabric", "quilt", "forge", "neoforge"], more: [] },
    mod: {
      main: ["fabric", "forge", "neoforge", "quilt", "legacy-fabric", "ornithe", "babric"],
      more: ["bta-babric", "liteloader", "rift", "modloader", "nilloader", "java-agent"],
    },
    shader: { main: ["iris", "optifine", "canvas", "vanilla"], more: [] },
  },
  curseforge: {
    modpack: { main: ["fabric", "quilt", "forge", "neoforge"], more: [] },
    mod: { main: ["fabric", "quilt", "forge", "neoforge"], more: ["liteloader", "cauldron"] },
  },
};

/** Every loader a source and kind can be filtered by. */
export function loaderChoices(source: Source, tab: Tab): string[] {
  const choice = LOADER_CHOICES[source][tab];
  return choice ? [...choice.main, ...choice.more] : [];
}

export const NOUNS: Record<Tab, [one: string, many: string]> = {
  modpack: ["modpack", "modpacks"],
  mod: ["mod", "mods"],
  resourcepack: ["resource pack", "resource packs"],
  shader: ["shader", "shaders"],
  datapack: ["data pack", "data packs"],
  world: ["world", "worlds"],
};

/** Modrinth's category groups, in the order the sidebar shows them. */
export const HEADER_ORDER = ["categories", "features", "resolutions", "performance impact"];
