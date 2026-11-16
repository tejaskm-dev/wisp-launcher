import { ChevronLeft, ChevronRight, Copy, FolderSearch, Globe, Trash2, X } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { deleteScreenshot, fileUrl, getScreenshotBytes, revealScreenshot, thumbUrl } from "../../data/api";
import type { Screenshot } from "../../data/types";
import { cn } from "../../lib/cn";
import { formatBytes } from "../../lib/format";
import { useApp } from "../../state/AppContext";

interface LightboxProps {
  shots: Screenshot[];
  /** The one showing; null when closed. */
  index: number | null;
  onIndex: (index: number | null) => void;
  /** After a screenshot moved to the Trash. */
  onDeleted?: (shot: Screenshot) => void;
  onOpenWorld?: (worldId: string) => void;
}

/** A screenshot full size, with arrows (and ← →) to move through the rest. */
export function Lightbox({ shots, index, onIndex, onDeleted, onOpenWorld }: LightboxProps) {
  const { notify } = useApp();
  const shot = index !== null ? shots[index] : undefined;
  const [confirming, setConfirming] = useState(false);
  const [loaded, setLoaded] = useState<string | null>(null);

  useEffect(() => setConfirming(false), [index]);
  useEffect(() => {
    if (index === null) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onIndex(null);
      else if (event.key === "ArrowRight") onIndex(Math.min(shots.length - 1, index + 1));
      else if (event.key === "ArrowLeft") onIndex(Math.max(0, index - 1));
      else return;
      event.preventDefault();
      event.stopPropagation();
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [index, shots.length, onIndex]);

  const fail = (title: string) => (failure: unknown) => notify({ tone: "info", title, body: failure instanceof Error ? failure.message : String(failure) });

  const copy = () => {
    if (!shot) return;
    // WebKit only lets the click write to the clipboard if the write starts right away, so the
    // picture goes in as a promise and arrives when it's read.
    const png = getScreenshotBytes(shot.instanceId, shot.file).then((bytes) => (isPng(bytes) ? new Blob([bytes], { type: "image/png" }) : toPng(new Blob([bytes]))));
    try {
      navigator.clipboard.write([new ClipboardItem({ "image/png": png })]).then(
        () => notify({ tone: "success", title: "Copied", body: "Paste it anywhere." }),
        fail("Couldn't copy the picture"),
      );
    } catch (failure: unknown) {
      fail("Couldn't copy the picture")(failure);
    }
  };

  const remove = async () => {
    if (!shot) return;
    try {
      await deleteScreenshot(shot.instanceId, shot.file);
      notify({ tone: "success", title: "Moved to the Trash", body: shot.file });
      onDeleted?.(shot);
      onIndex(shots.length <= 1 ? null : Math.min(index ?? 0, shots.length - 2));
    } catch (failure: unknown) {
      fail("Couldn't delete it")(failure);
    }
  };

  return createPortal(
    <AnimatePresence>
      {shot && index !== null && (
        <m.div
          role="dialog"
          aria-modal="true"
          aria-label={`Screenshot ${index + 1} of ${shots.length}`}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
          className="fixed inset-0 z-[80] flex flex-col bg-ink-950/95 backdrop-blur-md"
        >
          <header className="flex items-center gap-3 px-5 pb-3 pt-4">
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-bold">
                {new Date(shot.takenAt).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" })}
              </p>
              <p className="flex items-center gap-1.5 truncate text-xs text-mute">
                {shot.instanceName}
                {shot.world && (
                  <>
                    <span aria-hidden="true">·</span>
                    {onOpenWorld && shot.worldFolder ? (
                      <button
                        type="button"
                        onClick={() => {
                          onIndex(null);
                          onOpenWorld(`${shot.instanceId}/${shot.worldFolder}`);
                        }}
                        className="flex items-center gap-1 font-semibold text-cream-300 underline decoration-cream/25 underline-offset-4 hover:text-cream"
                      >
                        <Globe size={11} aria-hidden="true" />
                        {shot.world}
                      </button>
                    ) : (
                      shot.world
                    )}
                  </>
                )}
                {shot.width && shot.height && (
                  <>
                    <span aria-hidden="true">·</span>
                    {shot.width} × {shot.height}
                  </>
                )}
                {shot.sizeBytes > 0 && (
                  <>
                    <span aria-hidden="true">·</span>
                    {formatBytes(shot.sizeBytes)}
                  </>
                )}
              </p>
            </div>
            <span className="text-xs font-semibold tabular-nums text-mute">
              {index + 1} / {shots.length}
            </span>
            <ToolButton label="Copy" icon={<Copy size={15} aria-hidden="true" />} onClick={copy} />
            <ToolButton
              label="Show in folder"
              icon={<FolderSearch size={15} aria-hidden="true" />}
              onClick={() => void revealScreenshot(shot.instanceId, shot.file).catch(fail("Couldn't show it"))}
            />
            {confirming ? (
              <span className="flex items-center gap-1 rounded-full bg-coral/10 py-1 pl-3 pr-1 text-xs font-bold text-coral">
                Move to the Trash?
                <button type="button" onClick={() => void remove()} className="rounded-full bg-coral px-2.5 py-1 text-ink-900">
                  Delete
                </button>
                <button type="button" onClick={() => setConfirming(false)} className="rounded-full px-2 py-1 text-cream-300 hover:bg-cream/8">
                  Keep
                </button>
              </span>
            ) : (
              <ToolButton label="Delete" icon={<Trash2 size={15} aria-hidden="true" />} onClick={() => setConfirming(true)} />
            )}
            <button type="button" aria-label="Close" onClick={() => onIndex(null)} className="grid size-10 place-items-center rounded-xl text-mute hover:bg-cream/8 hover:text-cream">
              <X size={20} aria-hidden="true" />
            </button>
          </header>

          <div className="relative flex min-h-0 flex-1 items-center justify-center px-20 pb-8">
            {/* The thumbnail shows at once; the full picture fades in over it. */}
            <div className="relative flex max-h-full max-w-full items-center justify-center">
              <img src={thumbUrl(shot.path, 960, shot.takenAt)} alt="" aria-hidden="true" className="max-h-[calc(100vh-9rem)] max-w-full rounded-xl object-contain" />
              <img
                key={shot.id}
                src={fileUrl(shot.path, shot.takenAt)}
                alt={`Screenshot from ${shot.world ?? shot.instanceName}`}
                onLoad={() => setLoaded(shot.id)}
                className={cn(
                  "absolute inset-0 m-auto max-h-[calc(100vh-9rem)] max-w-full rounded-xl object-contain shadow-[0_30px_100px_-20px_rgba(0,0,0,0.9)] transition-opacity duration-300",
                  loaded === shot.id ? "opacity-100" : "opacity-0",
                )}
              />
            </div>
            <NavButton side="left" disabled={index === 0} onClick={() => onIndex(index - 1)} />
            <NavButton side="right" disabled={index === shots.length - 1} onClick={() => onIndex(index + 1)} />
          </div>
        </m.div>
      )}
    </AnimatePresence>,
    document.body,
  );
}

function ToolButton({ label, icon, onClick }: { label: string; icon: React.ReactNode; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} className="flex h-9 items-center gap-2 rounded-full border border-cream/12 px-3.5 text-xs font-bold text-cream-300 transition-colors hover:border-cream/30 hover:bg-cream/8 hover:text-cream">
      {icon}
      {label}
    </button>
  );
}

function NavButton({ side, disabled, onClick }: { side: "left" | "right"; disabled: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      aria-label={side === "left" ? "Previous screenshot" : "Next screenshot"}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "absolute top-1/2 grid size-12 -translate-y-1/2 place-items-center rounded-full bg-ink-800/80 text-cream ring-1 ring-cream/10 transition-[opacity,background-color] hover:bg-ink-700 disabled:pointer-events-none disabled:opacity-0",
        side === "left" ? "left-5" : "right-5",
      )}
    >
      {side === "left" ? <ChevronLeft size={22} aria-hidden="true" /> : <ChevronRight size={22} aria-hidden="true" />}
    </button>
  );
}

const PNG_SIGNATURE = [0x89, 0x50, 0x4e, 0x47];
const isPng = (bytes: ArrayBuffer) => new Uint8Array(bytes.slice(0, 4)).every((byte, index) => byte === PNG_SIGNATURE[index]);

/** The clipboard only takes PNGs; JPEG screenshots get converted first. */
async function toPng(blob: Blob): Promise<Blob> {
  const bitmap = await createImageBitmap(blob);
  const canvas = document.createElement("canvas");
  canvas.width = bitmap.width;
  canvas.height = bitmap.height;
  canvas.getContext("2d")?.drawImage(bitmap, 0, 0);
  return new Promise((resolve, reject) => canvas.toBlob((png) => (png ? resolve(png) : reject(new Error("couldn't convert it"))), "image/png"));
}

/** A Screenshot record for a file the journal mentions (it may have been deleted since). */
export function shotFromJournal(instanceId: string, instanceName: string, file: string, at: number, world: { name: string; folder: string } | null): Screenshot {
  return {
    id: `${instanceId}/${file}`,
    instanceId,
    instanceName,
    file,
    path: `instances/${instanceId}/minecraft/screenshots/${file}`,
    takenAt: at,
    sizeBytes: 0,
    width: null,
    height: null,
    world: world?.name ?? null,
    worldFolder: world?.folder ?? null,
  };
}
