/**
 * Whether an instance's Minecraft version can share settings with others. Minecraft 1.12 and
 * older store keybinds as numbers newer versions can't read, so they keep their own. Mirrors
 * `era` in crates/wisp-core/src/game_options.rs.
 */
export function sharesSettings(gameVersion: string): boolean {
  const release = /^1\.(\d+)/.exec(gameVersion);
  if (release) return Number(release[1]) >= 13;
  const snapshot = /^(\d\d)w(\d\d)/.exec(gameVersion);
  if (snapshot) {
    const [year, week] = [Number(snapshot[1]), Number(snapshot[2])];
    return year > 17 || (year === 17 && week >= 43);
  }
  // Year-numbered versions (26.1 on) are newer; alpha, beta and classic are older.
  return /^\d/.test(gameVersion);
}
