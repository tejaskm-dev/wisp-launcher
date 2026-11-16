import { useEffect, useRef } from "react";
import { DepthTexture, FloatType, HalfFloatType, Vector2, Vector3, WebGLRenderTarget } from "three";
import { UnrealBloomPass } from "three/examples/jsm/postprocessing/UnrealBloomPass.js";
import { FunctionAnimation, SkinViewer, type PlayerObject } from "skinview3d";
import type { SkinModel } from "../../data/types";
import { cn } from "../../lib/cn";
import { applyPose, blendPose, emoteById, idlePose, smooth, type Emote, type Pose } from "./emotes";
import type { WispGhost } from "./ghost";
import { makeProp, type Prop, type PropContext } from "./props";
import { StageScene, type StageMode } from "./scene";

/** A plain stage (transparent, for the editor), the island, or free roam's void. */
export type StageSetting = "plain" | "scene" | "void";

interface SkinStageProps {
  /** A data URL, or a canvas being edited (the editor redraws the viewer's own copy). */
  skin: string | HTMLCanvasElement;
  model: SkinModel;
  /** An emote id, "idle" to stand and breathe, or null to hold perfectly still (for editing). */
  emote: string | null;
  autoRotate?: boolean;
  nameTag?: string | null;
  /** Hands the viewer out (the editor paints through it). */
  onViewer?: (viewer: SkinViewer | null) => void;
  className?: string;
  /** How close the camera sits on a plain stage. */
  zoom?: number;
  /** Plain or not is fixed when the stage first shows; the island and the void switch freely. */
  setting?: StageSetting;
  /** Changing it plays the emote again from the start. */
  replay?: number;
  /** Changing it eases the camera back to where it started. */
  recenter?: number;
}

/** Seconds to ease from one emote into the next. */
const BLEND = 0.45;
/** Seconds the camera takes to move between framings. */
const GLIDE = 0.9;
/**
 * How far the camera may go on the island when it's low down. The statue, the ridge and the trees
 * behind the player are all further out than this, so circling the player never goes through
 * them; only from high above the canopies can it pull back to take in the whole island.
 */
const NEAR_ON_ISLAND = 100;
/** The turntable's speed, in radians a second: a full turn in about 18 seconds. */
const SPIN = 0.35;
const TAU = Math.PI * 2;

interface Framing {
  position: Vector3;
  target: Vector3;
  minDistance: number;
  maxDistance: number;
  maxPolar: number;
  pan: boolean;
}

const FRAMES: Record<StageMode, Framing> = {
  // Low and a little back, looking up past the player at the statue, the trees and the moon. Far
  // views are allowed only from up high (see `NEAR_ON_ISLAND`).
  scene: { position: new Vector3(0, 10, 100), target: new Vector3(0, 14, 0), minDistance: 34, maxDistance: 200, maxPolar: 1.76, pan: false },
  // Anywhere at all, close enough to see single pixels, underneath included.
  void: { position: new Vector3(0, 8, 74), target: new Vector3(0, 0, 0), minDistance: 10, maxDistance: 170, maxPolar: Math.PI, pan: true },
};

interface Runtime {
  emote: Emote | null;
  /** Holding still for the editor: poses aren't touched. */
  still: boolean;
  time: number;
  /** The pose being eased away from, while a new emote starts. */
  from: Pose | null;
  last: Pose;
  props: { current: Prop | null; leaving: Prop[] };
  glide: { from: Vector3; fromTarget: Vector3; to: Framing; t: number } | null;
  /** Where the head is turned to follow the ghost, eased so it never snaps. */
  gaze: { yaw: number; pitch: number };
  /** The turntable: whether it's on, and how far the player has turned on the spot. */
  spinning: boolean;
  spin: number;
}

const easeInOut = (x: number) => (x < 0.5 ? 4 * x * x * x : 1 - (-2 * x + 2) ** 3 / 2);
const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));
const wrap = (angle: number) => Math.atan2(Math.sin(angle), Math.cos(angle));

/**
 * The player keeps an eye on the ghost: the head turns after it as far as a neck can, as much as
 * it has their attention, and tips back when it boops them.
 */
function watch(pose: Pose, ghost: WispGhost, gaze: Runtime["gaze"], spin: number, dt: number): Pose {
  const dx = ghost.look.x - (pose.x ?? 0);
  const dy = ghost.look.y - (12 + (pose.y ?? 0));
  const dz = ghost.look.z - (pose.z ?? 0);
  const yaw = clamp(wrap(Math.atan2(dx, dz) - (pose.turn ?? 0) - spin), -1.1, 1.1);
  const pitch = clamp(-Math.atan2(dy, Math.hypot(dx, dz)), -0.7, 0.5);
  const ease = Math.min(1, dt * 5);
  gaze.yaw += (yaw - gaze.yaw) * ease;
  gaze.pitch += (pitch - gaze.pitch) * ease;
  const attention = ghost.attention;
  const flinch = ghost.flinch;
  if (attention < 0.01 && flinch < 0.01) return pose;
  const headY = pose.headY ?? 0;
  const headX = pose.headX ?? 0;
  return {
    ...pose,
    headY: headY + (gaze.yaw - headY) * attention,
    headX: headX + (gaze.pitch - headX) * attention - flinch * 0.35,
    lean: (pose.lean ?? 0) + flinch * 0.04,
    z: (pose.z ?? 0) - flinch * 0.6,
  };
}

function limits(viewer: SkinViewer, framing: Framing) {
  const controls = viewer.controls;
  controls.minDistance = framing.minDistance;
  controls.maxDistance = framing.maxDistance;
  controls.minPolarAngle = 0;
  controls.maxPolarAngle = framing.maxPolar;
  controls.enablePan = framing.pan;
}

/**
 * The player in 3D: standing on a little island at night (or in free roam's void), emoting, or
 * sitting on a throne. One WebGL view: it stops drawing while Wisp is in the background and is
 * freed when it leaves the page.
 */
export function SkinStage({ skin, model, emote, autoRotate = false, nameTag = null, onViewer, className, zoom = 0.86, setting = "plain", replay = 0, recenter = 0 }: SkinStageProps) {
  const holder = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const viewerRef = useRef<SkinViewer | null>(null);
  const stageRef = useRef<StageScene | null>(null);
  const contextRef = useRef<PropContext>({ light: null });
  const runtime = useRef<Runtime>({ emote: null, still: true, time: 0, from: null, last: {}, props: { current: null, leaving: [] }, glide: null, gaze: { yaw: 0, pitch: 0 }, spinning: false, spin: 0 });
  const lastEmote = useRef<string | null | undefined>(undefined);
  const scenic = useRef(setting !== "plain");
  const onViewerRef = useRef(onViewer);
  onViewerRef.current = onViewer;

  useEffect(() => {
    const element = canvas.current;
    const box = holder.current;
    if (!element || !box) return;
    const viewer = new SkinViewer({
      canvas: element,
      width: Math.max(1, box.clientWidth),
      height: Math.max(1, box.clientHeight),
      pixelRatio: Math.min(window.devicePixelRatio || 1, 2),
      fov: 40,
      zoom,
    });
    viewer.globalLight.intensity = 2.4;
    viewer.cameraLight.intensity = 0.9;
    viewer.controls.enablePan = false;
    viewer.controls.minDistance = 22;
    viewer.controls.maxDistance = 120;
    viewer.autoRotate = false;
    viewerRef.current = viewer;

    let stage: StageScene | null = null;
    let bloom: UnrealBloomPass | null = null;
    let target: WebGLRenderTarget | null = null;
    if (scenic.current) {
      stage = new StageScene(viewer);
      // Buffers that hold light brighter than white, so only lanterns, ember and fire bloom.
      const size = viewer.renderer.getDrawingBufferSize(new Vector2());
      target = new WebGLRenderTarget(size.x, size.y, { type: HalfFloatType, depthTexture: new DepthTexture(size.x, size.y, FloatType) });
      viewer.composer.reset(target);
      bloom = new UnrealBloomPass(new Vector2(size.x, size.y), 0.42, 0.32, 1.1);
      viewer.composer.insertPass(bloom, 1);
      const parts = viewer.playerObject.skin;
      for (const part of [parts.head, parts.body, parts.rightArm, parts.leftArm, parts.rightLeg, parts.leftLeg]) part.innerLayer.castShadow = true;
      const framing = setting === "void" ? FRAMES.void : FRAMES.scene;
      viewer.camera.position.copy(framing.position);
      viewer.controls.target.copy(framing.target);
      limits(viewer, framing);
      if (setting === "void") stage.setMode("void");
      viewer.controls.update();
    }
    stageRef.current = stage;
    contextRef.current = { light: stage?.effectLight ?? null };

    const run = runtime.current;
    const head = new Vector3();
    // One loop for everything: the pose (eased between emotes), props, the island and the ghost on
    // it, the camera.
    const tick = (player: PlayerObject, delta: number) => {
      const dt = Math.min(delta, 0.1);
      const scene = stageRef.current;
      if (!run.still) {
        run.time += dt;
        // The turntable turns the player on the spot, so the scene behind stays as it's framed; it
        // comes round to face front for a throne.
        if (run.emote?.prop === "throne") run.spin += (Math.round(run.spin / TAU) * TAU - run.spin) * Math.min(1, dt * 3);
        else if (run.spinning) run.spin = (run.spin + dt * SPIN) % (TAU * 64);
        const aim = run.emote ? run.emote.pose(run.time) : idlePose(run.time);
        let pose = run.from && run.time < BLEND ? blendPose(run.from, aim, smooth(run.time / BLEND)) : aim;
        if (run.time >= BLEND) run.from = null;
        if (scene) pose = watch(pose, scene.ghost, run.gaze, run.spin, dt);
        applyPose(player, pose);
        player.rotation.y += run.spin;
        run.last = pose;
      }
      const scale = viewer.renderer.domElement.height / (2 * Math.tan((viewer.camera.fov * Math.PI) / 360));
      stepProps(run.props, dt, scale);
      if (scene) {
        // The middle of the head, 4 above the neck it turns on.
        player.skin.head.updateWorldMatrix(true, false);
        player.skin.head.localToWorld(head.set(0, 4, 0));
        const activity = run.still || !run.emote ? "idle" : run.emote.id;
        scene.update(dt, scale, { activity, since: run.time, head, camera: viewer.camera.position });
      }
      if (scene && !run.glide && !scene.roaming) {
        const low = smooth((viewer.controls.getPolarAngle() - 0.6) / 0.4);
        viewer.controls.maxDistance = FRAMES.scene.maxDistance - (FRAMES.scene.maxDistance - NEAR_ON_ISLAND) * low;
      }
      if (run.glide) {
        const glide = run.glide;
        glide.t += dt / GLIDE;
        const k = easeInOut(Math.min(1, glide.t));
        viewer.camera.position.lerpVectors(glide.from, glide.to.position, k);
        viewer.controls.target.lerpVectors(glide.fromTarget, glide.to.target, k);
        if (glide.t >= 1) {
          limits(viewer, glide.to);
          run.glide = null;
        }
      }
    };
    viewer.animation = new FunctionAnimation((player, _progress, delta) => tick(player, delta));
    // Grabbing the view stops a glide where it is.
    const stopGlide = () => {
      if (!run.glide) return;
      limits(viewer, run.glide.to);
      run.glide = null;
    };
    viewer.controls.addEventListener("start", stopGlide);

    const resize = new ResizeObserver(() => viewer.setSize(Math.max(1, box.clientWidth), Math.max(1, box.clientHeight)));
    resize.observe(box);
    // Stop drawing while Wisp isn't the focused window (the rest of the app pauses then too).
    const root = document.documentElement;
    const idle = new MutationObserver(() => {
      viewer.renderPaused = root.hasAttribute("data-idle");
    });
    idle.observe(root, { attributes: true, attributeFilter: ["data-idle"] });
    viewer.renderPaused = root.hasAttribute("data-idle");

    return () => {
      resize.disconnect();
      idle.disconnect();
      viewer.controls.removeEventListener("start", stopGlide);
      run.props.current?.dispose();
      for (const prop of run.props.leaving) prop.dispose();
      run.props = { current: null, leaving: [] };
      stage?.dispose();
      stageRef.current = null;
      bloom?.dispose();
      viewerRef.current = null;
      viewer.dispose();
      target?.dispose();
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // A plain stage frames itself by zoom; the island and the void by their framings.
  useEffect(() => {
    const viewer = viewerRef.current;
    if (viewer && !scenic.current) viewer.zoom = zoom;
  }, [zoom]);

  const glideTo = (viewer: SkinViewer, framing: Framing) => {
    const controls = viewer.controls;
    // Loose limits while gliding, so the camera isn't clamped on the way.
    controls.minDistance = Math.min(controls.minDistance, framing.minDistance);
    controls.maxDistance = Math.max(controls.maxDistance, framing.maxDistance);
    controls.maxPolarAngle = Math.PI;
    runtime.current.glide = { from: viewer.camera.position.clone(), fromTarget: controls.target.clone(), to: framing, t: 0 };
  };

  // The island or the void: the scene eases over and the camera glides to that view.
  useEffect(() => {
    const viewer = viewerRef.current;
    const stage = stageRef.current;
    if (!viewer || !stage) return;
    const mode: StageMode = setting === "void" ? "void" : "scene";
    stage.setMode(mode);
    glideTo(viewer, FRAMES[mode]);
  }, [setting]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || !stageRef.current || recenter === 0) return;
    glideTo(viewer, FRAMES[setting === "void" ? "void" : "scene"]);
  }, [recenter]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer || !skin) return;
    const options = { model: model === "slim" ? ("slim" as const) : ("default" as const) };
    // Paused (Wisp in the background), the stage doesn't move but still shows the skin.
    const still = () => {
      if (viewerRef.current === viewer && viewer.renderPaused) viewer.render();
    };
    if (typeof skin === "string") void viewer.loadSkin(skin, options).then(still, () => {});
    else {
      viewer.loadSkin(skin, options);
      still();
    }
  }, [skin, model]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    const motion = document.documentElement.dataset.motion;
    const reduced = motion === "reduced" || (motion !== "full" && window.matchMedia("(prefers-reduced-motion: reduce)").matches);
    runtime.current.spinning = autoRotate && !reduced;
  }, [autoRotate]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (viewer) viewer.nameTag = nameTag || null;
  }, [nameTag]);

  // The emote: its prop arrives (the last one leaves), and the pose eases over from wherever it was.
  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    const run = runtime.current;
    const held = run.props;
    const again = lastEmote.current === emote;
    lastEmote.current = emote;
    if (held.current) {
      // Played again, it starts over at once; a different emote lets the old prop leave.
      if (again) held.current.dispose();
      else {
        held.current.leave();
        held.leaving.push(held.current);
      }
      held.current = null;
    }
    if (emote === null) {
      run.still = true;
      return;
    }
    const chosen = emoteById(emote);
    run.still = false;
    run.from = { ...run.last };
    run.time = 0;
    run.emote = chosen;
    if (chosen?.prop) {
      held.current = makeProp(chosen.prop, contextRef.current);
      viewer.playerWrapper.add(held.current.object);
    }
  }, [emote, replay]);

  // Handed out last, after the pose above is set, so a parent can adjust it.
  useEffect(() => {
    onViewerRef.current?.(viewerRef.current);
    return () => onViewerRef.current?.(null);
  }, []);

  return (
    <div ref={holder} className={cn("relative size-full", className)}>
      <canvas ref={canvas} className="absolute inset-0 size-full touch-none outline-none" />
    </div>
  );
}

/**
 * Moves every prop on a frame, throwing away those that have finished leaving. The leaving go
 * first, so the one arriving has the last word on the light they share.
 */
function stepProps(held: { current: Prop | null; leaving: Prop[] }, delta: number, scale: number) {
  held.leaving = held.leaving.filter((prop) => {
    prop.setScale(scale);
    if (prop.update(delta)) return true;
    prop.dispose();
    return false;
  });
  if (held.current) {
    held.current.setScale(scale);
    held.current.update(delta);
  }
}
