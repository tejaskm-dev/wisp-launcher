// Emotes for the 3D skin viewer: a few that mean something, each choreographed from keyframes.
// Joints follow skinview3d: arm and leg rotation.x < 0 swings forward and up; left arm
// rotation.z > 0 (right arm < 0) lifts sideways; head rotation.x > 0 looks down. The player is
// 32 units tall, centred on 0, feet at -16.

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
}

const KEYS: (keyof Pose)[] = [
  "headX", "headY", "headZ", "leftArmX", "leftArmZ", "rightArmX", "rightArmZ",
  "leftLegX", "leftLegZ", "rightLegX", "rightLegZ", "bend", "x", "y", "z", "turn", "lean",
];

const PI = Math.PI;
export const clamp01 = (value: number) => Math.min(1, Math.max(0, value));
export const smooth = (value: number) => {
  const x = clamp01(value);
  return x * x * (3 - 2 * x);
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
  skin.head.rotation.set(pose.headX ?? 0, pose.headY ?? 0, pose.headZ ?? 0);
  skin.leftArm.rotation.set(pose.leftArmX ?? 0, 0, pose.leftArmZ ?? 0);
  skin.rightArm.rotation.set(pose.rightArmX ?? 0, 0, pose.rightArmZ ?? 0);
  // Bending tips the skin around the hips (12 below the neck), which sits 8 up in the player.
  const bend = pose.bend ?? 0;
  skin.rotation.set(bend, 0, 0);
  skin.position.set(0, -4 + 12 * Math.cos(bend), 12 * Math.sin(bend));
  skin.leftLeg.rotation.set((pose.leftLegX ?? 0) - bend, 0, pose.leftLegZ ?? 0);
  skin.rightLeg.rotation.set((pose.rightLegX ?? 0) - bend, 0, pose.rightLegZ ?? 0);
  player.position.set(pose.x ?? 0, pose.y ?? 0, pose.z ?? 0);
  player.rotation.set(0, pose.turn ?? 0, pose.lean ?? 0);
}

/** Standing, breathing, looking around a little. */
export function idlePose(t: number): Pose {
  const breath = Math.cos(t * 2) * 0.03;
  return {
    leftArmZ: 0.06 + breath,
    rightArmZ: -0.06 - breath,
    headY: Math.sin(t * 0.6) * 0.18,
    headX: Math.sin(t * 0.9) * 0.05,
  };
}

export type EmoteId = "throne" | "dance" | "aura" | "tpose" | "crouch";

export interface Emote {
  id: EmoteId;
  label: string;
  /** A sentence for the tooltip. */
  hint: string;
  /** Something that appears with it. */
  prop?: "throne" | "aura";
  pose: (t: number) => Pose;
}

// ── Throne: it gathers out of sparkles, then you sit and survey your realm ──

/** Seat height: the hips rest 6 below where they stand. */
const SEATED: Pose = {
  y: -6,
  leftLegX: -PI / 2,
  rightLegX: -PI / 2,
  leftLegZ: 0.07,
  rightLegZ: -0.07,
  leftArmX: -1.05,
  rightArmX: -1.05,
  leftArmZ: 0.3,
  rightArmZ: -0.3,
  headX: -0.08,
};
export const THRONE_SIT_AT = 0.95;

function thronePose(t: number): Pose {
  if (t < THRONE_SIT_AT) return idlePose(t);
  const seated = add(SEATED, { headY: Math.sin((t - THRONE_SIT_AT) * 0.5) * 0.4, headX: Math.sin(t * 0.8) * 0.03 });
  return mix(idlePose(t), seated, smooth((t - THRONE_SIT_AT) / 0.55));
}

// ── A dance: two side steps with the arm up, arm rolls, a spin and a pose (4 s loop) ──

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

// ── Aura: a crouch while it gathers, then a burst and a hover in the light ──

export const AURA_BURST_AT = 1.25;
const CHARGING: Pose = { bend: 0.2, y: -0.8, headX: 0.38, leftArmX: 0.25, rightArmX: 0.25, leftArmZ: 0.2, rightArmZ: -0.2, leftLegZ: 0.13, rightLegZ: -0.13 };
const RISEN: Pose = { bend: -0.04, y: 2.6, headX: -0.2, leftArmX: 0.12, rightArmX: 0.12, leftArmZ: 0.58, rightArmZ: -0.58, leftLegZ: 0.15, rightLegZ: -0.15 };

function auraPose(t: number): Pose {
  if (t < AURA_BURST_AT) {
    const gather = smooth(t / 0.35);
    // Shaking harder as it builds.
    return add(mix(idlePose(t), CHARGING, gather), { x: Math.sin(t * 70) * 0.09 * smooth(t / AURA_BURST_AT) });
  }
  const rise = smooth((t - AURA_BURST_AT) / 0.3);
  return add(mix(CHARGING, RISEN, rise), {
    y: Math.sin((t - AURA_BURST_AT) * 2) * 0.5 * rise,
    leftArmZ: Math.sin(t * 1.7) * 0.05,
    rightArmZ: -Math.sin(t * 1.7) * 0.05,
  });
}

// ── Memes ──

const T_POSE: Pose = { leftArmZ: PI / 2, rightArmZ: -PI / 2 };

function tPose(t: number): Pose {
  // Snap into it, then turn slowly and hover, asserting dominance.
  return add(mix(idlePose(t), T_POSE, smooth(t / 0.12)), { turn: Math.max(0, t - 0.6) * 0.75, y: smooth((t - 0.4) / 0.6) * (0.8 + Math.sin(t * 1.5) * 0.4) });
}

const CROUCHED: Pose = { bend: 0.5, y: -1.8, headX: 0.28, leftArmX: -0.3, rightArmX: -0.3, leftArmZ: 0.08, rightArmZ: -0.08 };

function crouchSpam(t: number): Pose {
  // Square-ish, about five crouches a second: Minecraft's way of saying hi.
  const down = 0.5 + 0.5 * Math.tanh(Math.sin(t * 15) * 5);
  return add(mix(idlePose(t), CROUCHED, down), { headY: Math.sin(t * 3) * 0.2 });
}

export const EMOTES: Emote[] = [
  { id: "throne", label: "Throne", hint: "A throne gathers out of sparkles and you take your seat.", prop: "throne", pose: thronePose },
  { id: "dance", label: "Dance", hint: "Side steps, arm rolls, a spin and a pose.", pose: (t) => timeline(DANCE, t, 4) },
  { id: "aura", label: "Aura", hint: "Gather your power, then rise in a blaze of light.", prop: "aura", pose: auraPose },
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
