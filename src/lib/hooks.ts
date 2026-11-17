import { useEffect, useLayoutEffect, useRef, useState } from "react";

/** Current time, refreshed on an interval so relative labels ("2 min ago") stay fresh. */
export function useNow(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return now;
}

/** Marks <html data-idle> while the window is unfocused or hidden, which pauses ambient animations. */
export function useIdleWhenUnfocused() {
  useEffect(() => {
    const root = document.documentElement;
    const update = () => root.toggleAttribute("data-idle", document.hidden || !document.hasFocus());
    update();
    window.addEventListener("focus", update);
    window.addEventListener("blur", update);
    document.addEventListener("visibilitychange", update);
    return () => {
      window.removeEventListener("focus", update);
      window.removeEventListener("blur", update);
      document.removeEventListener("visibilitychange", update);
    };
  }, []);
}

/** `value`, but only after it has stopped changing for `delayMs`. */
export function useDebounced<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = window.setTimeout(() => setDebounced(value), delayMs);
    return () => window.clearTimeout(id);
  }, [value, delayMs]);
  return debounced;
}

/** A ref that always holds the latest value, for callbacks used inside effects. */
export function useLatest<T>(value: T) {
  const ref = useRef(value);
  useLayoutEffect(() => {
    ref.current = value;
  });
  return ref;
}

/** Tracks whether a scroller (sideways by default) has more content before or after the visible part. */
export function useScrollEdges<T extends HTMLElement>(axis: "x" | "y" = "x") {
  const ref = useRef<T>(null);
  const [edges, setEdges] = useState({ start: false, end: false });

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => {
      const [offset, size, total] = axis === "x" ? [el.scrollLeft, el.clientWidth, el.scrollWidth] : [el.scrollTop, el.clientHeight, el.scrollHeight];
      const start = offset > 4;
      const end = offset + size < total - 4;
      setEdges((prev) => (prev.start === start && prev.end === end ? prev : { start, end }));
    };
    update();
    el.addEventListener("scroll", update, { passive: true });
    // Its size changing, or what's inside it (instances coming and going), can move the edges.
    const observer = new ResizeObserver(update);
    observer.observe(el);
    for (const child of el.children) observer.observe(child);
    return () => {
      el.removeEventListener("scroll", update);
      observer.disconnect();
    };
  }, [axis]);

  return { ref, edges };
}
