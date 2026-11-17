import { Megaphone } from "lucide-react";
import { m } from "motion/react";
import { getJavaNews } from "../data/api";
import type { PatchNote } from "../data/types";
import { cn } from "../lib/cn";
import { formatDay } from "../lib/format";
import { rise } from "../lib/motion";
import { useResource } from "../lib/useResource";
import { ErrorState } from "./ui/ErrorState";
import { SectionHeader } from "./ui/SectionHeader";
import { Loading, Skeleton } from "./ui/Skeleton";

export function NewsPanel() {
  const [news, retry] = useResource(getJavaNews);

  return (
    <m.section variants={rise} aria-labelledby="news-title" className="flex flex-col gap-3.5">
      <SectionHeader id="news-title" title="Minecraft news" icon={<Megaphone size={18} strokeWidth={2} className="text-ember" aria-hidden="true" />} />

      {news.status === "loading" && (
        <Loading label="Loading Minecraft news" className="flex flex-col gap-2.5">
          <div className="flex gap-3.5 wisp-surface rounded-[18px] p-3.5">
            <Skeleton className="size-[72px] shrink-0 rounded-xl" />
            <div className="flex flex-1 flex-col gap-2 pt-1">
              <Skeleton className="h-3 w-24 rounded-full" />
              <Skeleton className="h-4 w-4/5 rounded-full" />
              <Skeleton className="h-3 w-full rounded-full" />
              <Skeleton className="h-3 w-3/5 rounded-full" />
            </div>
          </div>
          <div className="flex items-center gap-3 wisp-surface rounded-[18px] p-3">
            <Skeleton className="size-10 shrink-0 rounded-lg" />
            <div className="flex flex-1 flex-col gap-2">
              <Skeleton className="h-3 w-20 rounded-full" />
              <Skeleton className="h-3.5 w-3/4 rounded-full" />
            </div>
          </div>
        </Loading>
      )}

      {news.status === "error" && <ErrorState title="Couldn't load news" body="Mojang's news feed didn't respond." onRetry={retry} />}

      {news.status === "ready" && <NewsList notes={news.data} />}
    </m.section>
  );
}

function NewsList({ notes }: { notes: PatchNote[] }) {
  const release = notes.find((note) => note.kind === "release");
  const others = notes.filter((note) => note !== release);
  if (notes.length === 0) {
    return <p className="rounded-[18px] border border-dashed border-cream/15 p-4 text-sm text-mute">No news right now.</p>;
  }

  return (
    <m.div initial="hidden" animate="show" variants={{ hidden: {}, show: { transition: { staggerChildren: 0.08 } } }} className="flex flex-col gap-2.5">
      {release && (
        <m.article variants={item} className="flex gap-3.5 wisp-surface rounded-[18px] p-3.5">
          {release.imageUrl && <img src={release.imageUrl} alt="" loading="lazy" decoding="async" className="size-[72px] shrink-0 rounded-xl object-cover" />}
          <div className="min-w-0 flex-1">
            <NoteMeta note={release} />
            <h3 className="mt-1.5 line-clamp-2 text-[15px] font-bold leading-snug">{release.title}</h3>
            <p className="mt-1 line-clamp-3 text-[13px] leading-relaxed text-mute">{release.summary}</p>
          </div>
        </m.article>
      )}
      {others.map((note) => (
        <m.article key={note.id} variants={item} className="flex items-center gap-3 wisp-surface rounded-[18px] p-3">
          {note.imageUrl && <img src={note.imageUrl} alt="" loading="lazy" decoding="async" className="size-10 shrink-0 rounded-lg object-cover" />}
          <div className="min-w-0 flex-1">
            <NoteMeta note={note} />
            <h3 className="mt-1 truncate text-sm font-bold">{note.title}</h3>
          </div>
        </m.article>
      ))}
    </m.div>
  );
}

const item = { hidden: { opacity: 0, y: 10 }, show: { opacity: 1, y: 0, transition: { duration: 0.45, ease: [0.16, 1, 0.3, 1] as [number, number, number, number] } } };

function NoteMeta({ note }: { note: PatchNote }) {
  const isRelease = note.kind === "release";
  return (
    <p className="flex items-center gap-2 text-xs">
      <span className={cn("rounded-full px-2 py-0.5 font-bold", isRelease ? "bg-moss/12 text-moss-soft" : "bg-sky/12 text-sky")}>
        {isRelease ? "Release" : "Snapshot"}
      </span>
      <span className="text-mute-2">{formatDay(note.date)}</span>
    </p>
  );
}
