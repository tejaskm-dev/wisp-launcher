import { NotebookPen, X } from "lucide-react";
import { m } from "motion/react";
import { useId, useState } from "react";
import { addNote } from "../../data/api";
import { useApp } from "../../state/AppContext";

export interface NoteRequest {
  instanceId: string;
  /** The world's folder and name. */
  world: string;
  worldName: string;
  /** The journal entry of the session it follows. */
  session: string | null;
}

/** After a game closes: "Leave a note for next time", shown on Home's hero when you come back. */
export function NotePrompt({ request, onDone }: { request: NoteRequest; onDone: () => void }) {
  const { notify } = useApp();
  const [text, setText] = useState("");
  const [saving, setSaving] = useState(false);
  const id = useId();

  const save = async () => {
    if (!text.trim()) return onDone();
    setSaving(true);
    try {
      await addNote(request.instanceId, request.world, text, request.session);
      notify({ tone: "success", title: "Note saved", body: `It'll be waiting next time you open ${request.worldName}.` });
      onDone();
    } catch (failure: unknown) {
      setSaving(false);
      notify({ tone: "info", title: "Couldn't save the note", body: failure instanceof Error ? failure.message : String(failure) });
    }
  };

  return (
    <m.section
      aria-labelledby={id}
      initial={{ opacity: 0, y: 16, scale: 0.96 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={{ opacity: 0, x: 28, transition: { duration: 0.2 } }}
      transition={{ type: "spring", stiffness: 380, damping: 30 }}
      className="pointer-events-auto flex flex-col gap-2.5 rounded-2xl border border-cream/10 bg-ink-700/95 p-3.5 shadow-[0_20px_50px_-12px_rgba(0,0,0,0.8)]"
    >
      <div className="flex items-start gap-3">
        <span className="grid size-9 shrink-0 place-items-center rounded-xl bg-cream/8 text-cream-300">
          <NotebookPen size={16} aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <p id={id} className="text-sm font-bold">
            How was {request.worldName}?
          </p>
          <p className="text-xs text-mute">Leave yourself a note for next time.</p>
        </div>
        <button type="button" aria-label="Skip the note" onClick={onDone} className="rounded-lg p-1 text-mute transition-colors hover:bg-cream/8 hover:text-cream">
          <X size={14} aria-hidden="true" />
        </button>
      </div>
      <textarea
        autoFocus
        value={text}
        maxLength={2000}
        rows={2}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) void save();
          if (event.key === "Escape") onDone();
        }}
        placeholder="Finish the bridge, then find the village…"
        aria-label="Note"
        className="w-full resize-none rounded-xl border border-cream/12 bg-ink-900/60 px-3 py-2 font-serif text-[17px] italic leading-snug text-cream placeholder:text-mute-2 focus:border-cream/35 focus:outline-none"
      />
      <div className="flex justify-end gap-2">
        <button type="button" onClick={onDone} className="h-8 rounded-full px-3 text-xs font-bold text-cream-300 transition-colors hover:bg-cream/8 hover:text-cream">
          Not now
        </button>
        <button
          type="button"
          disabled={saving || !text.trim()}
          onClick={() => void save()}
          className="h-8 rounded-full bg-cream px-3.5 text-xs font-bold text-ink-900 transition-opacity disabled:opacity-40"
        >
          Save note
        </button>
      </div>
    </m.section>
  );
}
