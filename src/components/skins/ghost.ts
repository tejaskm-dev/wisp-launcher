// Wisp, alive: the launcher's ghost as a little Minecraft-style mob living on the stage. While the
// player stands about, it plays with them: wanders off, circles them, peeks over a shoulder, boops
// their head, sits on it, spins for joy. During emotes it joins in. The player's eyes follow it.
//
// Built like a mob model: boxes with one texel per unit, cream with dark eyes, a wavy hem of little
// tails, and the ember sparkle from Wisp's logo floating at its shoulder.

import { BoxGeometry, Color, Group, Mesh, MeshBasicMaterial, Vector3, type Material, type Texture } from "three";
import { box, halo, paint } from "./voxel";

type Mood = "hover" | "wander" | "orbit" | "peek" | "boop" | "perch" | "spin";

/** What the player is up to, as the ghost sees it. */
export interface Playmate {
  /** An emote id, or "idle". */
  activity: string;
  /** Seconds since it started. */
  since: number;
  /** The centre of the player's head, in stage space. */
  head: Vector3;
  /** Where the camera is, so the ghost can face it. */
  camera: Vector3;
  /** Free roam: keep a polite distance. */
  roaming: boolean;
}

const CREAM = ["#f4eee4", "#efe8dc", "#f8f4ec", "#e9e1d4"];
const TAU = Math.PI * 2;
/** The model is built at 1 unit per texel; it lives a little smaller, about a head and a half. */
const SIZE = 0.75;
const wrap = (angle: number) => Math.atan2(Math.sin(angle), Math.cos(angle));
const ease = (x: number) => (x <= 0 ? 0 : x >= 1 ? 1 : x * x * (3 - 2 * x));

/** The ghost's face: open eyes, a blink, and a happy squint. */
function faces() {
  const base = (p: Parameters<Parameters<typeof paint>[1]>[0]) => p.each((x, y) => p.px(x, y, p.pick(CREAM, [4, 3, 2, 1])));
  // The front face shows columns 0–13 and rows 2–15 of the picture (14×14 units).
  const open = paint(61, (p) => {
    base(p);
    for (const ex of [3, 9]) {
      for (let y = 6; y <= 8; y += 1) for (let x = ex; x <= ex + 1; x += 1) p.px(x, y, "#1d1a19");
      p.px(ex, 6, "#fbf7ef");
    }
  });
  const blink = paint(62, (p) => {
    base(p);
    for (const ex of [3, 9]) for (let x = ex; x <= ex + 1; x += 1) p.px(x, 8, "#1d1a19");
  });
  const happy = paint(63, (p) => {
    base(p);
    for (const ex of [3, 9]) {
      p.px(ex, 8, "#1d1a19");
      p.px(ex + 1, 7, "#1d1a19");
      p.px(ex + 2, 8, "#1d1a19");
    }
    p.px(2, 9, "#f2a7a7");
    p.px(11, 9, "#f2a7a7");
  });
  const plain = paint(64, base);
  return { open, blink, happy, plain };
}

export class WispGhost {
  readonly object = new Group();
  private readonly body = new Group();
  private readonly tails: Mesh[] = [];
  private readonly sparkle = new Group();
  private readonly face: MeshBasicMaterial;
  private readonly textures: ReturnType<typeof faces>;
  private readonly materials: Material[] = [];
  private readonly position = new Vector3(30, 4, 8);
  private readonly velocity = new Vector3();
  private readonly target = new Vector3(30, 4, 8);
  private mood: Mood = "hover";
  private moodTime = 0;
  private moodLength = 2.5;
  private side = 1;
  private yaw = 0;
  private spin = 0;
  private time = 0;
  private blinkAt = 3;
  private happyFor = 0;
  private booped = false;
  private lastActivity = "idle";

  /** Where the player should look (the ghost, mostly) and how much (0 to 1). */
  readonly look = new Vector3();
  attention = 0;
  /** A shove on the player's head right after a boop, easing off. */
  flinch = 0;

  constructor() {
    this.textures = faces();
    // A spirit glows from inside, so it isn't lit by the scene. Its faces are shaded the way the
    // game shades blocks (top brightest, sides darker, underneath darkest) so it keeps its shape.
    const shade = (amount: number) => new MeshBasicMaterial({ map: this.textures.plain, color: new Color(amount, amount, amount) });
    const top = shade(1);
    const front = shade(0.96);
    const back = shade(0.86);
    const side = shade(0.78);
    const under = shade(0.6);
    this.face = new MeshBasicMaterial({ map: this.textures.open });
    // Unfogged, like the wisps, so the two share a shader (the fog never comes this close anyway).
    const ember = new MeshBasicMaterial({ color: "#ff5a52", fog: false });
    this.materials.push(top, front, back, side, under, this.face, ember);
    // BoxGeometry's faces: +x, -x, +y, -y, +z (front), -z.
    const faces6 = [side, side, top, under, front, back];

    // Body: 14 wide, 13 tall, 12 deep, its face on the front (+z).
    this.body.add(new Mesh(box(14, 13, 12), [side, side, top, under, this.face, back]));
    const dome = new Mesh(box(10, 2, 9), faces6);
    dome.position.y = 7.5;
    this.body.add(dome);
    // A faint glow round it.
    const aura = halo("#fff4e4", 46, 0.12);
    this.materials.push(aura.material);
    this.body.add(aura);
    // The hem: little tails that wiggle as it floats.
    for (const [x, z] of [
      [-4.5, 3],
      [0, 3.5],
      [4.5, 3],
      [-3, -3],
      [3, -3],
    ]) {
      const tail = new Mesh(box(4, 3, 4), faces6);
      tail.position.set(x, -8, z);
      this.tails.push(tail);
      this.body.add(tail);
    }
    // The ember sparkle: a four-point star of glowing pixels, as in the logo.
    const bar = new BoxGeometry(1.4, 6, 1.4);
    const beam = new BoxGeometry(6, 1.4, 1.4);
    const heart = new BoxGeometry(2.4, 2.4, 2.4);
    for (const shape of [bar, beam, heart]) this.sparkle.add(new Mesh(shape, ember));
    const sparkleGlow = halo("#ff6a5a", 24, 0.6);
    this.materials.push(sparkleGlow.material);
    this.sparkle.add(sparkleGlow);
    // At its upper left as you look at it, as on the logo.
    this.sparkle.position.set(-10, 10, 0);
    this.body.add(this.sparkle);

    this.object.add(this.body);
    this.object.scale.setScalar(SIZE);
    this.object.position.copy(this.position);
  }

  /** Where to go next, given what the player is doing. */
  private choose(playmate: Playmate) {
    const options: [Mood, number][] = playmate.roaming
      ? [
          ["wander", 3],
          ["orbit", 2],
          ["spin", 1],
        ]
      : [
          ["wander", 3],
          ["orbit", 2],
          ["peek", 2],
          ["boop", 2],
          ["perch", 1.5],
          ["spin", 1.5],
        ];
    const choices = options.filter(([mood]) => mood !== this.mood);
    const total = choices.reduce((sum, [, weight]) => sum + weight, 0);
    let roll = Math.random() * total;
    let next: Mood = "wander";
    for (const [mood, weight] of choices) {
      roll -= weight;
      if (roll < 0) {
        next = mood;
        break;
      }
    }
    this.mood = next;
    this.moodTime = 0;
    this.side = Math.random() < 0.5 ? -1 : 1;
    this.booped = false;
    this.moodLength = { hover: 2, wander: 3 + Math.random() * 1.5, orbit: 4.2, peek: 3.2, boop: 2.4, perch: 3.6, spin: 1.5 }[next];
    if (next === "wander") {
      const far = playmate.roaming ? 52 : 34;
      const angle = (Math.random() - 0.5) * Math.PI * 1.3;
      const distance = far * (0.7 + Math.random() * 0.5);
      this.target.set(Math.sin(angle) * distance, -4 + Math.random() * 26, Math.cos(angle) * distance * 0.7 + 4);
    }
  }

  /** Steers towards `goal` with a soft spring: floaty, never snapping. */
  private steer(goal: Vector3, dt: number, stiffness = 5, damping = 4) {
    const pull = goal.clone().sub(this.position).multiplyScalar(stiffness);
    this.velocity.addScaledVector(pull, dt).multiplyScalar(Math.max(0, 1 - damping * dt));
    this.position.addScaledVector(this.velocity, dt);
  }

  update(dt: number, playmate: Playmate) {
    this.time += dt;
    const t = this.time;
    const head = playmate.head;
    if (playmate.activity !== this.lastActivity) {
      this.lastActivity = playmate.activity;
      this.mood = "hover";
      this.moodTime = 0;
    }
    let attention = 0;
    let faceTarget: Vector3 | null = null;
    const goal = new Vector3();
    const happy = () => (this.happyFor = 0.8);

    switch (playmate.activity) {
      case "throne":
        // A loyal companion beside the armrest, looking up at its royal.
        goal.set(36, 10 + Math.sin(t * 1.6) * 2, -4);
        faceTarget = head;
        break;
      case "dance": {
        const beat = (playmate.since % 0.5) / 0.5;
        goal.set(30 + Math.sin(playmate.since * Math.PI) * 4, 4 + Math.abs(Math.sin(beat * Math.PI)) * 5, 6);
        const loop = playmate.since % 4;
        this.spin = loop > 3.1 && loop < 3.7 ? ((loop - 3.1) / 0.6) * TAU : 0;
        faceTarget = playmate.camera;
        if (loop > 3.6 && loop < 3.7) happy();
        break;
      }
      case "aura":
        // Nervous while it builds, blown back by the burst, then peeking back.
        if (playmate.since < 1.25) goal.set(30 + Math.sin(t * 40) * 0.6, 6, 6);
        else if (playmate.since < 2.2) goal.set(64, 26, -16);
        else goal.set(44, 12, -2);
        faceTarget = head;
        break;
      case "tpose":
        // Keeping its distance, circling, unsure about this.
        goal.set(Math.cos(t * 0.6) * 40, 10 + Math.sin(t * 1.2) * 3, Math.sin(t * 0.6) * 30);
        faceTarget = head;
        break;
      case "crouch":
        // Bobbing back: the Minecraft hello, returned.
        goal.set(28, 2 + (0.5 + 0.5 * Math.tanh(Math.sin(playmate.since * 15) * 5)) * -6 + 8, 8);
        faceTarget = playmate.camera;
        break;
      default: {
        this.moodTime += dt;
        if (this.moodTime >= this.moodLength) this.choose(playmate);
        const m = this.moodTime;
        attention = 0.9;
        switch (this.mood) {
          case "hover":
            goal.set(30, 6, 8);
            faceTarget = playmate.camera;
            attention = 0.6;
            break;
          case "wander":
            goal.copy(this.target);
            faceTarget = this.velocity.lengthSq() > 40 ? this.position.clone().add(this.velocity) : playmate.camera;
            attention = 0.75;
            break;
          case "orbit": {
            const angle = m * 1.5 * this.side + 0.6;
            goal.set(Math.sin(angle) * 30, 10 + Math.sin(m * 3) * 4, Math.cos(angle) * 26);
            faceTarget = this.position.clone().add(this.velocity);
            break;
          }
          case "peek":
            // Behind the player, then out at a shoulder, then away.
            if (m < 1.1) goal.set(0, 6, -24);
            else if (m < 2.6) goal.set(this.side * 12, head.y + 3 + Math.sin(m * 6) * 1.5, head.z - 6);
            else goal.set(this.side * 34, 10, 4);
            faceTarget = head;
            if (m > 1.3 && m < 1.4) happy();
            break;
          case "boop":
            if (m < 1.1) goal.set(this.side * 10, head.y + 5, head.z + 14);
            else if (m < 1.4) {
              goal.set(this.side * 2, head.y + 3, head.z + 7);
              if (!this.booped && this.position.distanceTo(goal) < 6) {
                this.booped = true;
                this.flinch = 1;
                happy();
              }
            } else goal.set(this.side * 26, head.y + 10, head.z + 18);
            faceTarget = head;
            break;
          case "perch":
            // Sits on the player's head for a bit.
            goal.set(head.x, head.y + 12 + Math.sin(m * 3) * 0.4, head.z);
            faceTarget = playmate.camera;
            attention = m < 2.8 ? 0.55 : 0.9;
            if (m > 1 && m < 1.1) happy();
            break;
          case "spin":
            goal.copy(this.position).setY(this.position.y + (m < 0.5 ? 0.6 : 0));
            this.spin = ease(m / 1.1) * TAU;
            faceTarget = playmate.camera;
            if (m > 0.9 && m < 1) happy();
            break;
        }
        if (this.mood !== "spin") this.spin = 0;
      }
    }

    // Never inside the player's body.
    const fromHead = goal.clone().sub(new Vector3(head.x, goal.y, head.z));
    if (fromHead.length() < 9 && goal.y < head.y + 10) goal.copy(new Vector3(head.x, goal.y, head.z).add(fromHead.setLength(9)));
    const quick = this.mood === "boop" && this.moodTime > 1.1 && this.moodTime < 1.4;
    this.steer(goal, Math.min(dt, 0.05), quick ? 14 : 5, quick ? 6 : 4);
    this.object.position.copy(this.position);

    // Turn to face whatever it's interested in, smoothly.
    const facing = faceTarget ?? playmate.camera;
    const wanted = Math.atan2(facing.x - this.position.x, facing.z - this.position.z);
    this.yaw += wrap(wanted - this.yaw) * Math.min(1, dt * 6);
    this.object.rotation.y = this.yaw + this.spin;

    // Floating: a bob, a lean into its motion, wiggling tails, a twinkling sparkle.
    this.body.position.y = Math.sin(t * 2.2) * 1.4;
    this.body.rotation.z = Math.max(-0.35, Math.min(0.35, -this.velocity.x * 0.01));
    this.body.rotation.x = Math.max(-0.3, Math.min(0.3, this.velocity.z * 0.008));
    this.tails.forEach((tail, index) => {
      tail.position.y = -8 + Math.sin(t * 7 + index * 1.3) * 0.7;
      tail.scale.y = 1 + Math.sin(t * 7 + index * 1.3) * 0.2;
    });
    this.sparkle.rotation.z = t * 1.4;
    this.sparkle.scale.setScalar(0.85 + Math.sin(t * 4) * 0.15);

    // Blinks now and then; squints when happy.
    this.happyFor = Math.max(0, this.happyFor - dt);
    let face: Texture = this.textures.open;
    if (this.happyFor > 0) face = this.textures.happy;
    else if (t > this.blinkAt) {
      face = this.textures.blink;
      if (t > this.blinkAt + 0.13) this.blinkAt = t + 2.5 + Math.random() * 3.5;
    }
    if (this.face.map !== face) {
      this.face.map = face;
    }

    this.flinch = Math.max(0, this.flinch - dt * 2.2);
    this.attention += (attention - this.attention) * Math.min(1, dt * 3);
    this.look.copy(this.position).add(new Vector3(0, this.body.position.y, 0));
  }

  dispose() {
    this.object.removeFromParent();
    this.object.traverse((child) => {
      if (child instanceof Mesh) child.geometry.dispose();
    });
    for (const material of this.materials) material.dispose();
    for (const texture of Object.values(this.textures)) texture.dispose();
  }
}
