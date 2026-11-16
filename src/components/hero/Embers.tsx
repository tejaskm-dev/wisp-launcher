import { cn } from "../../lib/cn";

// Hand-placed so the drift reads as natural; positions cluster around the shrine side of the art.
const EMBERS = [
  { left: "54%", bottom: "16%", size: 3, delay: 0, duration: 9, dir: "right", opacity: 0.7 },
  { left: "61%", bottom: "28%", size: 2, delay: 2.2, duration: 11, dir: "left", opacity: 0.55 },
  { left: "68%", bottom: "12%", size: 3, delay: 4.1, duration: 10, dir: "right", opacity: 0.65 },
  { left: "74%", bottom: "34%", size: 2, delay: 1.3, duration: 12, dir: "right", opacity: 0.5 },
  { left: "81%", bottom: "20%", size: 3, delay: 5.6, duration: 9.5, dir: "left", opacity: 0.7 },
  { left: "88%", bottom: "10%", size: 2, delay: 3.4, duration: 11.5, dir: "right", opacity: 0.55 },
  { left: "47%", bottom: "8%", size: 2, delay: 6.8, duration: 10.5, dir: "right", opacity: 0.45 },
  { left: "93%", bottom: "30%", size: 3, delay: 7.5, duration: 9, dir: "left", opacity: 0.6 },
] as const;

/**
 * Glowing motes drifting up over the hero art. The keyframes only touch transform and opacity
 * with fixed values, so the GPU runs them without waking the page; the outer span sets each
 * mote's brightness.
 */
export function Embers() {
  return (
    <div aria-hidden="true" className="pointer-events-none absolute inset-0 -z-10 overflow-hidden">
      {EMBERS.map((ember, index) => (
        <span key={index} className="absolute" style={{ left: ember.left, bottom: ember.bottom, opacity: ember.opacity }}>
          <span
            className={cn(
              "block rounded-full bg-cream shadow-[0_0_8px_rgba(244,238,228,0.9)]",
              ember.dir === "left" ? "animate-drift-left" : "animate-drift-right",
            )}
            style={{
              width: ember.size,
              height: ember.size,
              animationDelay: `${ember.delay}s`,
              animationDuration: `${ember.duration}s`,
            }}
          />
        </span>
      ))}
    </div>
  );
}
