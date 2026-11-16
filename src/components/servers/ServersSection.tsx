import { Compass, Play, Server, Square } from "lucide-react";
import { m } from "motion/react";
import type { ServerSummary } from "../../data/types";
import { rise } from "../../lib/motion";
import { useApp } from "../../state/AppContext";
import { useOpenServerDiscovery } from "../../pages/ServersPage";
import { describeLaunch } from "../launch/describe";
import { SectionHeader } from "../ui/SectionHeader";
import { sameServer } from "./address";
import { ServerIcon } from "./ServerIcon";
import { playersLabel, useServerStatuses } from "./useServerStatuses";

const SHOWN = 4;

/** Home's servers: the most recently played few, with who's on and Play. */
export function ServersSection({ servers }: { servers: ServerSummary[] | null }) {
  const { navigate } = useApp();
  const shown = (servers ?? []).slice(0, SHOWN);
  const { statuses } = useServerStatuses(shown.map((server) => server.address));
  const discover = useOpenServerDiscovery();

  return (
    <m.section variants={rise} aria-labelledby="servers-title" className="flex flex-col gap-3.5">
      <SectionHeader
        id="servers-title"
        title="Servers"
        icon={<Server size={17} className="text-cream-300" aria-hidden="true" />}
        count={servers && servers.length > SHOWN ? servers.length : undefined}
        action={
          servers && servers.length > 0 ? (
            <button type="button" onClick={() => navigate("servers")} className="rounded-full px-3 py-1.5 text-[13px] font-bold text-cream-300 transition-colors hover:bg-cream/8 hover:text-cream">
              See all
            </button>
          ) : undefined
        }
      />
      {servers !== null && servers.length === 0 ? (
        <button
          type="button"
          onClick={discover}
          className="group flex items-center gap-3 rounded-[20px] border border-dashed border-cream/15 p-3.5 text-left transition-colors hover:border-cream/35 hover:bg-cream/4"
        >
          <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-cream/6 text-cream-300">
            <Compass size={18} aria-hidden="true" />
          </span>
          <span className="min-w-0">
            <span className="block text-sm font-bold">Find a server</span>
            <span className="block text-xs text-mute">Popular servers, with who's online right now</span>
          </span>
        </button>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {shown.map((server) => (
            <ServerLine key={server.address} server={server} players={playersLabel(statuses[server.address])} online={statuses[server.address]?.online} icon={statuses[server.address]?.icon ?? server.icon} />
          ))}
        </ul>
      )}
    </m.section>
  );
}

function ServerLine({ server, players, online, icon }: { server: ServerSummary; players: string | null; online: boolean | undefined; icon: string | null }) {
  const { launches, launchServers, play, stop, now } = useApp();
  const instanceId = server.instanceIds[0];
  const update = launches[instanceId];
  const view = describeLaunch(update, now);
  const mine = !!update && sameServer(launchServers[instanceId], server.address);
  const active = view.kind === "busy" || view.kind === "running";

  return (
    <li className="group flex items-center gap-3 rounded-2xl p-1.5 transition-colors hover:bg-cream/5">
      <ServerIcon icon={icon} name={server.name} className="size-10 text-base" />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-bold">{server.name}</p>
        <p className="flex items-center gap-1.5 truncate text-xs text-mute">
          {online !== undefined && <span aria-hidden="true" className={online ? "size-1.5 rounded-full bg-moss" : "size-1.5 rounded-full bg-mute-2"} />}
          {mine && active ? <span className="font-semibold text-moss-soft">{view.label}</span> : (players ?? server.address)}
        </p>
      </div>
      {mine && active ? (
        <button type="button" aria-label={`Stop ${server.name}`} onClick={() => stop(instanceId)} className="grid size-9 place-items-center rounded-full border border-cream/15 text-cream-300 transition-colors hover:border-cream/35 hover:text-cream">
          <Square size={10} fill="currentColor" strokeWidth={0} aria-hidden="true" />
        </button>
      ) : (
        <button
          type="button"
          aria-label={`Play ${server.name}`}
          disabled={active}
          onClick={() => play(instanceId, null, server.address)}
          className="grid size-9 place-items-center rounded-full bg-cream text-ink-900 transition-transform active:scale-95 disabled:opacity-40"
        >
          <Play size={13} fill="currentColor" strokeWidth={0} aria-hidden="true" />
        </button>
      )}
    </li>
  );
}
