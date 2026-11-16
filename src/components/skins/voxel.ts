// Minecraft-style building for the skin stage: 16×16 pixel textures drawn here from code (original
// art in the game's style, nothing taken from it), block shapes with one texel per unit, and a
// sparse voxel world meshed the way the game does it: only the faces that can be seen, one mesh
// per material, with soft shading in the corners where blocks meet.
//
// Units are skin pixels: a block is 16, so the player is two blocks tall.

import {
  BoxGeometry,
  BufferAttribute,
  BufferGeometry,
  CanvasTexture,
  Color,
  DoubleSide,
  Float32BufferAttribute,
  Group,
  Mesh,
  MeshBasicMaterial,
  MeshLambertMaterial,
  NearestFilter,
  PlaneGeometry,
  RepeatWrapping,
  type Material,
  type Texture,
} from "three";

export const BLOCK = 16;
/** Where the player's feet are, and so the top of the ground. */
export const GROUND = -16;

/** The world position of a block's centre: layer 0 is the ground the player stands on. */
export const at = (bx: number, by: number, bz: number): [number, number, number] => [bx * BLOCK, GROUND - BLOCK / 2 + by * BLOCK, bz * BLOCK];

// ── Pixel textures ──

export interface Pen {
  px(x: number, y: number, color: string): void;
  clear(x: number, y: number): void;
  random(): number;
  /** One of the colours, more often the heavier weights. */
  pick(colors: string[], weights?: number[]): string;
  each(fn: (x: number, y: number) => void): void;
}

export function seeded(seed: number) {
  let state = Math.max(1, Math.floor(seed)) % 2147483647;
  return () => {
    state = (state * 16807) % 2147483647;
    return (state - 1) / 2147483646;
  };
}

/** A pixel texture, crisp (no smoothing), repeating. */
export function paint(seed: number, draw: (pen: Pen) => void, size = 16): CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = size;
  canvas.height = size;
  const g = canvas.getContext("2d") as CanvasRenderingContext2D;
  const random = seeded(seed * 7919 + 13);
  const pen: Pen = {
    px: (x, y, color) => {
      g.fillStyle = color;
      g.fillRect(((x % size) + size) % size, ((y % size) + size) % size, 1, 1);
    },
    clear: (x, y) => g.clearRect(x, y, 1, 1),
    random,
    pick: (colors, weights) => {
      const total = weights ? weights.reduce((sum, weight) => sum + weight, 0) : colors.length;
      let roll = random() * total;
      for (let index = 0; index < colors.length; index += 1) {
        roll -= weights ? weights[index] : 1;
        if (roll < 0) return colors[index];
      }
      return colors[colors.length - 1];
    },
    each: (fn) => {
      for (let y = 0; y < size; y += 1) for (let x = 0; x < size; x += 1) fn(x, y);
    },
  };
  draw(pen);
  const texture = new CanvasTexture(canvas);
  texture.magFilter = NearestFilter;
  texture.minFilter = NearestFilter;
  texture.generateMipmaps = false;
  texture.wrapS = RepeatWrapping;
  texture.wrapT = RepeatWrapping;
  return texture;
}

/** Noise in a palette: the usual way a block's face is speckled. */
const speckle = (colors: string[], weights?: number[]) => (p: Pen) => p.each((x, y) => p.px(x, y, p.pick(colors, weights)));

/** Rounded stones: each pixel belongs to its nearest seed; edges are mortar. */
function cobbled(p: Pen, stones: string[], mortar: string, accent?: { colors: string[]; chance: number }) {
  const seeds = Array.from({ length: 8 }, () => [p.random() * 16, p.random() * 16]);
  p.each((x, y) => {
    const distances = seeds
      .flatMap(([sx, sy]) => [-16, 0, 16].flatMap((ox) => [-16, 0, 16].map((oy) => [Math.hypot(x - sx - ox, y - sy - oy), x - sx - ox + (y - sy - oy)])))
      .sort((a, b) => a[0] - b[0]);
    const [nearest, second] = distances;
    if (second[0] - nearest[0] < 1.1) p.px(x, y, mortar);
    else if (accent && p.random() < accent.chance) p.px(x, y, p.pick(accent.colors));
    else p.px(x, y, nearest[1] < -1.5 ? stones[0] : nearest[1] > 1.5 ? stones[2] : stones[1]);
  });
}

/** Planks: four boards, seams offset row to row. */
function planks(p: Pen, colors: string[], line: string) {
  p.each((x, y) => {
    const row = Math.floor(y / 4);
    if (y % 4 === 3 || x === (row * 7 + 3) % 16) p.px(x, y, line);
    else p.px(x, y, p.pick(colors, [3, 2, 1]));
  });
}

/** A block face with a bevel: light top-left edges, dark bottom-right. */
function bevelled(p: Pen, light: string, dark: string, body: string[], weights?: number[]) {
  p.each((x, y) => {
    if (x === 0 || y === 0) p.px(x, y, light);
    else if (x === 15 || y === 15) p.px(x, y, dark);
    else p.px(x, y, p.pick(body, weights));
  });
}

/** Leaves: a palette with holes the sky shows through. */
const leaves = (colors: string[], weights?: number[]) => (p: Pen) =>
  p.each((x, y) => {
    if (p.random() < 0.16) p.clear(x, y);
    else p.px(x, y, p.pick(colors, weights));
  });

const GRASS = ["#4f6b3a", "#5a7843", "#66844b", "#718f53", "#48613a", "#7c9a5c"];
const GRASS_W = [2, 3, 3, 2, 2, 1];
const DIRT = ["#4f3828", "#5d4230", "#6b4c37", "#77573f", "#433022"];
const DIRT_W = [2, 3, 3, 2, 1];

/** Every texture the stage uses, made once. */
export function makeTextures() {
  return {
    grassTop: paint(1, speckle(GRASS, GRASS_W)),
    grassSide: paint(2, (p) => {
      p.each((x, y) => p.px(x, y, p.pick(DIRT, DIRT_W)));
      for (let x = 0; x < 16; x += 1) {
        const depth = 3 + (p.random() < 0.5 ? 1 : 0) + (p.random() < 0.2 ? 1 : 0);
        for (let y = 0; y < depth; y += 1) p.px(x, y, p.pick(GRASS, GRASS_W));
      }
    }),
    dirt: paint(3, speckle(DIRT, DIRT_W)),
    moss: paint(4, speckle(["#4a6a2e", "#55783a", "#3f5d27", "#628544", "#6f9150"], [3, 3, 2, 2, 1])),
    stone: paint(5, (p) => {
      p.each((x, y) => p.px(x, y, p.pick(["#5f6066", "#6b6c72", "#76777d", "#81828a", "#58595f"], [2, 3, 3, 2, 1])));
      for (let streak = 0; streak < 5; streak += 1) {
        const x = Math.floor(p.random() * 16);
        const y = Math.floor(p.random() * 16);
        for (let k = 0; k < 2 + Math.floor(p.random() * 3); k += 1) p.px(x + k, y, "#505157");
      }
    }),
    andesite: paint(6, speckle(["#7d7e83", "#8a8b90", "#71727a", "#96979c", "#686970"], [3, 3, 2, 1, 1])),
    // Deepslate: dark, in horizontal layers, the way it splits.
    deepslate: paint(7, (p) => {
      const bands = Array.from({ length: 16 }, () => p.pick(["#36363d", "#3e3e46", "#2f2f35", "#45454e"], [3, 3, 2, 1]));
      p.each((x, y) => p.px(x, y, p.random() < 0.18 ? p.pick(["#28282d", "#4b4b54"]) : bands[y]));
    }),
    deepslateTiles: paint(8, (p) =>
      p.each((x, y) => {
        const edge = x % 8 === 7 || y % 8 === 7;
        const light = x % 8 === 0 || y % 8 === 0;
        p.px(x, y, edge ? "#1f1f24" : light ? "#4c4c55" : p.pick(["#36363d", "#3c3c44", "#33333a"]));
      }),
    ),
    mossyCobble: paint(9, (p) => cobbled(p, ["#8a8b90", "#6f7076", "#56575d"], "#3d3e43", { colors: ["#4f6c2e", "#5d7a36", "#466427"], chance: 0.32 })),
    // Calcite: the statue's stone, a warm cream.
    calcite: paint(10, speckle(["#efe9df", "#e6dfd3", "#f5f1ea", "#ddd5c8", "#faf7f1"], [3, 3, 2, 1, 1])),
    // Polished calcite: the same cream, bevelled, so each block of a carving shows.
    calcitePolished: paint(33, (p) => bevelled(p, "#fcfaf5", "#cfc5b5", ["#efe9df", "#ebe4d8", "#f3eee6", "#e6dfd2"], [3, 2, 2, 1])),
    calciteMossy: paint(11, (p) =>
      p.each((x, y) => p.px(x, y, p.random() < 0.3 + (y / 16) * 0.35 ? p.pick(["#55783a", "#4a6a2e", "#628544"]) : p.pick(["#e6dfd3", "#ddd5c8", "#efe9df"]))),
    ),
    // Moonstone: the plaza's polished cream tiles.
    moonstone: paint(12, (p) =>
      p.each((x, y) => {
        const edge = x === 0 || y === 0 || x === 15 || y === 15;
        p.px(x, y, edge ? "#cbbfa9" : p.pick(["#e9e1d2", "#e2d9c8", "#efe8db"], [3, 2, 2]));
      }),
    ),
    logSide: paint(13, (p) => {
      const columns = Array.from({ length: 16 }, () => p.pick(["#2f2420", "#3a2c26", "#251c19", "#46362e"], [3, 3, 2, 1]));
      p.each((x, y) => p.px(x, y, p.random() < 0.12 ? "#1c1513" : columns[x]));
    }),
    logTop: paint(14, (p) =>
      p.each((x, y) => {
        const ring = Math.floor(Math.max(Math.abs(x - 7.5), Math.abs(y - 7.5)));
        p.px(x, y, ring >= 7 ? "#2f2420" : ring % 2 === 0 ? "#9b6a48" : "#87593b");
      }),
    ),
    leavesCrimson: paint(15, leaves(["#7f2420", "#952b25", "#a9342b", "#6c1d1a", "#b8402f"], [3, 3, 2, 2, 1])),
    leavesEmber: paint(16, leaves(["#b9452a", "#c95530", "#d86636", "#a63b25", "#e2793f"], [3, 3, 2, 2, 1])),
    leavesGold: paint(17, leaves(["#d58a31", "#e29c3f", "#c4782a", "#efb455"], [3, 2, 2, 1])),
    // Glowing blocks: ember crystal (the sparkle) and wisp light (lanterns, hanging lights).
    ember: paint(18, (p) => bevelled(p, "#ffb3a6", "#a3241f", ["#e5484d", "#f05a55", "#d93a40", "#ff7a6e"], [3, 2, 2, 1])),
    wispLight: paint(19, (p) =>
      p.each((x, y) => {
        const frame = x < 1 || y < 1 || x > 14 || y > 14;
        const d = Math.hypot(x - 7.5, y - 7.5);
        p.px(x, y, frame ? "#3a332c" : d < 3.5 ? "#fffaf0" : d < 5.5 ? "#f6eedd" : "#e8dcc5");
      }),
    ),
    lanternEmber: paint(20, (p) =>
      p.each((x, y) => {
        const frame = x < 2 || y < 2 || x > 13 || y > 13 || x === 7 || x === 8 || y === 7 || y === 8;
        const d = Math.hypot(x - 7.5, y - 7.5);
        p.px(x, y, frame ? ((x + y) % 3 === 0 ? "#3a3c45" : "#2a2c33") : d < 3 ? "#ffd2a8" : d < 5 ? "#ff8a5c" : "#e5484d");
      }),
    ),
    water: paint(21, speckle(["#25407f", "#2c4c94", "#3457a6", "#203a73", "#3f64b5"], [3, 3, 2, 2, 1])),
    gold: paint(22, (p) => bevelled(p, "#fff3a0", "#c48c16", ["#fbe36a", "#f5c937", "#ecbc2e", "#ddaa22"], [1, 3, 2, 1])),
    redWool: paint(23, (p) => p.each((x, y) => p.px(x, y, (Math.floor(x / 2) + Math.floor(y / 2)) % 2 === 0 ? p.pick(["#a62824", "#b02e29"]) : p.pick(["#951f1c", "#9e2420"])))),
    darkPlanks: paint(24, (p) => planks(p, ["#3b2b1f", "#46331f", "#33251a"], "#22170e")),
    tallGrass: paint(25, (p) => {
      p.each((x, y) => p.clear(x, y));
      for (let x = 1; x < 15; x += 1) {
        if (p.random() < 0.3) continue;
        const height = 5 + Math.floor(p.random() * 9);
        for (let y = 16 - height; y < 16; y += 1) p.px(x, y, p.pick(GRASS, GRASS_W));
      }
    }),
    fern: paint(26, (p) => {
      p.each((x, y) => p.clear(x, y));
      for (let y = 2; y < 16; y += 1) {
        const spread = Math.floor((16 - y) / 3);
        for (let dx = -spread; dx <= spread; dx += 1) if ((y + dx) % 2 === 0) p.px(7 + dx, y, p.pick(["#3f6a2e", "#4b7a36", "#365c27"]));
      }
    }),
    // A pale flower that glows a little: moon blossoms.
    moonBlossom: paint(27, (p) => {
      p.each((x, y) => p.clear(x, y));
      for (let y = 8; y < 16; y += 1) p.px(7, y, "#3f6a2e");
      p.px(6, 11, "#4b7a36");
      p.px(8, 12, "#4b7a36");
      for (const [x, y] of [[7, 4], [6, 5], [8, 5], [5, 6], [9, 6], [6, 7], [8, 7], [7, 6]]) p.px(x, y, p.pick(["#f6efe2", "#fffaf2", "#e9dfcd"]));
      p.px(7, 5, "#e5484d");
    }),
    emberFlower: paint(28, (p) => {
      p.each((x, y) => p.clear(x, y));
      for (let y = 9; y < 16; y += 1) p.px(8, y, "#3f6a2e");
      for (const [x, y] of [[7, 6], [8, 6], [9, 6], [7, 7], [8, 7], [9, 7], [8, 5], [8, 8]]) p.px(x, y, p.pick(["#e5484d", "#f05a55", "#c93a3a"]));
      p.px(8, 6, "#ffd2a8");
    }),
    vines: paint(29, (p) =>
      p.each((x, y) => {
        if ((x * 7 + Math.floor(y / 3) * 5) % 4 === 0 && p.random() < 0.85) p.px(x, y, p.pick(["#3f6a2e", "#4b7a36", "#2f5422"]));
        else if (p.random() < 0.16) p.px(x, y, "#3f6a2e");
        else p.clear(x, y);
      }),
    ),
    chain: paint(30, (p) =>
      p.each((x, y) => {
        if ((x === 7 || x === 8) && y % 4 !== 3) p.px(x, y, y % 4 === 0 ? "#5b5f6b" : "#3a3d46");
        else p.clear(x, y);
      }),
    ),
    glass: paint(31, (p) =>
      p.each((x, y) => {
        const edge = x === 0 || y === 0 || x === 15 || y === 15;
        const shine = (x === 3 && y > 2 && y < 7) || (x === 4 && y === 3);
        p.px(x, y, edge ? "#e9e9e9" : shine ? "#ffffff" : "#cfcfcf");
      }),
    ),
    cloud: paint(32, speckle(["#e9e6f2", "#f1eef7", "#dedaea"], [3, 2, 1])),
  };
}

export type Textures = ReturnType<typeof makeTextures>;
export type TextureName = keyof Textures;

/** A matte block material, the way the game lights things. */
export function blockMaterial(map: Texture, extra: ConstructorParameters<typeof MeshLambertMaterial>[0] = {}): MeshLambertMaterial {
  return new MeshLambertMaterial({ map, ...extra });
}

/**
 * A block that gives off light (lanterns, crystals): full-bright, the way the game draws light
 * sources, so no other light washes it out. Above 1, its brightest pixels bloom.
 */
export function glowMaterial(map: Texture, strength = 1.2): MeshBasicMaterial {
  return new MeshBasicMaterial({ map, color: new Color(strength, strength, strength) });
}

/** A see-through cut-out material (leaves, plants, vines). */
export function cutoutMaterial(map: Texture): MeshLambertMaterial {
  return new MeshLambertMaterial({ map, alphaTest: 0.5, side: DoubleSide });
}

// ── Shapes ──

/**
 * A box showing its texture at one texel per unit, the way slabs, fences and walls show part of a
 * block's texture. `v0` shifts which rows of the picture its sides start from.
 */
export function box(width: number, height: number, depth: number, v0 = 0): BoxGeometry {
  const geometry = new BoxGeometry(width, height, depth);
  const uv = geometry.getAttribute("uv") as BufferAttribute;
  // BoxGeometry's faces: +x, -x, +y, -y, +z, -z, four corners each.
  const sizes: [number, number][] = [
    [depth, height],
    [depth, height],
    [width, depth],
    [width, depth],
    [width, height],
    [width, height],
  ];
  sizes.forEach(([u, v], face) => {
    for (let corner = 0; corner < 4; corner += 1) {
      const index = face * 4 + corner;
      uv.setXY(index, (uv.getX(index) * u) / BLOCK, (uv.getY(index) * v) / BLOCK + v0);
    }
  });
  uv.needsUpdate = true;
  return geometry;
}

/** Two crossed planes, the way the game draws grass and flowers. */
export function cross(size = BLOCK, height = size): BufferGeometry {
  const half = size / 2;
  const positions = [-half, 0, -half, half, 0, half, half, height, half, -half, height, -half, -half, 0, half, half, 0, -half, half, height, -half, -half, height, half];
  const uvs = [0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1];
  const geometry = new BufferGeometry();
  geometry.setAttribute("position", new BufferAttribute(new Float32Array(positions), 3));
  geometry.setAttribute("uv", new BufferAttribute(new Float32Array(uvs), 2));
  geometry.setAttribute("normal", new BufferAttribute(new Float32Array(Array.from({ length: 8 }, () => [0, 1, 0]).flat()), 3));
  geometry.setIndex([0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7]);
  return geometry;
}

/** A flat square lying on the ground. */
export function flat(size = BLOCK): BufferGeometry {
  const geometry = new PlaneGeometry(size, size);
  geometry.rotateX(-Math.PI / 2);
  return geometry;
}

// ── A world of blocks ──

const pack = (x: number, y: number, z: number) => ((x + 512) * 1024 + (y + 512)) * 1024 + (z + 512);

type Triple = [number, number, number];

/**
 * A block's six faces, in BoxGeometry's order (so a kind's six materials line up): the way each
 * faces, and its corners in a unit cube, anticlockwise seen from outside, carrying the picture's
 * (0,0), (1,0), (1,1), (0,1).
 */
const FACES: { normal: Triple; corners: Triple[] }[] = [
  { normal: [1, 0, 0], corners: [[1, 0, 1], [1, 0, 0], [1, 1, 0], [1, 1, 1]] },
  { normal: [-1, 0, 0], corners: [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]] },
  { normal: [0, 1, 0], corners: [[0, 1, 1], [1, 1, 1], [1, 1, 0], [0, 1, 0]] },
  { normal: [0, -1, 0], corners: [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]] },
  { normal: [0, 0, 1], corners: [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]] },
  { normal: [0, 0, -1], corners: [[1, 0, 0], [0, 0, 0], [0, 1, 0], [1, 1, 0]] },
];
const UVS = [
  [0, 0],
  [1, 0],
  [1, 1],
  [0, 1],
];
/** How light a corner stays with 0 to 3 blocks crowding it: the game's soft shading. */
const SHADE = [1, 0.8, 0.66, 0.52];

interface Bucket {
  material: Material;
  cast: boolean;
  position: number[];
  normal: number[];
  uv: number[];
  color: number[];
  index: number[];
}

/**
 * Blocks by grid position. Building it meshes only the faces that can be seen (a face against a
 * solid block is never drawn, so the inside of a hill costs nothing and no hidden face can
 * flicker through a seam), with corner shading in the vertex colours.
 */
export class World {
  private readonly cells = new Map<number, string>();
  private readonly positions = new Map<number, Triple>();

  set(x: number, y: number, z: number, kind: string | null) {
    const key = pack(x, y, z);
    if (kind === null) {
      this.cells.delete(key);
      this.positions.delete(key);
    } else {
      this.cells.set(key, kind);
      this.positions.set(key, [x, y, z]);
    }
  }

  get(x: number, y: number, z: number): string | undefined {
    return this.cells.get(pack(x, y, z));
  }

  /** Sets a block only where there's air. */
  fill(x: number, y: number, z: number, kind: string) {
    if (!this.cells.has(pack(x, y, z))) this.set(x, y, z, kind);
  }

  /**
   * One mesh per material. `see` lists kinds that don't hide what's behind them (leaves, water):
   * faces against them are drawn, except between two of the same kind. `cast` lists the kinds
   * that throw shadows. Materials given here are switched to vertex colours for the shading.
   */
  build(parent: Group, materials: Record<string, Material | Material[]>, options: { see: Set<string>; cast: Set<string> }) {
    const solid = (x: number, y: number, z: number) => {
      const kind = this.cells.get(pack(x, y, z));
      return kind !== undefined && !options.see.has(kind);
    };
    const buckets = new Map<string, Bucket>();
    let faces = 0;
    for (const [key, kind] of this.cells) {
      const material = materials[kind];
      if (!material) continue;
      const [x, y, z] = this.positions.get(key) as Triple;
      const [cx, cy, cz] = at(x, y, z);
      const cast = options.cast.has(kind);
      FACES.forEach((face, side) => {
        const [nx, ny, nz] = face.normal;
        const next = this.cells.get(pack(x + nx, y + ny, z + nz));
        if (next !== undefined && (!options.see.has(next) || next === kind)) return;
        const faceMaterial = Array.isArray(material) ? material[side] : material;
        const id = `${faceMaterial.uuid}:${cast}`;
        let bucket = buckets.get(id);
        if (!bucket) buckets.set(id, (bucket = { material: faceMaterial, cast, position: [], normal: [], uv: [], color: [], index: [] }));
        const base = bucket.position.length / 3;
        // The layer this face looks into, and the two directions across it.
        const [ox, oy, oz] = [x + nx, y + ny, z + nz];
        const [a, b] = [0, 1, 2].filter((axis) => face.normal[axis] === 0);
        // Light sources aren't shaded.
        const lit = !(faceMaterial instanceof MeshBasicMaterial);
        const shades = face.corners.map((corner, index) => {
          bucket.position.push(cx + (corner[0] - 0.5) * BLOCK, cy + (corner[1] - 0.5) * BLOCK, cz + (corner[2] - 0.5) * BLOCK);
          bucket.normal.push(nx, ny, nz);
          bucket.uv.push(UVS[index][0], UVS[index][1]);
          let shade = 1;
          if (lit) {
            const da: Triple = [0, 0, 0];
            const db: Triple = [0, 0, 0];
            da[a] = corner[a] * 2 - 1;
            db[b] = corner[b] * 2 - 1;
            const one = solid(ox + da[0], oy + da[1], oz + da[2]);
            const two = solid(ox + db[0], oy + db[1], oz + db[2]);
            const both = solid(ox + da[0] + db[0], oy + da[1] + db[1], oz + da[2] + db[2]);
            shade = SHADE[one && two ? 3 : Number(one) + Number(two) + Number(both)];
          }
          bucket.color.push(shade, shade, shade);
          return shade;
        });
        // Split the square along the diagonal that keeps a dark corner from streaking across it.
        if (shades[0] + shades[2] >= shades[1] + shades[3]) bucket.index.push(base, base + 1, base + 2, base, base + 2, base + 3);
        else bucket.index.push(base, base + 1, base + 3, base + 1, base + 2, base + 3);
        faces += 1;
      });
    }
    for (const bucket of buckets.values()) {
      const geometry = new BufferGeometry();
      geometry.setAttribute("position", new Float32BufferAttribute(bucket.position, 3));
      geometry.setAttribute("normal", new Float32BufferAttribute(bucket.normal, 3));
      geometry.setAttribute("uv", new Float32BufferAttribute(bucket.uv, 2));
      geometry.setAttribute("color", new Float32BufferAttribute(bucket.color, 3));
      geometry.setIndex(bucket.index);
      bucket.material.vertexColors = true;
      const mesh = new Mesh(geometry, bucket.material);
      mesh.castShadow = bucket.cast;
      mesh.receiveShadow = true;
      parent.add(mesh);
    }
    return faces;
  }
}
