import { Check, ClipboardCopy, FileDown, ImagePlus, RotateCcw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { getSkinTemplate, saveSkinTemplate, skinFromPainting } from "../../data/api";
import type { SkinModel, SkinTemplate } from "../../data/types";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Segmented } from "../ui/Segmented";
import { SkinDoll } from "./SkinDoll";

const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure));
const asPng = (base64: string) => `data:image/png;base64,${base64}`;

type Keep = (png: string, model: SkinModel, name: string) => Promise<void>;

/** Reads a picture file as a data URL. */
function readFile(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => (typeof reader.result === "string" ? resolve(reader.result) : reject(new Error("That picture didn't come through.")));
    reader.onerror = () => reject(new Error("That picture didn't come through."));
    reader.readAsDataURL(file);
  });
}

/** Calls `take` with a picture pasted while `active`. */
function usePastedPicture(active: boolean, take: (file: File) => void) {
  const latest = useRef(take);
  latest.current = take;
  useEffect(() => {
    if (!active) return;
    const paste = (event: ClipboardEvent) => {
      const file = [...(event.clipboardData?.items ?? [])].find((item) => item.type.startsWith("image/"))?.getAsFile();
      if (file) {
        event.preventDefault();
        latest.current(file);
      }
    };
    window.addEventListener("paste", paste);
    return () => window.removeEventListener("paste", paste);
  }, [active]);
}

/**
 * Makes a skin from any character with an image tool the player already uses (Gemini, ChatGPT):
 * Wisp gives it a template and a prompt, then reads its painting back into a skin.
 */
export function SkinMakerDialog({ open, onClose, onKeep }: { open: boolean; onClose: () => void; onKeep: Keep }) {
  const [build, setBuild] = useState<SkinModel>("classic");
  const [name, setName] = useState("My character");
  return (
    <Dialog
      open={open}
      onClose={onClose}
      wide
      title="Make a skin"
      description="From any character: your own, fan art, a sketch. Describe the details that matter and Wisp makes the skin."
    >
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center gap-3">
          <Segmented<SkinModel>
            label="Arms"
            value={build}
            onChange={setBuild}
            options={[
              { value: "classic", label: "Classic arms" },
              { value: "slim", label: "Slim arms" },
            ]}
          />
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            aria-label="Skin name"
            className="h-9 min-w-[140px] flex-1 rounded-xl border border-cream/12 bg-ink-900/60 px-3 text-sm text-cream outline-none focus:border-cream/35"
          />
        </div>
        <WithTool open={open} build={build} name={name} onKeep={onKeep} onDone={onClose} />
      </div>
    </Dialog>
  );
}

function Step({ number, title, children }: { number: number; title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-3 rounded-2xl bg-cream/[0.04] p-4">
      <h3 className="flex items-center gap-2 font-display text-base font-bold">
        <span className="grid size-6 place-items-center rounded-full bg-cream/10 text-xs">{number}</span>
        {title}
      </h3>
      {children}
    </section>
  );
}

/** The image-tool way: template and prompt out, painting in, skin out. */
function WithTool({ open, build, name, onKeep, onDone }: { open: boolean; build: SkinModel; name: string; onKeep: Keep; onDone: () => void }) {
  const { notify } = useApp();
  const [template, setTemplate] = useState<SkinTemplate | null>(null);
  const [painting, setPainting] = useState<string | null>(null);
  const [outerPainting, setOuterPainting] = useState<string | null>(null);
  const [skin, setSkin] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  const [readingOuter, setReadingOuter] = useState(false);
  const [keeping, setKeeping] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const outerInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    getSkinTemplate(build).then(setTemplate, (failure: unknown) => notify({ tone: "info", title: "Couldn't make the template", body: message(failure) }));
  }, [open, build, notify]);

  const copyText = async (text: string | undefined, title: string) => {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      notify({ tone: "success", title });
    } catch {
      notify({ tone: "info", title: "Couldn't copy it", body: "Select the text and copy it yourself." });
    }
  };

  const copyTemplate = async () => {
    if (!template) return;
    try {
      const blob = await (await fetch(asPng(template.png))).blob();
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
      notify({ tone: "success", title: "Copied the template", body: "Paste it into your image tool next to your character." });
    } catch {
      notify({ tone: "info", title: "Couldn't copy the picture", body: "Save the template instead and attach the file." });
    }
  };

  const save = async () => {
    try {
      const where = await saveSkinTemplate(build);
      if (where) notify({ tone: "success", title: "Saved the template", body: where });
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't save it", body: message(failure) });
    }
  };

  const take = async (file: File) => {
    if (!file.type.startsWith("image/")) {
      notify({ tone: "info", title: "That isn't a picture", body: "Choose the PNG or JPEG your image tool made." });
      return;
    }
    setReading(true);
    setSkin(null);
    try {
      const picture = await readFile(file);
      setPainting(picture);
      setSkin(asPng(await skinFromPainting(picture, build)));
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't read that painting", body: message(failure) });
    } finally {
      setReading(false);
    }
  };
  // The follow-up painting: the same picture with only what sits on top, read as the outer layer.
  const takeOuter = async (file: File) => {
    if (!painting) return;
    if (!file.type.startsWith("image/")) {
      notify({ tone: "info", title: "That isn't a picture", body: "Choose the PNG or JPEG your image tool made." });
      return;
    }
    setReadingOuter(true);
    try {
      const picture = await readFile(file);
      setSkin(asPng(await skinFromPainting(painting, build, picture)));
      setOuterPainting(picture);
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't read the outer layer", body: message(failure) });
    } finally {
      setReadingOuter(false);
    }
  };
  usePastedPicture(open && !reading && !readingOuter, (file) => void (skin ? takeOuter(file) : take(file)));

  const keep = async () => {
    if (!skin) return;
    setKeeping(true);
    try {
      await onKeep(skin.replace("data:image/png;base64,", ""), build, name.trim() || "My character");
      onDone();
    } finally {
      setKeeping(false);
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <input ref={fileInput} type="file" accept="image/png,image/jpeg,image/webp" hidden onChange={(event) => (event.target.files?.[0] && void take(event.target.files[0]), (event.target.value = ""))} />
      <input ref={outerInput} type="file" accept="image/png,image/jpeg,image/webp" hidden onChange={(event) => (event.target.files?.[0] && void takeOuter(event.target.files[0]), (event.target.value = ""))} />
      <Step number={1} title="Give your image tool your character, this template and the prompt">
        <p className="text-xs leading-relaxed text-mute">
          Gemini paints these best, its Pro image model most reliably: it keeps the template's layout to the square. ChatGPT and other image tools that take pictures work too. Attach a picture of your
          character first and this template second, then paste the prompt. Add the details that matter at the end of the prompt, like “the right arm is red, the left is black, the head is a red belt with the buckle as the left eye”.
        </p>
        <div className="grid gap-3 sm:grid-cols-[200px_minmax(0,1fr)]">
          <div className="flex flex-col gap-2">
            {template ? (
              <img src={asPng(template.png)} alt="The template: a blank character from the front, right, back and left" className="w-full rounded-xl" />
            ) : (
              <div className="aspect-[3/2] w-full rounded-xl bg-ink-900/60" />
            )}
            <div className="flex gap-2">
              <Button variant="secondary" icon={ClipboardCopy} onClick={() => void copyTemplate()} disabled={!template} className="h-9 flex-1 px-3 text-xs">
                Copy
              </Button>
              <Button variant="secondary" icon={FileDown} onClick={() => void save()} disabled={!template} className="h-9 flex-1 px-3 text-xs">
                Save
              </Button>
            </div>
          </div>
          <div className="flex min-w-0 flex-col gap-2">
            <textarea
              readOnly
              value={template?.prompt ?? ""}
              aria-label="The prompt"
              className="h-[150px] w-full resize-none rounded-xl border border-cream/12 bg-ink-900/60 p-3 text-xs leading-relaxed text-cream-300 outline-none"
            />
            <Button variant="secondary" icon={ClipboardCopy} onClick={() => void copyText(template?.prompt, "Copied the prompt")} disabled={!template} className="h-9 self-start px-3 text-xs">
              Copy the prompt
            </Button>
          </div>
        </div>
      </Step>
      <Step number={2} title="Bring back what it painted">
        {!skin ? (
          <button
            type="button"
            onClick={() => fileInput.current?.click()}
            disabled={reading}
            className="flex min-h-[110px] flex-col items-center justify-center gap-2 rounded-2xl border-[1.5px] border-dashed border-cream/18 px-6 text-center transition-colors hover:border-cream/40 disabled:opacity-60"
          >
            <ImagePlus size={24} className="text-cream-300" aria-hidden="true" />
            <span className="text-sm font-bold">{reading ? "Reading the painting…" : "Choose the picture it made, or paste it"}</span>
            <span className="text-xs text-mute">Wisp reads the four views back into a skin, square by square.</span>
          </button>
        ) : (
          <div className="flex flex-wrap items-center gap-4">
            <div className="grid h-[200px] w-[120px] place-items-center rounded-2xl bg-ink-900/50">
              <SkinDoll src={skin} slim={build === "slim"} className="h-[170px] w-[85px] drop-shadow-[0_10px_12px_rgba(0,0,0,0.5)]" />
            </div>
            {painting && <img src={painting} alt="What the image tool painted" className="h-[120px] rounded-xl object-contain" />}
            <div className="ml-auto flex gap-2">
              <Button variant="secondary" icon={RotateCcw} onClick={() => (setSkin(null), setPainting(null), setOuterPainting(null))}>
                Try another
              </Button>
              <Button icon={Check} onClick={() => void keep()} disabled={keeping}>
                {keeping ? "Keeping…" : "Keep this skin"}
              </Button>
            </div>
          </div>
        )}
      </Step>
      {skin && (
        <Step number={3} title="For a proper outer layer, ask for it too (optional)">
          <p className="text-xs leading-relaxed text-mute">
            Skins have a second layer for what stands out: hair, a hood, glasses, a jacket, boots. Send this follow-up in the same chat. The image tool keeps only what sits on top and paints the rest magenta,
            and Wisp makes that the outer layer. Skip it and Wisp works the outer layer out itself.
          </p>
          <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_200px]">
            <div className="flex min-w-0 flex-col gap-2">
              <textarea
                readOnly
                value={template?.outerPrompt ?? ""}
                aria-label="The follow-up prompt"
                className="h-[96px] w-full resize-none rounded-xl border border-cream/12 bg-ink-900/60 p-3 text-xs leading-relaxed text-cream-300 outline-none"
              />
              <Button variant="secondary" icon={ClipboardCopy} onClick={() => void copyText(template?.outerPrompt, "Copied the follow-up")} disabled={!template} className="h-9 self-start px-3 text-xs">
                Copy the follow-up
              </Button>
            </div>
            <button
              type="button"
              onClick={() => outerInput.current?.click()}
              disabled={readingOuter}
              className="flex min-h-[110px] flex-col items-center justify-center gap-2 rounded-2xl border-[1.5px] border-dashed border-cream/18 px-4 text-center transition-colors hover:border-cream/40 disabled:opacity-60"
            >
              {outerPainting ? (
                <img src={outerPainting} alt="What the image tool kept for the outer layer" className="h-[64px] rounded-lg object-contain" />
              ) : (
                <ImagePlus size={22} className="text-cream-300" aria-hidden="true" />
              )}
              <span className="text-xs font-bold">{readingOuter ? "Reading the outer layer…" : outerPainting ? "Outer layer added. Choose another" : "Choose what it painted, or paste it"}</span>
            </button>
          </div>
        </Step>
      )}
    </div>
  );
}
