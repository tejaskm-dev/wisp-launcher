/** The gradients over the hero art that keep its text readable. The startup intro fades these in
 * on its floating art as it lands, so both must stay identical. */
export function HeroScrims() {
  return (
    <>
      <div className="absolute inset-0 bg-linear-to-r from-ink-900/95 via-ink-900/60 via-35% to-transparent to-62%" />
      <div className="absolute inset-x-0 bottom-0 h-1/2 bg-linear-to-t from-ink-900/75 to-transparent" />
    </>
  );
}
