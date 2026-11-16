// Particles for the skin stage, in Minecraft's style: small square pixel sprites, crisp, no soft
// glows. Puffs when a block is placed, sparks, fire embers, autumn leaves drifting off the trees,
// and cream motes rising over the hollow. One THREE.Points each, with a tiny shader (per-particle
// size, colour, fade) and a fixed budget, so they stay cheap.

import { AdditiveBlending, BufferAttribute, BufferGeometry, CanvasTexture, NearestFilter, NormalBlending, Points, ShaderMaterial, type Object3D } from "three";

interface Particle {
  x: number;
  y: number;
  z: number;
  sx: number;
  sy: number;
  sz: number;
  vx: number;
  vy: number;
  vz: number;
  age: number;
  life: number;
  size: number;
  r: number;
  g: number;
  b: number;
  seed: number;
}

type Sprite = "pixel" | "spark" | "puff" | "leaf";

/** Where some kinds spawn: a point with a spread, or a ring's radius for the embers. */
export interface SpawnArea {
  x: number;
  y: number;
  z: number;
  spread: number;
}

interface Preset {
  sprite: Sprite;
  /** Light (added) or things (painted over). */
  additive: boolean;
  /** Particles per second while emitting. */
  rate: number;
  max: number;
  spawn: (p: Particle, random: () => number, area: SpawnArea) => void;
  step: (p: Particle, dt: number, f: number) => void;
  size: (f: number) => number;
  alpha: (f: number) => number;
}

export type ParticleKind = "puff" | "spark" | "embers" | "leaves" | "motes";

const TAU = Math.PI * 2;
type RGB = [number, number, number];
const GOLDS: RGB[] = [
  [1, 0.86, 0.45],
  [1, 0.95, 0.75],
  [1, 0.7, 0.3],
];
const EMBERS: RGB[] = [
  [1, 0.55, 0.2],
  [1, 0.78, 0.3],
  [0.95, 0.3, 0.2],
];
const LEAVES: RGB[] = [
  [0.58, 0.17, 0.15],
  [0.79, 0.33, 0.19],
  [0.89, 0.6, 0.25],
];
const pick = <T>(random: () => number, list: T[]) => list[Math.floor(random() * list.length)];

const PRESETS: Record<ParticleKind, Preset> = {
  // The little cloud a placed block kicks up.
  puff: {
    sprite: "puff",
    additive: false,
    rate: 0,
    max: 220,
    spawn: (p, random, area) => {
      p.x = area.x + (random() - 0.5) * area.spread;
      p.y = area.y + (random() - 0.5) * area.spread;
      p.z = area.z + (random() - 0.5) * area.spread;
      const angle = random() * TAU;
      p.vx = Math.cos(angle) * (4 + random() * 6);
      p.vz = Math.sin(angle) * (4 + random() * 6);
      p.vy = 4 + random() * 6;
      p.life = 0.45 + random() * 0.35;
      p.size = 3 + random() * 2.5;
      const shade = 0.82 + random() * 0.18;
      [p.r, p.g, p.b] = [shade, shade * 0.97, shade * 0.93];
    },
    step: (p, dt) => {
      p.vx *= 1 - dt * 3;
      p.vz *= 1 - dt * 3;
      p.x += p.vx * dt;
      p.y += p.vy * dt;
      p.z += p.vz * dt;
    },
    size: (f) => 1 - f * 0.4,
    alpha: (f) => (1 - f) * 0.9,
  },
  // Sparks flung out (a throne completing, an aura bursting).
  spark: {
    sprite: "spark",
    additive: true,
    rate: 0,
    max: 200,
    spawn: (p, random, area) => {
      p.x = area.x + (random() - 0.5) * area.spread;
      p.y = area.y + random() * area.spread * 0.6;
      p.z = area.z + (random() - 0.5) * area.spread;
      const angle = random() * TAU;
      const speed = 12 + random() * 22;
      p.vx = Math.cos(angle) * speed;
      p.vz = Math.sin(angle) * speed;
      p.vy = 10 + random() * 18;
      p.life = 0.6 + random() * 0.6;
      p.size = 2.4 + random() * 1.6;
      [p.r, p.g, p.b] = pick(random, GOLDS);
    },
    step: (p, dt) => {
      p.vx *= 1 - dt * 2;
      p.vz *= 1 - dt * 2;
      p.vy -= dt * 26;
      p.x += p.vx * dt;
      p.y += p.vy * dt;
      p.z += p.vz * dt;
    },
    size: (f) => 1 - f * 0.5,
    alpha: (f) => 1 - f * f,
  },
  // Fire embers rising round the player, a little wobble on the way.
  embers: {
    sprite: "pixel",
    additive: true,
    rate: 70,
    max: 220,
    spawn: (p, random, area) => {
      const angle = random() * TAU;
      const distance = area.spread * (0.6 + random() * 0.5);
      p.x = area.x + Math.cos(angle) * distance;
      p.z = area.z + Math.sin(angle) * distance;
      p.y = area.y + random() * 6;
      p.vy = 18 + random() * 22;
      p.life = 0.8 + random() * 0.7;
      p.size = 1.6 + random() * 1.4;
      [p.r, p.g, p.b] = pick(random, EMBERS);
    },
    step: (p, dt) => {
      p.x += Math.sin(p.age * 9 + p.seed * 40) * dt * 6;
      p.y += p.vy * dt;
      p.g = Math.max(0.2, p.g - dt * 0.4);
    },
    size: (f) => 1 - f * 0.6,
    alpha: (f) => Math.min(1, f * 8) * (1 - f),
  },
  // Autumn leaves letting go of the ember and gold trees.
  leaves: {
    sprite: "leaf",
    additive: false,
    rate: 2.2,
    max: 40,
    spawn: (p, random) => {
      const [tx, tz] = random() < 0.6 ? [-96, -96] : [96, -112];
      p.x = tx + (random() - 0.5) * 70;
      p.y = 40 + random() * 30;
      p.z = tz + (random() - 0.5) * 60;
      p.vx = 3 + random() * 4;
      p.vy = -(6 + random() * 4);
      p.vz = 2 + random() * 3;
      p.life = 7 + random() * 4;
      p.size = 3 + random() * 1.2;
      [p.r, p.g, p.b] = pick(random, LEAVES);
    },
    step: (p, dt) => {
      p.x += (p.vx + Math.sin(p.age * 1.8 + p.seed * 20) * 6) * dt;
      p.y += p.vy * dt;
      p.z += (p.vz + Math.cos(p.age * 1.3 + p.seed * 13) * 4) * dt;
    },
    size: () => 1,
    alpha: (f) => Math.min(1, f * 10) * Math.min(1, (1 - f) * 6),
  },
  // Cream motes rising over the hollow, like the wisps' little cousins.
  motes: {
    sprite: "pixel",
    additive: true,
    rate: 4,
    max: 40,
    spawn: (p, random) => {
      p.sx = (random() - 0.5) * 200;
      p.sy = -14 + random() * 20;
      p.sz = -40 - random() * 150;
      p.x = p.sx;
      p.y = p.sy;
      p.z = p.sz;
      p.life = 4 + random() * 4;
      p.size = 1.4 + random() * 1;
      [p.r, p.g, p.b] = random() < 0.25 ? [1, 0.45, 0.4] : [1, 0.96, 0.88];
    },
    step: (p) => {
      p.x = p.sx + Math.sin(p.age * 0.9 + p.seed * 30) * 6;
      p.y = p.sy + p.age * 7;
      p.z = p.sz + Math.cos(p.age * 0.7 + p.seed * 11) * 6;
    },
    size: () => 1,
    alpha: (f) => Math.min(1, f * 5) * (1 - f) * (0.65 + 0.35 * Math.sin(f * 30)),
  },
};

const sprites = new Map<Sprite, CanvasTexture>();

/** An 8×8 pixel sprite, drawn here. */
function sprite(kind: Sprite): CanvasTexture {
  const known = sprites.get(kind);
  if (known) return known;
  const canvas = document.createElement("canvas");
  canvas.width = 8;
  canvas.height = 8;
  const g = canvas.getContext("2d") as CanvasRenderingContext2D;
  const px = (x: number, y: number, alpha = 1) => {
    g.fillStyle = `rgba(255,255,255,${alpha})`;
    g.fillRect(x, y, 1, 1);
  };
  if (kind === "pixel") {
    for (let y = 2; y < 6; y += 1) for (let x = 2; x < 6; x += 1) px(x, y, x === 2 || y === 2 ? 1 : 0.8);
  } else if (kind === "spark") {
    for (let k = 1; k < 7; k += 1) {
      px(k, 3);
      px(k, 4);
      px(3, k);
      px(4, k);
    }
  } else if (kind === "puff") {
    for (let y = 1; y < 7; y += 1) for (let x = 1; x < 7; x += 1) if (Math.hypot(x - 3.5, y - 3.5) < 3) px(x, y, Math.hypot(x - 3.5, y - 3.5) < 2 ? 1 : 0.6);
  } else {
    // A leaf: a little diamond with a stem.
    for (const [x, y] of [[3, 1], [4, 1], [2, 2], [3, 2], [4, 2], [5, 2], [2, 3], [3, 3], [4, 3], [5, 3], [3, 4], [4, 4], [4, 5]]) px(x, y);
  }
  const texture = new CanvasTexture(canvas);
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestFilter;
  texture.generateMipmaps = false;
  sprites.set(kind, texture);
  return texture;
}

const VERTEX = /* glsl */ `
  attribute float size;
  attribute float alpha;
  attribute vec3 tint;
  uniform float scale;
  uniform float maxSize;
  varying float vAlpha;
  varying vec3 vTint;
  void main() {
    vAlpha = alpha;
    vTint = tint;
    vec4 view = modelViewMatrix * vec4(position, 1.0);
    // Capped, so one passing close to the camera doesn't fill the view.
    gl_PointSize = min(size * scale / -view.z, maxSize);
    gl_Position = projectionMatrix * view;
  }
`;

const FRAGMENT = /* glsl */ `
  uniform sampler2D map;
  uniform float brightness;
  varying float vAlpha;
  varying vec3 vTint;
  void main() {
    vec4 texel = texture2D(map, vec2(gl_PointCoord.x, 1.0 - gl_PointCoord.y));
    float a = texel.a * vAlpha;
    if (a < 0.02) discard;
    gl_FragColor = vec4(vTint * texel.rgb * brightness, a);
  }
`;

export class ParticleSystem {
  readonly object: Points;
  /** Whether new particles keep coming. */
  emitting = true;
  /** Where kinds that spawn at a place do; move it as the effect moves. */
  area: SpawnArea = { x: 0, y: 0, z: 0, spread: 8 };
  private readonly preset: Preset;
  private readonly particles: Particle[] = [];
  private readonly geometry = new BufferGeometry();
  private readonly material: ShaderMaterial;
  private readonly buffers: Record<"position" | "tint", Float32Array> & Record<"size" | "alpha", Float32Array>;
  private due = 0;
  private seed: number;

  /** `brightness` above 1 makes light particles bright enough to bloom. */
  constructor(kind: ParticleKind, brightness = 1) {
    this.preset = PRESETS[kind];
    this.seed = 1 + Math.floor(Math.random() * 100_000);
    const max = this.preset.max;
    this.buffers = { position: new Float32Array(max * 3), tint: new Float32Array(max * 3), size: new Float32Array(max), alpha: new Float32Array(max) };
    this.geometry.setAttribute("position", new BufferAttribute(this.buffers.position, 3));
    this.geometry.setAttribute("tint", new BufferAttribute(this.buffers.tint, 3));
    this.geometry.setAttribute("size", new BufferAttribute(this.buffers.size, 1));
    this.geometry.setAttribute("alpha", new BufferAttribute(this.buffers.alpha, 1));
    this.geometry.setDrawRange(0, 0);
    this.material = new ShaderMaterial({
      uniforms: { map: { value: sprite(this.preset.sprite) }, scale: { value: 400 }, maxSize: { value: 40 }, brightness: { value: brightness } },
      vertexShader: VERTEX,
      fragmentShader: FRAGMENT,
      transparent: true,
      depthWrite: false,
      blending: this.preset.additive ? AdditiveBlending : NormalBlending,
    });
    this.object = new Points(this.geometry, this.material);
    this.object.frustumCulled = false;
  }

  private random = () => {
    this.seed = (this.seed * 16807) % 2147483647;
    return (this.seed - 1) / 2147483646;
  };

  private add(area: SpawnArea) {
    const p: Particle = { x: 0, y: 0, z: 0, sx: 0, sy: 0, sz: 0, vx: 0, vy: 0, vz: 0, age: 0, life: 1, size: 1, r: 1, g: 1, b: 1, seed: this.random() };
    this.preset.spawn(p, this.random, area);
    this.particles.push(p);
  }

  /** Sends out `count` particles at once, from the current area. */
  burst(count: number) {
    for (let index = 0; index < count && this.particles.length < this.preset.max; index += 1) this.add(this.area);
  }

  /** Sends out `count` particles from one spot. */
  burstAt(x: number, y: number, z: number, count: number, spread = 8) {
    const area = { x, y, z, spread };
    for (let index = 0; index < count && this.particles.length < this.preset.max; index += 1) this.add(area);
  }

  /** How many pixels a 1-unit sprite covers at distance 1, for this canvas and lens. */
  setScale(pixelsPerUnit: number) {
    this.material.uniforms.scale.value = pixelsPerUnit;
    this.material.uniforms.maxSize.value = Math.max(12, pixelsPerUnit * 0.03);
  }

  /** Whether anything is still showing. */
  get alive(): boolean {
    return this.particles.length > 0;
  }

  update(dt: number) {
    const step = Math.min(dt, 0.1);
    const preset = this.preset;
    if (this.emitting && preset.rate > 0) {
      this.due += preset.rate * step;
      while (this.due >= 1 && this.particles.length < preset.max) {
        this.add(this.area);
        this.due -= 1;
      }
      this.due = Math.min(this.due, 1);
    }
    const { position, tint, size, alpha } = this.buffers;
    let count = 0;
    for (let index = 0; index < this.particles.length; index += 1) {
      const p = this.particles[index];
      p.age += step;
      if (p.age >= p.life) {
        this.particles.splice(index, 1);
        index -= 1;
        continue;
      }
      const f = p.age / p.life;
      preset.step(p, step, f);
      position.set([p.x, p.y, p.z], count * 3);
      tint.set([p.r, p.g, p.b], count * 3);
      size[count] = p.size * preset.size(f);
      alpha[count] = preset.alpha(f);
      count += 1;
    }
    this.geometry.setDrawRange(0, count);
    for (const name of ["position", "tint", "size", "alpha"]) (this.geometry.getAttribute(name) as BufferAttribute).needsUpdate = true;
  }

  attach(parent: Object3D) {
    parent.add(this.object);
  }

  dispose() {
    this.object.removeFromParent();
    this.geometry.dispose();
    this.material.dispose();
  }
}
