import { AnimatePresence, m, type Transition, type Variants } from "motion/react";
import { useEffect, useLayoutEffect, useState } from "react";
import { cn } from "../../lib/cn";
import { easeOutExpo } from "../../lib/motion";
import { Ghost } from "../brand/Ghost";
import { LOGO_GHOST_CENTER, LOGO_GHOST_SIZE, LOGO_ROW, LOGO_SPARKLE, WORDMARK, WORDMARK_LETTERS } from "../brand/Logo";
import { Sparkle } from "../brand/Sparkle";
import { HeroScrims } from "../hero/HeroScrims";
import type { IntroPhase } from "./useIntroTimeline";

interface StartupIntroProps {
  phase: Exclude<IntroPhase, "done">;
  quick: boolean;
  /** The home hero's art. It fills the screen during the intro, then settles into the hero card. */
  art: string;
  /** The sidebar is collapsed to icons, so the wordmark fades out as the logo lands. */
  railOnly: boolean;
  onSkip: () => void;
}

/** How much bigger the logo is in the middle of the screen than in the sidebar. */
const SCALE = 2.6;
/** The big logo sits this far above the centre, leaving room for the tagline under it. */
const LIFT = 28;
const CENTER_TOP = `calc(50% - ${LIFT}px)`;
/** Ease for the art settling into the hero card: slow start, slow finish. */
const SETTLE: [number, number, number, number] = [0.76, 0, 0.24, 1];

/** Where the sidebar logo is: the flying logo is laid out there and transformed out to the centre. */
interface LogoBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * Plays at startup, over the dashboard (which mounts underneath so its data loads meanwhile):
 * a wisp of light gathers, bursts, and the ghost and wordmark appear over the moonlit art. Then the
 * logo flies into the sidebar while the art shrinks into the home hero, and the app is there.
 * Any key or click skips ahead to that last part.
 */
export function StartupIntro({ phase, quick, art, railOnly, onSkip }: StartupIntroProps) {
  const logo = useSidebarLogoBox();
  const docking = phase === "dock";

  useEffect(() => {
    if (docking) return;
    window.addEventListener("keydown", onSkip);
    return () => window.removeEventListener("keydown", onSkip);
  }, [docking, onSkip]);

  return (
    <div aria-hidden="true">
      <m.div
        className={cn(
          "fixed inset-0 z-[80] bg-[radial-gradient(ellipse_at_50%_45%,#1b1816_0%,#0a0908_70%)]",
          docking && "pointer-events-none",
        )}
        initial={false}
        animate={{ opacity: docking ? 0 : 1 }}
        transition={{ duration: 0.6, ease: "easeOut" }}
      />

      <FloatingArt art={art} phase={phase} />

      <div onPointerDown={docking ? undefined : onSkip} className={cn("fixed inset-0 z-[100] overflow-hidden", docking && "pointer-events-none")}>
        <AnimatePresence>{phase === "orb" && <Orb key="orb" />}</AnimatePresence>
        {phase !== "orb" && <Ignite />}
        {logo && <Flourish phase={phase} quick={quick} logo={logo} />}
        {logo && <FlyingLogo phase={phase} quick={quick} logo={logo} railOnly={railOnly} />}
      </div>
    </div>
  );
}

function useSidebarLogoBox() {
  const [box, setBox] = useState<LogoBox | null>(null);
  useLayoutEffect(() => {
    const measure = () => {
      const logo = document.querySelector<HTMLElement>("[data-logo]");
      if (!logo) return;
      // The sidebar is never transformed, so this is the logo's laid-out box. Its wordmark stays in
      // the layout even when the sidebar is collapsed, so the width is the full logo's either way.
      const rect = logo.getBoundingClientRect();
      setBox({ left: rect.left, top: rect.top, width: rect.width, height: rect.height });
    };
    measure();
    // The wordmark's width changes once the display font has loaded.
    let active = true;
    void document.fonts.ready.then(() => active && measure());
    window.addEventListener("resize", measure);
    return () => {
      active = false;
      window.removeEventListener("resize", measure);
    };
  }, []);
  return box;
}

// ─── orb ─────────────────────────────────────────────────────────────────────────────────────────

const MOTES = [
  { angle: 0, radius: 124, size: 6 },
  { angle: 72, radius: 94, size: 4 },
  { angle: 144, radius: 138, size: 5 },
  { angle: 216, radius: 106, size: 4 },
  { angle: 288, radius: 130, size: 6 },
];

const SPIRAL: Transition = { duration: 0.8, ease: [0.5, 0, 0.75, 0] };

/** A point of light with motes spiralling into it. */
function Orb() {
  return (
    <m.div
      className="absolute left-1/2"
      style={{ top: CENTER_TOP }}
      exit={{ opacity: 0, scale: 0.3, transition: { duration: 0.2, ease: "easeIn" } }}
    >
      <m.div
        className="absolute size-[320px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-[radial-gradient(circle,rgba(244,238,228,0.24),rgba(244,238,228,0.05)_40%,transparent_68%)]"
        initial={{ scale: 0.2, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={{ duration: 0.8, ease: easeOutExpo }}
      />
      <m.div
        className="absolute size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-cream shadow-[0_0_16px_6px_rgba(244,238,228,0.7),0_0_70px_24px_rgba(244,238,228,0.22)]"
        initial={{ scale: 0 }}
        animate={{ scale: [0, 1.25, 0.95, 1.4] }}
        transition={{ duration: 0.75, times: [0, 0.3, 0.6, 1], ease: "easeInOut" }}
      />
      {MOTES.map((mote) => (
        <m.div key={mote.angle} className="absolute" initial={{ rotate: mote.angle }} animate={{ rotate: mote.angle + 280 }} transition={SPIRAL}>
          <m.span
            className="absolute block -translate-x-1/2 -translate-y-1/2 rounded-full bg-cream shadow-[0_0_10px_rgba(244,238,228,0.9)]"
            style={{ width: mote.size, height: mote.size }}
            initial={{ x: mote.radius, opacity: 0 }}
            animate={{ x: 6, opacity: [0, 1, 1, 0.5] }}
            transition={SPIRAL}
          />
        </m.div>
      ))}
    </m.div>
  );
}

/** The orb bursts: a flash of light and two shockwave rings. */
function Ignite() {
  return (
    <>
      <m.div
        className="absolute inset-0 bg-[radial-gradient(circle_at_50%_calc(50%_-_28px),rgba(244,238,228,0.5),rgba(244,238,228,0.1)_22%,transparent_50%)]"
        initial={{ opacity: 0 }}
        animate={{ opacity: [0, 1, 0] }}
        transition={{ duration: 0.8, times: [0, 0.12, 1], ease: "easeOut" }}
      />
      {[
        { delay: 0, tone: "border-cream/80" },
        { delay: 0.12, tone: "border-ember/70" },
      ].map((ring) => (
        <div key={ring.delay} className="absolute left-1/2" style={{ top: CENTER_TOP }}>
          <m.div
            className={cn("size-[140px] -translate-x-1/2 -translate-y-1/2 rounded-full border", ring.tone)}
            initial={{ scale: 0.25, opacity: 0.9 }}
            animate={{ scale: 5.5, opacity: 0 }}
            transition={{ duration: 1.15, delay: ring.delay, ease: easeOutExpo }}
          />
        </div>
      ))}
    </>
  );
}

// ─── flourish: burst sparkles, tagline, "boo!" ─────────────────────────────────────────────────────

const BURST = [
  { x: -246, y: -98, size: 22, tone: "text-ember" },
  { x: 234, y: -86, size: 14, tone: "text-cream" },
  { x: 204, y: 74, size: 11, tone: "text-ember" },
  { x: -214, y: 82, size: 10, tone: "text-cream-300" },
  { x: 36, y: -132, size: 9, tone: "text-cream" },
  { x: 150, y: 132, size: 8, tone: "text-ember-soft" },
  { x: 306, y: -8, size: 8, tone: "text-cream-300" },
  { x: -312, y: 4, size: 12, tone: "text-cream" },
];

const burst: Variants = {
  hidden: { x: 0, y: 0, scale: 0, rotate: -90, opacity: 0 },
  shown: ({ x, y, delay }: { x: number; y: number; delay: number }) => ({
    x,
    y,
    rotate: 0,
    opacity: 1,
    scale: [0, 1.4, 1],
    transition: { duration: 0.9, delay, ease: easeOutExpo, scale: { duration: 0.6, delay, times: [0, 0.45, 1] } },
  }),
};

const tagline: Variants = {
  hidden: { opacity: 0, y: 10 },
  shown: { opacity: 1, y: 0, transition: { duration: 0.8, delay: 0.82, ease: easeOutExpo } },
};

const boo: Variants = {
  hidden: { opacity: 0, scale: 0.4, y: 8 },
  shown: {
    opacity: [0, 1, 1, 0],
    scale: [0.4, 1.12, 1, 0.94],
    y: [8, 0, 0, -6],
    transition: { duration: 0.64, delay: 1.02, times: [0, 0.22, 0.82, 1], ease: "easeOut" },
  },
};

/** Screen-space decoration around the big logo, positioned from the screen centre. */
function Flourish({ phase, quick, logo }: { phase: StartupIntroProps["phase"]; quick: boolean; logo: LogoBox }) {
  const state = phase === "orb" ? "hidden" : "shown";
  // Where the big ghost's left edge and top end up, relative to the centre point.
  const ghostLeft = (LOGO_GHOST_CENTER - LOGO_GHOST_SIZE / 2 - logo.width / 2) * SCALE;
  const ghostTop = -(LOGO_GHOST_SIZE / 2) * SCALE;

  return (
    <m.div
      className="absolute left-1/2"
      style={{ top: CENTER_TOP }}
      initial={false}
      animate={{ opacity: phase === "dock" ? 0 : 1 }}
      transition={{ duration: 0.25 }}
    >
      {BURST.map((sparkle, index) => (
        <m.div
          key={index}
          className={cn("absolute", sparkle.tone)}
          style={{ width: sparkle.size, height: sparkle.size, marginLeft: -sparkle.size / 2, marginTop: -sparkle.size / 2 }}
          custom={{ x: sparkle.x, y: sparkle.y, delay: (quick ? 0.08 : 0.3) + index * 0.035 }}
          variants={burst}
          initial="hidden"
          animate={state}
        >
          <Sparkle twinkle className="size-full" />
        </m.div>
      ))}

      {!quick && (
        <>
          <m.p
            className="absolute top-[76px] -translate-x-1/2 whitespace-nowrap font-serif text-[28px] italic text-cream-300"
            variants={tagline}
            initial="hidden"
            animate={state}
          >
            your worlds follow you
          </m.p>
          <m.div
            className="absolute origin-bottom-right"
            style={{ right: -(ghostLeft + 14), bottom: -(ghostTop + 18) }}
            variants={boo}
            initial="hidden"
            animate={state}
          >
            <div className="rounded-2xl rounded-br-[4px] bg-cream px-3.5 py-1.5 font-display text-lg font-extrabold text-ink-900 shadow-[0_10px_30px_-8px_rgba(0,0,0,0.6)]">
              boo!
            </div>
          </m.div>
        </>
      )}
    </m.div>
  );
}

// ─── the logo ─────────────────────────────────────────────────────────────────────────────────────

/** Pops out of the orb, then (full intro) a little wiggle once the wordmark is in. */
const ghostFull: Variants = {
  hidden: { scale: 0, rotate: -16, y: 8 },
  shown: {
    scale: [0, 1.25, 0.9, 1, 1, 1.04, 1.04, 1, 1],
    rotate: [-16, 10, -4, 0, 0, -10, 8, -3, 0],
    y: [8, -4, 1, 0, 0, -3, -3, 0, 0],
    transition: { duration: 1.6, times: [0, 0.17, 0.27, 0.35, 0.72, 0.8, 0.88, 0.95, 1], ease: "easeInOut" },
  },
};

const ghostQuick: Variants = {
  hidden: { scale: 0, rotate: -16, y: 8 },
  shown: { scale: [0, 1.2, 0.92, 1], rotate: [-16, 8, -3, 0], y: [8, -3, 1, 0], transition: { duration: 0.55, times: [0, 0.45, 0.75, 1] } },
};

const letter: Variants = {
  hidden: { y: 24, opacity: 0, rotate: -12 },
  shown: ({ index, quick }: { index: number; quick: boolean }) => {
    const delay = (quick ? 0.12 : 0.42) + index * (quick ? 0.045 : 0.07);
    return {
      y: 0,
      opacity: 1,
      rotate: 0,
      transition: { type: "spring", stiffness: 420, damping: 19, delay, opacity: { duration: 0.2, delay } },
    };
  },
};

const accent: Variants = {
  hidden: { scale: 0, rotate: -60 },
  shown: (quick: boolean) => ({
    scale: [0, 1.6, 1],
    rotate: [-60, 25, 0],
    transition: { duration: 0.55, delay: quick ? 0.35 : 0.8, ease: "easeOut" },
  }),
};

/** A band of light that sweeps across the wordmark once it's in. */
const gleam: Variants = {
  hidden: { backgroundPosition: "100% 0%" },
  shown: { backgroundPosition: "0% 0%", transition: { duration: 0.9, delay: 0.98, ease: [0.45, 0, 0.2, 1] } },
};

/** Different stiffness on each axis makes the logo swoop up, then glide left into place. */
const FLIGHT: Transition = {
  x: { type: "spring", stiffness: 115, damping: 19 },
  y: { type: "spring", stiffness: 155, damping: 21 },
  scale: { type: "spring", stiffness: 160, damping: 18 },
};

/** Laid out exactly where the sidebar logo is, and transformed out to the middle of the screen. */
function FlyingLogo({ phase, quick, logo, railOnly }: { phase: StartupIntroProps["phase"]; quick: boolean; logo: LogoBox; railOnly: boolean }) {
  const state = phase === "orb" ? "hidden" : "shown";
  const docking = phase === "dock";
  const dx = window.innerWidth / 2 - (logo.left + logo.width / 2);
  const dy = window.innerHeight / 2 - LIFT - (logo.top + logo.height / 2);
  /** The ghost first appears in the middle (where the orb was), then slides to its spot. */
  const ghostShift = logo.width / 2 - LOGO_GHOST_CENTER;
  const blinking = useBlinkOnce(phase === "show" && !quick, 960);

  return (
    <m.div
      className="absolute"
      style={{ left: logo.left, top: logo.top, width: logo.width, height: logo.height }}
      initial={{ x: dx, y: dy, scale: SCALE }}
      animate={docking ? { x: 0, y: 0, scale: 1 } : { x: dx, y: dy, scale: SCALE }}
      transition={docking ? FLIGHT : { duration: 0 }}
    >
      <div className={cn(LOGO_ROW, "h-full")}>
        <m.div
          className="relative"
          initial={{ x: ghostShift }}
          animate={{ x: phase === "orb" ? ghostShift : 0 }}
          transition={phase === "orb" ? { duration: 0 } : { delay: quick ? 0.1 : 0.3, duration: quick ? 0.5 : 0.75, ease: easeOutExpo }}
        >
          <m.span
            className="absolute left-1/2 top-1/2 size-[72px] -translate-x-1/2 -translate-y-1/2 rounded-full bg-[radial-gradient(circle,rgba(244,238,228,0.3),transparent_68%)]"
            initial={{ opacity: 0 }}
            animate={{ opacity: phase === "show" ? 1 : 0 }}
            transition={{ duration: 0.4 }}
          />
          <m.div className="relative" variants={quick ? ghostQuick : ghostFull} initial="hidden" animate={state}>
            <Ghost size={LOGO_GHOST_SIZE} blinking={blinking} />
          </m.div>
        </m.div>

        <m.span
          className={cn(WORDMARK, "relative")}
          initial={false}
          animate={{ opacity: docking && railOnly ? 0 : 1 }}
          transition={{ duration: 0.25 }}
        >
          {WORDMARK_LETTERS.map((char, index) => (
            <m.span key={index} className="inline-block" custom={{ index, quick }} variants={letter} initial="hidden" animate={state}>
              {char}
            </m.span>
          ))}
          {!quick && (
            <m.span
              className="pointer-events-none absolute inset-0 bg-[linear-gradient(100deg,transparent_38%,rgba(255,255,255,0.95)_50%,transparent_62%)] bg-size-[300%_100%] bg-clip-text text-transparent"
              style={{ WebkitBackgroundClip: "text" }}
              variants={gleam}
              initial="hidden"
              animate={state}
            >
              wisp
            </m.span>
          )}
        </m.span>

        <m.div className={LOGO_SPARKLE} custom={quick} variants={accent} initial="hidden" animate={state}>
          <Sparkle className="size-full" />
        </m.div>
      </div>
    </m.div>
  );
}

/** True for one blink, `afterMs` after `active` turns on. */
function useBlinkOnce(active: boolean, afterMs: number) {
  const [blinking, setBlinking] = useState(false);
  useEffect(() => {
    if (!active) return;
    const close = window.setTimeout(() => setBlinking(true), afterMs);
    const open = window.setTimeout(() => setBlinking(false), afterMs + 140);
    return () => {
      window.clearTimeout(close);
      window.clearTimeout(open);
      setBlinking(false);
    };
  }, [active, afterMs]);
  return blinking;
}

// ─── the art ──────────────────────────────────────────────────────────────────────────────────────

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/**
 * The hero art, full screen behind the logo. When docking it shrinks into the home hero's card
 * ([data-hero]) and takes on the hero's scrims, so the hand-off to the real hero is seamless.
 */
function FloatingArt({ art, phase }: { art: string; phase: StartupIntroProps["phase"] }) {
  const docking = phase === "dock";
  const [viewport, setViewport] = useState(() => ({ width: window.innerWidth, height: window.innerHeight }));
  const [hero, setHero] = useState<Box | "missing" | null>(null);

  useEffect(() => {
    const update = () => setViewport({ width: window.innerWidth, height: window.innerHeight });
    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, []);

  useLayoutEffect(() => {
    if (!docking) return;
    const element = document.querySelector<HTMLElement>("[data-hero]");
    if (!element) {
      setHero("missing");
      return;
    }
    // Track the hero while landing, in case the layout shifts (say the library finishes loading).
    const measure = () => {
      const rect = element.getBoundingClientRect();
      setHero({ left: rect.left, top: rect.top, width: rect.width, height: rect.height });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    window.addEventListener("resize", measure);
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, [docking]);

  const landing = docking && hero !== null && hero !== "missing" ? hero : null;
  const fullScreen = { left: 0, top: 0, width: viewport.width, height: viewport.height, borderRadius: 0 };

  return (
    <m.div
      className="pointer-events-none fixed z-[90] overflow-hidden bg-ink-850"
      initial={{ ...fullScreen, opacity: 0 }}
      animate={
        landing
          ? { ...landing, borderRadius: 26, opacity: 1 }
          : { ...fullScreen, opacity: phase === "orb" || (docking && hero === "missing") ? 0 : 1 }
      }
      transition={landing ? { duration: 0.8, ease: SETTLE } : { duration: 0.9, ease: "easeOut" }}
    >
      <m.img
        src={art}
        alt=""
        draggable={false}
        decoding="async"
        className="absolute inset-0 size-full object-cover object-[62%_42%]"
        initial={{ scale: 1.24 }}
        animate={{ scale: landing ? 1.06 : 1.12 }}
        transition={landing ? { duration: 0.8, ease: SETTLE } : { duration: 2.8, ease: easeOutExpo }}
      />
      {/* Darkens the art behind the logo; gives way to the hero's own scrims as it lands. */}
      <m.div className="absolute inset-0 bg-ink-950" initial={{ opacity: 0.6 }} animate={{ opacity: landing ? 0 : 0.6 }} transition={{ duration: 0.7 }} />
      <m.div className="absolute inset-0" initial={{ opacity: 0 }} animate={{ opacity: landing ? 1 : 0 }} transition={{ duration: 0.7 }}>
        <HeroScrims />
      </m.div>
    </m.div>
  );
}
