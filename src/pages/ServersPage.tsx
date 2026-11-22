import { Check, Compass, Copy, KeyRound, MoreHorizontal, Play, Plus, RefreshCw, Server, ServerCog, Square, Trash2, Users } from "lucide-react";
import { m } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { Ghost } from "../components/brand/Ghost";
import { HostedServerCard } from "../components/hosting/HostedServerCard";
import { JoinCodeDialog } from "../components/hosting/JoinCodeDialog";
import { useHostedServers } from "../components/hosting/useHosted";
import { describeLaunch } from "../components/launch/describe";
import { AddServerDialog } from "../components/servers/AddServerDialog";
import { readDiscoverMode, saveDiscoverMode, sameServer } from "../components/servers/address";
import { JoinServerDialog, type JoinTarget } from "../components/servers/JoinServerDialog";
import { RowButton, ServerRow } from "../components/servers/ServerRow";
import { useServerStatuses } from "../components/servers/useServerStatuses";
import { Button } from "../components/ui/Button";
import { Dialog } from "../components/ui/Dialog";
import { EmptyState } from "../components/ui/EmptyState";
import { Page } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { PageTabs } from "../components/ui/PageTabs";
import { MenuItem, Popover } from "../components/ui/Popover";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { joinServerAddress, onJoinProgress, removeServer, startHostedServer, stopHostedServer } from "../data/api";
import type { JoinProgress, ServerSummary } from "../data/types";
import { timeAgo } from "../lib/format";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";

/** Opens Discover on its Servers tab. */
export function useOpenServerDiscovery() {
  const { navigate } = useApp();
  return () => {
    if (readDiscoverMode() !== "servers") saveDiscoverMode("servers");
    navigate("discover");
  };
}

/** Every server in every instance's list, with live status and Quick Play. */
export function ServersPage() {
  const { home, hostServer } = useApp();
  const servers = home?.servers ?? null;
  const { statuses, loading, refresh } = useServerStatuses(servers?.map((server) => server.address) ?? []);
  const [adding, setAdding] = useState(false);
  const [joining, setJoining] = useState<JoinTarget | null>(null);
  const [joiningCode, setJoiningCode] = useState(false);
  const discover = useOpenServerDiscovery();

  return (
    <Page>
      <PageHeader
        title="Servers"
        subtitle="Servers from every instance's list, with who's online right now. Play goes straight to the server; the lists are the same ones the game's Multiplayer screen shows."
        actions={
          <>
            {servers && servers.length > 0 && (
              <Button variant="secondary" icon={RefreshCw} onClick={refresh} disabled={loading}>
                Refresh
              </Button>
            )}
            <Button variant="secondary" icon={Compass} onClick={discover}>
              Find servers
            </Button>
            <Button variant="secondary" icon={KeyRound} onClick={() => setJoiningCode(true)}>
              Join with a code
            </Button>
            <Button variant="secondary" icon={Plus} onClick={() => setAdding(true)}>
              Add server
            </Button>
            <Button icon={ServerCog} onClick={() => hostServer()}>
              Host a server
            </Button>
          </>
        }
      />
      <PageTabs
        label="Worlds and servers"
        current="servers"
        tabs={[
          { id: "worlds", label: "Worlds", count: home?.worlds.length },
          { id: "servers", label: "Servers", count: servers?.length },
        ]}
      />

      <HostedSection />

      {servers === null ? (
        <Loading label="Loading servers" className="grid gap-3 xl:grid-cols-2">
          {[0, 1, 2, 3].map((index) => (
            <Skeleton key={index} className="h-[132px] rounded-[22px]" />
          ))}
        </Loading>
      ) : servers.length === 0 ? (
        <EmptyState
          art={<Server size={40} className="text-mute" aria-hidden="true" />}
          title="No servers yet"
          body="Add one you know, or find one on Discover. Servers you add here show up in the game's Multiplayer list too."
          actions={
            <>
              <Button variant="secondary" icon={Compass} onClick={discover}>
                Find servers
              </Button>
              <Button icon={Plus} onClick={() => setAdding(true)}>
                Add server
              </Button>
            </>
          }
        />
      ) : (
        <m.ul variants={fadeUp} className="grid gap-3 xl:grid-cols-2">
          {servers.map((server) => (
            <li key={server.address} className="flex">
              <SavedServerRow server={server} status={statuses[server.address]} onJoinWith={() => setJoining({ name: server.name, address: server.address, instanceIds: server.instanceIds })} />
            </li>
          ))}
        </m.ul>
      )}

      <AddServerDialog open={adding} onClose={() => setAdding(false)} />
      {joiningCode && <JoinCodeDialog open onClose={() => setJoiningCode(false)} />}
      <JoinServerDialog target={joining} status={joining ? statuses[joining.address] : undefined} onClose={() => setJoining(null)} />
    </Page>
  );
}

/** Servers hosted on this computer, above the lists of servers to join. */
function HostedSection() {
  const { now, notify, openHosted, hostServer } = useApp();
  const { servers, reload } = useHostedServers((_, failure) => notify({ tone: "info", title: "A server couldn't start", body: failure }));
  const [busy, setBusy] = useState<string | null>(null);
  const act = async (id: string, work: () => Promise<unknown>, failTitle: string) => {
    setBusy(id);
    try {
      await work();
      reload();
    } catch (failure: unknown) {
      notify({ tone: "info", title: failTitle, body: failure instanceof Error ? failure.message : String(failure) });
    } finally {
      setBusy(null);
    }
  };

  if (servers === null) return <Skeleton className="h-[132px] rounded-[22px]" />;
  if (servers.length === 0) {
    return (
      <m.section variants={fadeUp} className="wisp-surface relative flex flex-wrap items-center gap-5 overflow-hidden rounded-[24px] p-6">
        <div className="pointer-events-none absolute -right-12 -top-16 size-64 rounded-full bg-[radial-gradient(circle,rgba(244,238,228,0.08),transparent_65%)]" />
        <Ghost size={48} floating className="drop-shadow-[0_0_14px_rgba(244,238,228,0.25)]" />
        <div className="min-w-[240px] flex-1">
          <h2 className="font-display text-xl font-extrabold tracking-[-0.01em]">Host your own server</h2>
          <p className="mt-1 text-sm leading-relaxed text-mute">On this computer, free, modded or not. Friends join with one click in Wisp and get the same mods by themselves, without opening ports or paying for a host.</p>
        </div>
        <Button icon={ServerCog} onClick={() => hostServer()}>
          Host a server
        </Button>
      </m.section>
    );
  }
  return (
    <m.section variants={fadeUp} className="flex flex-col gap-3">
      <h2 className="text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">Hosted on this computer</h2>
      <ul className="grid gap-3 xl:grid-cols-2">
        {servers.map((server) => (
          <li key={server.id} className="flex">
            <HostedServerCard
              server={server}
              now={now}
              busy={busy === server.id}
              onOpen={() => openHosted(server.id)}
              onStart={() => void act(server.id, () => startHostedServer(server.id), `Couldn't start ${server.name}`)}
              onStop={() => void act(server.id, () => stopHostedServer(server.id), `Couldn't stop ${server.name}`)}
            />
          </li>
        ))}
      </ul>
      <h2 className="mt-3 text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">Servers to join</h2>
    </m.section>
  );
}

function SavedServerRow({ server, status, onJoinWith }: { server: ServerSummary; status: ReturnType<typeof useServerStatuses>["statuses"][string] | undefined; onJoinWith: () => void }) {
  const { home, launches, launchServers, play, stop, now, notify, refreshHome } = useApp();
  const [menu, setMenu] = useState(false);
  const [confirming, setConfirming] = useState(false);
  const [copied, setCopied] = useState(false);
  const more = useRef<HTMLButtonElement>(null);
  const instanceId = server.instanceIds[0];
  const instances = server.instanceIds.map((id) => home?.instances.find((instance) => instance.id === id)).filter((instance) => instance !== undefined);
  const update = launches[instanceId];
  const view = describeLaunch(update, now);
  const mine = !!update && sameServer(launchServers[instanceId], server.address);
  const active = view.kind === "busy" || view.kind === "running";
  const blocked = active && !mine;
  // A server hosted with Wisp: Play brings its mods and your skin along.
  const wisp = status?.wisp === true;
  const [joining, setJoining] = useState<JoinProgress | null>(null);
  const [consent, setConsent] = useState<{ server: string; files: string[] } | null>(null);
  useEffect(() => (wisp ? onJoinProgress((progress) => progress.code === server.address && setJoining(progress.step === "done" || progress.step === "failed" ? null : progress)) : undefined), [wisp, server.address]);
  const joinWisp = async (allowHostFiles: boolean) => {
    setConsent(null);
    setJoining({ code: server.address, serverId: "", step: "asking", doneBytes: 0, totalBytes: 0, message: null });
    try {
      const result = await joinServerAddress(server.address, allowHostFiles);
      if (result.kind === "plain") play(instanceId, null, server.address);
      else if (result.kind === "needs-consent") setConsent(result);
    } catch (failure: unknown) {
      notify({ tone: "info", title: `Couldn't join ${server.name}`, body: failure instanceof Error ? failure.message : String(failure) });
    } finally {
      setJoining(null);
    }
  };
  const joinLabel =
    joining?.step === "mods" && joining.totalBytes > 0
      ? `Getting mods ${Math.round((joining.doneBytes / joining.totalBytes) * 100)}%`
      : joining?.step === "launching"
        ? "Starting the game"
        : joining
          ? "Connecting"
          : null;

  const remove = async () => {
    try {
      for (const id of server.instanceIds) await removeServer(id, server.address);
      await refreshHome();
      notify({ tone: "success", title: `Removed ${server.name}`, body: "It's out of the server list." });
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't remove it", body: failure instanceof Error ? failure.message : String(failure) });
    }
  };

  return (
    <>
    <ServerRow
      className="w-full"
      name={server.name}
      address={server.address}
      icon={server.icon}
      status={status}
      footer={
        <>
          <span className="flex items-center gap-1.5 rounded-full bg-cream/6 px-2.5 py-1 text-[11px] font-bold text-cream-300" title={instances.map((instance) => instance.name).join(", ")}>
            <Users size={11} aria-hidden="true" />
            {instances[0]?.name ?? "Unknown instance"}
            {instances.length > 1 && <span className="text-mute-2">+{instances.length - 1}</span>}
          </span>
          {server.lastPlayed !== null && <span className="text-[11px] text-mute-2">Played {timeAgo(server.lastPlayed, now)}</span>}
          {status?.version && <span className="truncate text-[11px] text-mute-2">{status.version}</span>}
          {wisp && (
            <span className="rounded-full bg-ember/12 px-2.5 py-1 text-[11px] font-bold text-ember-soft" title="Play from Wisp brings its mods and your skin along.">
              Hosted with Wisp
            </span>
          )}
        </>
      }
      actions={
        <>
          {mine && active ? (
            <>
              <span className="text-xs font-bold text-moss-soft">{view.label}</span>
              <RowButton onClick={() => stop(instanceId)} label={view.kind === "running" ? "Stop" : "Cancel"}>
                <Square size={9} fill="currentColor" strokeWidth={0} aria-hidden="true" />
                {view.kind === "running" ? "Stop" : "Cancel"}
              </RowButton>
            </>
          ) : (
            <RowButton primary disabled={(blocked && !wisp) || !instanceId || joining !== null} onClick={() => (wisp ? void joinWisp(false) : play(instanceId, null, server.address))} label={`Play ${server.name}`}>
              <Play size={11} fill="currentColor" strokeWidth={0} aria-hidden="true" />
              {joinLabel ?? (blocked && !wisp ? "Instance busy" : "Play")}
            </RowButton>
          )}
          <button
            ref={more}
            type="button"
            aria-label={`More for ${server.name}`}
            aria-expanded={menu}
            onClick={() => setMenu((open) => !open)}
            className="grid size-8 place-items-center rounded-full text-mute transition-colors hover:bg-cream/8 hover:text-cream"
          >
            <MoreHorizontal size={16} aria-hidden="true" />
          </button>
          <Popover open={menu} anchorRef={more} placement="bottom-end" onClose={() => (setMenu(false), setConfirming(false))} label={`${server.name} options`} className="w-[240px]">
            <div role="menu" className="flex flex-col">
              <MenuItem icon={<Play size={15} aria-hidden="true" />} label="Join with another instance" onSelect={() => (setMenu(false), onJoinWith())} />
              <MenuItem
                icon={copied ? <Check size={15} aria-hidden="true" /> : <Copy size={15} aria-hidden="true" />}
                label={copied ? "Copied" : "Copy address"}
                onSelect={() =>
                  void navigator.clipboard.writeText(server.address).then(() => {
                    setCopied(true);
                    window.setTimeout(() => setCopied(false), 1500);
                  })
                }
              />
              <div className="mx-2 my-1 h-px bg-cream/8" />
              {confirming ? (
                <div className="flex items-center gap-1 px-2 py-1.5 text-xs">
                  <span className="flex-1 font-semibold text-cream-300">Remove it?</span>
                  <button type="button" onClick={() => (setMenu(false), setConfirming(false), void remove())} className="rounded-full bg-coral px-2.5 py-1 font-bold text-ink-900">
                    Remove
                  </button>
                  <button type="button" onClick={() => setConfirming(false)} className="rounded-full px-2 py-1 font-bold text-cream-300 hover:bg-cream/8">
                    Keep
                  </button>
                </div>
              ) : (
                <MenuItem icon={<Trash2 size={15} aria-hidden="true" />} label={server.instanceIds.length > 1 ? `Remove from ${server.instanceIds.length} lists` : "Remove from the list"} onSelect={() => setConfirming(true)} />
              )}
            </div>
          </Popover>
        </>
      }
    />
    <Dialog
      open={consent !== null}
      onClose={() => setConsent(null)}
      title={`${consent?.server ?? server.name} wants more than Modrinth has`}
      description="Joining also installs files only this server has, not from Modrinth. Mods run code on your computer, so allow this only if you trust whoever runs it."
      footer={
        <>
          <Button variant="secondary" onClick={() => setConsent(null)}>
            Don't join
          </Button>
          <Button onClick={() => void joinWisp(true)}>Install and join</Button>
        </>
      }
    >
      <ul className="flex max-h-48 flex-col gap-1 overflow-y-auto rounded-xl bg-ink-900/60 p-3 font-mono text-xs text-cream-300">
        {consent?.files.map((file) => (
          <li key={file} className="truncate">
            {file}
          </li>
        ))}
      </ul>
    </Dialog>
    </>
  );
}
