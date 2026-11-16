import sheet from "../../assets/icons/instance-icons.webp";
import type { Loader } from "../../data/types";

/**
 * Wisp's 50 built-in instance icons: one 10 × 5 sprite sheet of transparent 128 px tiles, each icon
 * centred on its own shape and sized to fill 84% of its tile (Melius-generated, cut locally).
 */
export const ICON_SHEET = sheet;
export const ICON_COLUMNS = 10;
export const ICON_ROWS = 5;

/** Names in sheet order (left to right, top to bottom), for labels and tooltips. */
export const INSTANCE_ICONS = [
  "Grass block", "Stone", "Sand", "Snowy block", "Lava", "Crate", "Pickaxe", "Crystal sword", "Axe", "Fishing rod",
  "Oak tree", "Cherry blossom", "Cactus", "Mushroom", "Sunflower", "Pumpkin lantern", "Apple", "Bread", "Cake", "Tea",
  "Moon", "Sun", "Shooting star", "Storm cloud", "Snowflake", "Campfire", "Paper lantern", "Torch", "Potion", "Spell book",
  "Treasure chest", "Gold coins", "Gem", "Crown", "Key", "Treasure map", "Compass", "Hourglass", "Shield", "Bow",
  "Ghost", "Torii gate", "Rocket", "Castle tower", "Cottage", "Sailboat", "Mountain", "Volcano", "Fox", "Bee",
] as const;

export const iconId = (index: number) => `wisp:${index}`;

/** The sheet index of a "wisp:<n>" icon, or null for anything else. */
export function builtinIcon(icon: string | null | undefined): number | null {
  const match = /^wisp:(\d{1,2})$/.exec(icon ?? "");
  const index = match ? Number(match[1]) : -1;
  return index >= 0 && index < INSTANCE_ICONS.length ? index : null;
}

/** What a new instance starts with until you pick something else. */
export const DEFAULT_ICONS: Record<Loader, number> = { vanilla: 0, fabric: 5, quilt: 11, forge: 6, neoforge: 26 };
