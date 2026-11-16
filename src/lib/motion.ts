import type { TargetAndTransition, Transition, Variants } from "motion/react";

/**
 * Shared motion language for Flux Code. One easing curve, a few durations.
 * Every animated surface in the app should use these tokens so movement
 * feels like one product.
 */

/** The one easing curve used across the app. */
export const EASE: [number, number, number, number] = [0.25, 0.8, 0.25, 1];

/** Durations in seconds: fast feedback, base transitions, slower entrances. */
export const D = {
  fast: 0.12,
  base: 0.18,
  slow: 0.26,
} as const;

export const baseTransition: Transition = { duration: D.base, ease: EASE };
export const fastTransition: Transition = { duration: D.fast, ease: EASE };

type Targets = {
  initial: TargetAndTransition;
  animate: TargetAndTransition;
  exit: TargetAndTransition;
};

/** Fade + small rise, for content entering (messages, sections, cards). */
export const fadeRise: Targets = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -4 },
};

/** Fade only — used when reduced motion is on or for quick swaps. */
export const fade: Targets = {
  initial: { opacity: 0 },
  animate: { opacity: 1 },
  exit: { opacity: 0 },
};

/** Modal panel: fade, slight scale up and a small rise. */
export const modalPanel: Targets = {
  initial: { opacity: 0, scale: 0.985, y: 8 },
  animate: { opacity: 1, scale: 1, y: 0 },
  exit: { opacity: 0, scale: 0.99, y: 4 },
};

/** Backdrop behind modals and drawers. */
export const backdrop: Variants = {
  initial: { opacity: 0 },
  animate: { opacity: 1 },
  exit: { opacity: 0 },
};

/** Menu popover: fades and rises a few px from its anchor edge. */
export const menuPop = (direction: "up" | "down"): Targets => ({
  initial: { opacity: 0, y: direction === "up" ? 5 : -5, scale: 0.98 },
  animate: { opacity: 1, y: 0, scale: 1 },
  exit: { opacity: 0, y: direction === "up" ? 2 : -2, scale: 0.99, transition: { duration: D.fast, ease: EASE } },
});

/** List item enter (chips, todos, provider rows). */
export const listItem: Targets = {
  initial: { opacity: 0, y: -4, scale: 0.99 },
  animate: { opacity: 1, y: 0, scale: 1 },
  exit: { opacity: 0, scale: 0.98, transition: { duration: D.fast, ease: EASE } },
};
