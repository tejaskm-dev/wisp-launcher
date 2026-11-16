import type { Transition, Variants } from "motion/react";

export const easeOutExpo: [number, number, number, number] = [0.16, 1, 0.3, 1];

export const spring: Transition = { type: "spring", stiffness: 260, damping: 30 };

/** Container that reveals its children one after another. */
export const stagger: Variants = {
  hidden: {},
  show: { transition: { staggerChildren: 0.07, delayChildren: 0.06 } },
};

/** A section rising into place; it also staggers its own children. */
export const rise: Variants = {
  hidden: { opacity: 0, y: 18 },
  show: { opacity: 1, y: 0, transition: { ...spring, staggerChildren: 0.06, delayChildren: 0.12 } },
};

export const fadeUp: Variants = {
  hidden: { opacity: 0, y: 10 },
  show: { opacity: 1, y: 0, transition: { duration: 0.5, ease: easeOutExpo } },
};
