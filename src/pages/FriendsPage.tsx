import { Check, Copy, Crown, Gamepad2, KeyRound, LoaderCircle, LogIn, LogOut, Lock, MessageCircle, PartyPopper, Plus, Send, Server, ShieldCheck, UserPlus, UserRoundX, Users, X } from "lucide-react";
import { m } from "motion/react";
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { Ghost } from "../components/brand/Ghost";
import { ChatPanel } from "../components/friends/ChatPanel";
import { Button } from "../components/ui/Button";
import { EmptyState } from "../components/ui/EmptyState";
import { Page, Panel } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { Toggle } from "../components/ui/Toggle";
import {
  acceptFriend,
  addFriend,
  createParty,
  declineFriend,
  declineParty,
  declineServerInvite,
  getSocial,
  inviteToParty,
  isDesktop,
  joinFriendServer,
  joinParty,
  leaveParty,
  onChat,
  onJoinProgress,
  onSocialChange,
  removeFriend,
  setShareActivity,
  setSocialEnabled,
  setSocialProfile,
} from "../data/api";
import type { Friend, FriendStatus, JoinProgress, ServerCard, SocialView } from "../data/types";
import { cn } from "../lib/cn";
import { formatBytes, loaderName, timeAgo } from "../lib/format";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

/** "npub1abc…wxyz" */
const shortCode = (code: string) => (code.length > 16 ? `${code.slice(0, 9)}…${code.slice(-4)}` : code);

export function FriendsPage() {
  const { notify, now } = useApp();
  const [social, setSocial] = useState<SocialView | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  // Joining a friend's server, as it goes; and the friend whose chat is open.
  const [joining, setJoining] = useState<(JoinProgress & { name: string }) | null>(null);
  const [chatWith, setChatWith] = useState<string | null>(null);

  const reload = useCallback(() => {
    getSocial().then(
      (found) => {
        setSocial(found);
        setFailed(null);
      },
      (failure: unknown) => setFailed(failure instanceof Error ? failure.message : String(failure)),
    );
  }, []);
  useEffect(() => {
    return onJoinProgress((progress) => {
      setJoining((current) => (current && current.serverId === progress.serverId ? { ...progress, name: current.name } : current));
      if (progress.step === "done" || progress.step === "failed") window.setTimeout(() => setJoining((current) => (current?.serverId === progress.serverId ? null : current)), progress.step === "done" ? 2500 : 0);
    });
  }, []);
  // Unread counts move as messages arrive.
  useEffect(() => onChat(() => reload()), [reload]);

  const join = async (code: string, server: ServerCard) => {
    setJoining({ code, serverId: server.id, name: server.name, step: "asking", doneBytes: 0, totalBytes: 0, message: null });
    try {
      await joinFriendServer(code, server.id);
    } catch (failure: unknown) {
      setJoining(null);
      notify({ tone: "info", title: `Couldn't join ${server.name}`, body: failure instanceof Error ? failure.message : String(failure) });
    }
  };

  useEffect(() => {
    reload();
    const stop = onSocialChange(() => reload());
    // Statuses go stale on their own (a friend quietly closing Wisp).
    const timer = window.setInterval(reload, 30_000);
    return () => {
      stop();
      window.clearInterval(timer);
    };
  }, [reload]);

  /** Runs an action that returns the new state, with a spinner and a toast if it fails. */
  const act = async (label: string, work: () => Promise<SocialView>, failTitle: string) => {
    setBusy(label);
    try {
      setSocial(await work());
      return true;
    } catch (failure: unknown) {
      notify({ tone: "info", title: failTitle, body: failure instanceof Error ? failure.message : String(failure) });
      return false;
    } finally {
      setBusy(null);
    }
  };

  return (
    <Page>
      <PageHeader title="Friends" subtitle="See who's on and what they're playing, join their servers, get a party together and chat." />

      {failed && !social ? (
        <EmptyState
          art={<Ghost size={64} floating sleeping />}
          title={isDesktop ? "Friends couldn't start" : "Friends needs the desktop app"}
          body={isDesktop ? failed : "Friends talks to the relays from Wisp's Rust side, which the browser preview doesn't have. Run pnpm tauri dev to try it."}
          actions={isDesktop ? <Button onClick={reload}>Try again</Button> : undefined}
        />
      ) : social === null ? (
        <Loading label="Loading friends" className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
          <Skeleton className="h-72 rounded-[24px]" />
          <Skeleton className="h-72 rounded-[24px]" />
        </Loading>
      ) : !social.enabled ? (
        <Intro relays={social.relays} busy={busy === "enable"} onEnable={() => void act("enable", () => setSocialEnabled(true), "Couldn't turn friends on")} />
      ) : (
        <m.div variants={fadeUp} className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_340px]">
          <div className="flex min-w-0 flex-col gap-5">
            {joining && <JoinBanner joining={joining} />}

            {social.serverInvites.length > 0 && (
              <Panel title="Server invites" icon={<Server size={19} aria-hidden="true" />}>
                <ul className="flex flex-col divide-y divide-cream/7">
                  {social.serverInvites.map((invite) => {
                    const host = social.friends.find((friend) => friend.code === invite.from);
                    const live = host?.hosting.find((server) => server.id === invite.server.id);
                    return (
                      <li key={`${invite.from}-${invite.server.id}`} className="flex items-center gap-3 py-3">
                        <Face face={host?.profile.face ?? null} name={host?.profile.name ?? "?"} />
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-sm font-bold">{invite.server.name}</p>
                          <p className="truncate text-xs text-mute">
                            {host?.profile.name ?? shortCode(invite.from)} · {serverLine(live ?? invite.server)} · {live?.online ? "Up now" : live?.starting ? "Starting up" : "Not running right now"}
                          </p>
                        </div>
                        <button
                          type="button"
                          onClick={() => void act(`decline-server-${invite.server.id}`, () => declineServerInvite(invite.from, invite.server.id), "Couldn't decline it")}
                          className="h-9 rounded-full px-3 text-xs font-bold text-cream-300 hover:bg-cream/8 hover:text-cream"
                        >
                          Decline
                        </button>
                        <Button icon={LogIn} disabled={joining !== null} onClick={() => void join(invite.from, live ?? invite.server)}>
                          Join
                        </Button>
                      </li>
                    );
                  })}
                </ul>
              </Panel>
            )}

            <AddFriend busy={busy === "add"} onAdd={(code, note) => act("add", () => addFriend(code, note), "Couldn't send the request")} />

            {(social.incoming.length > 0 || social.outgoing.length > 0) && (
              <Panel title="Requests" icon={<UserPlus size={19} aria-hidden="true" />}>
                <ul className="flex flex-col divide-y divide-cream/7">
                  {social.incoming.map((request) => (
                    <li key={request.code} className="flex items-center gap-3 py-3">
                      <Face face={request.profile.face} name={request.profile.name} />
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm font-bold">{request.profile.name}</p>
                        <p className="truncate text-xs text-mute">{request.note ? `“${request.note}”` : `${shortCode(request.code)} · ${timeAgo(request.at, now)}`}</p>
                      </div>
                      <button
                        type="button"
                        onClick={() => void act(`decline-${request.code}`, () => declineFriend(request.code), "Couldn't decline it")}
                        className="h-9 rounded-full px-3 text-xs font-bold text-cream-300 hover:bg-cream/8 hover:text-cream"
                      >
                        Decline
                      </button>
                      <Button onClick={() => void act(`accept-${request.code}`, () => acceptFriend(request.code), "Couldn't accept it")} disabled={busy !== null}>
                        Accept
                      </Button>
                    </li>
                  ))}
                  {social.outgoing.map((request) => (
                    <li key={request.code} className="flex items-center gap-3 py-3">
                      <Face face={null} name="?" />
                      <div className="min-w-0 flex-1">
                        <p className="truncate font-mono text-sm font-semibold text-cream-300">{shortCode(request.code)}</p>
                        <p className="text-xs text-mute">Sent {timeAgo(request.at, now)} · waiting for them to accept</p>
                      </div>
                      <button
                        type="button"
                        onClick={() => void act(`cancel-${request.code}`, () => removeFriend(request.code), "Couldn't cancel it")}
                        className="h-9 rounded-full px-3 text-xs font-bold text-cream-300 hover:bg-cream/8 hover:text-cream"
                      >
                        Cancel
                      </button>
                    </li>
                  ))}
                </ul>
              </Panel>
            )}

            <Panel
              title="Friends"
              icon={<Users size={19} aria-hidden="true" />}
              description={
                social.friends.length > 0
                  ? `${social.friends.filter((friend) => friend.status.state !== "offline").length} of ${social.friends.length} on right now.`
                  : undefined
              }
            >
              {social.friends.length === 0 ? (
                <p className="py-4 text-sm leading-relaxed text-mute">
                  No friends yet. Send someone your friend code (on the right), or paste theirs above. Once they accept, you'll see when they're on and what they're playing.
                </p>
              ) : (
                <ul className="flex flex-col divide-y divide-cream/7">
                  {social.friends.map((friend) => (
                    <FriendRow
                      key={friend.code}
                      friend={friend}
                      now={now}
                      inParty={social.party?.members.includes(friend.code) ?? false}
                      canInvite={social.party?.leading ?? false}
                      busy={busy}
                      joining={joining !== null}
                      onJoin={(server) => void join(friend.code, server)}
                      chatting={chatWith === friend.code}
                      onChat={() => setChatWith((current) => (current === friend.code ? null : friend.code))}
                      onInvite={() =>
                        void act(`invite-${friend.code}`, () => inviteToParty(friend.code), "Couldn't send the invite").then(
                          (sent) => sent && notify({ tone: "success", title: `Invited ${friend.profile.name}`, body: "They'll see it on their Friends page." }),
                        )
                      }
                      onRemove={() => void act(`remove-${friend.code}`, () => removeFriend(friend.code), "Couldn't remove them")}
                    />
                  ))}
                </ul>
              )}
            </Panel>
          </div>

          <aside className="flex flex-col gap-5 lg:sticky lg:top-24">
            {chatWith && social.friends.some((friend) => friend.code === chatWith) && (
              <ChatPanel
                key={chatWith}
                conversation={`friend:${chatWith}`}
                title={social.friends.find((friend) => friend.code === chatWith)?.profile.name ?? "Chat"}
                me={social.code}
                nameOf={(code) => social.friends.find((friend) => friend.code === code)?.profile.name ?? shortCode(code)}
                now={now}
                onClose={() => setChatWith(null)}
                onFailed={(message) => notify({ tone: "info", title: "Couldn't send it", body: message })}
              />
            )}
            <YouCard social={social} busy={busy} act={act} />
            <PartyCard social={social} now={now} busy={busy} act={act} onFailed={(message) => notify({ tone: "info", title: "Couldn't send it", body: message })} />
          </aside>
        </m.div>
      )}
    </Page>
  );
}

type Act = (label: string, work: () => Promise<SocialView>, failTitle: string) => Promise<boolean>;

function Intro({ relays, busy, onEnable }: { relays: string[]; busy: boolean; onEnable: () => void }) {
  const points: { icon: ReactNode; title: string; body: string }[] = [
    { icon: <KeyRound size={18} aria-hidden="true" />, title: "Your own friend code", body: "Wisp makes you a code to share. No sign-up, no email, nothing to remember." },
    {
      icon: <Lock size={18} aria-hidden="true" />,
      title: "Private by design",
      body: "Requests and updates are end-to-end encrypted and sent from a throwaway key, so the relays carrying them can't read them or tell who sent them.",
    },
    { icon: <Gamepad2 size={18} aria-hidden="true" />, title: "What everyone's playing", body: "See who's on, their instance and world (you choose whether to share yours), and start a party." },
  ];
  return (
    <m.section variants={fadeUp} className="wisp-surface relative overflow-hidden rounded-[26px] p-8">
      <div className="pointer-events-none absolute -right-10 -top-10 size-72 rounded-full bg-[radial-gradient(circle,rgba(244,238,228,0.1),transparent_65%)]" />
      <div className="relative flex flex-wrap items-start gap-8">
        <div className="flex min-w-[260px] flex-1 flex-col gap-5">
          <div className="flex items-center gap-3">
            <Ghost size={52} floating className="drop-shadow-[0_0_16px_rgba(244,238,228,0.3)]" />
            <h2 className="font-display text-[28px] font-extrabold leading-tight tracking-[-0.02em]">Play with friends</h2>
          </div>
          <ul className="flex flex-col gap-4">
            {points.map((point) => (
              <li key={point.title} className="flex gap-3">
                <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-cream/7 text-cream-300">{point.icon}</span>
                <span>
                  <span className="block text-[15px] font-bold">{point.title}</span>
                  <span className="block text-sm leading-relaxed text-mute">{point.body}</span>
                </span>
              </li>
            ))}
          </ul>
          <div className="flex flex-wrap items-center gap-3">
            <Button size="lg" icon={busy ? undefined : Users} onClick={onEnable} disabled={busy}>
              {busy ? "Connecting…" : "Turn on friends"}
            </Button>
            <span className="text-xs text-mute">You can turn it off any time.</span>
          </div>
        </div>
        <div className="w-full max-w-[300px] rounded-2xl bg-ink-900/60 p-4 text-xs leading-relaxed text-mute">
          <p className="mb-2 flex items-center gap-1.5 font-bold text-cream-300">
            <ShieldCheck size={14} aria-hidden="true" />
            How it works
          </p>
          Wisp has no servers. Messages travel through public Nostr relays, which anyone can run:
          <ul className="my-2 flex flex-col gap-0.5 font-mono text-[11px] text-cream-400">
            {relays.map((relay) => (
              <li key={relay}>{relay.replace("wss://", "")}</li>
            ))}
          </ul>
          A relay can see that someone sent your code a message and when, but not who or what. Your key stays on this computer.
        </div>
      </div>
    </m.section>
  );
}

function AddFriend({ busy, onAdd }: { busy: boolean; onAdd: (code: string, note: string | null) => Promise<boolean> }) {
  const [code, setCode] = useState("");
  const [note, setNote] = useState("");
  const valid = /^(nostr:)?npub1[02-9ac-hj-np-z]{58}$/.test(code.trim()) || /^[0-9a-f]{64}$/i.test(code.trim());
  const submit = async () => {
    if (!valid) return;
    if (await onAdd(code.trim(), note.trim() || null)) {
      setCode("");
      setNote("");
    }
  };
  return (
    <Panel title="Add a friend" icon={<UserPlus size={19} aria-hidden="true" />} description="Paste their friend code. They'll get a request to accept.">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
        className="mt-2 flex flex-col gap-2.5"
      >
        <label className="flex h-11 items-center gap-2 rounded-xl border border-cream/12 bg-ink-900/60 px-3.5 focus-within:border-cream/35">
          <KeyRound size={15} className="shrink-0 text-mute" aria-hidden="true" />
          <span className="sr-only">Friend code</span>
          <input value={code} onChange={(event) => setCode(event.target.value)} placeholder="npub1…" spellCheck={false} autoCapitalize="off" className="min-w-0 flex-1 bg-transparent font-mono text-sm text-cream outline-none placeholder:text-mute-2" />
          {code && !valid && <span className="shrink-0 text-xs text-coral">Not a friend code yet</span>}
        </label>
        <div className="flex gap-2.5">
          <label className="flex h-11 flex-1 items-center rounded-xl border border-cream/12 bg-ink-900/60 px-3.5 focus-within:border-cream/35">
            <span className="sr-only">A note (optional)</span>
            <input value={note} maxLength={200} onChange={(event) => setNote(event.target.value)} placeholder="A note, so they know it's you (optional)" className="min-w-0 flex-1 bg-transparent text-sm text-cream outline-none placeholder:text-mute-2" />
          </label>
          <Button type="submit" icon={busy ? undefined : Send} disabled={!valid || busy}>
            {busy ? "Sending…" : "Send request"}
          </Button>
        </div>
      </form>
    </Panel>
  );
}

function statusLine(status: FriendStatus, lastSeen: number | null, now: number): { text: string; tone: string } {
  if (status.state === "playing") {
    const what = [status.label, status.world].filter(Boolean).join(" · ");
    return { text: what ? `Playing ${what}` : "Playing Minecraft", tone: "text-moss-soft" };
  }
  if (status.state === "online") return { text: "In Wisp", tone: "text-sky" };
  return { text: lastSeen ? `Last seen ${timeAgo(lastSeen, now)}` : "Offline", tone: "text-mute" };
}

function Face({ face, name, status }: { face: string | null; name: string; status?: FriendStatus }) {
  return (
    <span className="relative shrink-0">
      {face ? (
        <img src={face} alt="" className="size-10 rounded-xl bg-ink-700 [image-rendering:pixelated]" />
      ) : (
        <span className="grid size-10 place-items-center rounded-xl bg-cream/8 font-display text-base font-bold text-cream-300">{name.charAt(0).toUpperCase()}</span>
      )}
      {status && status.state !== "offline" && (
        <span aria-hidden="true" className={cn("absolute -bottom-0.5 -right-0.5 size-3.5 rounded-full border-[2.5px] border-ink-800", status.state === "playing" ? "bg-moss" : "bg-sky")} />
      )}
    </span>
  );
}

/** "Fabric 26.3 · 40 mods", "Paper 26.3". */
function serverLine(server: ServerCard): string {
  const runs = server.software ? server.software.charAt(0).toUpperCase() + server.software.slice(1) : loaderName(server.loader);
  return [`${runs} ${server.gameVersion}`, server.mods > 0 ? `${server.mods} mods` : null].filter(Boolean).join(" · ");
}

const JOIN_STEPS: Record<JoinProgress["step"], string> = {
  asking: "Asking their Wisp…",
  waiting: "Waiting for the server to start…",
  connecting: "Connecting to their computer…",
  mods: "Getting the server's mods…",
  launching: "Starting Minecraft…",
  done: "In the game",
  failed: "Couldn't join",
};

function JoinBanner({ joining }: { joining: JoinProgress & { name: string } }) {
  const share = joining.step === "mods" && joining.totalBytes > 0 ? Math.min(1, joining.doneBytes / joining.totalBytes) : null;
  return (
    <m.section variants={fadeUp} initial="hidden" animate="show" className="wisp-surface flex items-center gap-4 rounded-[22px] p-4">
      {joining.step === "done" ? <Check size={20} className="shrink-0 text-moss-soft" aria-hidden="true" /> : <LoaderCircle size={20} className="shrink-0 animate-spin text-cream-300" aria-hidden="true" />}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-bold">Joining {joining.name}</p>
        <p className="truncate text-xs text-mute">
          {JOIN_STEPS[joining.step]}
          {share !== null && ` ${Math.floor(share * 100)}% · ${formatBytes(joining.doneBytes)} of ${formatBytes(joining.totalBytes)}`}
        </p>
        {share !== null && (
          <span className="mt-2 block h-1.5 overflow-hidden rounded-full bg-cream/10">
            <span className="block h-full rounded-full bg-cream transition-[width] duration-300" style={{ width: `${share * 100}%` }} />
          </span>
        )}
      </div>
    </m.section>
  );
}

function FriendRow({
  friend,
  now,
  inParty,
  canInvite,
  busy,
  joining,
  onJoin,
  chatting,
  onChat,
  onInvite,
  onRemove,
}: {
  friend: Friend;
  now: number;
  inParty: boolean;
  canInvite: boolean;
  busy: string | null;
  joining: boolean;
  onJoin: (server: ServerCard) => void;
  chatting: boolean;
  onChat: () => void;
  onInvite: () => void;
  onRemove: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const line = statusLine(friend.status, friend.lastSeen, now);
  return (
    <li className="group flex flex-col gap-2 py-3">
      <div className="flex items-center gap-3">
      <Face face={friend.profile.face} name={friend.profile.name} status={friend.status} />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-bold">{friend.profile.name}</p>
        <p className={cn("truncate text-xs font-semibold", line.tone)}>{line.text}</p>
      </div>
      <button
        type="button"
        aria-label={`Chat with ${friend.profile.name}`}
        aria-pressed={chatting}
        onClick={onChat}
        className={cn("relative grid size-8 place-items-center rounded-full transition-colors hover:bg-cream/8 hover:text-cream", chatting ? "bg-cream/10 text-cream" : "text-mute")}
      >
        <MessageCircle size={15} aria-hidden="true" />
        {friend.unread > 0 && <span className="absolute -right-0.5 -top-0.5 grid min-w-4 place-items-center rounded-full bg-ember px-1 text-[10px] font-bold text-ink-900">{friend.unread}</span>}
      </button>
      {confirming ? (
        <span className="flex items-center gap-1 text-xs">
          <span className="font-semibold text-cream-300">Remove {friend.profile.name}?</span>
          <button type="button" onClick={onRemove} className="rounded-full bg-coral px-2.5 py-1 font-bold text-ink-900">
            Remove
          </button>
          <button type="button" onClick={() => setConfirming(false)} className="rounded-full px-2 py-1 font-bold text-cream-300 hover:bg-cream/8">
            Keep
          </button>
        </span>
      ) : (
        <>
          {canInvite &&
            (inParty ? (
              <span className="flex items-center gap-1 rounded-full bg-cream/6 px-2.5 py-1 text-xs font-bold text-cream-300">
                <Check size={12} aria-hidden="true" />
                In your party
              </span>
            ) : (
              <button
                type="button"
                onClick={onInvite}
                disabled={busy === `invite-${friend.code}`}
                className="flex h-8 items-center gap-1.5 rounded-full border border-cream/15 px-3 text-xs font-bold text-cream-300 transition-colors hover:border-cream/35 hover:text-cream"
              >
                <PartyPopper size={13} aria-hidden="true" />
                Invite
              </button>
            ))}
          <button
            type="button"
            aria-label={`Remove ${friend.profile.name}`}
            onClick={() => setConfirming(true)}
            className="grid size-8 place-items-center rounded-full text-mute opacity-0 transition-opacity hover:bg-cream/8 hover:text-cream group-hover:opacity-100 focus-visible:opacity-100"
          >
            <UserRoundX size={15} aria-hidden="true" />
          </button>
        </>
      )}
      </div>
      {friend.hosting.map((server) => (
        <div key={server.id} className="ml-[52px] flex items-center gap-3 rounded-xl bg-cream/[0.04] p-2.5">
          <Server size={15} className="shrink-0 text-ember-soft" aria-hidden="true" />
          <div className="min-w-0 flex-1">
            <p className="truncate text-sm font-bold">{server.name}</p>
            <p className="truncate text-xs text-mute">
              {serverLine(server)} · {server.starting && !server.online ? "starting up" : `${server.players} of ${server.maxPlayers} on`}
            </p>
          </div>
          <button
            type="button"
            onClick={() => onJoin(server)}
            disabled={joining}
            className="tail wag flex h-8 items-center gap-1.5 rounded-full bg-cream px-3.5 text-xs font-bold text-ink-900 transition-transform active:scale-95 disabled:opacity-50 [--tail:17px]"
          >
            <LogIn size={12} aria-hidden="true" />
            Join
          </button>
        </div>
      ))}
    </li>
  );
}

function YouCard({ social, busy, act }: { social: SocialView; busy: string | null; act: Act }) {
  const [name, setName] = useState(social.profile.name);
  const [copied, setCopied] = useState(false);
  useEffect(() => setName(social.profile.name), [social.profile.name]);
  const copy = async () => {
    await navigator.clipboard.writeText(social.code);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };
  const connected = social.relaysConnected;
  return (
    <section aria-label="You" className="wisp-surface rounded-[24px] p-5">
      <div className="flex items-center gap-3">
        <Face face={social.profile.face} name={social.profile.name} status={{ state: "online" }} />
        <label className="flex h-10 min-w-0 flex-1 items-center rounded-xl border border-transparent px-2 hover:border-cream/12 focus-within:border-cream/35">
          <span className="sr-only">Your name for friends</span>
          <input
            value={name}
            maxLength={32}
            onChange={(event) => setName(event.target.value)}
            onBlur={() => name.trim() !== social.profile.name && void act("name", () => setSocialProfile(name, social.profile.face), "Couldn't change your name")}
            onKeyDown={(event) => event.key === "Enter" && event.currentTarget.blur()}
            className="min-w-0 flex-1 bg-transparent font-display text-lg font-bold text-cream outline-none"
          />
        </label>
      </div>
      <p className="mt-3 text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">Your friend code</p>
      <div className="mt-1.5 flex items-center gap-2 rounded-xl bg-ink-900/60 py-1.5 pl-3 pr-1.5">
        <code className="min-w-0 flex-1 truncate font-mono text-xs text-cream-300" title={social.code}>
          {social.code}
        </code>
        <button type="button" onClick={() => void copy()} className="flex h-8 items-center gap-1.5 rounded-lg bg-cream px-3 text-xs font-bold text-ink-900">
          {copied ? <Check size={13} aria-hidden="true" /> : <Copy size={13} aria-hidden="true" />}
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <p className="mt-1.5 text-xs text-mute">Share it anywhere: Discord, a text, a sign in your base.</p>
      <div className="mt-2 border-t border-cream/7">
        <Toggle
          label="Share what I'm playing"
          description="Friends see your instance and world while you play. Off: they just see you're playing."
          checked={social.shareActivity}
          onChange={(share) => void act("share", () => setShareActivity(share), "Couldn't change that")}
        />
      </div>
      <div className="flex items-center gap-2 border-t border-cream/7 pt-3 text-xs text-mute">
        <span className={cn("size-2 rounded-full", connected > 0 ? "bg-moss" : "bg-amber")} aria-hidden="true" />
        {connected > 0 ? `Connected to ${connected} of ${social.relays.length} relays` : "Connecting to the relays…"}
        <button
          type="button"
          onClick={() => void act("disable", () => setSocialEnabled(false), "Couldn't turn friends off")}
          disabled={busy === "disable"}
          className="ml-auto rounded-full px-2.5 py-1 font-bold text-cream-300 hover:bg-cream/8 hover:text-cream"
        >
          {busy === "disable" ? "Turning off…" : "Turn off"}
        </button>
      </div>
    </section>
  );
}

function PartyCard({ social, now, busy, act, onFailed }: { social: SocialView; now: number; busy: string | null; act: Act; onFailed: (message: string) => void }) {
  const [name, setName] = useState("");
  const party = social.party;
  const nameOf = (code: string) => social.friends.find((friend) => friend.code === code)?.profile.name ?? shortCode(code);
  return (
    <section aria-label="Party" className="wisp-surface rounded-[24px] p-5">
      <h2 className="flex items-center gap-2 font-display text-lg font-bold">
        <PartyPopper size={18} className="text-ember-soft" aria-hidden="true" />
        {party ? party.name : "Party"}
      </h2>
      {party ? (
        <>
          <ul className="mt-3 flex flex-col gap-2.5">
            {party.membersShown.map((member) => {
              const line = statusLine(member.status, null, now);
              return (
                <li key={member.code} className="flex items-center gap-3">
                  <Face face={member.face} name={member.name} status={member.status} />
                  <div className="min-w-0 flex-1">
                    <p className="flex items-center gap-1.5 truncate text-sm font-bold">
                      {member.name}
                      {member.me && <span className="font-normal text-mute">(you)</span>}
                      {member.leader && <Crown size={12} className="text-amber-soft" aria-label="Leader" />}
                    </p>
                    <p className={cn("truncate text-xs font-semibold", line.tone)}>{line.text}</p>
                  </div>
                </li>
              );
            })}
          </ul>
          {party.leading && <p className="mt-3 text-xs leading-relaxed text-mute">Invite friends from your list.</p>}
          <ChatPanel
            conversation={`party:${party.id}`}
            title={`${party.name} chat`}
            me={social.code}
            nameOf={(code) => (code === social.code ? "You" : nameOf(code))}
            now={now}
            className="mt-3"
            onFailed={onFailed}
          />
          <button
            type="button"
            onClick={() => void act("leave", leaveParty, "Couldn't leave the party")}
            disabled={busy === "leave"}
            className="mt-3 flex h-9 items-center gap-1.5 rounded-full border border-cream/15 px-3.5 text-xs font-bold text-cream-300 transition-colors hover:border-cream/35 hover:text-cream"
          >
            <LogOut size={13} aria-hidden="true" />
            {party.leading ? "End the party" : "Leave"}
          </button>
        </>
      ) : (
        <form
          onSubmit={(event) => {
            event.preventDefault();
            void act("party", () => createParty(name || "Party"), "Couldn't start a party").then(() => setName(""));
          }}
          className="mt-3 flex gap-2"
        >
          <label className="flex h-10 min-w-0 flex-1 items-center rounded-xl border border-cream/12 bg-ink-900/60 px-3 focus-within:border-cream/35">
            <span className="sr-only">Party name</span>
            <input value={name} maxLength={32} onChange={(event) => setName(event.target.value)} placeholder="Build night" className="min-w-0 flex-1 bg-transparent text-sm text-cream outline-none placeholder:text-mute-2" />
          </label>
          <Button type="submit" icon={Plus} disabled={busy === "party"}>
            Start
          </Button>
        </form>
      )}

      {social.invites.length > 0 && (
        <div className="mt-4 flex flex-col gap-2 border-t border-cream/7 pt-4">
          <p className="text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">Invites</p>
          {social.invites.map((invite) => (
            <div key={invite.party.id} className="flex items-center gap-2 rounded-xl bg-cream/[0.04] p-2.5">
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-bold">{invite.party.name}</p>
                <p className="truncate text-xs text-mute">
                  from {nameOf(invite.from)} · {invite.party.members.length} in it
                </p>
              </div>
              <button
                type="button"
                aria-label={`Decline ${invite.party.name}`}
                onClick={() => void act(`decline-party-${invite.party.id}`, () => declineParty(invite.party.id), "Couldn't decline it")}
                className="grid size-8 place-items-center rounded-full text-mute hover:bg-cream/8 hover:text-cream"
              >
                <X size={14} aria-hidden="true" />
              </button>
              <button
                type="button"
                onClick={() => void act(`join-${invite.party.id}`, () => joinParty(invite.party.id), "Couldn't join it")}
                className="flex h-8 items-center gap-1.5 rounded-full bg-cream px-3 text-xs font-bold text-ink-900"
              >
                {busy === `join-${invite.party.id}` ? <LoaderCircle size={12} className="animate-spin" aria-hidden="true" /> : null}
                Join
              </button>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
