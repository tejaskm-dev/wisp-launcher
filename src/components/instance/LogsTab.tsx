import { Check, Copy, FileWarning, FileText, FolderOpen, Search } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { getInstanceLogs, openInstanceFolder, readInstanceLog } from "../../data/api";
import type { Instance, LogFile } from "../../data/types";
import { cn } from "../../lib/cn";
import { formatBytes, timeAgo } from "../../lib/format";
import { useApp } from "../../state/AppContext";
import { Sparkle } from "../brand/Sparkle";
import { EmptyState } from "../ui/EmptyState";
import { Loading, Skeleton } from "../ui/Skeleton";
import { message } from "./shared";

/** Lines shown at most: the end of the log, where what went wrong usually is. */
const MAX_LINES = 5000;
const PROBLEM = /\b(ERROR|FATAL|Exception|Caused by|Error:)/;
const WARNING = /\bWARN\b/;

function logTitle(log: LogFile, now: number): string {
  if (log.kind === "latest") return "The last time it ran";
  if (log.kind === "crash") return `Crash ${timeAgo(log.modified, now)}`;
  return log.name.replace(/^logs\//, "").replace(/\.log(\.gz)?$/, "");
}

/** The game's logs and crash reports, readable here: what went wrong, without digging for files. */
export function LogsTab({ instance }: { instance: Instance }) {
  const { now, journalVersion } = useApp();
  const [logs, setLogs] = useState<LogFile[] | null>(null);
  const [current, setCurrent] = useState<string | null>(null);
  const [text, setText] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [find, setFind] = useState("");
  const [problemsOnly, setProblemsOnly] = useState(false);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let live = true;
    getInstanceLogs(instance.id).then(
      (found) => {
        if (!live) return;
        setLogs(found);
        setCurrent((selected) => (selected && found.some((log) => log.name === selected) ? selected : (found[0]?.name ?? null)));
      },
      () => live && setLogs([]),
    );
    return () => {
      live = false;
    };
  }, [instance.id, journalVersion]);

  useEffect(() => {
    if (!current) return;
    let live = true;
    setText(null);
    setFailed(null);
    readInstanceLog(instance.id, current).then(
      (read) => live && setText(read),
      (failure: unknown) => live && setFailed(message(failure)),
    );
    return () => {
      live = false;
    };
  }, [instance.id, current]);

  const lines = useMemo(() => (text ?? "").split("\n").slice(-MAX_LINES), [text]);
  const needle = find.trim().toLowerCase();
  const shown = useMemo(
    () => lines.map((line, index) => ({ line, index })).filter(({ line }) => (!problemsOnly || PROBLEM.test(line) || WARNING.test(line)) && (!needle || line.toLowerCase().includes(needle))),
    [lines, problemsOnly, needle],
  );
  const problems = useMemo(() => lines.filter((line) => PROBLEM.test(line)).length, [lines]);

  if (logs === null) {
    return (
      <Loading label="Loading logs" className="grid gap-4 md:grid-cols-[240px_minmax(0,1fr)]">
        <Skeleton className="h-48 rounded-2xl" />
        <Skeleton className="h-96 rounded-2xl" />
      </Loading>
    );
  }
  if (logs.length === 0) {
    return <EmptyState art={<FileText size={36} className="text-mute" aria-hidden="true" />} title="No logs yet" body="Once the game has run, its log and any crash reports show up here." />;
  }

  const groups: { title: string; items: LogFile[] }[] = [
    { title: "Latest", items: logs.filter((log) => log.kind === "latest") },
    { title: "Crash reports", items: logs.filter((log) => log.kind === "crash") },
    { title: "Earlier", items: logs.filter((log) => log.kind === "older") },
  ].filter((group) => group.items.length > 0);
  const selected = logs.find((log) => log.name === current) ?? null;

  return (
    <div className="grid items-start gap-4 md:grid-cols-[240px_minmax(0,1fr)]">
      <nav aria-label="Logs" className="flex flex-col gap-4 rounded-[20px] border border-cream/7 bg-ink-800 p-3">
        {groups.map((group) => (
          <div key={group.title} className="flex flex-col gap-1">
            <p className="px-2 text-xs font-bold text-mute-2">{group.title}</p>
            {group.items.map((log) => (
              <button
                key={log.name}
                type="button"
                onClick={() => setCurrent(log.name)}
                aria-current={log.name === current ? "true" : undefined}
                className={cn(
                  "flex items-center gap-2 rounded-xl px-2.5 py-2 text-left text-[13px] font-semibold transition-colors",
                  log.name === current ? "bg-cream text-ink-900" : "text-cream-300 hover:bg-cream/6 hover:text-cream",
                )}
              >
                {log.kind === "crash" ? <FileWarning size={14} className={log.name === current ? "" : "text-coral"} aria-hidden="true" /> : <FileText size={14} aria-hidden="true" />}
                <span className="min-w-0 flex-1 truncate">{logTitle(log, now)}</span>
              </button>
            ))}
          </div>
        ))}
      </nav>

      <section aria-label={selected ? logTitle(selected, now) : "Log"} className="flex min-w-0 flex-col gap-3 rounded-[20px] border border-cream/7 bg-ink-800 p-4">
        <div className="flex flex-wrap items-center gap-2.5">
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-bold">{selected ? logTitle(selected, now) : ""}</p>
            {selected && (
              <p className="truncate font-mono text-[11px] text-mute-2">
                {selected.name} · {formatBytes(selected.size)} · {timeAgo(selected.modified, now)}
              </p>
            )}
          </div>
          <label className="flex h-9 w-[200px] items-center gap-2 rounded-full border border-cream/12 bg-ink-900/40 px-3 focus-within:border-cream/35">
            <Search size={13} className="shrink-0 text-mute" aria-hidden="true" />
            <span className="sr-only">Find in the log</span>
            <input value={find} onChange={(event) => setFind(event.target.value)} placeholder="Find" spellCheck={false} className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-mute-2" />
          </label>
          <button
            type="button"
            role="switch"
            aria-checked={problemsOnly}
            onClick={() => setProblemsOnly((only) => !only)}
            className={cn(
              "flex h-9 items-center gap-1.5 rounded-full border px-3.5 text-[13px] font-bold transition-colors",
              problemsOnly ? "border-cream bg-cream text-ink-900" : "lit border-cream/12 text-cream-300 hover:border-cream/30 hover:text-cream",
            )}
          >
            {problemsOnly && <Sparkle className="-ml-0.5 size-2.5 text-ember" />}
            Problems only{problems > 0 && <span className={cn("tabular-nums", problemsOnly ? "text-ink-900/60" : "text-coral")}>{problems}</span>}
          </button>
          <button
            type="button"
            disabled={!text}
            onClick={() => text && void navigator.clipboard.writeText(text).then(() => (setCopied(true), setTimeout(() => setCopied(false), 1600)))}
            aria-label="Copy the log"
            title="Copy the log"
            className="grid size-9 place-items-center rounded-full border border-cream/12 text-cream-300 transition-colors hover:border-cream/30 hover:text-cream disabled:opacity-40"
          >
            {copied ? <Check size={14} className="text-moss-soft" aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
          </button>
          <button
            type="button"
            onClick={() => void openInstanceFolder(instance.id)}
            aria-label="Open the game folder"
            title="Open the game folder"
            className="grid size-9 place-items-center rounded-full border border-cream/12 text-cream-300 transition-colors hover:border-cream/30 hover:text-cream"
          >
            <FolderOpen size={14} aria-hidden="true" />
          </button>
        </div>
        {failed ? (
          <p className="text-sm text-coral">{failed}</p>
        ) : text === null ? (
          <Skeleton className="h-96 w-full rounded-xl" />
        ) : (
          <>
            {lines.length >= MAX_LINES && <p className="text-xs text-mute-2">Showing its last {MAX_LINES.toLocaleString()} lines.</p>}
            <pre className="max-h-[62vh] overflow-auto rounded-xl bg-ink-950 p-3 font-mono text-[11.5px] leading-[1.6]">
              {shown.length === 0 ? (
                <span className="text-mute">Nothing matches.</span>
              ) : (
                shown.map(({ line, index }) => (
                  <div key={index} className={cn("whitespace-pre-wrap break-all", PROBLEM.test(line) ? "text-coral" : WARNING.test(line) ? "text-ember-soft" : "text-cream-300")}>
                    {line || " "}
                  </div>
                ))
              )}
            </pre>
          </>
        )}
      </section>
    </div>
  );
}
