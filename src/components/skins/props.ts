// Things that appear with an emote on the skin stage, built from blocks in Wisp's colours: a throne
// that builds itself a block at a time, a ring of pixel fire for the aura, and a stained-glass floor
// that lights up on the beat. Units are skin pixels: the player stands 32 tall, feet at y = -16.

import {
  BoxGeometry,
  CanvasTexture,
  Color,
  DoubleSide,
  Group,
  InstancedMesh,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  NearestFilter,
  type BufferGeometry,
  type Material,
  type Object3D,
  type PointLight,
} from "three";
import { AURA_BURST_AT, DANCE_BEAT, THRONE_Z, clamp01, smooth } from "./emotes";
import { ParticleSystem } from "./particles";
import { BLOCK, GROUND, blockMaterial, box, cross, glowMaterial, halo, makeTextures, seeded, type Textures } from "./voxel";

export interface Prop {
  readonly object: Object3D;
  /** Moves on by `dt` seconds. False once it has finished leaving and can be thrown away. */
  update(dt: number): boolean;
  /** Starts leaving; `update` returns false once it's gone. */
  leave(): void;
  /** Pixels per unit at distance 1, for its particles. */
  setScale(pixelsPerUnit: number): void;
  dispose(): void;
}

/** What the stage lends its props. */
export interface PropContext {
  /** A light always in the scene, for effects to colour and move (null on a plain stage). */
  light: PointLight | null;
}

/** A pop with a little overshoot. */
const backOut = (value: number) => {
  const x = clamp01(value) - 1;
  return 1 + 2.4 * x * x * x + 1.4 * x * x;
};

type Kind = "tiles" | "gold" | "wool" | "calcite" | "ember";

// Textures and materials every prop shares, made on first use and kept: they're tiny.
let shared: { textures: Textures; materials: Record<Kind, Material> } | null = null;
function library() {
  if (shared) return shared;
  const t = makeTextures();
  shared = {
    textures: t,
    materials: {
      tiles: blockMaterial(t.deepslateTiles),
      gold: blockMaterial(t.gold, { emissive: new Color("#ffcf4a"), emissiveMap: t.gold, emissiveIntensity: 0.18 }),
      wool: blockMaterial(t.redWool),
      calcite: blockMaterial(t.calcite, { emissive: new Color("#f4eee4"), emissiveMap: t.calcite, emissiveIntensity: 0.12 }),
      ember: glowMaterial(t.ember, 1.3),
    },
  };
  return shared;
}

// ── The throne ──

interface Piece {
  object: Object3D;
  /** Seconds after the throne starts that it's placed. */
  at: number;
  shown: boolean;
}

/** Seconds between one block going down and the next. */
const PLACE_EVERY = 0.03;
/** Seconds between blocks coming off, top first. */
const TAKE_EVERY = 0.016;

/**
 * A throne of deepslate, calcite, gold and red wool, crested with Wisp's ember sparkle. It builds
 * itself from the ground up, a puff for each block, and comes apart from the top down.
 */
export class ThroneProp implements Prop {
  readonly object = new Group();
  private readonly pieces: Piece[] = [];
  private readonly geometries: BufferGeometry[] = [];
  private readonly puff = new ParticleSystem("puff");
  private readonly spark = new ParticleSystem("spark", 1.7);
  private readonly crest = new Group();
  private readonly crestGlow = halo("#ff6a52", 64, 0.5);
  private readonly light: PointLight | null;
  private time = 0;
  private leaving: number | null = null;
  private crowned = false;

  constructor(context: PropContext) {
    const { materials } = library();
    const seat = THRONE_Z;
    const back = THRONE_Z - BLOCK;
    const blocks: { kind: Kind; x: number; y: number; z: number; size: [number, number, number] }[] = [];
    /** A block, or part of one, by where its bottom sits. */
    const put = (kind: Kind, x: number, bottom: number, z: number, w = BLOCK, h = BLOCK, d = BLOCK) => blocks.push({ kind, x, y: bottom + h / 2, z, size: [w, h, d] });

    // A red carpet out to where you stand.
    for (const z of [0, BLOCK, BLOCK * 2]) put("wool", 0, GROUND, z, BLOCK, 0.6, BLOCK);
    // The dais: a half-block step of deepslate tiles, three wide and two deep.
    for (const x of [-BLOCK, 0, BLOCK]) for (const z of [seat, back]) put("tiles", x, GROUND, z, BLOCK, 8, BLOCK);
    const dais = GROUND + 8;
    // The seat, between armrests of calcite topped with gold.
    put("wool", 0, dais, seat, BLOCK, 8, BLOCK);
    for (const side of [-1, 1]) {
      put("calcite", side * BLOCK, dais, seat, BLOCK, 8, BLOCK);
      put("gold", side * BLOCK, dais + 8, seat, BLOCK, 4, BLOCK);
    }
    // The back: a cushion of red wool between calcite columns, gold above it and across the top.
    for (const row of [0, 1, 2]) for (const x of [-BLOCK, 0, BLOCK]) put(x !== 0 ? "calcite" : row < 2 ? "wool" : "gold", x, dais + row * BLOCK, back);
    const top = dais + BLOCK * 3;
    for (const x of [-BLOCK, 0, BLOCK]) put("gold", x, top, back, BLOCK, 4, BLOCK);
    for (const side of [-1, 1]) put("ember", side * BLOCK, top + 4, back, 6, 6, 6);

    // Placed from the ground up, the middle of each layer first, front to back.
    blocks.sort((a, b) => a.y - b.y || Math.abs(a.x) - Math.abs(b.x) || b.z - a.z);
    for (const block of blocks) {
      const mesh = new Mesh(this.shape(box(...block.size)), materials[block.kind]);
      mesh.position.set(block.x, block.y, block.z);
      this.place(mesh);
    }

    // The crest: the sparkle from Wisp's logo, a four-point star of ember standing on the top.
    const star: [number, number, number, number][] = [
      [0, 0, 6, 4],
      [0, 5, 4, 3],
      [0, -5, 4, 3],
      [5, 0, 4, 3],
      [-5, 0, 4, 3],
      [0, 8, 2, 2],
      [0, -8, 2, 2],
      [8, 0, 2, 2],
      [-8, 0, 2, 2],
    ];
    for (const [x, y, size, depth] of star) {
      const mesh = new Mesh(this.shape(box(size, size, depth)), materials.ember);
      mesh.position.set(x, y, 0);
      this.crest.add(mesh);
    }
    this.crest.add(this.crestGlow);
    this.crest.position.set(0, top + 4 + 9, back);
    this.place(this.crest);

    this.light = context.light;
    this.puff.attach(this.object);
    this.spark.attach(this.object);
  }

  private shape(geometry: BufferGeometry) {
    this.geometries.push(geometry);
    return geometry;
  }

  private place(object: Object3D) {
    object.visible = false;
    this.object.add(object);
    this.pieces.push({ object, at: 0.05 + this.pieces.length * PLACE_EVERY, shown: false });
  }

  private puffAt(object: Object3D, count: number) {
    const { x, y, z } = object.position;
    this.puff.burstAt(x, y, z, count, 12);
  }

  update(dt: number): boolean {
    const before = this.time;
    this.time += dt;
    const t = this.time;
    const crownedAt = this.pieces[this.pieces.length - 1].at;
    let glow = 0;
    if (this.leaving === null) {
      for (const piece of this.pieces) {
        if (t < piece.at) continue;
        if (!piece.shown) {
          piece.shown = true;
          piece.object.visible = true;
          this.puffAt(piece.object, piece.object === this.crest ? 10 : 5);
        }
        piece.object.scale.setScalar(Math.max(0.001, backOut((t - piece.at) / 0.18)));
      }
      if (!this.crowned && t > crownedAt + 0.15) {
        this.crowned = true;
        const { x, y, z } = this.crest.position;
        this.spark.burstAt(x, y, z, 60, 14);
      }
      glow = smooth((t - crownedAt) / 0.35);
    } else {
      // Taken apart from the top down.
      const since = t - this.leaving;
      const was = before - this.leaving;
      const count = this.pieces.length;
      this.pieces.forEach((piece, index) => {
        if (!piece.shown) return;
        const at = (count - 1 - index) * TAKE_EVERY;
        if (since < at) return;
        if (was <= at) this.puffAt(piece.object, 3);
        const left = 1 - clamp01((since - at) / 0.1);
        piece.object.scale.setScalar(Math.max(0.001, left));
        piece.object.visible = left > 0;
      });
      glow = this.crowned ? 1 - smooth(since / 0.25) : 0;
    }
    // The crest's light, warm on the gold and the top of the player's head.
    if (this.light) {
      this.light.color.set("#ff7a5c");
      this.light.position.set(0, this.crest.position.y, this.crest.position.z + 14);
      this.light.intensity = glow * (520 + Math.sin(t * 2.4) * 60);
    }
    this.crest.rotation.y = this.crowned ? Math.sin((t - crownedAt) * 1.3) * 0.25 : 0;
    this.puff.update(dt);
    this.spark.update(dt);
    const gone = this.leaving !== null && t - this.leaving > this.pieces.length * TAKE_EVERY + 0.15;
    return !gone || this.puff.alive || this.spark.alive;
  }

  leave() {
    if (this.leaving === null) this.leaving = this.time;
  }

  setScale(pixelsPerUnit: number) {
    this.puff.setScale(pixelsPerUnit);
    this.spark.setScale(pixelsPerUnit);
  }

  dispose() {
    this.object.removeFromParent();
    this.puff.dispose();
    this.spark.dispose();
    for (const geometry of this.geometries) geometry.dispose();
    this.crestGlow.material.dispose();
    if (this.light) this.light.intensity = 0;
  }
}

// ── The aura: a ring of pixel fire ──

const FIRE_FRAMES = 12;
const FIRE_COLORS = ["", "#7a1a10", "#b32e18", "#e2531c", "#ff8326", "#ffb43a", "#ffe07a"];

/**
 * Pixel fire, drawn here: heat rising from a white-hot base, drifting and cooling as it goes, so
 * it breaks into tongues of yellow, orange and red. Twelve frames stacked in one picture.
 */
function fireAtlas(): HTMLCanvasElement {
  const width = 16;
  const height = 16;
  const hottest = FIRE_COLORS.length - 1;
  const heat = new Array<number>(width * height).fill(0);
  const random = seeded(4242);
  const spread = () => {
    for (let x = 0; x < width; x += 1) heat[(height - 1) * width + x] = hottest;
    for (let y = 1; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        const from = heat[y * width + x];
        const drift = Math.floor(random() * 3) - 1;
        const to = (y - 1) * width + ((x + drift + width) % width);
        heat[to] = from === 0 ? 0 : Math.max(0, from - (random() < 0.5 ? 1 : 0));
      }
    }
  };
  for (let warm = 0; warm < 40; warm += 1) spread();
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height * FIRE_FRAMES;
  const g = canvas.getContext("2d") as CanvasRenderingContext2D;
  for (let frame = 0; frame < FIRE_FRAMES; frame += 1) {
    spread();
    spread();
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        const level = heat[y * width + x];
        if (level === 0) continue;
        g.fillStyle = FIRE_COLORS[level];
        g.fillRect(x, frame * height + y, 1, 1);
      }
    }
  }
  return canvas;
}

let fireCanvas: HTMLCanvasElement | null = null;
const AURA_FIRES = 12;
/** How far out the ring burns while gathering, and once it's flung out. */
const RING_IN = 14;
const RING_OUT = 26;

/** Fire round the player: small at the feet while it gathers, flung out into a ring, burning on. */
export class AuraProp implements Prop {
  readonly object = new Group();
  private readonly fires: Mesh[] = [];
  private readonly textures: CanvasTexture[] = [];
  private readonly materials: MeshBasicMaterial[] = [];
  private readonly shape: BufferGeometry;
  private readonly embers = new ParticleSystem("embers", 1.8);
  private readonly glow = halo("#ff6a3a", 120, 0);
  private readonly spark = new ParticleSystem("spark", 1.8);
  private readonly puff = new ParticleSystem("puff");
  private readonly light: PointLight | null;
  private frame = 0;
  private frameTime = 0;
  private time = 0;
  private leaving: number | null = null;
  private burst = false;

  constructor(context: PropContext) {
    fireCanvas ??= fireAtlas();
    const atlas = new CanvasTexture(fireCanvas);
    atlas.magFilter = NearestFilter;
    atlas.minFilter = NearestFilter;
    atlas.generateMipmaps = false;
    atlas.repeat.set(1, 1 / FIRE_FRAMES);
    // Three copies of the picture a few frames apart, so neighbouring flames don't flicker as one.
    for (let copy = 0; copy < 3; copy += 1) {
      const texture = copy === 0 ? atlas : atlas.clone();
      this.textures.push(texture);
      this.materials.push(new MeshBasicMaterial({ map: texture, alphaTest: 0.5, side: DoubleSide, color: new Color(1.15, 1.05, 1), fog: false }));
    }
    this.showFrame();
    this.shape = cross(BLOCK, BLOCK * 1.5);
    for (let index = 0; index < AURA_FIRES; index += 1) {
      const fire = new Mesh(this.shape, this.materials[index % 3]);
      fire.rotation.y = index * 1.3;
      this.fires.push(fire);
      this.object.add(fire);
    }
    this.embers.area = { x: 0, y: GROUND, z: 0, spread: RING_IN };
    this.embers.emitting = false;
    for (const system of [this.embers, this.spark, this.puff]) system.attach(this.object);
    this.glow.position.set(0, GROUND + 10, 0);
    this.object.add(this.glow);
    this.light = context.light;
  }

  private showFrame() {
    this.textures.forEach((texture, copy) => {
      const frame = (this.frame + copy * 4) % FIRE_FRAMES;
      texture.offset.y = 1 - (frame + 1) / FIRE_FRAMES;
    });
  }

  update(dt: number): boolean {
    this.time += dt;
    const t = this.time;
    this.frameTime += dt;
    if (this.frameTime > 1 / 14) {
      this.frameTime = 0;
      this.frame = (this.frame + 1) % FIRE_FRAMES;
      this.showFrame();
    }
    const since = t - AURA_BURST_AT;
    if (!this.burst && since >= 0) {
      this.burst = true;
      this.spark.burstAt(0, GROUND + 6, 0, 80, 18);
      for (let index = 0; index < 20; index += 1) {
        const angle = (index / 20) * Math.PI * 2;
        this.puff.burstAt(Math.cos(angle) * RING_OUT, GROUND + 4, Math.sin(angle) * RING_OUT, 2, 6);
      }
    }
    const fading = this.leaving === null ? 1 : 1 - smooth((t - this.leaving) / 0.4);
    const out = since < 0 ? 0 : backOut(since / 0.3);
    const radius = RING_IN + (RING_OUT - RING_IN) * out;
    // Flaring up at the burst, then burning lower so the player stays in view.
    const flare = since < 0 ? 0 : since < 0.3 ? out : 1 - smooth((since - 0.3) / 0.7);
    const size = (since < 0 ? 0.3 + 0.25 * smooth(t / AURA_BURST_AT) : 0.55 + 0.3 * flare) * fading;
    this.fires.forEach((fire, index) => {
      const angle = (index / AURA_FIRES) * Math.PI * 2 + t * 0.35;
      const flicker = 1 + Math.sin(t * 9 + index * 2.1) * 0.1;
      fire.position.set(Math.cos(angle) * radius, GROUND, Math.sin(angle) * radius);
      fire.scale.set(size, size * flicker, size);
      fire.visible = size > 0.02;
    });
    this.embers.emitting = this.leaving === null && t > AURA_BURST_AT * 0.4;
    this.embers.area.spread = radius;
    for (const system of [this.embers, this.spark, this.puff]) system.update(dt);
    if (this.light) {
      const flash = since > 0 && since < 0.4 ? (1 - since / 0.4) * 900 : 0;
      this.light.color.set("#ff7a3a");
      this.light.position.set(0, -8, 12);
      this.light.intensity = ((since < 0 ? 60 + 140 * smooth(t / AURA_BURST_AT) : 340) + Math.sin(t * 13) * 40 + flash) * fading;
    }
    // The glow at the feet, standing in for bloom: it gathers, flares at the burst, then burns on.
    this.glow.material.opacity = ((since < 0 ? 0.12 + 0.2 * smooth(t / AURA_BURST_AT) : 0.32) + flare * 0.45 + Math.sin(t * 11) * 0.03) * fading;
    this.glow.scale.setScalar(radius * 4.6);
    return this.leaving === null || t - this.leaving < 0.45 || this.embers.alive || this.spark.alive;
  }

  leave() {
    if (this.leaving === null) this.leaving = this.time;
  }

  setScale(pixelsPerUnit: number) {
    for (const system of [this.embers, this.spark, this.puff]) system.setScale(pixelsPerUnit);
  }

  dispose() {
    this.object.removeFromParent();
    for (const system of [this.embers, this.spark, this.puff]) system.dispose();
    this.shape.dispose();
    this.glow.material.dispose();
    for (const material of this.materials) material.dispose();
    for (const texture of this.textures) texture.dispose();
    if (this.light) this.light.intensity = 0;
  }
}

// ── The dance floor ──

const DANCE_COLORS = ["#ff4f9a", "#38d9ff", "#ffd54a", "#9b6bff", "#5cff8a", "#ff8a3a"].map((hex) => new Color(hex));
/** Tiles out from the middle: a 5×5 floor. */
const FLOOR = 2;

/**
 * A floor of stained glass under the feet, lighting up on the beat: rippling out from the middle,
 * then a checkerboard, then a star, a new colour every beat.
 */
export class DanceProp implements Prop {
  readonly object = new Group();
  private readonly tiles: InstancedMesh;
  private readonly material: MeshBasicMaterial;
  private readonly shape: BoxGeometry;
  private readonly cells: { x: number; z: number; ring: number }[] = [];
  private readonly spark = new ParticleSystem("spark", 1.6);
  private readonly light: PointLight | null;
  private readonly matrix = new Matrix4();
  private readonly color = new Color();
  private time = 0;
  private beat = -1;
  private leaving: number | null = null;

  constructor(context: PropContext) {
    const { textures } = library();
    for (let x = -FLOOR; x <= FLOOR; x += 1) for (let z = -FLOOR; z <= FLOOR; z += 1) this.cells.push({ x, z, ring: Math.max(Math.abs(x), Math.abs(z)) });
    this.shape = new BoxGeometry(BLOCK - 1, 1, BLOCK - 1);
    this.material = new MeshBasicMaterial({ map: textures.glass });
    this.tiles = new InstancedMesh(this.shape, this.material, this.cells.length);
    this.cells.forEach((_, index) => {
      this.tiles.setMatrixAt(index, this.matrix.makeScale(0, 0, 0));
      this.tiles.setColorAt(index, this.color.setRGB(0, 0, 0));
    });
    this.object.add(this.tiles);
    this.spark.attach(this.object);
    this.light = context.light;
  }

  update(dt: number): boolean {
    this.time += dt;
    const t = this.time;
    const beat = Math.floor(t / DANCE_BEAT);
    const phase = (t % DANCE_BEAT) / DANCE_BEAT;
    const pulse = 1 - phase;
    const hue = DANCE_COLORS[beat % DANCE_COLORS.length];
    // Each pattern holds for two beats.
    const pattern = Math.floor(beat / 2) % 3;
    let most = 0;
    this.cells.forEach((cell, index) => {
      // Tiles ripple in from the middle as it arrives, and go from the edge inwards as it leaves.
      const shown =
        this.leaving === null ? smooth((t - cell.ring * 0.07) / 0.15) : 1 - smooth((t - this.leaving - (FLOOR - cell.ring) * 0.07) / 0.15);
      most = Math.max(most, shown);
      this.matrix.makeScale(shown, 1, shown).setPosition(cell.x * BLOCK, GROUND - 0.4, cell.z * BLOCK);
      this.tiles.setMatrixAt(index, this.matrix);
      let lit: number;
      if (pattern === 0) lit = cell.ring === Math.floor(phase * (FLOOR + 1)) ? 1 : 0;
      else if (pattern === 1) lit = Math.abs(cell.x + cell.z + beat) % 2 === 0 ? pulse : 0;
      else lit = (cell.x === 0 || cell.z === 0 || Math.abs(cell.x) === Math.abs(cell.z)) === (beat % 2 === 0) ? pulse : 0;
      this.tiles.setColorAt(index, this.color.copy(hue).multiplyScalar(0.22 + lit * 1.9));
    });
    this.tiles.instanceMatrix.needsUpdate = true;
    if (this.tiles.instanceColor) this.tiles.instanceColor.needsUpdate = true;
    if (beat !== this.beat && this.leaving === null) {
      this.beat = beat;
      for (const cell of this.cells) if ((cell.x * 3 + cell.z * 5 + beat) % 7 === 0) this.spark.burstAt(cell.x * BLOCK, GROUND + 2, cell.z * BLOCK, 4, 10);
    }
    this.spark.update(dt);
    const shown = this.leaving === null ? smooth(t / 0.3) : 1 - smooth((t - this.leaving) / 0.35);
    if (this.light) {
      this.light.color.copy(hue);
      this.light.position.set(0, -8, 14);
      this.light.intensity = shown * (110 + pulse * 170);
    }
    const gone = this.leaving !== null && most <= 0;
    return !gone || this.spark.alive;
  }

  leave() {
    if (this.leaving === null) this.leaving = this.time;
  }

  setScale(pixelsPerUnit: number) {
    this.spark.setScale(pixelsPerUnit);
  }

  dispose() {
    this.object.removeFromParent();
    this.spark.dispose();
    this.shape.dispose();
    this.material.dispose();
    this.tiles.dispose();
    if (this.light) this.light.intensity = 0;
  }
}

export type PropKind = "throne" | "aura" | "dance";

export function makeProp(kind: PropKind, context: PropContext): Prop {
  if (kind === "throne") return new ThroneProp(context);
  if (kind === "aura") return new AuraProp(context);
  return new DanceProp(context);
}
