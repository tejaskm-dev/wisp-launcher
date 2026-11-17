/**
 * A wisp of light follows the pointer across anything with the `lit` class: one listener for the
 * whole app writes the pointer's position into that element's --mx and --my, and a background
 * gradient (the `lit` utility in index.css) reads them. At most one paint per frame, and nothing
 * at all while the pointer isn't over a lit element.
 */
export function startWispLight(): () => void {
  let current: HTMLElement | null = null;
  let frame = 0;
  let x = 0;
  let y = 0;

  const clear = (element: HTMLElement | null) => {
    element?.style.removeProperty("--mx");
    element?.style.removeProperty("--my");
  };
  const paint = () => {
    frame = 0;
    if (!current) return;
    const box = current.getBoundingClientRect();
    current.style.setProperty("--mx", `${x - box.left}px`);
    current.style.setProperty("--my", `${y - box.top}px`);
  };
  const onMove = (event: PointerEvent) => {
    const target = event.target instanceof Element ? event.target.closest<HTMLElement>(".lit") : null;
    if (target !== current) {
      clear(current);
      current = target;
    }
    if (!current) return;
    x = event.clientX;
    y = event.clientY;
    if (frame === 0) frame = requestAnimationFrame(paint);
  };
  const onLeave = () => {
    clear(current);
    current = null;
  };

  document.addEventListener("pointermove", onMove, { passive: true });
  document.documentElement.addEventListener("pointerleave", onLeave);
  return () => {
    document.removeEventListener("pointermove", onMove);
    document.documentElement.removeEventListener("pointerleave", onLeave);
    cancelAnimationFrame(frame);
    onLeave();
  };
}
