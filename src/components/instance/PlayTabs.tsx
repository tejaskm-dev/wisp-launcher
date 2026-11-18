import { FolderInput, Globe, Images, LoaderCircle, Play, Plus, Trash2 } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { addNote, addServer, getScreenshots, removeServer, thumbUrl } from "../../data/api";
import type { Instance, Screenshot } from "../../data/types";
import { cn } from "../../lib/cn";
import { timeAgo } from "../../lib/format";
import { useApp } from "../../state/AppContext";
import { NoteComposer, Timeline } from "../journal/Timeline";
import { useJournal } from "../journal/useJournal";
import { Lightbox } from "../screenshots/Lightbox";
import { playersLabel, useServerStatuses } from "../servers/useServerStatuses";
import { Button } from "../ui/Button";
import { EmptyState } from "../ui/EmptyState";
import { Loading, Skeleton } from "../ui/Skeleton";
import { WorldThumb, worldMode } from "../worlds/WorldArt";
import { message } from "./shared";

/** An instance's worlds, newest played first: open one's page, or play straight into it. */
export function WorldsTab({ instance, locked }: { instance: Instance; locked: boolean }) {
  const { home, now, openWorld, play, importWorlds } = useApp();
  const worlds = useMemo(() => (home?.worlds ?? []).filter((world) => world.instanceId === instance.id).sort((a, b) => b.lastPlayed - a.lastPlayed), [home?.worlds, instance.id]);

  if (home === null) {
    return (
      <Loading label="Loading worlds" className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
        {[0, 1, 2].map((index) => (
          <Skeleton key={index} className="h-52 rounded-[20px]" />
        ))}
      </Loading>
    );
  }
  if (worlds.length === 0) {
    return (
      <EmptyState
        title="No worlds yet"
        body={`Worlds you make in ${instance.name} show up here, each with its own journal and restore points.`}
        actions={
          <Button variant="secondary" icon={FolderInput} onClick={() => importWorlds()}>
            Bring in a world
          </Button>
        }
      />
    );
  }
  return (
    <ul className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
      {worlds.map((world) => (
        <li key={world.id} className="group relative overflow-hidden rounded-[20px] border border-cream/7 bg-ink-800 transition-colors hover:border-cream/15">
          <button type="button" onClick={() => openWorld(world.id)} className="block w-full text-left">
            <WorldThumb world={world} className="aspect-[16/9] w-full" />
            <div className="px-4 pb-4 pt-3">
              <p className="truncate font-display text-base font-bold">{world.name}</p>
              <p className="mt-0.5 truncate text-xs text-mute">
                {worldMode(world)} · played {timeAgo(world.lastPlayed, now)}
              </p>
            </div>
          </button>
          <button
            type="button"
            disabled={locked}
            onClick={() => play(instance.id, world.folder)}
            aria-label={`Play ${world.name}`}
            className="tail wag absolute right-3 top-3 flex h-9 items-center gap-1.5 rounded-full bg-cream px-3.5 text-xs font-bold text-ink-900 opacity-0 shadow-lg transition-[opacity,transform] duration-200 [--tail:15px] group-hover:opacity-100 focus-visible:opacity-100 disabled:hidden active:scale-95"
          >
            <Play size={11} fill="currentColor" strokeWidth={0} aria-hidden="true" />
            Play
          </button>
        </li>
      ))}
    </ul>
  );
}

/** Screenshots taken in an instance, newest first; one opens full size. */
export function ScreenshotsTab({ instance }: { instance: Instance }) {
  const { journalVersion, openWorld } = useApp();
  const [shots, setShots] = useState<Screenshot[] | null>(null);
  const [open, setOpen] = useState<number | null>(null);

  useEffect(() => {
    let live = true;
    getScreenshots().then(
      (all) => live && setShots(all.filter((shot) => shot.instanceId === instance.id).sort((a, b) => b.takenAt - a.takenAt)),
      () => live && setShots([]),
    );
    return () => {
      live = false;
    };
  }, [instance.id, journalVersion]);

  if (shots === null) {
    return (
      <Loading label="Loading screenshots" className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
        {[0, 1, 2, 3].map((index) => (
          <Skeleton key={index} className="aspect-video rounded-2xl" />
        ))}
      </Loading>
    );
  }
  if (shots.length === 0) {
    return <EmptyState art={<Images size={36} className="text-mute" aria-hidden="true" />} title="No screenshots yet" body="Press F2 while you play and the screenshot lands here, with the world it was taken in." />;
  }
  return (
    <>
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
        {shots.map((shot, index) => (
          <li key={shot.id}>
            <button
              type="button"
              onClick={() => setOpen(index)}
              aria-label={`Screenshot from ${timeAgo(shot.takenAt)}${shot.world ? ` in ${shot.world}` : ""}`}
              className="group relative block aspect-video w-full overflow-hidden rounded-2xl bg-ink-800"
            >
              <img src={thumbUrl(shot.path, 480)} alt="" loading="lazy" decoding="async" className="size-full object-cover transition-transform duration-500 ease-out-expo group-hover:scale-[1.04]" />
              {shot.world && <span className="absolute bottom-2 left-2 max-w-[80%] truncate rounded-full bg-ink-900/70 px-2.5 py-1 text-[11px] font-bold backdrop-blur">{shot.world}</span>}
            </button>
          </li>
        ))}
      </ul>
      <Lightbox
        shots={shots}
        index={open}
        onIndex={setOpen}
        onDeleted={(gone) => setShots((current) => (current ?? []).filter((shot) => shot.id !== gone.id))}
        onOpenWorld={(worldId) => {
          setOpen(null);
          openWorld(worldId);
        }}
      />
    </>
  );
}

/** What happened in an instance: sessions, notes, what was added, fixes; and a note of your own. */
export function JournalTab({ instance }: { instance: Instance }) {
  const { home, now, openWorld } = useApp();
  const journal = useJournal({ instanceId: instance.id });
  return (
    <div className="flex max-w-[860px] flex-col gap-4">
      <NoteComposer placeholder={`A note about ${instance.name}…`} autoFocus={false} onSave={async (text) => void (await addNote(instance.id, null, text))} />
      {journal.failed ? (
        <EmptyState title="Couldn't read the journal" body="Something went wrong reading it." actions={<Button onClick={journal.retry}>Try again</Button>} />
      ) : journal.entries === null ? (
        <Loading label="Loading the journal" className="flex flex-col gap-3">
          <Skeleton className="h-20 w-full rounded-2xl" />
          <Skeleton className="h-14 w-full rounded-2xl" />
        </Loading>
      ) : journal.entries.length === 0 ? (
        <p className="rounded-2xl border border-dashed border-cream/12 px-5 py-8 text-center text-sm leading-relaxed text-mute">
          Nothing here yet. Play it and Wisp writes down each session, the worlds you played and the screenshots you took.
        </p>
      ) : (
        <>
          <Timeline entries={journal.entries} home={home} now={now} lanes={false} onOpenWorld={openWorld} />
          {journal.more && (
            <Button variant="secondary" disabled={journal.loadingMore} onClick={journal.loadMore} className="self-center">
              {journal.loadingMore ? "Loading…" : "Older entries"}
            </Button>
          )}
        </>
      )}
    </div>
  );
}

/** The servers in an instance's multiplayer list: join one, remove one, or add one. */
export function ServersTab({ instance, locked }: { instance: Instance; locked: boolean }) {
  const { home, play, notify, refreshHome, now } = useApp();
  const servers = useMemo(() => (home?.servers ?? []).filter((server) => server.instanceIds.includes(instance.id)), [home?.servers, instance.id]);
  const { statuses } = useServerStatuses(servers.map((server) => server.address));
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [busy, setBusy] = useState<string | null>(null);

  const add = async (event: FormEvent) => {
    event.preventDefault();
    if (!address.trim()) return;
    setBusy("add");
    try {
      await addServer(instance.id, name.trim() || address.trim(), address.trim());
      setName("");
      setAddress("");
      await refreshHome();
    } catch (failure) {
      notify({ tone: "info", title: "Couldn't add it", body: message(failure) });
    } finally {
      setBusy(null);
    }
  };
  const remove = async (target: string) => {
    setBusy(target);
    try {
      await removeServer(instance.id, target);
      await refreshHome();
    } catch (failure) {
      notify({ tone: "info", title: "Couldn't remove it", body: message(failure) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="flex max-w-[860px] flex-col gap-4">
      {servers.length === 0 ? (
        <p className="rounded-2xl border border-dashed border-cream/12 px-5 py-8 text-center text-sm leading-relaxed text-mute">
          No servers in its multiplayer list yet. Add one below, or join one in the game and it shows up here.
        </p>
      ) : (
        <ul className="flex flex-col gap-2">
          {servers.map((server) => {
            const status = statuses[server.address];
            return (
              <li key={server.address} className="flex items-center gap-3.5 rounded-2xl border border-cream/6 bg-ink-800 px-3.5 py-3">
                {server.icon ? (
                  <img src={server.icon} alt="" className="size-11 shrink-0 rounded-xl [image-rendering:pixelated]" />
                ) : (
                  <span className="grid size-11 shrink-0 place-items-center rounded-xl bg-cream/6 text-cream-300">
                    <Globe size={18} aria-hidden="true" />
                  </span>
                )}
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[15px] font-bold">{server.name}</p>
                  <p className="mt-0.5 truncate text-xs text-mute">
                    <span className="font-mono">{server.address}</span>
                    {playersLabel(status) && <span className={cn(status?.online ? "text-moss-soft" : "text-mute-2")}> · {playersLabel(status)}</span>}
                    {server.lastPlayed !== null && <span> · joined {timeAgo(server.lastPlayed, now)}</span>}
                  </p>
                </div>
                <button
                  type="button"
                  disabled={locked}
                  onClick={() => play(instance.id, null, server.address)}
                  className="tail wag relative flex h-9 items-center gap-1.5 rounded-full bg-cream px-3.5 text-xs font-bold text-ink-900 transition-transform [--tail:15px] active:scale-95 disabled:opacity-40"
                >
                  <Play size={11} fill="currentColor" strokeWidth={0} aria-hidden="true" />
                  Join
                </button>
                <button
                  type="button"
                  disabled={busy !== null}
                  onClick={() => void remove(server.address)}
                  aria-label={`Remove ${server.name}`}
                  title="Remove from its list"
                  className="grid size-9 shrink-0 place-items-center rounded-full text-mute transition-colors hover:bg-cream/8 hover:text-coral disabled:opacity-40"
                >
                  {busy === server.address ? <LoaderCircle size={15} className="animate-spin" aria-hidden="true" /> : <Trash2 size={15} aria-hidden="true" />}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      <form onSubmit={add} className="flex flex-wrap items-end gap-2.5 rounded-2xl border border-cream/7 bg-ink-850 p-3.5">
        <label className="flex min-w-[160px] flex-1 flex-col gap-1">
          <span className="text-xs font-semibold text-mute">Name</span>
          <input value={name} onChange={(event) => setName(event.target.value)} placeholder="Our server" className="h-10 rounded-xl border border-cream/12 bg-ink-900/50 px-3 text-sm outline-none focus:border-cream/35" />
        </label>
        <label className="flex min-w-[200px] flex-[2] flex-col gap-1">
          <span className="text-xs font-semibold text-mute">Address</span>
          <input value={address} onChange={(event) => setAddress(event.target.value)} placeholder="play.example.com" spellCheck={false} className="h-10 rounded-xl border border-cream/12 bg-ink-900/50 px-3 font-mono text-sm outline-none focus:border-cream/35" />
        </label>
        <Button type="submit" variant="secondary" icon={Plus} disabled={!address.trim() || busy !== null} className="h-10 px-4 text-[13px]">
          Add server
        </Button>
      </form>
    </div>
  );
}
