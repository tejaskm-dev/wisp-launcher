// Small particle systems for the skin viewer's effects: sparkles that gather into the throne,
// a burst when it leaves, and sparks rising through the aura. One THREE.Points each, with a
// tiny shader (per-particle size, colour, fade and spin) and a fixed budget, so they stay cheap.

import { AdditiveBlending, BufferAttribute, BufferGeometry, CanvasTexture, Points, ShaderMaterial, type Object3D } from "three";

interface Particle {
  x: number;
  y: number;
  z: number;
  /** Where it started and where it's going, for particles that travel to a point. */
  sx: number;
  sy: number;
  sz: number;
  tx: number;
  ty: number;
  tz: number;
  vx: number;
  vy: number;
  vz: number;
  age: number;
  life: number;
  size: number;
  angle: number;
  spin: number;
  r: number;
  g: number;
  b: number;
  seed: number;
}

type Sprite = "glow" | "sparkle";

interface Preset {
  sprite: Sprite;
  /** Particles per second while emitting. */
  rate: number;
  max: number;
  spawn: (p: Particle, random: () => number) => void;
  step: (p: Particle, dt: number, f: number) => void;
  size: (f: number) => number;
  alpha: (f: number) => number;
}

export type ParticleKind = "gather" | "burst" | "sparks";

const TAU = Math.PI * 2;
const GOLDS: [number, number, number][] = [
  [1, 0.86, 0.5],
  [1, 0.95, 0.82],
  [1, 0.42, 0.38],
];
const pick = <T>(random: () => number, list: T[]) => list[Math.floor(random() * list.length)];
const easeInOut = (x: number) => (x < 0.5 ? 2 * x * x : 1 - (-2 * x + 2) ** 2 / 2);

const PRESETS: Record<ParticleKind, Preset> = {
  // Sparkles that spiral in from all around and settle into the throne's shape.
  gather: {
    sprite: "sparkle",
    rate: 140,
    max: 150,
    spawn: (p, random) => {
      const angle = random() * TAU;
      const lift = (random() - 0.5) * 1.6;
      const distance = 17 + random() * 8;
      p.sx = Math.cos(angle) * distance;
      p.sy = 4 + lift * 12;
      p.sz = Math.sin(angle) * distance;
      // Somewhere on the throne: the back, the seat, the arms.
      p.tx = (random() - 0.5) * 18;
      p.ty = -15 + random() * 30;
      p.tz = -5 + random() * 9;
      p.life = 0.55 + random() * 0.35;
      p.size = 1.6 + random() * 1.6;
      p.spin = (random() - 0.5) * 6;
      [p.r, p.g, p.b] = pick(random, GOLDS);
    },
    step: (p, _dt, f) => {
      const k = easeInOut(f);
      // A quarter turn of swirl on the way in.
      const swirl = (1 - k) * 1.4;
      const x = p.sx + (p.tx - p.sx) * k;
      const z = p.sz + (p.tz - p.sz) * k;
      p.x = x * Math.cos(swirl) - z * Math.sin(swirl);
      p.z = x * Math.sin(swirl) + z * Math.cos(swirl);
      p.y = p.sy + (p.ty - p.sy) * k;
    },
    size: (f) => 0.5 + (1 - f) * 0.7,
    alpha: (f) => Math.min(1, f * 5) * (f > 0.85 ? (1 - f) / 0.15 : 1),
  },
  // Sparkles flung outward and fading (a throne leaving, an aura bursting).
  burst: {
    sprite: "sparkle",
    rate: 0,
    max: 120,
    spawn: (p, random) => {
      p.x = (random() - 0.5) * 16;
      p.y = -14 + random() * 28;
      p.z = (random() - 0.5) * 10;
      const speed = 8 + random() * 14;
      const angle = random() * TAU;
      p.vx = Math.cos(angle) * speed;
      p.vz = Math.sin(angle) * speed;
      p.vy = 4 + random() * 10;
      p.life = 0.6 + random() * 0.5;
      p.size = 1.4 + random() * 1.8;
      p.spin = (random() - 0.5) * 8;
      [p.r, p.g, p.b] = pick(random, GOLDS);
    },
    step: (p, dt) => {
      p.vx *= 1 - dt * 2.5;
      p.vz *= 1 - dt * 2.5;
      p.vy -= dt * 12;
      p.x += p.vx * dt;
      p.y += p.vy * dt;
      p.z += p.vz * dt;
    },
    size: (f) => 1 - f * 0.6,
    alpha: (f) => 1 - f,
  },
  // Embers streaming up through the aura, pulled in a little as they rise.
  sparks: {
    sprite: "glow",
    rate: 60,
    max: 160,
    spawn: (p, random) => {
      const angle = random() * TAU;
      const distance = 4 + random() * 7;
      p.x = Math.cos(angle) * distance;
      p.z = Math.sin(angle) * distance;
      p.y = -16 + random() * 5;
      p.vy = 12 + random() * 14;
      p.life = 0.9 + random() * 0.8;
      p.size = 0.9 + random() * 1.2;
      p.r = 1;
      p.g = 0.93;
      p.b = 0.78;
    },
    step: (p, dt, f) => {
      p.x *= 1 - dt * 0.6;
      p.z *= 1 - dt * 0.6;
      p.x += Math.sin(p.seed * 40 + p.y * 0.3) * dt * 1.5;
      p.y += p.vy * dt;
      // Cream at the base, ember red as they rise.
      p.g = 0.93 - f * 0.55;
      p.b = 0.78 - f * 0.55;
    },
    size: (f) => 1 - f * 0.5,
    alpha: (f) => Math.min(1, f * 6) * (1 - f) * 1.2,
  },
};

const sprites = new Map<Sprite, CanvasTexture>();

function sprite(kind: Sprite): CanvasTexture {
  const known = sprites.get(kind);
  if (known) return known;
  const size = 64;
  const c = size / 2;
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const g = canvas.getContext("2d") as CanvasRenderingContext2D;
  if (kind === "glow") {
    // A bright core in a soft halo.
    const halo = g.createRadialGradient(c, c, 0, c, c, c);
    halo.addColorStop(0, "rgba(255,255,255,1)");
    halo.addColorStop(0.18, "rgba(255,255,255,0.9)");
    halo.addColorStop(0.45, "rgba(255,255,255,0.25)");
    halo.addColorStop(1, "rgba(255,255,255,0)");
    g.fillStyle = halo;
    g.fillRect(0, 0, size, size);
  } else {
    // Wisp's four-point sparkle, slim, with a small glow at its heart.
    g.fillStyle = "#fff";
    g.beginPath();
    g.moveTo(c, 1);
    g.bezierCurveTo(c + 2, c - 6, c + 6, c - 2, size - 1, c);
    g.bezierCurveTo(c + 6, c + 2, c + 2, c + 6, c, size - 1);
    g.bezierCurveTo(c - 2, c + 6, c - 6, c + 2, 1, c);
    g.bezierCurveTo(c - 6, c - 2, c - 2, c - 6, c, 1);
    g.fill();
    const heart = g.createRadialGradient(c, c, 0, c, c, c * 0.45);
    heart.addColorStop(0, "rgba(255,255,255,0.8)");
    heart.addColorStop(1, "rgba(255,255,255,0)");
    g.fillStyle = heart;
    g.fillRect(0, 0, size, size);
  }
  const texture = new CanvasTexture(canvas);
  sprites.set(kind, texture);
  return texture;
}

const VERTEX = /* glsl */ `
  attribute float size;
  attribute float alpha;
  attribute float angle;
  attribute vec3 tint;
  uniform float scale;
  varying float vAlpha;
  varying float vAngle;
  varying vec3 vTint;
  void main() {
    vAlpha = alpha;
    vAngle = angle;
    vTint = tint;
    vec4 view = modelViewMatrix * vec4(position, 1.0);
    gl_PointSize = size * scale / -view.z;
    gl_Position = projectionMatrix * view;
  }
`;

const FRAGMENT = /* glsl */ `
  uniform sampler2D map;
  varying float vAlpha;
  varying float vAngle;
  varying vec3 vTint;
  void main() {
    vec2 uv = gl_PointCoord - 0.5;
    float c = cos(vAngle);
    float s = sin(vAngle);
    uv = vec2(c * uv.x - s * uv.y, s * uv.x + c * uv.y) + 0.5;
    if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0) discard;
    vec4 texel = texture2D(map, vec2(uv.x, 1.0 - uv.y));
    float a = texel.a * vAlpha;
    if (a < 0.01) discard;
    gl_FragColor = vec4(vTint * texel.rgb, a);
  }
`;

export class ParticleSystem {
  readonly object: Points;
  /** Whether new particles keep coming. */
  emitting = true;
  private readonly preset: Preset;
  private readonly particles: Particle[] = [];
  private readonly geometry = new BufferGeometry();
  private readonly material: ShaderMaterial;
  private readonly buffers: Record<"position" | "tint", Float32Array> & Record<"size" | "alpha" | "angle", Float32Array>;
  private due = 0;
  private seed = 1;

  constructor(kind: ParticleKind) {
    this.preset = PRESETS[kind];
    const max = this.preset.max;
    this.buffers = {
      position: new Float32Array(max * 3),
      tint: new Float32Array(max * 3),
      size: new Float32Array(max),
      alpha: new Float32Array(max),
      angle: new Float32Array(max),
    };
    this.geometry.setAttribute("position", new BufferAttribute(this.buffers.position, 3));
    this.geometry.setAttribute("tint", new BufferAttribute(this.buffers.tint, 3));
    this.geometry.setAttribute("size", new BufferAttribute(this.buffers.size, 1));
    this.geometry.setAttribute("alpha", new BufferAttribute(this.buffers.alpha, 1));
    this.geometry.setAttribute("angle", new BufferAttribute(this.buffers.angle, 1));
    this.geometry.setDrawRange(0, 0);
    this.material = new ShaderMaterial({
      uniforms: { map: { value: sprite(this.preset.sprite) }, scale: { value: 400 } },
      vertexShader: VERTEX,
      fragmentShader: FRAGMENT,
      transparent: true,
      depthWrite: false,
      blending: AdditiveBlending,
    });
    this.object = new Points(this.geometry, this.material);
    this.object.frustumCulled = false;
  }

  private random = () => {
    this.seed = (this.seed * 16807) % 2147483647;
    return (this.seed - 1) / 2147483646;
  };

  private add() {
    const p: Particle = { x: 0, y: 0, z: 0, sx: 0, sy: 0, sz: 0, tx: 0, ty: 0, tz: 0, vx: 0, vy: 0, vz: 0, age: 0, life: 1, size: 1, angle: 0, spin: 0, r: 1, g: 1, b: 1, seed: this.random() };
    this.preset.spawn(p, this.random);
    this.particles.push(p);
  }

  /** Sends out `count` particles at once. */
  burst(count: number) {
    for (let index = 0; index < count && this.particles.length < this.preset.max; index += 1) this.add();
  }

  /** How many pixels a 1-unit sprite covers at distance 1, for this canvas and lens. */
  setScale(pixelsPerUnit: number) {
    this.material.uniforms.scale.value = pixelsPerUnit;
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
        this.add();
        this.due -= 1;
      }
      this.due = Math.min(this.due, 1);
    }
    const { position, tint, size, alpha, angle } = this.buffers;
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
      p.angle += p.spin * step;
      position.set([p.x, p.y, p.z], count * 3);
      tint.set([p.r, p.g, p.b], count * 3);
      size[count] = p.size * preset.size(f);
      alpha[count] = preset.alpha(f);
      angle[count] = p.angle;
      count += 1;
    }
    this.geometry.setDrawRange(0, count);
    for (const name of ["position", "tint", "size", "alpha", "angle"]) (this.geometry.getAttribute(name) as BufferAttribute).needsUpdate = true;
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
