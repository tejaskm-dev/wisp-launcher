// The Skins page's stage: "the Wisp Hollow", a floating island at night built block by block in
// Minecraft's style. A giant Wisp statue carved from calcite stands in a horseshoe of stratified
// stone, the ember sparkle from the logo turning beside it; ember and gold trees hang wisp
// lanterns; will-o'-wisps drift over a sea of clouds under a pixel moon; and the little Wisp lives
// here, playing with the player. Everything is made here from code: no models, no borrowed art.
// Free roam swaps it all for a lit void, so every side of a skin can be seen up close.

import {
  AdditiveBlending,
  BackSide,
  BoxGeometry,
  BufferAttribute,
  BufferGeometry,
  Color,
  DirectionalLight,
  DoubleSide,
  Fog,
  Group,
  HemisphereLight,
  InstancedMesh,
  Matrix4,
  Mesh,
  MeshBasicMaterial,
  MeshLambertMaterial,
  PlaneGeometry,
  PointLight,
  Points,
  RingGeometry,
  ShaderMaterial,
  SphereGeometry,
  type Material,
  type Texture,
} from "three";
import type { SkinViewer } from "skinview3d";
import { WispGhost, type Playmate } from "./ghost";
import { ParticleSystem } from "./particles";
import { BLOCK, GROUND, World, at, blockMaterial, box, cross, cutoutMaterial, glowMaterial, makeTextures, paint, seeded } from "./voxel";

export { BLOCK, GROUND };
export type StageMode = "scene" | "void";

// ── Noise, for shapes that look grown rather than drawn ──

function hash(x: number, y: number, z: number, seed: number) {
  const s = Math.sin(x * 127.1 + y * 311.7 + z * 74.7 + seed * 19.19) * 43758.5453;
  return s - Math.floor(s);
}

/** Smooth value noise in 3D, about 0 to 1. */
function noise(x: number, y: number, z: number, seed = 1): number {
  const xi = Math.floor(x);
  const yi = Math.floor(y);
  const zi = Math.floor(z);
  const fx = x - xi;
  const fy = y - yi;
  const fz = z - zi;
  const u = fx * fx * (3 - 2 * fx);
  const v = fy * fy * (3 - 2 * fy);
  const w = fz * fz * (3 - 2 * fz);
  const corner = (dx: number, dy: number, dz: number) => hash(xi + dx, yi + dy, zi + dz, seed);
  const lerp = (a: number, b: number, k: number) => a + (b - a) * k;
  return lerp(
    lerp(lerp(corner(0, 0, 0), corner(1, 0, 0), u), lerp(corner(0, 1, 0), corner(1, 1, 0), u), v),
    lerp(lerp(corner(0, 0, 1), corner(1, 0, 1), u), lerp(corner(0, 1, 1), corner(1, 1, 1), u), v),
    w,
  );
}

// ── The build ──

/** The statue: where it stands and how big (in blocks). */
const STATUE = { x: 0, z: -11, rx: 3.1, rz: 2.6, height: 7 };
/** The ember sparkle at the statue's upper left, where it sits on the logo. */
const SPARKLE = { x: -4.6, y: 6.6, z: -11 };

/** Everything that isn't a full block: plants, chains, slabs, hanging lights. */
interface Decor {
  plants: { kind: "tallGrass" | "fern" | "moonBlossom" | "emberFlower"; x: number; y: number; z: number; turn: number }[];
  vines: { x: number; y: number; z: number; length: number; turn: number }[];
  chains: { x: number; y: number; z: number }[];
  hanging: { x: number; y: number; z: number }[];
  slabs: { x: number; y: number; z: number }[];
}

function buildHollow(world: World, decor: Decor) {
  const random = seeded(2208);
  const top = new Map<string, number>();
  const surface = (x: number, z: number) => top.get(`${x},${z}`);

  // The island: an organic oval, deepest under the middle, layered stone below.
  for (let x = -10; x <= 10; x += 1) {
    for (let z = -19; z <= 5; z += 1) {
      const dx = x / 7.6;
      const dz = (z + 7.5) / 10.2;
      const edge = Math.hypot(dx, dz) + (noise(x * 0.3, 0, z * 0.3, 3) - 0.5) * 0.24;
      if (edge > 1) continue;
      const depth = Math.round((1 - edge) * 10 + noise(x * 0.4, 1, z * 0.4, 5) * 3) + 1;
      // A gentle rise towards the back.
      const height = z < -4 && edge > 0.5 ? Math.round(noise(x * 0.25, 2, z * 0.25, 7) * 1.1) : 0;
      top.set(`${x},${z}`, height);
      for (let y = -depth; y <= height; y += 1) {
        const below = height - y;
        let kind = "stone";
        if (below === 0) kind = "grass";
        else if (below <= 2) kind = "dirt";
        else if (y < -6) kind = "deepslate";
        else if (noise(x * 0.5, y * 0.5, z * 0.5, 11) > 0.62) kind = "andesite";
        else if (y < -3 && noise(x * 0.6, y * 0.6, z * 0.6, 13) > 0.7) kind = "mossyCobble";
        world.set(x, y, z, kind);
      }
      // Vines hanging from the underside's rim.
      if (edge > 0.82 && random() < 0.35) decor.vines.push({ x, y: -depth, z, length: 1 + Math.floor(random() * 3), turn: Math.atan2(x, z + 7.5) });
    }
  }

  // The horseshoe of stone round the statue: stratified, mossy on top, open towards the plaza.
  for (let x = -10; x <= 10; x += 1) {
    for (let z = -19; z <= 2; z += 1) {
      const base = surface(x, z);
      if (base === undefined) continue;
      const rx = x - STATUE.x;
      const rz = z - STATUE.z;
      const r = Math.hypot(rx, rz);
      const angle = Math.abs(Math.atan2(rx, rz));
      if (angle < 0.95) continue;
      const along = Math.min(1, (angle - 0.95) / 0.7);
      const peak = (2.6 + noise(x * 0.3, 3, z * 0.3, 17) * 2.2) * (0.35 + 0.65 * along);
      const thickness = 1.7 + noise(x * 0.4, 4, z * 0.4, 19) * 0.6;
      const across = 1 - Math.abs(r - 7) / thickness;
      if (across <= 0) continue;
      const rise = Math.round(peak * Math.sqrt(across));
      for (let y = base + 1; y <= base + rise; y += 1) {
        const band = y + Math.round(noise(x * 0.3, y * 0.2, z * 0.3, 23) * 1.4);
        const kind = y === base + rise ? (noise(x, y, z, 29) > 0.35 ? "moss" : "mossyCobble") : band <= 1 ? "deepslate" : band === 2 ? "stone" : band === 3 ? "andesite" : "stone";
        world.set(x, y, z, kind);
      }
      if (rise > 0) top.set(`${x},${z}`, base + rise);
    }
  }

  // The plaza the player stands on: moonstone in a ring of deepslate tiles.
  for (let x = -3; x <= 3; x += 1) {
    for (let z = -3; z <= 3; z += 1) {
      const r = Math.hypot(x, z);
      if (r <= 2.45) world.set(x, 0, z, "moonstone");
      else if (r <= 3.2) world.set(x, 0, z, "deepslateTiles");
    }
  }
  // A path of moonstone back to the statue, its edges going to moss.
  for (let z = -4; z >= -7; z -= 1) {
    for (const x of [-1, 0, 1]) {
      if (surface(x, z) !== 0) continue;
      world.set(x, 0, z, x === 0 || random() < 0.6 ? "moonstone" : "moss");
    }
  }

  // A moon pool beside the path.
  for (const [x, z] of [
    [3, -5],
    [4, -5],
    [3, -6],
    [4, -6],
    [5, -6],
    [4, -7],
  ]) {
    if (surface(x, z) !== 0) continue;
    world.set(x, 0, z, "water");
    world.set(x, -1, z, "mossyCobble");
  }

  // The statue: Wisp, carved from calcite. A dome, a body, a wavy hem of three tails.
  const S = STATUE;
  for (let y = 1; y <= S.height; y += 1) {
    for (let x = Math.floor(S.x - S.rx - 1); x <= Math.ceil(S.x + S.rx + 1); x += 1) {
      for (let z = Math.floor(S.z - S.rz - 1); z <= Math.ceil(S.z + S.rz + 1); z += 1) {
        const lx = (x - S.x) / S.rx;
        const lz = (z - S.z) / S.rz;
        const flat = Math.hypot(lx, lz);
        const domeFrom = S.height - S.rx * 0.95;
        let inside: boolean;
        if (y > domeFrom) {
          const k = (y - domeFrom) / (S.height + 0.5 - domeFrom);
          inside = flat <= Math.sqrt(Math.max(0, 1 - k * k)) * 1.02;
        } else if (y <= 2) {
          // The hem: three tails reaching the ground, gaps between them.
          const around = Math.atan2(lx, lz);
          const tail = 0.5 + 0.5 * Math.cos(around * 3);
          inside = flat <= 1 && (y === 2 || tail > 0.45 || flat < 0.55);
        } else inside = flat <= 1;
        if (!inside) continue;
        const front = z >= S.z + S.rz - 1.2;
        // Eyes: two dark ovals on the face.
        const eyeRow = y === S.height - 3 || y === S.height - 2;
        const eye = front && eyeRow && (Math.abs(x - S.x - 1.3) < 0.8 || Math.abs(x - S.x + 1.3) < 0.8);
        const mossy = y <= 2 && noise(x * 0.8, y, z * 0.8, 31) > 0.4;
        const rough = noise(x * 0.7, y * 0.7, z * 0.7, 43) > 0.62;
        world.set(x, y, z, eye ? "deepslate" : mossy ? "calciteMossy" : rough ? "calcite" : "calcitePolished");
      }
    }
  }

  // Trees: ember on the left, gold on the right, each hung with wisp lanterns.
  const tree = (bx: number, bz: number, lean: number, palette: string[], height: number) => {
    let x = bx;
    const z = bz;
    for (let y = 1; y <= height; y += 1) {
      world.set(x, y, z, "log");
      if (y === 1) world.set(x + Math.sign(lean), y, z, "log");
      if (y === Math.floor(height / 2) + 1) x += Math.sign(lean);
    }
    const cx = x;
    const cy = height + 1;
    const cz = z;
    // Two branches out to the sides.
    for (const side of [-1, 1]) {
      world.set(cx + side, cy - 1, cz, "log");
      world.set(cx + side * 2, cy, cz + side, "log");
    }
    // The canopy: overlapping blobs, darker inside and below, brighter on top.
    const blobs: [number, number, number, number][] = [
      [cx, cy + 0.6, cz, 2.6],
      [cx - 1.6, cy, cz + 0.6, 1.9],
      [cx + 1.8, cy + 0.2, cz - 0.5, 2],
      [cx + 0.4, cy + 1.8, cz - 0.6, 1.7],
    ];
    for (const [ox, oy, oz, radius] of blobs) {
      for (let y = Math.floor(oy - radius); y <= Math.ceil(oy + radius); y += 1) {
        for (let lx = Math.floor(ox - radius); lx <= Math.ceil(ox + radius); lx += 1) {
          for (let lz = Math.floor(oz - radius); lz <= Math.ceil(oz + radius); lz += 1) {
            const d = Math.hypot(lx - ox, (y - oy) * 1.25, lz - oz) + (noise(lx * 0.7, y * 0.7, lz * 0.7, 37) - 0.5) * 0.9;
            if (d > radius) continue;
            const shade = y - (oy - radius) + noise(lx, y, lz, 41) * 1.5;
            world.fill(lx, y, lz, palette[Math.max(0, Math.min(palette.length - 1, Math.floor((shade / (radius * 2)) * palette.length)))]);
          }
        }
      }
    }
    // Lanterns on chains under the canopy.
    for (const [ox, oz] of [
      [cx - 2, cz + 1],
      [cx + 2, cz - 1],
    ]) {
      let y = cy + 3;
      while (y > cy - 3 && !world.get(ox, y, oz)) y -= 1;
      while (y > 2 && world.get(ox, y, oz)) y -= 1;
      if (y <= 2) continue;
      decor.chains.push({ x: ox, y, z: oz });
      decor.hanging.push({ x: ox, y: y - 1, z: oz });
    }
  };
  tree(-6, -6, -1, ["leavesCrimson", "leavesCrimson", "leavesEmber", "leavesEmber", "leavesGold"], 4);
  tree(6, -7, 1, ["leavesEmber", "leavesGold", "leavesGold"], 3);

  // Stone lanterns at the plaza's back corners: tiles, an ember light, a dark roof.
  for (const side of [-1, 1]) {
    const x = side * 3;
    const z = -2;
    world.set(x, 1, z, "deepslateTiles");
    world.set(x, 2, z, "lantern");
    decor.slabs.push({ x, y: 3, z });
  }

  // Plants on open grass.
  for (const [key, height] of top) {
    const [x, z] = key.split(",").map(Number);
    if (world.get(x, height, z) !== "grass" || world.get(x, height + 1, z)) continue;
    const roll = random();
    const kind = roll < 0.3 ? "tallGrass" : roll < 0.4 ? "fern" : roll < 0.47 ? "moonBlossom" : roll < 0.51 ? "emberFlower" : null;
    if (kind) decor.plants.push({ kind, x, y: height + 1, z, turn: random() * Math.PI });
  }
}

/** A small floating islet far off: a tapered rock, grass on top, a tree or a tiny Wisp. */
function buildIslet(world: World, ox: number, oy: number, oz: number, size: number, seed: number, statue: boolean) {
  for (let x = -size; x <= size; x += 1) {
    for (let z = -size; z <= size; z += 1) {
      const edge = Math.hypot(x, z) / size + (noise(x * 0.5 + seed, 0, z * 0.5, seed) - 0.5) * 0.3;
      if (edge > 1) continue;
      const depth = Math.round((1 - edge) * size * 1.4) + 1;
      for (let y = -depth; y <= 0; y += 1) world.set(ox + x, oy + y, oz + z, y === 0 ? "grass" : y > -2 ? "dirt" : y < -3 ? "deepslate" : "stone");
    }
  }
  if (statue) {
    for (let x = 0; x <= 1; x += 1) for (let z = 0; z <= 1; z += 1) for (let y = 1; y <= 2; y += 1) world.set(ox + x, oy + y, oz + z, "calcite");
  } else {
    for (let y = 1; y <= 3; y += 1) world.set(ox, oy + y, oz, "log");
    for (let x = -1; x <= 1; x += 1) for (let z = -1; z <= 1; z += 1) for (let y = 3; y <= 4; y += 1) world.fill(ox + x, oy + y, oz + z, seed % 2 ? "leavesEmber" : "leavesCrimson");
  }
}

const SKY_VERTEX = /* glsl */ `varying vec3 vDir; void main() { vDir = normalize(position); gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`;

/** The scene around the player. */
export class StageScene {
  readonly root = new Group();
  private readonly island = new Group();
  private readonly sky = new Group();
  private readonly materials: Material[] = [];
  private readonly textures: Texture[] = [];
  private readonly skyMaterial: ShaderMaterial;
  private readonly starMaterial: ShaderMaterial;
  private readonly moonMaterial: MeshBasicMaterial;
  private readonly floorMaterial: ShaderMaterial;
  private readonly sparkle = new Group();
  private readonly wisps: { mesh: Mesh; seed: number; radius: number; height: number; speed: number }[] = [];
  private readonly clouds: InstancedMesh;
  private readonly leaves = new ParticleSystem("leaves");
  private readonly motes = new ParticleSystem("motes", 2);
  private readonly lights: { light: DirectionalLight | PointLight | HemisphereLight; scene: number; void: number }[] = [];
  private readonly fog: Fog;
  readonly ghost = new WispGhost();
  private mode: StageMode = "scene";
  /** 0 on the island, 1 in the void; eases between. */
  private amount = 0;
  private time = 0;
  /** A light the effects (aura, dance) colour and move; always in the scene so shaders never rebuild. */
  readonly effectLight = new PointLight("#ff6a3d", 0, 160, 2);

  constructor(private readonly viewer: SkinViewer) {
    const t = makeTextures();
    this.textures.push(...Object.values(t));
    const keep = <T extends Material>(material: T) => {
      this.materials.push(material);
      return material;
    };
    const plain = (texture: Texture) => keep(blockMaterial(texture));
    const grassSide = plain(t.grassSide);
    const logSide = plain(t.logSide);
    const logTop = plain(t.logTop);
    const materials: Record<string, Material | Material[]> = {
      grass: [grassSide, grassSide, plain(t.grassTop), plain(t.dirt), grassSide, grassSide],
      dirt: plain(t.dirt),
      moss: plain(t.moss),
      stone: plain(t.stone),
      andesite: plain(t.andesite),
      deepslate: plain(t.deepslate),
      deepslateTiles: plain(t.deepslateTiles),
      mossyCobble: plain(t.mossyCobble),
      calcite: keep(new MeshLambertMaterial({ map: t.calcite, emissive: new Color("#f4eee4"), emissiveMap: t.calcite, emissiveIntensity: 0.04 })),
      calcitePolished: keep(new MeshLambertMaterial({ map: t.calcitePolished, emissive: new Color("#f4eee4"), emissiveMap: t.calcitePolished, emissiveIntensity: 0.04 })),
      calciteMossy: plain(t.calciteMossy),
      moonstone: plain(t.moonstone),
      log: [logSide, logSide, logTop, logTop, logSide, logSide],
      leavesCrimson: keep(cutoutMaterial(t.leavesCrimson)),
      leavesEmber: keep(cutoutMaterial(t.leavesEmber)),
      leavesGold: keep(cutoutMaterial(t.leavesGold)),
      lantern: keep(glowMaterial(t.lanternEmber, 1.25)),
      water: keep(new MeshLambertMaterial({ map: t.water, transparent: true, opacity: 0.82 })),
    };

    // ── The island, and three islets far off ──
    const world = new World();
    const decor: Decor = { plants: [], vines: [], chains: [], hanging: [], slabs: [] };
    buildHollow(world, decor);
    const far = new World();
    buildIslet(far, -34, 2, -40, 4, 51, true);
    buildIslet(far, 38, -3, -52, 5, 52, false);
    buildIslet(far, 26, 6, -30, 3, 53, false);
    const see = new Set(["leavesCrimson", "leavesEmber", "leavesGold", "water"]);
    const cast = new Set(["log", "leavesCrimson", "leavesEmber", "leavesGold", "calcite", "calcitePolished", "calciteMossy", "deepslate", "deepslateTiles", "lantern", "moss", "mossyCobble", "stone", "andesite"]);
    world.build(this.island, materials, { see, cast });
    far.build(this.island, materials, { see, cast: new Set() });

    // ── Decor: plants, vines, chains, hanging lights, lantern roofs ──
    const instanced = (geometry: BufferGeometry, material: Material, list: { x: number; y: number; z: number; turn?: number }[], offset: [number, number, number], shadow = false) => {
      if (list.length === 0) return;
      const mesh = new InstancedMesh(geometry, material, list.length);
      const matrix = new Matrix4();
      list.forEach((item, index) => {
        const [x, y, z] = at(item.x, item.y, item.z);
        matrix.makeRotationY(item.turn ?? 0).setPosition(x + offset[0], y + offset[1], z + offset[2]);
        mesh.setMatrixAt(index, matrix);
      });
      mesh.castShadow = shadow;
      mesh.receiveShadow = true;
      this.island.add(mesh);
    };
    const plantShape = cross(BLOCK);
    for (const kind of ["tallGrass", "fern", "moonBlossom", "emberFlower"] as const) {
      const map = t[kind];
      const material =
        kind === "moonBlossom"
          ? keep(new MeshLambertMaterial({ map, alphaTest: 0.5, side: DoubleSide, emissive: new Color("#fff4e0"), emissiveMap: map, emissiveIntensity: 0.6 }))
          : keep(cutoutMaterial(map));
      instanced(plantShape, material, decor.plants.filter((plant) => plant.kind === kind), [0, -BLOCK / 2, 0]);
    }
    const vineMaterial = keep(cutoutMaterial(t.vines));
    for (const length of [1, 2, 3]) {
      const shape = new PlaneGeometry(BLOCK, BLOCK * length);
      instanced(shape, vineMaterial, decor.vines.filter((vine) => vine.length === length), [0, -BLOCK / 2 - (BLOCK * length) / 2, 0]);
    }
    instanced(cross(4, BLOCK), keep(cutoutMaterial(t.chain)), decor.chains, [0, -BLOCK / 2, 0]);
    instanced(box(8, 9, 8), keep(glowMaterial(t.wispLight, 1.3)), decor.hanging, [0, 3, 0], true);
    instanced(box(BLOCK + 4, 6, BLOCK + 4), keep(blockMaterial(t.darkPlanks)), decor.slabs, [0, -BLOCK / 2 + 3, 0], true);

    // ── The ember sparkle beside the statue: a four-point star of glowing blocks ──
    const emberMaterial = keep(glowMaterial(t.ember, 1.3));
    const cube = new BoxGeometry(BLOCK * 0.8, BLOCK * 0.8, BLOCK * 0.8);
    const arms: [number, number][] = [
      [0, 0],
      [0, 1],
      [0, 2],
      [0, -1],
      [0, -2],
      [1, 0],
      [2, 0],
      [-1, 0],
      [-2, 0],
    ];
    for (const [ax, ay] of arms) {
      const piece = new Mesh(cube, emberMaterial);
      const scale = ax === 0 && ay === 0 ? 1.35 : Math.abs(ax) === 2 || Math.abs(ay) === 2 ? 0.65 : 1;
      piece.scale.setScalar(scale);
      piece.position.set(ax * BLOCK * 0.8, ay * BLOCK * 0.8, 0);
      piece.castShadow = true;
      this.sparkle.add(piece);
    }
    const [sx, sy, sz] = at(SPARKLE.x, SPARKLE.y, SPARKLE.z);
    this.sparkle.position.set(sx, sy, sz);
    this.island.add(this.sparkle);
    const sparkleLight = new PointLight("#ff5a4a", 0, 260, 2);
    // Between the sparkle and the statue, so the statue's face catches its glow.
    sparkleLight.position.set(sx - Math.sign(sx) * 20, sy - 10, sz + 30);
    this.island.add(sparkleLight);
    this.lights.push({ light: sparkleLight, scene: 2600, void: 0 });

    // The stone lanterns cast a little warmth.
    for (const side of [-1, 1]) {
      const glow = new PointLight("#ff8a5c", 0, 140, 2);
      const [lx, ly, lz] = at(side * 3, 2, -2);
      glow.position.set(lx, ly, lz + 10);
      this.island.add(glow);
      this.lights.push({ light: glow, scene: 700, void: 0 });
    }

    // ── Will-o'-wisps drifting over the island ──
    const wispGlow = keep(new MeshBasicMaterial({ color: new Color(2.2, 2.1, 1.9), fog: false }));
    const emberGlow = keep(new MeshBasicMaterial({ color: new Color(2.4, 0.9, 0.7), fog: false }));
    const wispShape = new BoxGeometry(3, 3, 3);
    const drift = seeded(77);
    for (let index = 0; index < 11; index += 1) {
      const mesh = new Mesh(wispShape, index % 4 === 3 ? emberGlow : wispGlow);
      this.wisps.push({ mesh, seed: drift() * 100, radius: 50 + drift() * 110, height: 10 + drift() * 70, speed: 0.08 + drift() * 0.12 });
      this.island.add(mesh);
    }

    this.root.add(this.island);

    // ── The sky: night over the hollow, square stars, a pixel moon ──
    this.skyMaterial = new ShaderMaterial({
      uniforms: { fade: { value: 1 } },
      vertexShader: SKY_VERTEX,
      fragmentShader: /* glsl */ `
        uniform float fade;
        varying vec3 vDir;
        void main() {
          float h = vDir.y;
          vec3 zenith = vec3(0.03, 0.035, 0.09);
          vec3 high = vec3(0.07, 0.07, 0.17);
          vec3 horizon = vec3(0.2, 0.12, 0.22);
          vec3 ember = vec3(0.32, 0.12, 0.14);
          vec3 below = vec3(0.05, 0.04, 0.08);
          vec3 color = h > 0.0 ? mix(mix(horizon, high, smoothstep(0.0, 0.25, h)), zenith, smoothstep(0.25, 0.9, h)) : mix(horizon, below, smoothstep(0.0, 0.35, -h));
          color = mix(color, ember, (1.0 - smoothstep(0.0, 0.08, abs(h))) * 0.5);
          vec3 voidColor = vec3(0.047, 0.043, 0.04);
          gl_FragColor = vec4(mix(voidColor, color, fade), 1.0);
        }`,
      side: BackSide,
      depthWrite: false,
      fog: false,
    });
    const dome = new Mesh(new SphereGeometry(1900, 32, 16), this.skyMaterial);
    dome.renderOrder = -10;
    this.root.add(dome);

    const stars = 1100;
    const random = seeded(5);
    const positions = new Float32Array(stars * 3);
    const sizes = new Float32Array(stars);
    const seeds = new Float32Array(stars);
    for (let index = 0; index < stars; index += 1) {
      const theta = random() * Math.PI * 2;
      const y = 0.06 + random() * 0.94;
      const ring = Math.sqrt(1 - y * y);
      positions.set([Math.cos(theta) * ring * 1750, y * 1750, Math.sin(theta) * ring * 1750], index * 3);
      sizes[index] = 1 + random() * random() * 3;
      seeds[index] = random() * 100;
    }
    const starGeometry = new BufferGeometry();
    starGeometry.setAttribute("position", new BufferAttribute(positions, 3));
    starGeometry.setAttribute("size", new BufferAttribute(sizes, 1));
    starGeometry.setAttribute("seed", new BufferAttribute(seeds, 1));
    this.starMaterial = new ShaderMaterial({
      uniforms: { time: { value: 0 }, fade: { value: 1 }, ratio: { value: 1 } },
      vertexShader: /* glsl */ `
        attribute float size;
        attribute float seed;
        uniform float time;
        uniform float ratio;
        varying float vTwinkle;
        void main() {
          vTwinkle = 0.6 + 0.4 * sin(time * (1.0 + fract(seed) * 2.0) + seed);
          gl_PointSize = size * ratio;
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }`,
      fragmentShader: /* glsl */ `
        uniform float fade;
        varying float vTwinkle;
        void main() {
          vec2 q = abs(gl_PointCoord - 0.5);
          float a = step(max(q.x, q.y), 0.42) * vTwinkle * fade;
          gl_FragColor = vec4(vec3(1.0, 0.97, 0.9) * a, a);
        }`,
      transparent: true,
      depthWrite: false,
      blending: AdditiveBlending,
      fog: false,
    });
    const starPoints = new Points(starGeometry, this.starMaterial);
    starPoints.renderOrder = -9;
    this.sky.add(starPoints);

    const moonTexture = paint(71, (p) =>
      p.each((x, y) => {
        const d = Math.hypot(x - 7.5, y - 7.5);
        if (d > 7.2) return p.clear(x, y);
        const crater = Math.hypot(x - 5, y - 6) < 1.6 || Math.hypot(x - 10, y - 9.5) < 1.3 || Math.hypot(x - 8, y - 3) < 0.9;
        p.px(x, y, crater ? "#d9cfbd" : d > 6.3 ? "#e8dfcf" : p.pick(["#f7f1e4", "#fbf7ee", "#f1eadc"]));
      }),
    );
    this.textures.push(moonTexture);
    this.moonMaterial = keep(new MeshBasicMaterial({ map: moonTexture, transparent: true, fog: false, depthWrite: false, color: new Color(1.4, 1.36, 1.3) }));
    const moon = new Mesh(new PlaneGeometry(190, 190), this.moonMaterial);
    moon.position.set(217, 568, -1500);
    moon.lookAt(0, 0, 0);
    moon.renderOrder = -8;
    this.sky.add(moon);
    this.root.add(this.sky);

    // ── A sea of clouds below, drifting ──
    const cloudMaterial = keep(new MeshLambertMaterial({ map: t.cloud, color: new Color("#b9b4d6"), transparent: true, opacity: 0.92 }));
    const cells: [number, number][] = [];
    for (let x = -36; x <= 36; x += 1) for (let z = -36; z <= 24; z += 1) if (noise(x * 0.18, 9, z * 0.18, 61) > 0.52) cells.push([x, z]);
    this.clouds = new InstancedMesh(new BoxGeometry(48, 14, 48), cloudMaterial, cells.length);
    const matrix = new Matrix4();
    cells.forEach(([x, z], index) => this.clouds.setMatrixAt(index, matrix.makeTranslation(x * 48, -300, z * 48)));
    this.island.add(this.clouds);

    // ── Free roam's floor: a faint ring at the feet, seen through from below ──
    this.floorMaterial = new ShaderMaterial({
      uniforms: { fade: { value: 0 } },
      vertexShader: /* glsl */ `varying vec2 vUv; void main() { vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
      fragmentShader: /* glsl */ `
        uniform float fade;
        varying vec2 vUv;
        void main() {
          vec2 q = vUv - 0.5;
          float d = length(q) * 2.0;
          float rings = smoothstep(0.035, 0.0, abs(fract(d * 4.0) - 0.5) - 0.46);
          float a = rings * (1.0 - smoothstep(0.55, 1.0, d)) * 0.22 * fade;
          gl_FragColor = vec4(vec3(0.96, 0.93, 0.89) * a, a);
        }`,
      transparent: true,
      depthWrite: false,
      side: DoubleSide,
      blending: AdditiveBlending,
      fog: false,
    });
    const floor = new Mesh(new RingGeometry(0.01, 60, 64, 1), this.floorMaterial);
    floor.rotation.x = -Math.PI / 2;
    floor.position.y = GROUND - 0.05;
    this.root.add(floor);

    // ── Light: moonlight with shadows, a night sky fill, studio lights for the void ──
    const moonlight = new DirectionalLight("#c4d0ff", 0);
    moonlight.position.set(-160, 280, 200);
    moonlight.target.position.set(0, GROUND, -80);
    moonlight.castShadow = true;
    moonlight.shadow.mapSize.set(2048, 2048);
    Object.assign(moonlight.shadow.camera, { left: -200, right: 200, top: 220, bottom: -200, near: 40, far: 900 });
    moonlight.shadow.camera.updateProjectionMatrix();
    moonlight.shadow.bias = -0.0005;
    moonlight.shadow.normalBias = 0.8;
    this.root.add(moonlight, moonlight.target);
    this.lights.push({ light: moonlight, scene: 1.25, void: 0 });
    const night = new HemisphereLight("#7380c8", "#2c2430", 0);
    this.root.add(night);
    this.lights.push({ light: night, scene: 1.0, void: 0 });
    const studio: [string, [number, number, number], number][] = [
      ["#fff4e6", [90, 120, 140], 1.6],
      ["#d6e4ff", [-140, 40, 90], 0.8],
      ["#ffffff", [0, 120, -160], 1.0],
      // From below, so the soles and undersides show.
      ["#fff0e0", [0, -200, 40], 0.6],
    ];
    for (const [color, [x, y, z], strength] of studio) {
      const light = new DirectionalLight(color, 0);
      light.position.set(x, y, z);
      this.root.add(light);
      this.lights.push({ light, scene: 0, void: strength });
    }
    this.root.add(this.effectLight);

    this.root.add(this.ghost.object);
    this.leaves.attach(this.island);
    this.motes.attach(this.island);

    this.fog = new Fog("#241a33", 500, 2100);
    viewer.scene.fog = this.fog;
    viewer.scene.add(this.root);
    viewer.renderer.shadowMap.enabled = true;
    // The light riding with the camera keeps the skin readable at any distance.
    viewer.cameraLight.decay = 0;
    this.applyMode(0);
  }

  /** The island, or free roam's void. The change eases in over `update`. */
  setMode(mode: StageMode) {
    this.mode = mode;
  }

  /** In free roam's void (or on the way there): nothing to bump into. */
  get roaming(): boolean {
    return this.mode === "void" || this.amount > 0;
  }

  /** Moves everything alive on, and eases between the island and the void. */
  update(dt: number, pixelsPerUnit: number, playmate: Omit<Playmate, "roaming">) {
    this.time += dt;
    const t = this.time;
    const target = this.mode === "void" ? 1 : 0;
    if (this.amount !== target) {
      const step = dt / 0.7;
      this.amount = target > this.amount ? Math.min(target, this.amount + step) : Math.max(target, this.amount - step);
      this.applyMode(this.amount);
    }
    this.starMaterial.uniforms.time.value = t;
    this.starMaterial.uniforms.ratio.value = this.viewer.renderer.getPixelRatio();

    // The sparkle turns slowly and bobs; the wisps drift in loose loops; the clouds slide by.
    this.sparkle.rotation.y = t * 0.6;
    this.sparkle.position.y = at(0, SPARKLE.y, 0)[1] + Math.sin(t * 1.1) * 3;
    for (const wisp of this.wisps) {
      const a = t * wisp.speed + wisp.seed;
      wisp.mesh.position.set(Math.sin(a) * wisp.radius, wisp.height + Math.sin(a * 2.3 + wisp.seed) * 12, -90 + Math.cos(a * 0.8) * wisp.radius * 0.7);
      wisp.mesh.rotation.set(t * 0.7 + wisp.seed, t * 0.9, 0);
    }
    this.clouds.position.x = ((t * 3) % 48) - 24;

    this.leaves.emitting = this.amount < 0.5;
    this.motes.emitting = this.amount < 0.5;
    for (const system of [this.leaves, this.motes]) {
      system.setScale(pixelsPerUnit);
      system.update(dt);
    }
    this.ghost.update(dt, { ...playmate, roaming: this.mode === "void" });
  }

  private applyMode(amount: number) {
    const k = amount * amount * (3 - 2 * amount);
    // The island drops away into the dark; the sky fades to the void.
    this.island.position.y = -520 * k * k;
    this.island.visible = k < 0.999;
    this.sky.visible = k < 0.999;
    this.skyMaterial.uniforms.fade.value = 1 - k;
    this.starMaterial.uniforms.fade.value = 1 - k;
    this.moonMaterial.opacity = 1 - k;
    this.floorMaterial.uniforms.fade.value = k;
    this.fog.near = 500 + k * 1e5;
    this.fog.far = 2100 + k * 1e5;
    for (const { light, scene, void: inVoid } of this.lights) light.intensity = scene + (inVoid - scene) * k;
    // At night the moon does the lighting; the camera's own light (it reaches everything) stays
    // faint so it doesn't flatten the island, and comes up in the void.
    this.viewer.globalLight.intensity = 0.35 + 0.75 * k;
    this.viewer.cameraLight.intensity = 0.3 + 0.4 * k;
  }

  dispose() {
    this.root.removeFromParent();
    this.viewer.scene.fog = null;
    this.ghost.dispose();
    this.leaves.dispose();
    this.motes.dispose();
    const geometries = new Set<BufferGeometry>();
    this.root.traverse((child) => {
      if (child instanceof Mesh || child instanceof Points) geometries.add(child.geometry);
    });
    for (const geometry of geometries) geometry.dispose();
    for (const material of this.materials) material.dispose();
    for (const texture of this.textures) texture.dispose();
    this.skyMaterial.dispose();
    this.starMaterial.dispose();
    this.floorMaterial.dispose();
    this.effectLight.dispose();
    for (const { light } of this.lights) light.dispose();
  }
}
