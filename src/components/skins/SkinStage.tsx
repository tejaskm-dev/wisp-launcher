import { useEffect, useRef } from "react";
import { FunctionAnimation, SkinViewer } from "skinview3d";
import type { SkinModel } from "../../data/types";
import { cn } from "../../lib/cn";
import { applyPose, emoteById, idlePose, resetPose } from "./emotes";
import { makeProp, type Prop } from "./props";

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
  zoom?: number;
  /** How high the player stands in the picture (in skin pixels), to stand on something drawn behind. */
  lift?: number;
  /** Changing it plays the emote again from the start. */
  replay?: number;
}

/**
 * The player in 3D, standing, emoting, or sitting on a throne. One WebGL view: it stops drawing
 * while Wisp is in the background and is freed when it leaves the page.
 */
export function SkinStage({ skin, model, emote, autoRotate = false, nameTag = null, onViewer, className, zoom = 0.86, lift = 0, replay = 0 }: SkinStageProps) {
  const holder = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const viewerRef = useRef<SkinViewer | null>(null);
  // The emote's prop, and older ones still fading out.
  const props = useRef<{ current: Prop | null; leaving: Prop[] }>({ current: null, leaving: [] });
  const lastEmote = useRef<string | null | undefined>(undefined);
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
    viewerRef.current = viewer;

    const resize = new ResizeObserver(() => viewer.setSize(Math.max(1, box.clientWidth), Math.max(1, box.clientHeight)));
    resize.observe(box);
    // Stop drawing while Wisp isn't the focused window (the rest of the app pauses then too).
    const root = document.documentElement;
    const idle = new MutationObserver(() => {
      viewer.renderPaused = root.hasAttribute("data-idle");
    });
    idle.observe(root, { attributes: true, attributeFilter: ["data-idle"] });
    viewer.renderPaused = root.hasAttribute("data-idle");

    const held = props.current;
    return () => {
      resize.disconnect();
      idle.disconnect();
      held.current?.dispose();
      for (const prop of held.leaving) prop.dispose();
      held.current = null;
      held.leaving = [];
      viewerRef.current = null;
      viewer.dispose();
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Framing, set before the first picture: the camera moves down rather than the player up, which
  // keeps skinview3d's name tag where it belongs.
  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    viewer.zoom = zoom;
    viewer.resetCameraPose();
    viewer.camera.position.y = -lift;
    viewer.controls.target.set(0, -lift, 0);
    viewer.controls.update();
  }, [zoom, lift]);

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
    viewer.autoRotate = autoRotate && !reduced;
    viewer.autoRotateSpeed = 0.5;
  }, [autoRotate]);

  useEffect(() => {
    const viewer = viewerRef.current;
    if (viewer) viewer.nameTag = nameTag || null;
  }, [nameTag]);

  // The emote drives each frame: the pose first, then whatever appeared with it.
  useEffect(() => {
    const viewer = viewerRef.current;
    if (!viewer) return;
    const held = props.current;
    const again = lastEmote.current === emote;
    lastEmote.current = emote;
    if (held.current) {
      // Played again, it starts over at once; a different emote lets the old prop fade away.
      if (again) held.current.dispose();
      else {
        held.current.leave();
        held.leaving.push(held.current);
      }
      held.current = null;
    }
    resetPose(viewer.playerObject);
    if (emote === null) {
      // Still, for editing: nothing moves, but leaving props finish fading.
      viewer.animation = new FunctionAnimation((_player, _progress, delta) => {
        settleTurn(viewer, delta);
        stepProps(viewer, held, delta);
      });
      return;
    }
    const chosen = emoteById(emote);
    if (chosen?.prop) {
      held.current = makeProp(chosen.prop);
      viewer.playerWrapper.add(held.current.object);
    }
    viewer.animation = new FunctionAnimation((player, progress, delta) => {
      applyPose(player, chosen ? chosen.pose(progress) : idlePose(progress));
      settleTurn(viewer, delta);
      stepProps(viewer, held, delta);
    });
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

/** With the turntable stopped, the player turns back to face you. */
function settleTurn(viewer: SkinViewer, delta: number) {
  const wrapper = viewer.playerWrapper;
  if (viewer.autoRotate || wrapper.rotation.y === 0) return;
  const full = 2 * Math.PI;
  const angle = wrapper.rotation.y % full;
  const rest = Math.abs(angle) > Math.PI ? Math.sign(angle) * full : 0;
  const next = angle + (rest - angle) * (1 - Math.exp(-delta * 4));
  wrapper.rotation.y = Math.abs(rest - next) < 0.002 ? 0 : next;
}

/** Moves every prop on a frame, throwing away those that have finished leaving. */
function stepProps(viewer: SkinViewer, held: { current: Prop | null; leaving: Prop[] }, delta: number) {
  // Sparkle sizes: pixels per world unit at distance 1, for this canvas and lens.
  const scale = viewer.renderer.domElement.height / (2 * Math.tan((viewer.camera.fov * Math.PI) / 360));
  if (held.current) {
    held.current.setScale(scale);
    held.current.update(delta);
  }
  held.leaving = held.leaving.filter((prop) => {
    prop.setScale(scale);
    if (prop.update(delta)) return true;
    prop.dispose();
    return false;
  });
}
