// Things that appear with an emote in the skin viewer: a throne that gathers out of sparkles,
// and an aura of light. Built from boxes and two small shaders. Units are skin pixels: the player
// stands 32 tall with the feet at -16.

import {
  AdditiveBlending,
  BoxGeometry,
  CanvasTexture,
  CylinderGeometry,
  DoubleSide,
  Group,
  Mesh,
  MeshStandardMaterial,
  NearestFilter,
  PointLight,
  RingGeometry,
  SRGBColorSpace,
  ShaderMaterial,
  type Material,
  type Object3D,
} from "three";
import { AURA_BURST_AT, clamp01, smooth } from "./emotes";
import { ParticleSystem } from "./particles";

export interface Prop {
  readonly object: Object3D;
  /** Moves on by `dt` seconds. False once it has finished leaving and can be thrown away. */
  update(dt: number): boolean;
  /** Starts leaving; `update` returns false once it's gone. */
  leave(): void;
  /** Pixels per unit at distance 1, for its sparkles. */
  setScale(pixelsPerUnit: number): void;
  dispose(): void;
}

/** A 16×16 texture of one colour with per-pixel jitter: blocky, like Minecraft. */
function voxelTexture(hex: string, spread: number, seed: number): CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 16;
  canvas.height = 16;
  const g = canvas.getContext("2d") as CanvasRenderingContext2D;
  const value = Number.parseInt(hex.slice(1), 16);
  const base = [(value >> 16) & 255, (value >> 8) & 255, value & 255];
  let state = seed;
  const random = () => {
    state = (state * 16807) % 2147483647;
    return state / 2147483647;
  };
  for (let y = 0; y < 16; y += 1) {
    for (let x = 0; x < 16; x += 1) {
      const k = 1 + (random() - 0.5) * spread;
      const [r, gr, b] = base.map((channel) => Math.max(0, Math.min(255, Math.round(channel * k))));
      g.fillStyle = `rgb(${r},${gr},${b})`;
      g.fillRect(x, y, 1, 1);
    }
  }
  const texture = new CanvasTexture(canvas);
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestFilter;
  texture.colorSpace = SRGBColorSpace;
  return texture;
}

const easeOutBack = (x: number) => 1 + 2.2 * (x - 1) ** 3 + 1.2 * (x - 1) ** 2;

/** A throne of dark stone, gold trim and red velvet, with a gem in its crest. */
export class ThroneProp implements Prop {
  readonly object = new Group();
  private readonly materials: MeshStandardMaterial[] = [];
  private readonly glowing: { material: MeshStandardMaterial; rest: number }[] = [];
  private readonly gem: MeshStandardMaterial;
  private readonly gather = new ParticleSystem("gather");
  private readonly burst = new ParticleSystem("burst");
  private readonly flash: Mesh;
  private readonly flashMaterial: ShaderMaterial;
  private time = 0;
  private leaving: number | null = null;

  constructor() {
    const material = (hex: string, spread: number, seed: number, roughness: number, metalness: number, glow = 0) => {
      const made = new MeshStandardMaterial({
        map: voxelTexture(hex, spread, seed),
        roughness,
        metalness,
        transparent: true,
        opacity: 0,
        emissive: hex,
        emissiveIntensity: 1,
      });
      this.materials.push(made);
      this.glowing.push({ material: made, rest: glow });
      return made;
    };
    const stone = material("#2c2523", 0.22, 7, 0.9, 0);
    const dark = material("#1f1a19", 0.2, 11, 0.9, 0);
    const gold = material("#d9a548", 0.18, 13, 0.4, 0.55, 0.06);
    const velvet = material("#a3242c", 0.16, 17, 0.95, 0, 0.02);
    this.gem = new MeshStandardMaterial({ color: "#e5484d", emissive: "#e5484d", emissiveIntensity: 1.6, transparent: true, opacity: 0, roughness: 0.3 });
    this.materials.push(this.gem);

    // [width, height, depth, x, y, z, material]
    const boxes: [number, number, number, number, number, number, Material][] = [
      [24, 2, 20, 0, -15, 2, stone], // dais
      [24.4, 0.4, 20.4, 0, -14.2, 2, gold], // its gold rim
      [16, 3, 12, 0, -12.5, -0.5, dark], // seat
      [14.5, 1, 10.5, 0, -10.5, -0.75, velvet], // cushion
      [16, 23, 2.5, 0, 0.5, -5.25, stone], // back
      [11, 17, 0.6, 0, -0.5, -3.7, velvet], // back cushion
      [1.6, 24, 3, -8.3, 0.5, -5.25, gold], // pillars
      [1.6, 24, 3, 8.3, 0.5, -5.25, gold],
      [17.6, 1.6, 3, 0, 12.4, -5.25, gold], // top beam
      [6, 4, 2.6, 0, 15.2, -5.25, stone], // crest
      [6.6, 0.8, 3, 0, 17.6, -5.25, gold],
      [2.4, 2.4, 2.4, -8.3, 14.4, -5.25, gold], // finials
      [2.4, 2.4, 2.4, 8.3, 14.4, -5.25, gold],
      [2.6, 5, 12, -9.3, -8.5, -0.5, stone], // arms
      [2.6, 5, 12, 9.3, -8.5, -0.5, stone],
      [3, 0.7, 12.4, -9.3, -5.65, -0.5, gold], // arm caps
      [3, 0.7, 12.4, 9.3, -5.65, -0.5, gold],
      [3, 3, 1, -9.3, -7.6, 5.7, gold], // arm fronts
      [3, 3, 1, 9.3, -7.6, 5.7, gold],
      [1.8, 1.8, 0.8, 0, 15.2, -3.8, this.gem], // gem
    ];
    for (const [w, h, d, x, y, z, made] of boxes) {
      const mesh = new Mesh(new BoxGeometry(w, h, d), made);
      mesh.position.set(x, y, z);
      this.object.add(mesh);
    }

    // A ring of light on the dais as it forms.
    this.flashMaterial = new ShaderMaterial({
      uniforms: { strength: { value: 0 } },
      vertexShader: /* glsl */ `varying vec2 vUv; void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
      fragmentShader: /* glsl */ `
        uniform float strength;
        varying vec2 vUv;
        void main() {
          float d = length(vUv - 0.5) * 2.0;
          float ring = smoothstep(0.55, 0.85, d) * (1.0 - smoothstep(0.85, 1.0, d));
          float a = ring * strength;
          gl_FragColor = vec4(vec3(1.0, 0.86, 0.55) * a, a);
        }`,
      transparent: true,
      depthWrite: false,
      blending: AdditiveBlending,
    });
    this.flash = new Mesh(new RingGeometry(0.01, 15, 64, 1), this.flashMaterial);
    this.flash.rotation.x = -Math.PI / 2;
    this.flash.position.set(0, -13.9, 2);
    this.object.add(this.flash);

    this.burst.emitting = false;
    this.gather.attach(this.object);
    this.burst.attach(this.object);
    this.object.scale.setScalar(0.82);
  }

  private set appearance(amount: number) {
    for (const made of this.materials) made.opacity = amount;
    // Glowing while it forms, settling to its own colours.
    for (const { material, rest } of this.glowing) material.emissiveIntensity = rest + (1 - amount) * 1.1;
    for (const made of this.materials) made.transparent = amount < 1;
  }

  update(dt: number): boolean {
    this.time += dt;
    const t = this.time;
    if (this.leaving === null) {
      this.gather.emitting = t < 0.75;
      const formed = smooth((t - 0.15) / 0.7);
      this.appearance = formed;
      this.object.scale.setScalar(0.82 + 0.18 * easeOutBack(formed));
      this.flashMaterial.uniforms.strength.value = Math.sin(clamp01((t - 0.55) / 0.8) * Math.PI) * 0.9;
      this.gem.emissiveIntensity = 1.3 + Math.sin(t * 2.4) * 0.5;
    } else {
      const gone = smooth((t - this.leaving) / 0.45);
      this.appearance = 1 - gone;
      this.object.scale.setScalar(1 - 0.12 * gone);
      this.flashMaterial.uniforms.strength.value = 0;
    }
    this.gather.update(dt);
    this.burst.update(dt);
    return this.leaving === null || t - this.leaving < 0.5 || this.burst.alive;
  }

  leave() {
    if (this.leaving !== null) return;
    this.leaving = this.time;
    this.gather.emitting = false;
    this.burst.burst(70);
  }

  setScale(pixelsPerUnit: number) {
    this.gather.setScale(pixelsPerUnit);
    this.burst.setScale(pixelsPerUnit);
  }

  dispose() {
    this.object.removeFromParent();
    this.gather.dispose();
    this.burst.dispose();
    this.object.traverse((child) => {
      if (child instanceof Mesh) child.geometry.dispose();
    });
    for (const made of this.materials) {
      made.map?.dispose();
      made.dispose();
    }
    this.flashMaterial.dispose();
  }
}

const NOISE = /* glsl */ `
  float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
  float noise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x), u.y);
  }
  float fbm(vec2 p) {
    float v = 0.0;
    float a = 0.5;
    for (int i = 0; i < 4; i++) { v += a * noise(p); p *= 2.03; a *= 0.5; }
    return v;
  }
`;

/** Tongues of light rising around the player, a glowing ring underfoot, sparks, and a warm light from below. */
export class AuraProp implements Prop {
  readonly object = new Group();
  private readonly shell: ShaderMaterial;
  private readonly ring: ShaderMaterial;
  private readonly sparks = new ParticleSystem("sparks");
  private readonly flare = new ParticleSystem("burst");
  private readonly light = new PointLight("#ff6a3d", 0, 90, 2);
  private time = 0;
  private leaving: number | null = null;
  private burst = false;

  constructor() {
    const uniforms = () => ({ time: { value: 0 }, strength: { value: 0 } });
    this.shell = new ShaderMaterial({
      uniforms: uniforms(),
      vertexShader: /* glsl */ `
        varying vec2 vUv;
        varying float vFacing;
        void main() {
          vUv = uv;
          vec4 view = modelViewMatrix * vec4(position, 1.0);
          vec3 normalView = normalize(normalMatrix * normal);
          vFacing = abs(dot(normalView, normalize(-view.xyz)));
          gl_Position = projectionMatrix * view;
        }`,
      fragmentShader: /* glsl */ `
        uniform float time;
        uniform float strength;
        varying vec2 vUv;
        varying float vFacing;
        ${NOISE}
        void main() {
          // Flames: noise stretched tall and scrolling up, thinning towards the top.
          vec2 p = vec2(vUv.x * 14.0, vUv.y * 2.6 - time * 1.7);
          float n = fbm(p) + fbm(p * 1.9 + vec2(time * 0.3, 0.0)) * 0.35;
          float flames = smoothstep(0.42, 0.95, n + (0.45 - vUv.y) * 0.85);
          float base = smoothstep(0.0, 0.06, vUv.y);
          // Brighter towards the silhouette's edges, like light seen through more of it.
          float rim = 0.3 + 0.7 * pow(1.0 - vFacing, 1.4);
          float a = flames * base * rim * strength;
          vec3 hot = vec3(1.0, 0.95, 0.85);
          vec3 ember = vec3(0.92, 0.26, 0.24);
          vec3 color = mix(ember, hot, smoothstep(0.55, 1.0, flames) * (1.0 - vUv.y * 0.6));
          gl_FragColor = vec4(color, a);
        }`,
      transparent: true,
      depthWrite: false,
      side: DoubleSide,
      blending: AdditiveBlending,
    });
    const shell = new Mesh(new CylinderGeometry(8.5, 10.5, 40, 56, 1, true), this.shell);
    shell.position.y = 3;
    this.object.add(shell);

    this.ring = new ShaderMaterial({
      uniforms: uniforms(),
      vertexShader: /* glsl */ `varying vec2 vUv; void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
      fragmentShader: /* glsl */ `
        uniform float time;
        uniform float strength;
        varying vec2 vUv;
        void main() {
          vec2 q = vUv - 0.5;
          float d = length(q) * 2.0;
          float angle = atan(q.y, q.x);
          // A soft disc with a bright rim and slow turning dashes.
          float glow = (1.0 - smoothstep(0.0, 1.0, d)) * 0.35;
          float rim = smoothstep(0.7, 0.86, d) * (1.0 - smoothstep(0.86, 0.98, d));
          float dashes = 0.55 + 0.45 * step(0.5, fract(angle * 3.8197 + time * 0.25));
          float a = (glow + rim * dashes) * strength;
          gl_FragColor = vec4(mix(vec3(0.95, 0.3, 0.26), vec3(1.0, 0.9, 0.75), rim) * a, a);
        }`,
      transparent: true,
      depthWrite: false,
      blending: AdditiveBlending,
    });
    const ring = new Mesh(new RingGeometry(0.01, 15, 72, 1), this.ring);
    ring.rotation.x = -Math.PI / 2;
    ring.position.y = -15.9;
    this.object.add(ring);

    this.light.position.set(0, -10, 6);
    this.object.add(this.light);
    this.flare.emitting = false;
    this.sparks.attach(this.object);
    this.flare.attach(this.object);
  }

  /** How strong the aura is at time t: flickering while it gathers, a flash at the burst, then a steady blaze. */
  private strength(t: number): number {
    if (t < AURA_BURST_AT) return (0.15 + 0.25 * smooth(t / AURA_BURST_AT)) * (0.85 + 0.15 * Math.sin(t * 31));
    const since = t - AURA_BURST_AT;
    const flash = since < 0.35 ? Math.sin((since / 0.35) * Math.PI) * 0.6 : 0;
    return 0.9 + flash + Math.sin(t * 7) * 0.05 + Math.sin(t * 13.3) * 0.04;
  }

  update(dt: number): boolean {
    this.time += dt;
    const t = this.time;
    let strength = this.strength(t);
    if (this.leaving !== null) strength *= 1 - smooth((t - this.leaving) / 0.5);
    if (!this.burst && t >= AURA_BURST_AT) {
      this.burst = true;
      this.flare.burst(60);
    }
    for (const material of [this.shell, this.ring]) {
      material.uniforms.time.value = t;
      material.uniforms.strength.value = strength;
    }
    this.light.intensity = strength * 650;
    this.sparks.emitting = this.leaving === null && t > AURA_BURST_AT * 0.6;
    this.sparks.update(dt);
    this.flare.update(dt);
    return this.leaving === null || t - this.leaving < 0.55 || this.sparks.alive;
  }

  leave() {
    if (this.leaving === null) this.leaving = this.time;
  }

  setScale(pixelsPerUnit: number) {
    this.sparks.setScale(pixelsPerUnit);
    this.flare.setScale(pixelsPerUnit);
  }

  dispose() {
    this.object.removeFromParent();
    this.sparks.dispose();
    this.flare.dispose();
    this.object.traverse((child) => {
      if (child instanceof Mesh) child.geometry.dispose();
    });
    this.shell.dispose();
    this.ring.dispose();
    this.light.dispose();
  }
}

export function makeProp(kind: "throne" | "aura"): Prop {
  return kind === "throne" ? new ThroneProp() : new AuraProp();
}
