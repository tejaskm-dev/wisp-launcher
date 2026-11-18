import { CloudDownload, CopyPlus, Laptop, LoaderCircle } from "lucide-react";
import { useState } from "react";
import type { LaunchCheck, SyncItem, SyncResolution } from "../../data/types";
import { cn } from "../../lib/cn";
import { timeAgo } from "../../lib/format";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";

export interface Gate {
  instanceId: string;
  instanceName: string;
  world: string | null;
  server: string | null;
  /** Where sync keeps things ("iCloud Drive"). */
  folder: string;
  check: LaunchCheck;
}

interface SyncGateProps {
  gate: Gate | null;
  now: number;
  onResolve: (item: SyncItem, resolution: SyncResolution) => Promise<void>;
  /** Starts the game without waiting on sync. */
  onPlay: () => void;
  /** Checks again (once a cloud app has had a moment). */
  onRetry: () => void;
  onCancel: () => void;
}

function possessive(name: string): string {
  return name.endsWith("s") ? `${name}'` : `${name}'s`;
}

/**
 * What stands between Play and the game when sync has something to say: worlds that changed on
 * two computers, the instance being open on another one, or the newest copy still arriving.
 */
export function SyncGate({ gate, now, onResolve, onPlay, onRetry, onCancel }: SyncGateProps) {
  const [working, setWorking] = useState<string | null>(null);
  const check = gate?.check;
  const conflicts = check?.conflicts ?? [];

  let title = "";
  let description = "";
  if (conflicts.length > 0) {
    title = conflicts.length === 1 ? `Choose which ${conflicts[0].name} to play` : "Choose which copies to play";
    description = "These worlds changed here and on another computer since they last synced. Wisp never mixes two copies of a world.";
  } else if (check?.inUse) {
    title = `${gate?.instanceName} is open on ${check.inUse}`;
    description = "If you play it here too, any world you both play ends up as two copies to choose between later.";
  } else if (check && check.waiting > 0) {
    title = "The newest copy is still arriving";
    description = `${gate?.folder} is still bringing ${check.waiting === 1 ? "a file" : `${check.waiting} files`} from your other computer.`;
  }

  const resolve = async (item: SyncItem, resolution: SyncResolution) => {
    setWorking(item.key);
    try {
      await onResolve(item, resolution);
    } finally {
      setWorking(null);
    }
  };

  return (
    <Dialog
      open={gate !== null}
      onClose={onCancel}
      title={title}
      description={description}
      footer={
        conflicts.length > 0 ? (
          <Button variant="secondary" disabled={working !== null} onClick={onPlay}>
            Play without choosing
          </Button>
        ) : check?.inUse ? (
          <>
            <Button variant="secondary" onClick={onCancel}>
              Not now
            </Button>
            <Button onClick={onPlay}>Play anyway</Button>
          </>
        ) : (
          <>
            <Button variant="secondary" onClick={onPlay}>
              Play this computer's copy
            </Button>
            <Button onClick={onRetry}>Wait for it</Button>
          </>
        )
      }
    >
      {conflicts.length > 0 ? (
        <ul className="flex flex-col gap-2.5">
          {conflicts.map((item) => {
            if (item.state.kind !== "conflict") return null;
            const state = item.state;
            const busy = working === item.key;
            return (
              <li key={item.key} className="rounded-[18px] bg-cream/4 p-4">
                <p className="text-[15px] font-bold">{item.name}</p>
                <p className="mt-0.5 text-[13px] text-mute">
                  Played here{state.hereAt ? ` ${timeAgo(state.hereAt, now)}` : ""} and on {state.choices.map((choice) => `${choice.device} ${timeAgo(choice.at, now)}`).join(" and on ")}.
                </p>
                <div className="mt-3 flex flex-wrap gap-2">
                  <Button variant="secondary" icon={Laptop} disabled={working !== null} onClick={() => void resolve(item, { keep: "here" })}>
                    This computer's
                  </Button>
                  {state.choices.map((choice) => (
                    <Button key={choice.commit} variant="secondary" icon={CloudDownload} disabled={working !== null} onClick={() => void resolve(item, { keep: "cloud", commit: choice.commit })}>
                      {possessive(choice.device)}
                    </Button>
                  ))}
                  <Button
                    variant="secondary"
                    icon={busy ? LoaderCircle : CopyPlus}
                    className={cn(busy && "[&_svg]:animate-spin")}
                    disabled={working !== null}
                    onClick={() => void resolve(item, { keep: "both", commit: state.choices[0].commit })}
                  >
                    Keep both
                  </Button>
                </div>
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="text-sm leading-relaxed text-cream-300">
          {check?.inUse
            ? `Wisp will warn ${check.inUse} too. You can always pick a copy later on the Cloud sync page.`
            : "It usually takes a few seconds. Playing now uses the copy on this computer, and the two are sorted out after."}
        </p>
      )}
    </Dialog>
  );
}
