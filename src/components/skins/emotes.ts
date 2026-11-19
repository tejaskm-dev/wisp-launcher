// Emotes for the 3D skin stage: a few that mean something, each choreographed from keyframes,
// plus the blending that eases from one into the next. Joints follow skinview3d: arm and leg
// rotation.x < 0 swings forward and up; left arm rotation.z > 0 (right arm < 0) lifts sideways;
// head rotation.x > 0 looks down. The player is 32 units tall, centred on 0, feet at -16.

import type { PlayerObject } from "skinview3d";

/** Joint angles (radians) and offsets (units) for a pose; anything left out is at rest. */
export interface Pose {
  headX?: number;
  headY?: number;
  headZ?: number;
  leftArmX?: number;
  leftArmZ?: number;
  rightArmX?: number;
  rightArmZ?: number;
  leftLegX?: number;
  leftLegZ?: number;
  rightLegX?: number;
  rightLegZ?: number;
  /** Upper body tipped forward from the hips (legs stay planted). */
  bend?: number;
  x?: number;
  y?: number;
  z?: number;
  /** Turn around the vertical axis. */
  turn?: number;
  /** Sideways lean. */
  lean?: number;
  /** Minecraft's sneak (see `applyPose`): 1 is down, 0 standing. It switches; it never blends. */
  sneak?: number;
}

const KEYS: (keyof Pose)[] = [
  "headX", "headY", "headZ", "leftArmX", "leftArmZ", "rightArmX", "rightArmZ",
  "leftLegX", "leftLegZ", "rightLegX", "rightLegZ", "bend", "x", "y", "z", "turn", "lean", "sneak",
];

const PI = Math.PI;
export const clamp01 = (value: number) => Math.min(1, Math.max(0, value));
export const smooth = (value: number) => {
  const x = clamp01(value);
  return x * x * (3 - 2 * x);
};
/** Eases out past the end and back: a snap with a little overshoot. */
const backOut = (value: number) => {
  const x = clamp01(value) - 1;
  return 1 + 2.4 * x * x * x + 1.4 * x * x;
};

/** Blends two poses; a joint missing from one counts as rest. */
export function mix(a: Pose, b: Pose, k: number): Pose {
  const out: Pose = {};
  for (const key of KEYS) {
    const from = a[key] ?? 0;
    const to = b[key] ?? 0;
    if (from !== 0 || to !== 0) out[key] = from + (to - from) * k;
  }
  return out;
}

/** An angle brought into -π to π. */
const wrap = (angle: number) => Math.atan2(Math.sin(angle), Math.cos(angle));

/** Like `mix`, but a turn takes the short way round (an emote that spun ten times eases back in one). */
export function blendPose(from: Pose, to: Pose, k: number): Pose {
  const out = mix(from, to, k);
  const start = wrap(from.turn ?? 0);
  out.turn = start + wrap((to.turn ?? 0) - start) * k;
  // Sneaking is on or off in the game, never halfway down.
  out.sneak = to.sneak ?? 0;
  return out;
}

/** Adds offsets onto a pose (for wobbles and bobs on top of keyframes). */
export function add(base: Pose, extra: Pose): Pose {
  const out: Pose = { ...base };
  for (const key of KEYS) if (extra[key] !== undefined) out[key] = (out[key] ?? 0) + (extra[key] ?? 0);
  return out;
}

/** The pose at time t along keyframes, eased between each pair. With `loop`, t wraps. */
export function timeline(frames: [number, Pose][], t: number, loop?: number): Pose {
  const time = loop ? t % loop : t;
  if (time <= frames[0][0]) return frames[0][1];
  for (let index = 1; index < frames.length; index += 1) {
    const [end, to] = frames[index];
    if (time <= end) {
      const [start, from] = frames[index - 1];
      return mix(from, to, smooth((time - start) / Math.max(0.0001, end - start)));
    }
  }
  return frames[frames.length - 1][1];
}

/** Puts a pose on the player. */
export function applyPose(player: PlayerObject, pose: Pose) {
  const skin = player.skin;
  // Sneaking, as the game's player model does it: the body tips 0.5 forward on a hinge at its top,
  // 3.2 below the neck, so the hips go back; the head drops 4.2 and keeps looking where it was;
  // the arms drop 3.2 and hang 0.4 back, along the body; the legs step 4 back (and 0.2 down) to
  // stay under the hips. The game also draws a sneaking player 2 pixels lower, feet and all.
  const sneak = pose.sneak ?? 0;
  const tip = 0.5 * sneak;
  skin.head.position.y = -4.2 * sneak;
  skin.head.rotation.set(pose.headX ?? 0, pose.headY ?? 0, pose.headZ ?? 0);
  skin.body.rotation.set(tip, 0, 0);
  skin.body.position.set(0, -3.2 * sneak - 6 * Math.cos(tip), -6 * Math.sin(tip));
  skin.leftArm.position.y = skin.rightArm.position.y = -2 - 3.2 * sneak;
  skin.leftArm.rotation.set((pose.leftArmX ?? 0) + 0.4 * sneak, 0, pose.leftArmZ ?? 0);
  skin.rightArm.rotation.set((pose.rightArmX ?? 0) + 0.4 * sneak, 0, pose.rightArmZ ?? 0);
  skin.leftLeg.position.y = skin.rightLeg.position.y = -12 - 0.2 * sneak;
  skin.leftLeg.position.z = skin.rightLeg.position.z = -0.1 - 3.9 * sneak;
  // Bending tips the skin around the hips (12 below the neck), which sits 8 up in the player.
  const bend = pose.bend ?? 0;
  skin.rotation.set(bend, 0, 0);
  skin.position.set(0, -4 + 12 * Math.cos(bend), 12 * Math.sin(bend));
  skin.leftLeg.rotation.set((pose.leftLegX ?? 0) - bend, 0, pose.leftLegZ ?? 0);
  skin.rightLeg.rotation.set((pose.rightLegX ?? 0) - bend, 0, pose.rightLegZ ?? 0);
  // Those 2 pixels are the world's; players are drawn at 15/16 size, so they're 2.13 of the model's.
  player.position.set(pose.x ?? 0, (pose.y ?? 0) - (2 / 0.9375) * sneak, pose.z ?? 0);
  player.rotation.set(0, pose.turn ?? 0, pose.lean ?? 0);
}

/** Standing: breathing, shifting weight a little, looking around now and then. */
export function idlePose(t: number): Pose {
  const breath = Math.sin(t * 1.9);
  return {
    leftArmZ: 0.07 + breath * 0.025,
    rightArmZ: -0.07 - breath * 0.025,
    leftArmX: Math.sin(t * 0.8) * 0.03,
    rightArmX: -Math.sin(t * 0.8) * 0.03,
    headY: Math.sin(t * 0.45) * 0.24 + Math.sin(t * 1.3) * 0.04,
    headX: Math.sin(t * 0.7) * 0.06 - 0.02,
    lean: Math.sin(t * 0.5) * 0.015,
    y: breath * 0.15,
  };
}

export type EmoteId = "throne" | "dance" | "aura" | "tpose" | "crouch";

export interface Emote {
  id: EmoteId;
  label: string;
  /** A sentence for the tooltip. */
  hint: string;
  /** Something that appears with it. */
  prop?: "throne" | "aura" | "dance";
  pose: (t: number) => Pose;
}

// ── Throne: it builds itself behind you; you glance back, hop up and take your seat ──

/** Where the throne's seat is: the block behind the one the player stands on. */
export const THRONE_Z = -16;
export const THRONE_SIT_AT = 1.0;
// Sitting on the seat's top (half a block up on the dais, then a wool slab: y = 0), thighs flat on
// it, back near the backrest, hands out on the armrests.
const SEATED: Pose = {
  y: 6,
  z: THRONE_Z - 3,
  leftLegX: -PI / 2,
  rightLegX: -PI / 2,
  leftLegZ: 0.06,
  rightLegZ: -0.06,
  leftArmX: -0.6,
  rightArmX: -0.6,
  leftArmZ: 0.4,
  rightArmZ: -0.4,
  headX: -0.06,
};

function thronePose(t: number): Pose {
  if (t < THRONE_SIT_AT) {
    // A look over the shoulder at it forming.
    const glance = Math.sin(clamp01((t - 0.1) / 0.85) * PI) * 0.85;
    return add(idlePose(t), { headY: glance, headX: -0.08 * glance });
  }
  const since = t - THRONE_SIT_AT;
  const k = clamp01(since / 0.8);
  const seated = add(SEATED, {
    headY: Math.sin(since * 0.45) * 0.35,
    headX: Math.sin(t * 0.8) * 0.03,
    // A slow drum of the fingers on the armrest.
    rightArmZ: Math.max(0, Math.sin(t * 5)) * 0.03 * smooth(k * 2 - 1),
    y: Math.sin(t * 1.9) * 0.12,
  });
  const pose = mix(idlePose(t), seated, smooth(k));
  // Up onto the seat with a hop.
  pose.y = (pose.y ?? 0) + Math.sin(k * PI) * 5;
  return pose;
}

// ── A dance: side steps with an arm up, arm rolls, a spin and a pose, bouncing on every beat ──

export const DANCE_BEAT = 0.5;
const DANCE: [number, Pose][] = [
  [0, { leftArmZ: 0.12, rightArmZ: -0.12 }],
  [0.25, { y: 1.2, x: 1.6, lean: -0.08, rightArmZ: -2.5, rightArmX: -0.2, leftArmZ: 0.35, leftLegZ: 0.12, headZ: -0.12, headX: 0.1 }],
  [0.5, { y: 0, x: 1.6, lean: -0.04, rightArmZ: -1.3, leftArmZ: 0.25 }],
  [0.75, { y: 1.2, x: 1.6, lean: -0.08, rightArmZ: -2.5, rightArmX: -0.2, leftArmZ: 0.35, leftLegZ: 0.12, headZ: -0.12, headX: 0.1 }],
  [1, { leftArmZ: 0.3, rightArmZ: -0.3 }],
  [1.25, { y: 1.2, x: -1.6, lean: 0.08, leftArmZ: 2.5, leftArmX: -0.2, rightArmZ: -0.35, rightLegZ: -0.12, headZ: 0.12, headX: 0.1 }],
  [1.5, { y: 0, x: -1.6, lean: 0.04, leftArmZ: 1.3, rightArmZ: -0.25 }],
  [1.75, { y: 1.2, x: -1.6, lean: 0.08, leftArmZ: 2.5, leftArmX: -0.2, rightArmZ: -0.35, rightLegZ: -0.12, headZ: 0.12, headX: 0.1 }],
  [2, { leftArmZ: 0.3, rightArmZ: -0.3 }],
  [2.25, { y: -1, bend: 0.12, leftArmX: -1.45, rightArmX: -0.85, leftArmZ: -0.22, rightArmZ: 0.22, headX: 0.18 }],
  [2.5, { y: 0, bend: 0.04, leftArmX: -0.85, rightArmX: -1.45, leftArmZ: -0.22, rightArmZ: 0.22 }],
  [2.75, { y: -1, bend: 0.12, leftArmX: -1.45, rightArmX: -0.85, leftArmZ: -0.22, rightArmZ: 0.22, headX: 0.18 }],
  [3, { y: 0, leftArmX: -0.85, rightArmX: -1.45, leftArmZ: -0.22, rightArmZ: 0.22 }],
  [3.45, { y: 1.6, turn: 2 * PI, leftArmZ: 1.25, rightArmZ: -1.25 }],
  [3.7, { y: 2.6, turn: 2 * PI, leftArmZ: 2.6, rightArmZ: -2.6, headX: -0.28, leftLegZ: 0.16, rightLegZ: -0.16 }],
  [4, { turn: 2 * PI, leftArmZ: 0.12, rightArmZ: -0.12 }],
];

function dancePose(t: number): Pose {
  const beat = (t % DANCE_BEAT) / DANCE_BEAT;
  // Down on the beat, up between: the bounce that makes it a dance.
  return add(timeline(DANCE, t, 4), { y: -Math.sin(beat * PI) * 0.9, headX: Math.sin(beat * PI * 2) * 0.06 });
}

// ── Aura: crouch while it gathers, fling it out, then hover in the light ──

export const AURA_BURST_AT = 1.25;
const CHARGING: Pose = { bend: 0.2, y: -0.8, headX: 0.38, leftArmX: 0.35, rightArmX: 0.35, leftArmZ: 0.16, rightArmZ: -0.16, leftLegZ: 0.13, rightLegZ: -0.13 };
const FLUNG: Pose = { bend: -0.12, y: 2, headX: -0.35, leftArmX: -0.2, rightArmX: -0.2, leftArmZ: 1.05, rightArmZ: -1.05, leftLegZ: 0.2, rightLegZ: -0.2 };
const RISEN: Pose = { bend: -0.04, y: 2.6, headX: -0.18, leftArmX: 0.12, rightArmX: 0.12, leftArmZ: 0.58, rightArmZ: -0.58, leftLegZ: 0.15, rightLegZ: -0.15 };

function auraPose(t: number): Pose {
  if (t < AURA_BURST_AT) {
    // Shaking harder as it builds.
    return add(mix(idlePose(t), CHARGING, smooth(t / 0.35)), { x: Math.sin(t * 70) * 0.1 * smooth(t / AURA_BURST_AT), headZ: Math.sin(t * 53) * 0.03 });
  }
  const since = t - AURA_BURST_AT;
  if (since < 0.22) return mix(CHARGING, FLUNG, backOut(since / 0.22));
  const settle = smooth((since - 0.22) / 0.6);
  return add(mix(FLUNG, RISEN, settle), {
    y: Math.sin(since * 2) * 0.5 * settle,
    leftArmZ: Math.sin(t * 1.7) * 0.05,
    rightArmZ: -Math.sin(t * 1.7) * 0.05,
  });
}

// ── Memes ──

function tPose(t: number): Pose {
  // Snap into it (a touch past level, then back), turn slowly and hover, asserting dominance.
  const snap = backOut(t / 0.2);
  const arms = { leftArmZ: (PI / 2) * snap, rightArmZ: (-PI / 2) * snap };
  return add(mix(idlePose(t), arms, clamp01(t / 0.2)), { turn: Math.max(0, t - 0.6) * 0.75, y: smooth((t - 0.4) / 0.6) * (0.8 + Math.sin(t * 1.5) * 0.4) });
}

// Crouch spam, Minecraft's way of saying hi, as it comes through in the game: a game tick (a
// twentieth of a second) a character, "x" down. A burst of six, a breath, four more, a longer one.
const SPAM = `xx..xxx..xx...xx..xxx..xx${".".repeat(18)}xxx..xx..xx...xx${".".repeat(22)}`;

/** Whether the player is down, t seconds into crouch spam (the ghost bobs along with it). */
export function crouchedAt(t: number): boolean {
  return t >= 0 && SPAM[Math.floor(t * 20) % SPAM.length] === "x";
}

function crouchSpam(t: number): Pose {
  // Eyes on whoever's being greeted, drifting a little.
  return { ...idlePose(t), headY: Math.sin(t * 0.6) * 0.07, headX: -0.02, sneak: crouchedAt(t) ? 1 : 0 };
}

export const EMOTES: Emote[] = [
  { id: "throne", label: "Throne", hint: "A throne builds itself behind you, block by block, and you take your seat.", prop: "throne", pose: thronePose },
  { id: "dance", label: "Dance", hint: "Side steps, arm rolls, a spin and a pose, on a glass floor that lights up with the beat.", prop: "dance", pose: dancePose },
  { id: "aura", label: "Aura", hint: "Gather your power, then fling it out in a ring of fire.", prop: "aura", pose: auraPose },
  { id: "tpose", label: "T-pose", hint: "Assert dominance.", pose: tPose },
  { id: "crouch", label: "Crouch spam", hint: "The Minecraft hello.", pose: crouchSpam },
];

export function emoteById(id: string): Emote | null {
  return EMOTES.find((emote) => emote.id === id) ?? null;
}

/** Back to standing, including what emotes move that skinview3d's reset doesn't. */
export function resetPose(player: PlayerObject) {
  player.resetJoints();
  player.scale.set(1, 1, 1);
  applyPose(player, {});
}
