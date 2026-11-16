import { NotebookPen, RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";
import { getRestorePointUsage } from "../../data/api";
import { formatBytes } from "../../lib/format";
import { useApp } from "../../state/AppContext";
import { Panel } from "../ui/Page";
import { Toggle } from "../ui/Toggle";
import { Divider, Note, NumberField, Row, type SectionProps } from "./parts";

export function JournalSection({ settings, update }: SectionProps) {
  const { journalVersion } = useApp();
  const journal = settings.journal;
  const set = (patch: Partial<typeof journal>) => update((current) => ({ journal: { ...current.journal, ...patch } }));
  const [usage, setUsage] = useState<number | null>(null);
  useEffect(() => {
    let active = true;
    getRestorePointUsage().then((bytes) => active && setUsage(bytes), () => {});
    return () => {
      active = false;
    };
  }, [journalVersion]);

  return (
    <>
      <Panel
        title="Restore points"
        icon={<RotateCcw size={19} aria-hidden="true" />}
        description="Copies of a world you can go back to, kept like commits: a file that didn't change since the last one isn't stored again."
      >
        <Toggle
          label="Keep a restore point after playing"
          description="When the game closes, Wisp keeps a copy of each world you played. Worlds you didn't open are left alone."
          checked={journal.autoRestorePoints}
          onChange={(autoRestorePoints) => set({ autoRestorePoints })}
        />
        <Divider />
        <Row label="Automatic restore points per world" description="Older ones are deleted. The ones you make yourself are always kept.">
          <NumberField label="Automatic restore points per world" value={journal.keepRestorePoints} min={1} max={100} onCommit={(keepRestorePoints) => set({ keepRestorePoints })} />
        </Row>
        <Note>Restore points use {usage === null ? "…" : formatBytes(usage)} in Wisp's data folder. Delete single ones from a world's page.</Note>
      </Panel>

      <Panel title="Notes" icon={<NotebookPen size={19} aria-hidden="true" />}>
        <Toggle
          label="Ask for a note when the game closes"
          description="A small card asks how it went; what you write shows on Home the next time you pick that world."
          checked={journal.askForNote}
          onChange={(askForNote) => set({ askForNote })}
        />
      </Panel>
    </>
  );
}
