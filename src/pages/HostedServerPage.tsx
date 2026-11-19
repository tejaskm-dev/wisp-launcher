import {
  ArrowLeft,
  Check,
  Copy,
  Cpu,
  FolderOpen,
  Gamepad2,
  Globe2,
  HardDrive,
  LoaderCircle,
  Play,
  RotateCcw,
  Router,
  Send,
  Server,
  Settings2,
  ShieldCheck,
  Square,
  Terminal,
  Trash2,
  UserPlus,
  Users,
} from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { hostedStatus, TONE_DOT, TONE_TEXT } from "../components/hosting/describe";
import { useHostedServer } from "../components/hosting/useHosted";
import { InstanceAvatar } from "../components/instances/InstanceAvatar";
import { Button } from "../components/ui/Button";
import { Dialog } from "../components/ui/Dialog";
import { EmptyState } from "../components/ui/EmptyState";
import { Page, Panel } from "../components/ui/Page";
import { ProgressBar } from "../components/ui/ProgressBar";
import { Select, type SelectOption } from "../components/ui/Select";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { Toggle } from "../components/ui/Toggle";
import {
  agreeServerEula,
  backUpServer,
  deleteHostedServer,
  getServerBackups,
  getServerPlayers,
  getServerProperties,
  getSocial,
  getSystemMemory,
  inviteToServer,
  openExternal,
  openServerFolder,
  playHostedServer,
  restartHostedServer,
  restoreServerBackup,
  sendServerCommand,
  setServerPlayer,
  setServerProperties,
  setServerWorld,
  startHostedServer,
  stopHostedServer,
  updateHostedServer,
} from "../data/api";
import type { HostedView, PageId, PlayerList, RestorePoint, ServerPlayers, SocialView } from "../data/types";
import { cn } from "../lib/cn";
import { formatBytes, loaderName, timeAgo } from "../lib/format";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

const BACK_LABELS: Partial<Record<PageId, string>> = { servers: "Servers", friends: "Friends", worlds: "Worlds", home: "Home" };
type Tab = "overview" | "console" | "players" | "settings" | "backups";
const TABS: { id: Tab; label: string; icon: typeof Server }[] = [
  { id: "overview", label: "Overview", icon: Server },
  { id: "console", label: "Console", icon: Terminal },
  { id: "players", label: "Players", icon: Users },
  { id: "settings", label: "Settings", icon: Settings2 },
  { id: "backups", label: "Backups", icon: RotateCcw },
];
const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure));

/** One server hosted on this computer: start and stop it, who can join and how, its console, players, settings and backups. */
export function HostedServerPage({ serverId, backTo, onBack }: { serverId: string; backTo: PageId; onBack: () => void }) {
  const { notify, now } = useApp();
  const { server, setServer, console, reload } = useHostedServer(serverId, (failure) => notify({ tone: "info", title: "The server couldn't start", body: failure }));
  const [tab, setTab] = useState<Tab>("overview");
  const [busy, setBusy] = useState<string | null>(null);

  const run = async (label: string, work: () => Promise<unknown>, failTitle: string) => {
    setBusy(label);
    try {
      await work();
      return true;
    } catch (failure: unknown) {
      notify({ tone: "info", title: failTitle, body: message(failure) });
      return false;
    } finally {
      setBusy(null);
    }
  };

  if (server === undefined) {
    return (
      <Page>
        <Loading label="Loading the server" className="flex flex-col gap-5">
          <Skeleton className="h-[260px] w-full rounded-[26px]" />
          <Skeleton className="h-24 w-full rounded-2xl" />
        </Loading>
      </Page>
    );
  }
  if (server === null) {
    return (
      <Page>
        <BackButton label={BACK_LABELS[backTo] ?? "Back"} onBack={onBack} />
        <EmptyState title="That server isn't here anymore" body="It may have been deleted. Its folder went to the Trash." actions={<Button onClick={onBack}>Go back</Button>} />
      </Page>
    );
  }

  const status = hostedStatus(server, now);
  const live = server.state !== "stopped" && server.state !== "crashed";
  const needsEula = !server.eula || server.exit?.reason === "eula";

  const start = () => void run("start", async () => setServer(await startHostedServer(server.id)), `Couldn't start ${server.name}`);
  const stop = () => void run("stop", async () => setServer(await stopHostedServer(server.id)), `Couldn't stop ${server.name}`);

  return (
    <Page>
      <BackButton label={BACK_LABELS[backTo] ?? "Back"} onBack={onBack} />

      <m.section variants={fadeUp} className="wisp-surface relative isolate overflow-hidden rounded-[26px] p-7">
        <div className="pointer-events-none absolute -right-16 -top-20 -z-10 size-80 rounded-full bg-[radial-gradient(circle,rgba(244,238,228,0.08),transparent_65%)]" />
        <div className="flex flex-wrap items-start gap-6">
          <span className="relative flex shrink-0 self-start">
            <InstanceAvatar instance={{ name: server.name, loader: server.loader, icon: server.icon }} className="size-24 text-4xl" />
            <span aria-hidden="true" className={cn("absolute -bottom-1 -right-1 size-5 rounded-full border-[3px] border-ink-850", TONE_DOT[status.tone], status.busy && "animate-pulse")} />
          </span>
          <div className="flex min-w-[240px] flex-1 flex-col gap-3">
            <p className="text-sm font-semibold text-cream-300">
              Hosted on this computer · {loaderName(server.loader)} {server.gameVersion}
              {server.instanceName && ` · played with ${server.instanceName}`}
            </p>
            <h1 className="font-display text-[clamp(34px,4vw,52px)] font-extrabold leading-none tracking-[-0.035em]">{server.name}</h1>
            <p className={cn("text-sm font-bold", TONE_TEXT[status.tone])}>
              {status.label}
              <span className="font-normal text-mute"> · {status.detail}</span>
            </p>
            {status.progress !== null && <ProgressBar value={status.progress} label="Setting up the server" tone="amber" className="max-w-md" />}
          </div>
        </div>
        <div className="mt-6 flex flex-wrap items-center gap-2.5">
          <button
            type="button"
            onClick={live ? stop : start}
            disabled={busy !== null || server.state === "stopping" || (!live && needsEula && !server.eula)}
            className={cn(
              "relative flex h-14 min-w-[150px] items-center justify-center gap-3 rounded-full px-7 font-display text-[19px] font-extrabold transition-transform duration-200 ease-out-expo active:scale-[0.97] disabled:opacity-50",
              live ? "border border-moss/40 bg-moss/15 text-moss-soft" : "tail wag bg-cream text-ink-900 shadow-[0_12px_40px_-6px_rgba(244,238,228,0.35)] [--tail:30px] enabled:hover:scale-[1.03]",
            )}
          >
            {busy === "start" || busy === "stop" || server.state === "stopping" ? (
              <LoaderCircle size={18} className="animate-spin" aria-hidden="true" />
            ) : live ? (
              <Square size={14} fill="currentColor" strokeWidth={0} aria-hidden="true" />
            ) : (
              <Play size={18} fill="currentColor" strokeWidth={0} aria-hidden="true" />
            )}
            {live ? (server.state === "preparing" ? "Cancel" : "Stop") : "Start"}
          </button>
          <Action
            icon={<Gamepad2 size={15} aria-hidden="true" />}
            label="Play on it"
            disabled={server.state !== "running" || busy !== null}
            onClick={() =>
              void run(
                "play",
                async () => {
                  const instanceId = await playHostedServer(server.id);
                  reload();
                  notify({ tone: "success", title: "Joining your server", body: `Starting ${instanceId === server.instanceId ? (server.instanceName ?? "the instance") : "a new instance with its mods"}.` });
                },
                "Couldn't join it",
              )
            }
          />
          {live && server.state === "running" && (
            <Action icon={<RotateCcw size={15} aria-hidden="true" />} label="Restart" disabled={busy !== null} onClick={() => void run("restart", async () => setServer(await restartHostedServer(server.id)), "Couldn't restart it")} />
          )}
          <Action icon={<FolderOpen size={15} aria-hidden="true" />} label="Open folder" onClick={() => void openServerFolder(server.id).catch((failure: unknown) => notify({ tone: "info", title: "Couldn't open the folder", body: message(failure) }))} />
        </div>
        {needsEula && !live && (
          <div className="mt-5 flex flex-wrap items-center gap-3 rounded-2xl border border-amber/25 bg-amber/8 p-4 text-sm">
            <ShieldCheck size={18} className="shrink-0 text-amber-soft" aria-hidden="true" />
            <p className="min-w-[220px] flex-1 leading-relaxed text-cream-200">Every Minecraft server needs its host to agree to the Minecraft EULA before it starts.</p>
            <button type="button" onClick={() => void openExternal("https://aka.ms/MinecraftEULA")} className="text-sm font-bold text-cream underline-offset-4 hover:underline">
              Read it
            </button>
            <Button onClick={() => void run("eula", async () => setServer(await agreeServerEula(server.id)), "Couldn't save that")}>I agree</Button>
          </div>
        )}
      </m.section>

      <m.dl variants={fadeUp} className="grid grid-cols-2 gap-3 md:grid-cols-4">
        <Stat icon={<Users size={13} aria-hidden="true" />} label="Players" value={live ? `${server.players.length} of ${server.maxPlayers}` : `Up to ${server.maxPlayers}`} />
        <Stat icon={<Cpu size={13} aria-hidden="true" />} label="Memory" value={`${(server.effectiveMemoryMb / 1024).toFixed(server.effectiveMemoryMb % 1024 === 0 ? 0 : 1)} GB${server.automaticMemory ? " (auto)" : ""}`} />
        <Stat icon={<HardDrive size={13} aria-hidden="true" />} label="Mods" value={server.modCount > 0 ? server.modCount.toLocaleString() : "None"} />
        <Stat icon={<Globe2 size={13} aria-hidden="true" />} label="World" value={server.worldExists ? "Ready" : "Made on first start"} />
      </m.dl>

      <m.div variants={fadeUp} className="flex flex-col gap-5">
        <div role="tablist" aria-label={server.name} className="flex w-fit max-w-full gap-1 overflow-x-auto rounded-full border border-cream/8 bg-ink-850 p-1">
          {TABS.map((option) => {
            const selected = option.id === tab;
            const Icon = option.icon;
            return (
              <button
                key={option.id}
                type="button"
                role="tab"
                aria-selected={selected}
                onClick={() => setTab(option.id)}
                className={cn("group/tab relative h-9 shrink-0 rounded-full px-4 text-sm font-bold transition-colors duration-200", selected ? "text-ink-900" : "text-cream-300 hover:text-cream")}
              >
                {selected && <m.span layoutId={`hosted-tab-${server.id}`} className="tail absolute inset-0 rounded-full bg-cream [--tail:14px]" transition={{ type: "spring", stiffness: 520, damping: 40 }} />}
                <span className="relative flex items-center gap-1.5">
                  <Icon size={14} aria-hidden="true" />
                  {option.label}
                  {option.id === "console" && server.state === "running" && <span aria-hidden="true" className="size-1.5 rounded-full bg-moss" />}
                </span>
              </button>
            );
          })}
        </div>
        <AnimatePresence mode="popLayout" initial={false}>
          <m.div key={tab} role="tabpanel" initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0 }} transition={{ duration: 0.18 }}>
            {tab === "overview" && <OverviewTab server={server} onChange={setServer} />}
            {tab === "console" && <ConsoleTab server={server} lines={console} />}
            {tab === "players" && <PlayersTab server={server} />}
            {tab === "settings" && <SettingsTab server={server} onChange={setServer} onDeleted={onBack} />}
            {tab === "backups" && <BackupsTab server={server} />}
          </m.div>
        </AnimatePresence>
      </m.div>
    </Page>
  );
}

function BackButton({ label, onBack }: { label: string; onBack: () => void }) {
  return (
    <m.button variants={fadeUp} type="button" onClick={onBack} className="group -mb-2 flex w-fit items-center gap-1.5 rounded-full px-2 py-1 text-sm font-semibold text-cream-300 transition-colors hover:text-cream">
      <ArrowLeft size={16} className="transition-transform duration-300 group-hover:-translate-x-0.5" aria-hidden="true" />
      {label}
    </m.button>
  );
}

function Action({ icon, label, onClick, disabled = false }: { icon: ReactNode; label: string; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="flex h-11 items-center gap-2 rounded-full border border-cream/18 bg-ink-900/50 px-4 text-sm font-bold text-cream-200 transition-colors enabled:hover:border-cream/40 enabled:hover:text-cream disabled:opacity-40"
    >
      {icon}
      {label}
    </button>
  );
}

function Stat({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <div className="rounded-2xl border border-cream/7 bg-ink-800 px-4 py-3.5">
      <dt className="flex items-center gap-1.5 text-xs font-semibold text-mute">
        {icon}
        {label}
      </dt>
      <dd className="mt-1 truncate font-display text-lg font-bold">{value}</dd>
    </div>
  );
}

function CopyLine({ label, value, hint }: { label: string; value: string; hint?: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex items-center gap-3 rounded-xl bg-ink-900/60 py-2 pl-3.5 pr-2">
      <div className="min-w-0 flex-1">
        <p className="text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">{label}</p>
        <code className="block truncate font-mono text-sm text-cream-200">{value}</code>
        {hint && <p className="mt-0.5 text-xs text-mute">{hint}</p>}
      </div>
      <button
        type="button"
        onClick={() =>
          void navigator.clipboard.writeText(value).then(() => {
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1500);
          })
        }
        className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-cream px-3 text-xs font-bold text-ink-900"
      >
        {copied ? <Check size={13} aria-hidden="true" /> : <Copy size={13} aria-hidden="true" />}
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}

// ── Overview: who can join, and how ──

function OverviewTab({ server, onChange }: { server: HostedView; onChange: (server: HostedView) => void }) {
  const { notify, navigate } = useApp();
  const [social, setSocial] = useState<SocialView | null | "off">(null);
  const [inviting, setInviting] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    getSocial().then(
      (found) => active && setSocial(found.enabled ? found : "off"),
      () => active && setSocial("off"),
    );
    return () => {
      active = false;
    };
  }, []);

  const invite = async (code: string, name: string) => {
    setInviting(code);
    try {
      onChange(await inviteToServer(server.id, code));
      notify({ tone: "success", title: `Invited ${name}`, body: server.state === "running" ? "They can join from their Friends page." : "They can join once the server's running." });
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't send the invite", body: message(failure) });
    } finally {
      setInviting(null);
    }
  };

  return (
    <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_360px]">
      <div className="flex min-w-0 flex-col gap-5">
        <Panel title="Friends" icon={<UserPlus size={19} aria-hidden="true" />} description={server.friendsJoin ? "Everyone on your friends list can join while it runs. Invite them so they see it." : "Only friends you invite can join."}>
          {social === null ? (
            <Skeleton className="mt-2 h-24 rounded-2xl" />
          ) : social === "off" ? (
            <div className="mt-2 flex flex-wrap items-center gap-3">
              <p className="min-w-[220px] flex-1 text-sm leading-relaxed text-mute">Friends join through the Friends page. Turn it on to invite them.</p>
              <Button variant="secondary" onClick={() => navigate("friends")}>
                Open Friends
              </Button>
            </div>
          ) : social.friends.length === 0 ? (
            <p className="mt-2 text-sm text-mute">No friends yet. Add them on the Friends page with their friend code.</p>
          ) : (
            <ul className="mt-1 flex flex-col divide-y divide-cream/7">
              {social.friends.map((friend) => {
                const invited = server.invited.includes(friend.code);
                const joined = server.friendsIn.includes(friend.code);
                return (
                  <li key={friend.code} className="flex items-center gap-3 py-2.5">
                    {friend.profile.face ? (
                      <img src={friend.profile.face} alt="" className="size-9 rounded-xl [image-rendering:pixelated]" />
                    ) : (
                      <span className="grid size-9 place-items-center rounded-xl bg-cream/8 font-display font-bold text-cream-300">{friend.profile.name.charAt(0).toUpperCase()}</span>
                    )}
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-bold">{friend.profile.name}</p>
                      <p className="truncate text-xs text-mute">{joined ? "Connected to it" : invited ? "Invited" : friend.status.state === "offline" ? "Offline" : "Online"}</p>
                    </div>
                    <button
                      type="button"
                      onClick={() => void invite(friend.code, friend.profile.name)}
                      disabled={inviting !== null}
                      className="flex h-8 items-center gap-1.5 rounded-full border border-cream/15 px-3 text-xs font-bold text-cream-300 transition-colors hover:border-cream/35 hover:text-cream disabled:opacity-50"
                    >
                      {inviting === friend.code ? <LoaderCircle size={12} className="animate-spin" aria-hidden="true" /> : <Send size={12} aria-hidden="true" />}
                      {invited ? "Invite again" : "Invite"}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </Panel>
        {server.players.length > 0 && (
          <Panel title="On now" icon={<Users size={19} aria-hidden="true" />}>
            <ul className="mt-1 flex flex-wrap gap-2">
              {server.players.map((player) => (
                <li key={player} className="flex items-center gap-1.5 rounded-full bg-moss/12 px-3 py-1.5 text-sm font-bold text-moss-soft">
                  <span aria-hidden="true" className="size-1.5 rounded-full bg-moss" />
                  {player}
                </li>
              ))}
            </ul>
          </Panel>
        )}
      </div>
      <aside className="flex flex-col gap-5">
        <section className="wisp-surface flex flex-col gap-3 rounded-[24px] p-5">
          <h2 className="flex items-center gap-2 font-display text-lg font-bold">
            <Router size={18} className="text-ember-soft" aria-hidden="true" />
            Ways in
          </h2>
          <p className="text-sm leading-relaxed text-mute">Friends with Wisp just press Join. Wisp links your two computers directly and gives them the server's mods.</p>
          {server.lanAddress && <CopyLine label="Same network" value={server.lanAddress} hint="For people on your Wi-Fi, from the game's Multiplayer screen." />}
          {server.publicAddress && <CopyLine label="Anyone with the address" value={server.publicAddress} hint="Your router opened the port. Only whitelisted players get in." />}
          {server.reach?.strictRouter && !server.reach.routerOpened && (
            <p className="rounded-xl bg-amber/8 p-3 text-xs leading-relaxed text-amber-soft">
              Your router is strict about incoming connections, so a friend whose router is strict too may not get through. Turning on UPnP in your router fixes it for everyone.
            </p>
          )}
        </section>
      </aside>
    </div>
  );
}

// ── Console ──

const LEVEL_TONE = { info: "text-cream-300", warn: "text-amber-soft", error: "text-coral", input: "text-sky" } as const;

function ConsoleTab({ server, lines }: { server: HostedView; lines: { at: number; text: string; level: keyof typeof LEVEL_TONE }[] }) {
  const { notify } = useApp();
  const [command, setCommand] = useState("");
  const scroller = useRef<HTMLDivElement>(null);
  const following = useRef(true);
  useEffect(() => {
    const element = scroller.current;
    if (element && following.current) element.scrollTop = element.scrollHeight;
  }, [lines]);
  const running = server.state === "running" || server.state === "starting";
  const send = async () => {
    const text = command.trim();
    if (!text) return;
    try {
      await sendServerCommand(server.id, text);
      setCommand("");
      following.current = true;
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't send that", body: message(failure) });
    }
  };
  return (
    <section className="flex flex-col overflow-hidden rounded-[22px] border border-cream/8 bg-ink-900">
      <div
        ref={scroller}
        onScroll={(event) => {
          const element = event.currentTarget;
          following.current = element.scrollHeight - element.scrollTop - element.clientHeight < 40;
        }}
        className="h-[460px] overflow-y-auto px-4 py-3 font-mono text-[12.5px] leading-relaxed"
        role="log"
        aria-live="polite"
        aria-label="Server console"
      >
        {lines.length === 0 ? (
          <p className="text-mute">{running ? "Waiting for the server to say something…" : "The console fills up while the server runs."}</p>
        ) : (
          lines.map((line, index) => (
            <p key={`${line.at}-${index}`} className={cn("whitespace-pre-wrap break-words", LEVEL_TONE[line.level])}>
              {line.text}
            </p>
          ))
        )}
      </div>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void send();
        }}
        className="flex items-center gap-2 border-t border-cream/8 bg-ink-850 p-2.5"
      >
        <span className="pl-2 font-mono text-sm text-mute" aria-hidden="true">
          /
        </span>
        <label className="flex-1">
          <span className="sr-only">Command</span>
          <input
            value={command}
            onChange={(event) => setCommand(event.target.value)}
            disabled={!running}
            placeholder={running ? "say Hello, op Alex, time set day…" : "Start the server to type commands"}
            spellCheck={false}
            autoCapitalize="off"
            className="h-10 w-full bg-transparent font-mono text-sm text-cream outline-none placeholder:text-mute-2 disabled:cursor-not-allowed"
          />
        </label>
        <Button type="submit" icon={Send} disabled={!running || !command.trim()}>
          Send
        </Button>
      </form>
    </section>
  );
}

// ── Players ──

const LISTS: { id: PlayerList; title: string; description: string; empty: string }[] = [
  { id: "whitelist", title: "Whitelist", description: "Who may join. Friends who join through Wisp are added by themselves.", empty: "Nobody yet." },
  { id: "ops", title: "Operators", description: "Who can use commands in the game.", empty: "Nobody. You can always type commands in the console." },
  { id: "banned", title: "Banned", description: "Who can't join, whatever the whitelist says.", empty: "Nobody." },
];

function PlayersTab({ server }: { server: HostedView }) {
  const { notify } = useApp();
  const [players, setPlayers] = useState<ServerPlayers | null>(null);
  const [names, setNames] = useState<Record<PlayerList, string>>({ whitelist: "", ops: "", banned: "" });
  const [busy, setBusy] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    getServerPlayers(server.id).then(
      (found) => active && setPlayers(found),
      () => active && setPlayers({ whitelist: [], ops: [], banned: [], online: [] }),
    );
    return () => {
      active = false;
    };
  }, [server.id, server.players.length, server.state]);

  const change = async (list: PlayerList, name: string, add: boolean) => {
    setBusy(`${list}-${name}`);
    try {
      setPlayers(await setServerPlayer(server.id, list, name, add));
      if (add) setNames((current) => ({ ...current, [list]: "" }));
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't change that", body: message(failure) });
    } finally {
      setBusy(null);
    }
  };

  if (!players) return <Skeleton className="h-64 rounded-[24px]" />;
  return (
    <div className="grid items-start gap-5 xl:grid-cols-3">
      {LISTS.map((list) => {
        const entries = players[list.id];
        const name = names[list.id];
        return (
          <Panel key={list.id} title={list.title} description={list.description}>
            <form
              onSubmit={(event) => {
                event.preventDefault();
                if (name.trim()) void change(list.id, name.trim(), true);
              }}
              className="mt-2 flex gap-2"
            >
              <label className="flex h-10 min-w-0 flex-1 items-center rounded-xl border border-cream/12 bg-ink-900/60 px-3 focus-within:border-cream/35">
                <span className="sr-only">Player name</span>
                <input
                  value={name}
                  maxLength={16}
                  onChange={(event) => setNames((current) => ({ ...current, [list.id]: event.target.value }))}
                  placeholder="Minecraft name"
                  spellCheck={false}
                  className="min-w-0 flex-1 bg-transparent text-sm text-cream outline-none placeholder:text-mute-2"
                />
              </label>
              <Button type="submit" variant="secondary" disabled={!name.trim() || busy !== null}>
                Add
              </Button>
            </form>
            {entries.length === 0 ? (
              <p className="mt-3 text-sm text-mute">{list.empty}</p>
            ) : (
              <ul className="mt-2 flex flex-col divide-y divide-cream/7">
                {entries.map((entry) => (
                  <li key={entry.name} className="flex items-center gap-2 py-2">
                    <span className={cn("size-1.5 shrink-0 rounded-full", players.online.includes(entry.name) ? "bg-moss" : "bg-mute-2")} aria-hidden="true" />
                    <span className="min-w-0 flex-1 truncate text-sm font-semibold">{entry.name}</span>
                    <button
                      type="button"
                      onClick={() => void change(list.id, entry.name, false)}
                      disabled={busy !== null}
                      className="rounded-full px-2.5 py-1 text-xs font-bold text-cream-300 hover:bg-cream/8 hover:text-cream disabled:opacity-50"
                    >
                      {busy === `${list.id}-${entry.name}` ? "…" : "Remove"}
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {list.id !== "banned" && <p className="mt-3 text-xs text-mute-2">{ACCOUNTS_NOTE[server.accounts]}</p>}
          </Panel>
        );
      })}
    </div>
  );
}

const ACCOUNTS_NOTE: Record<HostedView["accounts"], string> = {
  mojang: "Names are checked with Mojang.",
  wisp: "Players sign in through Wisp, wearing their Wisp skins.",
  offline: "Nobody's account is checked: keep the whitelist on.",
};

// ── Settings ──

type Field = { key: string; label: string; kind: "text" | "number" | "bool" | "choice"; choices?: SelectOption<string>[]; hint?: string };
const FIELDS: Field[] = [
  { key: "motd", label: "Message in the server list", kind: "text" },
  {
    key: "difficulty",
    label: "Difficulty",
    kind: "choice",
    choices: ["peaceful", "easy", "normal", "hard"].map((value) => ({ value, label: value.replace(/^./, (c) => c.toUpperCase()) })),
  },
  {
    key: "gamemode",
    label: "Game mode for new players",
    kind: "choice",
    choices: ["survival", "creative", "adventure", "spectator"].map((value) => ({ value, label: value.replace(/^./, (c) => c.toUpperCase()) })),
  },
  { key: "max-players", label: "Most players at once", kind: "number" },
  { key: "pvp", label: "Players can hurt each other", kind: "bool" },
  { key: "white-list", label: "Only whitelisted players", kind: "bool", hint: "Friends who join through Wisp are added to it by themselves." },
  { key: "hardcore", label: "Hardcore", kind: "bool" },
  { key: "allow-flight", label: "Allow flying (some mods need it)", kind: "bool" },
  { key: "view-distance", label: "View distance (chunks)", kind: "number" },
  { key: "simulation-distance", label: "Simulation distance (chunks)", kind: "number" },
  { key: "spawn-protection", label: "Spawn protection (blocks)", kind: "number" },
  { key: "server-port", label: "Port", kind: "number" },
];
const SHOWN = new Set(FIELDS.map((field) => field.key));

function SettingsTab({ server, onChange, onDeleted }: { server: HostedView; onChange: (server: HostedView) => void; onDeleted: () => void }) {
  const { notify, home } = useApp();
  const [saved, setSaved] = useState<[string, string][] | null>(null);
  const [edits, setEdits] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [systemMb, setSystemMb] = useState<number | null>(null);
  const [replacing, setReplacing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const live = server.state !== "stopped" && server.state !== "crashed";

  useEffect(() => {
    let active = true;
    getServerProperties(server.id).then(
      (found) => active && setSaved(found),
      () => active && setSaved([]),
    );
    getSystemMemory().then((mb) => active && setSystemMb(mb), () => {});
    return () => {
      active = false;
    };
  }, [server.id]);

  const values = useMemo(() => Object.fromEntries(saved ?? []), [saved]);
  const value = (key: string) => edits[key] ?? values[key] ?? "";
  const dirty = Object.entries(edits).some(([key, edited]) => (values[key] ?? "") !== edited);
  const others = (saved ?? []).filter(([key]) => !SHOWN.has(key));

  const save = async () => {
    setSaving(true);
    try {
      const changed = Object.entries(edits).filter(([key, edited]) => (values[key] ?? "") !== edited) as [string, string][];
      setSaved(await setServerProperties(server.id, changed));
      setEdits({});
      notify({ tone: "success", title: "Saved", body: live ? "Restart the server for the changes to apply." : "They apply the next time it starts." });
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't save the settings", body: message(failure) });
    } finally {
      setSaving(false);
    }
  };
  const update = (changes: Parameters<typeof updateHostedServer>[1]) =>
    void updateHostedServer(server.id, changes).then(onChange, (failure: unknown) => notify({ tone: "info", title: "Couldn't change that", body: message(failure) }));

  const maxMb = Math.max(4096, Math.floor(((systemMb ?? 16384) * 0.75) / 512) * 512);
  return (
    <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_360px]">
      <Panel
        title="Server settings"
        description={live ? "The server reads these when it starts: restart it after saving." : "What's in server.properties."}
        action={
          <Button onClick={() => void save()} disabled={!dirty || saving}>
            {saving ? "Saving…" : "Save"}
          </Button>
        }
      >
        {saved === null ? (
          <Skeleton className="mt-2 h-64 rounded-2xl" />
        ) : (
          <div className="mt-2 flex flex-col divide-y divide-cream/7">
            {FIELDS.map((field) => (
              <div key={field.key} className="flex flex-wrap items-center gap-3 py-3">
                <div className="min-w-[200px] flex-1">
                  <p className="text-sm font-semibold">{field.label}</p>
                  {field.hint && <p className="text-xs text-mute">{field.hint}</p>}
                </div>
                {field.kind === "bool" ? (
                  <Toggle compact label={field.label} checked={value(field.key) === "true"} onChange={(checked) => setEdits((current) => ({ ...current, [field.key]: String(checked) }))} />
                ) : field.kind === "choice" ? (
                  <Select label={field.label} value={value(field.key) || (field.choices?.[0]?.value ?? "")} options={field.choices ?? []} onChange={(picked) => setEdits((current) => ({ ...current, [field.key]: picked }))} className="w-44" />
                ) : (
                  <label className="flex h-10 w-44 items-center rounded-xl border border-cream/12 bg-ink-900/60 px-3 focus-within:border-cream/35">
                    <span className="sr-only">{field.label}</span>
                    <input
                      value={value(field.key)}
                      inputMode={field.kind === "number" ? "numeric" : undefined}
                      onChange={(event) => setEdits((current) => ({ ...current, [field.key]: field.kind === "number" ? event.target.value.replace(/[^0-9]/g, "") : event.target.value }))}
                      className="min-w-0 flex-1 bg-transparent text-sm text-cream outline-none"
                    />
                  </label>
                )}
              </div>
            ))}
            {others.length > 0 && (
              <details className="py-3">
                <summary className="cursor-pointer text-sm font-bold text-cream-300 hover:text-cream">Every other setting ({others.length})</summary>
                <div className="mt-3 flex flex-col gap-2">
                  {others.map(([key]) => (
                    <label key={key} className="flex items-center gap-3">
                      <span className="w-56 shrink-0 truncate font-mono text-xs text-mute">{key}</span>
                      <input
                        value={value(key)}
                        onChange={(event) => setEdits((current) => ({ ...current, [key]: event.target.value }))}
                        className="h-9 min-w-0 flex-1 rounded-lg border border-cream/12 bg-ink-900/60 px-2.5 font-mono text-xs text-cream outline-none focus:border-cream/35"
                      />
                    </label>
                  ))}
                </div>
              </details>
            )}
          </div>
        )}
      </Panel>
      <aside className="flex flex-col gap-5">
        <section className="wisp-surface rounded-[24px] p-5">
          <h2 className="font-display text-lg font-bold">Hosting</h2>
          <div className="mt-1 flex flex-col divide-y divide-cream/7">
            <Toggle label="Friends can join" description="Everyone on your friends list, while it runs. Off: only friends you invite." checked={server.friendsJoin} onChange={(friendsJoin) => update({ friendsJoin })} />
            <Toggle label="Open the port on my router" description="Lets anyone with the address join (whitelist still applies). Needs UPnP on the router. Applies from the next start." checked={server.openPort} onChange={(openPort) => update({ openPort })} />
            <Toggle label="Back up the world when it stops" description="A restore point each time, stored once per changed file." checked={server.backups} onChange={(backups) => update({ backups })} />
          </div>
        </section>
        <section className="wisp-surface rounded-[24px] p-5">
          <h2 className="font-display text-lg font-bold">Memory</h2>
          <Toggle label="Let Wisp pick" description="More for more mods, leaving room for you to play on this computer too." checked={server.automaticMemory} onChange={(automatic) => update(automatic ? { automaticMemory: true } : { memoryMb: server.effectiveMemoryMb })} />
          {!server.automaticMemory && (
            <div className="pt-1">
              <p className="mb-2 text-sm font-semibold">Memory for the server</p>
              <MemoryCommit server={server} max={maxMb} onCommit={(memoryMb) => update({ memoryMb })} />
            </div>
          )}
        </section>
        <section className="wisp-surface flex flex-col gap-3 rounded-[24px] p-5">
          <h2 className="font-display text-lg font-bold">World</h2>
          <p className="text-sm leading-relaxed text-mute">Swap in a copy of one of your singleplayer worlds. The current one is kept as a backup and in the server's folder.</p>
          <Button variant="secondary" icon={Globe2} disabled={live || !home?.worlds.length} onClick={() => setReplacing(true)}>
            Use another world
          </Button>
          <div className="mt-2 border-t border-cream/7 pt-3">
            <Button variant="secondary" icon={Trash2} disabled={live} onClick={() => setDeleting(true)}>
              Delete the server
            </Button>
            {live && <p className="mt-2 text-xs text-mute">Stop it first.</p>}
          </div>
        </section>
      </aside>
      {replacing && <ReplaceWorldDialog server={server} onClose={() => setReplacing(false)} onReplaced={onChange} />}
      <Dialog
        open={deleting}
        onClose={() => setDeleting(false)}
        title={`Delete ${server.name}?`}
        description="Its folder, world included, goes to the Trash. Backups of its world stay in Wisp until you clear them."
        footer={
          <>
            <Button variant="secondary" onClick={() => setDeleting(false)}>
              Keep it
            </Button>
            <Button
              icon={Trash2}
              onClick={() =>
                void deleteHostedServer(server.id).then(
                  () => {
                    setDeleting(false);
                    notify({ tone: "success", title: `Deleted ${server.name}`, body: "Its folder is in the Trash." });
                    onDeleted();
                  },
                  (failure: unknown) => notify({ tone: "info", title: "Couldn't delete it", body: message(failure) }),
                )
              }
            >
              Delete
            </Button>
          </>
        }
      >
        <p className="text-sm text-mute">You can take it back out of the Trash.</p>
      </Dialog>
    </div>
  );
}

/** The memory slider for a server with its own memory: saved when let go. */
function MemoryCommit({ server, max, onCommit }: { server: HostedView; max: number; onCommit: (memoryMb: number) => void }) {
  const [value, setValue] = useState(server.effectiveMemoryMb);
  useEffect(() => setValue(server.effectiveMemoryMb), [server.effectiveMemoryMb]);
  return (
    <span className="flex items-center gap-2">
      <input
        type="range"
        min={1024}
        max={max}
        step={512}
        value={value}
        aria-label="Memory for the server"
        onChange={(event) => setValue(Number(event.target.value))}
        onPointerUp={() => value !== server.effectiveMemoryMb && onCommit(value)}
        onKeyUp={() => value !== server.effectiveMemoryMb && onCommit(value)}
        className="w-full accent-[#f4eee4]"
      />
      <span className="w-14 shrink-0 text-right text-xs font-bold text-cream-300">{(value / 1024).toFixed(value % 1024 === 0 ? 0 : 1)} GB</span>
    </span>
  );
}

function ReplaceWorldDialog({ server, onClose, onReplaced }: { server: HostedView; onClose: () => void; onReplaced: (server: HostedView) => void }) {
  const { home, notify } = useApp();
  const worlds = home?.worlds ?? [];
  const [picked, setPicked] = useState(worlds[0]?.id ?? "");
  const [busy, setBusy] = useState(false);
  const world = worlds.find((candidate) => candidate.id === picked) ?? null;
  return (
    <Dialog
      open
      onClose={onClose}
      title="Use another world"
      description="The server gets a copy; the singleplayer world stays as it is."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button
            icon={Globe2}
            disabled={!world || busy}
            onClick={() => {
              if (!world) return;
              setBusy(true);
              void setServerWorld(server.id, { instanceId: world.instanceId, folder: world.folder }).then(
                (updated) => {
                  onReplaced(updated);
                  notify({ tone: "success", title: `${server.name} has a new world`, body: `A copy of ${world.name}. The old one is in the backups.` });
                  onClose();
                },
                (failure: unknown) => {
                  setBusy(false);
                  notify({ tone: "info", title: "Couldn't swap the world", body: message(failure) });
                },
              );
            }}
          >
            {busy ? "Copying…" : "Use this world"}
          </Button>
        </>
      }
    >
      <Select label="World" value={picked} options={worlds.map((candidate) => ({ value: candidate.id, label: candidate.name, note: candidate.instanceName }))} onChange={setPicked} searchable />
      {world && world.loader !== `${loaderName(server.loader)} ${server.gameVersion}` && (
        <p className="mt-3 text-xs leading-relaxed text-amber-soft">
          It was last played with {world.loader}; the server runs {loaderName(server.loader)} {server.gameVersion}. Worlds move forward to newer versions, but not back.
        </p>
      )}
    </Dialog>
  );
}

// ── Backups ──

function BackupsTab({ server }: { server: HostedView }) {
  const { notify, now } = useApp();
  const [backups, setBackups] = useState<RestorePoint[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<RestorePoint | null>(null);
  const live = server.state !== "stopped" && server.state !== "crashed";
  const reload = () => {
    getServerBackups(server.id).then(setBackups, () => setBackups([]));
  };
  useEffect(reload, [server.id, server.state]); // eslint-disable-line react-hooks/exhaustive-deps

  return (
    <Panel
      title="Backups"
      description={server.backups ? "A restore point of the world each time the server stops. Each one stores only what changed." : "Backups when it stops are off (Settings)."}
      action={
        <Button
          variant="secondary"
          icon={RotateCcw}
          disabled={busy !== null || !server.worldExists}
          onClick={() => {
            setBusy("backup");
            void backUpServer(server.id)
              .then(
                (point) => {
                  notify({ tone: "success", title: "Backed up", body: `${point.files} files, ${point.addedBytes > 0 ? `${formatBytes(point.addedBytes)} new` : "nothing new to store"}.` });
                  reload();
                },
                (failure: unknown) => notify({ tone: "info", title: "Couldn't back it up", body: message(failure) }),
              )
              .finally(() => setBusy(null));
          }}
        >
          {busy === "backup" ? "Backing up…" : "Back up now"}
        </Button>
      }
    >
      {backups === null ? (
        <Skeleton className="mt-2 h-32 rounded-2xl" />
      ) : backups.length === 0 ? (
        <p className="mt-2 text-sm text-mute">None yet. The first one is made when the server stops.</p>
      ) : (
        <ul className="mt-1 flex flex-col divide-y divide-cream/7">
          {backups.map((point) => (
            <li key={point.id} className="flex items-center gap-3 py-2.5">
              <RotateCcw size={15} className="shrink-0 text-mute" aria-hidden="true" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-semibold">{point.label ?? (point.reason === "manual" ? "Backed up by hand" : "When the server stopped")}</p>
                <p className="text-xs text-mute">
                  {timeAgo(point.createdAt, now)} · {formatBytes(point.totalBytes)}
                </p>
              </div>
              <button
                type="button"
                disabled={live || busy !== null}
                onClick={() => setConfirm(point)}
                title={live ? "Stop the server first" : undefined}
                className="rounded-full border border-cream/15 px-3 py-1.5 text-xs font-bold text-cream-300 transition-colors enabled:hover:border-cream/35 enabled:hover:text-cream disabled:opacity-40"
              >
                Restore
              </button>
            </li>
          ))}
        </ul>
      )}
      <Dialog
        open={confirm !== null}
        onClose={() => setConfirm(null)}
        title="Go back to this backup?"
        description="The world as it is now is kept as a backup first, so you can come back to it."
        footer={
          <>
            <Button variant="secondary" onClick={() => setConfirm(null)}>
              Cancel
            </Button>
            <Button
              icon={RotateCcw}
              onClick={() => {
                const point = confirm;
                setConfirm(null);
                if (!point) return;
                setBusy("restore");
                void restoreServerBackup(server.id, point.id)
                  .then(
                    () => {
                      notify({ tone: "success", title: "Restored", body: "The world is back to how it was." });
                      reload();
                    },
                    (failure: unknown) => notify({ tone: "info", title: "Couldn't restore it", body: message(failure) }),
                  )
                  .finally(() => setBusy(null));
              }}
            >
              Restore
            </Button>
          </>
        }
      >
        <p className="text-sm text-mute">{confirm ? `From ${timeAgo(confirm.createdAt, now)}.` : ""}</p>
      </Dialog>
    </Panel>
  );
}
