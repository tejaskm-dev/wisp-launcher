import {
  ArrowLeft,
  Cpu,
  FolderOpen,
  Gamepad2,
  Globe2,
  HardDrive,
  ImagePlus,
  LoaderCircle,
  Package,
  PackageMinus,
  Play,
  RotateCcw,
  Send,
  Server,
  Settings2,
  ShieldCheck,
  Square,
  Terminal,
  Trash2,
  Undo2,
  UserPlus,
  Users,
} from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AddressPanel } from "../components/hosting/AddressPanel";
import { ServerContentTab } from "../components/hosting/ServerContentTab";
import { hostedStatus, softwareName, TONE_DOT, TONE_TEXT } from "../components/hosting/describe";
import { useHostedServer } from "../components/hosting/useHosted";
import { InstanceAvatar } from "../components/instances/InstanceAvatar";
import { Button } from "../components/ui/Button";
import { Dialog } from "../components/ui/Dialog";
import { EmptyState } from "../components/ui/EmptyState";
import { Page, Panel } from "../components/ui/Page";
import { Popover } from "../components/ui/Popover";
import { ProgressBar } from "../components/ui/ProgressBar";
import { Select, type SelectOption } from "../components/ui/Select";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { Toggle } from "../components/ui/Toggle";
import {
  addServerFiles,
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
  pickServerIcon,
  onFileDrop,
  playHostedServer,
  putBackServerMod,
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
type Tab = "overview" | "console" | "content" | "players" | "settings" | "backups";
const TABS: { id: Tab; label: string; icon: typeof Server }[] = [
  { id: "overview", label: "Overview", icon: Server },
  { id: "console", label: "Console", icon: Terminal },
  { id: "content", label: "Mods", icon: Package },
  { id: "players", label: "Players", icon: Users },
  { id: "settings", label: "Settings", icon: Settings2 },
  { id: "backups", label: "Backups", icon: RotateCcw },
];
/** The tabs a server has: plugin servers have Plugins, modded ones Mods, Vanilla neither. */
const tabsFor = (server: HostedView) =>
  TABS.flatMap((tab) => {
    if (tab.id !== "content") return [tab];
    if (server.software) return [{ ...tab, label: "Plugins" }];
    return server.loader === "vanilla" ? [] : [tab];
  });
const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure));

/** One server hosted on this computer: start and stop it, who can join and how, its console, players, settings and backups. */
export function HostedServerPage({ serverId, backTo, onBack }: { serverId: string; backTo: PageId; onBack: () => void }) {
  const { notify, now } = useApp();
  const { server, setServer, console, reload } = useHostedServer(
    serverId,
    (failure) => notify({ tone: "info", title: "The server couldn't start", body: failure }),
    (fixed) => notify({ tone: "success", title: "Wisp fixed the server", body: fixed }),
  );
  const [tab, setTab] = useState<Tab>("overview");
  const [busy, setBusy] = useState<string | null>(null);
  // Files dropped anywhere on the page go into this server: plugins, mods, data packs.
  const [dropping, setDropping] = useState(false);
  const [added, setAdded] = useState(0);
  useEffect(
    () =>
      onFileDrop((drop) => {
        if (drop.type === "over") return setDropping(drop.paths.length > 0);
        setDropping(false);
        if (drop.type !== "drop" || drop.paths.length === 0) return;
        void addServerFiles(serverId, drop.paths).then(
          (names) => {
            if (names.length === 0) return;
            notify({ tone: "success", title: names.length === 1 ? `Added ${names[0]}` : `Added ${names.length} files`, body: "They load when the server next starts." });
            setAdded((count) => count + 1);
            setTab("content");
          },
          (failure: unknown) => notify({ tone: "info", title: "Couldn't add that", body: message(failure) }),
        );
      }),
    [serverId, notify],
  );

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
              Hosted on this computer · {softwareName(server)} {server.gameVersion}
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
        <MemoryStat server={server} onChange={setServer} />
        <Stat icon={<HardDrive size={13} aria-hidden="true" />} label={server.software ? "Plugins" : "Mods"} value={server.modCount > 0 ? server.modCount.toLocaleString() : "None"} />
        <Stat icon={<Globe2 size={13} aria-hidden="true" />} label="World" value={server.worldExists ? "Ready" : "Made on first start"} />
      </m.dl>

      <m.div variants={fadeUp} className="flex flex-col gap-5">
        <div role="tablist" aria-label={server.name} className="flex w-fit max-w-full gap-1 overflow-x-auto rounded-full border border-cream/8 bg-ink-850 p-1">
          {tabsFor(server).map((option) => {
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
            {tab === "content" && <ServerContentTab key={added} server={server} />}
            {tab === "players" && <PlayersTab server={server} />}
            {tab === "settings" && <SettingsTab server={server} onChange={setServer} onDeleted={onBack} />}
            {tab === "backups" && <BackupsTab server={server} />}
          </m.div>
        </AnimatePresence>
      </m.div>
      {dropping && (
        <div className="pointer-events-none fixed inset-4 z-50 grid place-items-center rounded-[28px] border-2 border-dashed border-cream/40 bg-ink-900/80 text-center backdrop-blur-sm">
          <div>
            <p className="font-display text-2xl font-bold">Drop to add to {server.name}</p>
            <p className="mt-1 text-sm text-mute">{server.software ? "Plugins (.jar), zips of them, and data packs." : "Mods (.jar), zips of them, and data packs."}</p>
          </div>
        </div>
      )}
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

/** The memory figure up top, which opens the memory control. */
function MemoryStat({ server, onChange }: { server: HostedView; onChange: (server: HostedView) => void }) {
  const { notify } = useApp();
  const [open, setOpen] = useState(false);
  const [systemMb, setSystemMb] = useState<number | null>(null);
  const anchor = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (open && systemMb === null) getSystemMemory().then(setSystemMb, () => setSystemMb(16384));
  }, [open, systemMb]);
  const update = (changes: Parameters<typeof updateHostedServer>[1]) =>
    void updateHostedServer(server.id, changes).then(onChange, (failure: unknown) => notify({ tone: "info", title: "Couldn't change the memory", body: message(failure) }));
  const max = Math.max(4096, Math.floor(((systemMb ?? 16384) * 0.75) / 512) * 512);
  return (
    <>
      <button
        ref={anchor}
        type="button"
        onClick={() => setOpen((shown) => !shown)}
        aria-expanded={open}
        className="rounded-2xl border border-cream/7 bg-ink-800 px-4 py-3.5 text-left transition-colors hover:border-cream/20"
      >
        <span className="flex items-center gap-1.5 text-xs font-semibold text-mute">
          <Cpu size={13} aria-hidden="true" />
          Memory
        </span>
        <span className="mt-1 block truncate font-display text-lg font-bold">
          {gigabytes(server.effectiveMemoryMb)}
          <span className="ml-1.5 text-xs font-semibold text-mute">{server.automaticMemory ? "auto" : "change"}</span>
        </span>
      </button>
      <Popover open={open} anchorRef={anchor} placement="bottom-start" onClose={() => setOpen(false)} label="Memory for the server" className="w-80 p-4">
        <MemoryControl server={server} max={max} update={update} />
      </Popover>
    </>
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
        {server.setAside.length > 0 && <SetAsidePanel server={server} onChange={onChange} />}
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
        <AddressPanel server={server} onChange={onChange} />
        {server.reach?.strictRouter && !server.reach.routerOpened && !server.address && (
          <p className="rounded-2xl bg-amber/8 p-4 text-xs leading-relaxed text-amber-soft">
            Your router is strict about incoming connections, so a friend whose router is strict too may not get through directly. An address from playit fixes that for everyone.
          </p>
        )}
      </aside>
    </div>
  );
}

/** Mods Wisp took out of the server because they only run in the game. */
function SetAsidePanel({ server, onChange }: { server: HostedView; onChange: (server: HostedView) => void }) {
  const { notify } = useApp();
  const [busy, setBusy] = useState<string | null>(null);
  const putBack = async (file: string, name: string) => {
    setBusy(file);
    try {
      onChange(await putBackServerMod(server.id, file));
      notify({ tone: "success", title: `${name} is back in the server`, body: "It starts with it from now on." });
    } catch (failure: unknown) {
      notify({ tone: "info", title: `Couldn't put ${name} back`, body: message(failure) });
    } finally {
      setBusy(null);
    }
  };
  return (
    <Panel title="Set aside by Wisp" icon={<PackageMinus size={19} aria-hidden="true" />} description="These only run in the game, so they stopped the server as it loaded. Players still get them when they join.">
      <ul className="mt-1 flex flex-col divide-y divide-cream/7">
        {server.setAside.map((mod) => (
          <li key={mod.file} className="flex items-center gap-3 py-2.5">
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-bold">{mod.name || mod.file}</p>
              <p className="truncate text-xs text-mute">{mod.file}</p>
            </div>
            <button
              type="button"
              onClick={() => void putBack(mod.file, mod.name || mod.file)}
              disabled={busy !== null}
              className="flex h-8 items-center gap-1.5 rounded-full border border-cream/15 px-3 text-xs font-bold text-cream-300 transition-colors hover:border-cream/35 hover:text-cream disabled:opacity-50"
            >
              {busy === mod.file ? <LoaderCircle size={12} className="animate-spin" aria-hidden="true" /> : <Undo2 size={12} aria-hidden="true" />}
              Put back
            </button>
          </li>
        ))}
      </ul>
    </Panel>
  );
}

// ── Console ──

const LEVEL_TONE = { info: "text-cream-300", warn: "text-amber-soft", error: "text-coral", input: "text-sky", wisp: "font-semibold text-ember-soft" } as const;

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
        <RunningSection server={server} update={update} />
        <section className="wisp-surface rounded-[24px] p-5">
          <h2 className="font-display text-lg font-bold">Memory</h2>
          <MemoryControl server={server} max={maxMb} update={update} />
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

const RESTARTS: SelectOption<string>[] = [
  { value: "0", label: "Never" },
  { value: "6", label: "Every 6 hours" },
  { value: "12", label: "Every 12 hours" },
  { value: "24", label: "Once a day" },
];
const JAVAS: SelectOption<string>[] = [
  { value: "0", label: "Automatic", note: "What its version wants" },
  { value: "8", label: "Java 8" },
  { value: "17", label: "Java 17" },
  { value: "21", label: "Java 21" },
  { value: "25", label: "Java 25" },
];

/** How it runs: starting with Wisp, scheduled restarts, Java and its flags, the icon. */
function RunningSection({ server, update }: { server: HostedView; update: (changes: Parameters<typeof updateHostedServer>[1]) => void }) {
  const { notify } = useApp();
  const [args, setArgs] = useState(server.jvmArgs ?? "");
  useEffect(() => setArgs(server.jvmArgs ?? ""), [server.jvmArgs]);
  const pickIcon = () =>
    void pickServerIcon(server.id).then(
      (picked) => picked && notify({ tone: "success", title: "Icon set", body: "It shows beside the server in the game's server list." }),
      (failure: unknown) => notify({ tone: "info", title: "Couldn't use that picture", body: message(failure) }),
    );
  return (
    <section className="wisp-surface rounded-[24px] p-5">
      <h2 className="font-display text-lg font-bold">Running it</h2>
      <div className="mt-1 flex flex-col divide-y divide-cream/7">
        <Toggle label="Start it when Wisp opens" description="It's up whenever Wisp is, without a click." checked={server.startWithWisp ?? false} onChange={(startWithWisp) => update({ startWithWisp })} />
        <div className="flex items-center gap-3 py-3">
          <div className="min-w-0 flex-1">
            <p className="text-sm font-semibold">Restart it</p>
            <p className="text-xs text-mute">Keeps a long-running server fresh. Players get a warning first.</p>
          </div>
          <Select label="Restart it" value={String(server.restartEveryHours ?? 0)} options={RESTARTS} onChange={(hours) => update({ restartEveryHours: Number(hours) })} className="w-40" />
        </div>
        <div className="flex items-center gap-3 py-3">
          <div className="min-w-0 flex-1">
            <p className="text-sm font-semibold">Java</p>
            <p className="text-xs text-mute">Some mods need a newer one than the game asks for.</p>
          </div>
          <Select label="Java" value={String(server.javaMajor ?? 0)} options={JAVAS} onChange={(major) => update({ javaMajor: Number(major) })} className="w-40" />
        </div>
        <Toggle label="Aikar's flags" description="Java tuning most busy servers use: steadier ticks with lots of players or mods." checked={server.aikarFlags ?? false} onChange={(aikarFlags) => update({ aikarFlags })} />
        <form
          className="flex flex-col gap-1.5 py-3"
          onSubmit={(event) => {
            event.preventDefault();
            if (args.trim() !== (server.jvmArgs ?? "")) update({ jvmArgs: args.trim() });
          }}
        >
          <p className="text-sm font-semibold">Java arguments</p>
          <input
            value={args}
            onChange={(event) => setArgs(event.target.value)}
            onBlur={() => args.trim() !== (server.jvmArgs ?? "") && update({ jvmArgs: args.trim() })}
            placeholder="-Dsome.setting=true"
            spellCheck={false}
            aria-label="Java arguments"
            className="h-9 rounded-xl border border-cream/12 bg-ink-900/60 px-3 font-mono text-xs text-cream outline-none placeholder:text-mute-2 focus:border-cream/35"
          />
          <p className="text-xs text-mute">Added after Wisp's own, so yours win. Applies from the next start.</p>
        </form>
        <div className="flex items-center gap-3 pt-3">
          <div className="min-w-0 flex-1">
            <p className="text-sm font-semibold">Icon</p>
            <p className="text-xs text-mute">Beside it in the game's server list.</p>
          </div>
          <Button variant="secondary" icon={ImagePlus} className="h-9 px-4 text-xs" onClick={pickIcon}>
            Choose
          </Button>
        </div>
      </div>
    </section>
  );
}

/** GB, the way people say it: "4 GB", "4.5 GB". */
const gigabytes = (mb: number) => `${(mb / 1024).toFixed(mb % 1024 === 0 ? 0 : 1)} GB`;

/**
 * The server's memory: always a slider. Moving it sets the server's own amount (saved a moment
 * after you let go); Auto hands it back to Wisp, which gives more for more mods and leaves room
 * for you to play on this computer too.
 */
function MemoryControl({ server, max, update }: { server: HostedView; max: number; update: (changes: Parameters<typeof updateHostedServer>[1]) => void }) {
  const [value, setValue] = useState(server.effectiveMemoryMb);
  const timer = useRef<number | null>(null);
  useEffect(() => setValue(server.effectiveMemoryMb), [server.effectiveMemoryMb]);
  useEffect(() => () => void (timer.current !== null && window.clearTimeout(timer.current)), []);
  const pick = (mb: number) => {
    setValue(mb);
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      timer.current = null;
      if (mb !== server.effectiveMemoryMb || server.automaticMemory) update({ memoryMb: mb });
    }, 450);
  };
  const live = server.state !== "stopped" && server.state !== "crashed";
  return (
    <div className="mt-2 flex flex-col gap-3">
      <div className="flex items-center gap-3">
        <span className="font-display text-2xl font-bold tabular-nums">{gigabytes(value)}</span>
        <div className="ml-auto flex rounded-full border border-cream/12 p-0.5" role="group" aria-label="Who picks the memory">
          <button
            type="button"
            aria-pressed={server.automaticMemory}
            onClick={() => !server.automaticMemory && update({ automaticMemory: true })}
            className={cn("h-8 rounded-full px-3 text-xs font-bold transition-colors", server.automaticMemory ? "bg-cream text-ink-900" : "text-cream-300 hover:text-cream")}
          >
            Auto
          </button>
          <button
            type="button"
            aria-pressed={!server.automaticMemory}
            onClick={() => server.automaticMemory && update({ memoryMb: server.effectiveMemoryMb })}
            className={cn("h-8 rounded-full px-3 text-xs font-bold transition-colors", !server.automaticMemory ? "bg-cream text-ink-900" : "text-cream-300 hover:text-cream")}
          >
            Custom
          </button>
        </div>
      </div>
      <input
        type="range"
        min={1024}
        max={Math.max(max, value)}
        step={512}
        value={value}
        aria-label="Memory for the server"
        aria-valuetext={gigabytes(value)}
        onChange={(event) => pick(Number(event.target.value))}
        className="w-full accent-[#f4eee4]"
      />
      <div className="flex justify-between text-[11px] font-semibold text-mute-2">
        <span>1 GB</span>
        <span>{gigabytes(Math.max(max, value))}</span>
      </div>
      <p className="text-xs leading-relaxed text-mute">
        {server.automaticMemory
          ? "Wisp picks it: more for more mods, leaving room for you to play on this computer too. Drag to set your own."
          : `Your own amount. Wisp would pick ${gigabytes(server.pickedMemoryMb)}.`}
        {live ? " Applies when the server restarts." : ""}
      </p>
    </div>
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
