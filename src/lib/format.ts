const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Short relative time: "just now", "12 min ago", "3 h ago", "last night", "4 days ago". */
export function timeAgo(timestamp: number, now = Date.now()): string {
  const diff = Math.max(0, now - timestamp);
  if (diff < MINUTE) return "just now";
  if (diff < HOUR) return `${Math.round(diff / MINUTE)} min ago`;

  const startOfToday = new Date(now).setHours(0, 0, 0, 0);
  if (timestamp >= startOfToday) return `${Math.round(diff / HOUR)} h ago`;
  if (timestamp >= startOfToday - DAY) {
    return new Date(timestamp).getHours() >= 18 ? "last night" : "yesterday";
  }

  const days = Math.ceil((startOfToday - timestamp) / DAY);
  if (days < 7) return `${days} days ago`;
  return new Date(timestamp).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** Decimal byte sizes, the way storage providers show them: "38 MB", "1.3 GB". */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  const rounded = value >= 10 || unit === 0 ? Math.round(value).toString() : value.toFixed(1);
  return `${rounded} ${units[unit]}`;
}

const compactNumber = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });

/** "19.3M", "845K". */
export function formatCount(count: number): string {
  return compactNumber.format(count);
}

/** "Nov 10", or "Sep 15, 2025" for another year. Empty if the date can't be read. */
export function formatDay(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const sameYear = date.getFullYear() === new Date().getFullYear();
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: sameYear ? undefined : "numeric" });
}

const LOADER_NAMES: Record<string, string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  forge: "Forge",
  neoforge: "NeoForge",
  quilt: "Quilt",
  iris: "Iris",
  optifine: "OptiFine",
  canvas: "Canvas",
  datapack: "Data pack",
  babric: "Babric",
  "bta-babric": "BTA (Babric)",
  "java-agent": "Java Agent",
  "legacy-fabric": "Legacy Fabric",
  liteloader: "LiteLoader",
  modloader: "Risugami's ModLoader",
  nilloader: "NilLoader",
  ornithe: "Ornithe",
  rift: "Rift",
  cauldron: "Cauldron",
};

export function loaderName(id: string): string {
  return LOADER_NAMES[id] ?? id;
}

/** "Fabric · 26.3" or "Vanilla · 26.3". */
export function instanceLabel(instance: { loader: string; gameVersion: string }): string {
  return `${loaderName(instance.loader)} · ${instance.gameVersion}`;
}

/** 0xRRGGBB → "#rrggbb". */
export function hexColor(rgb: number): string {
  return `#${rgb.toString(16).padStart(6, "0")}`;
}

export function greeting(date = new Date()): string {
  const hour = date.getHours();
  if (hour >= 5 && hour < 12) return "Good morning";
  if (hour >= 12 && hour < 18) return "Good afternoon";
  return "Good evening";
}
