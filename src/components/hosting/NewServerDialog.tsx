import { ExternalLink, Globe2, Server, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { canAddOfflineAccounts, createHostedServer, getVersions, openExternal, startHostedServer } from "../../data/api";
import type { Loader } from "../../data/types";
import { cn } from "../../lib/cn";
import { loaderName } from "../../lib/format";
import { useResource } from "../../lib/useResource";
import { useApp } from "../../state/AppContext";
import { InstancePicker } from "../servers/InstancePicker";
import { TextField } from "../servers/TextField";
import { Button } from "../ui/Button";
import { Chips, type ChipOption } from "../ui/Chips";
import { Dialog } from "../ui/Dialog";
import { Segmented, type SegmentOption } from "../ui/Segmented";
import { Select, type SelectOption } from "../ui/Select";

const EULA = "https://aka.ms/MinecraftEULA";
const FORM_ID = "new-server";
const NEW_WORLD = "";

type Source = "instance" | "fresh";
const SOURCES: SegmentOption<Source>[] = [
  { value: "instance", label: "From an instance" },
  { value: "fresh", label: "From scratch" },
];

const LOADERS: SegmentOption<Loader>[] = [
  { value: "vanilla", label: "Vanilla" },
  { value: "fabric", label: "Fabric" },
  { value: "quilt", label: "Quilt" },
  { value: "forge", label: "Forge" },
  { value: "neoforge", label: "NeoForge" },
];

type Difficulty = "peaceful" | "easy" | "normal" | "hard";
const DIFFICULTIES: ChipOption<Difficulty>[] = [
  { value: "peaceful", label: "Peaceful" },
  { value: "easy", label: "Easy" },
  { value: "normal", label: "Normal" },
  { value: "hard", label: "Hard" },
];

type GameMode = "survival" | "creative" | "adventure";
const GAME_MODES: ChipOption<GameMode>[] = [
  { value: "survival", label: "Survival" },
  { value: "creative", label: "Creative" },
  { value: "adventure", label: "Adventure" },
];

/**
 * Makes a server hosted on this computer. From an instance, it runs that instance's version, loader
 * and mods (and friends who join get the same), and can start from one of its worlds. Mount it
 * fresh for each use, so every open starts from a clean form.
 */
export function NewServerDialog({ open, onClose, instanceId: startInstance, world: startWorld }: { open: boolean; onClose: () => void; instanceId?: string | null; world?: string | null }) {
  const { home, notify, openHosted } = useApp();
  const instances = useMemo(() => home?.instances ?? [], [home]);
  const [source, setSource] = useState<Source>(instances.length > 0 ? "instance" : "fresh");
  const [instanceId, setInstanceId] = useState(startInstance ?? instances[0]?.id ?? "");
  const [world, setWorld] = useState(startWorld ?? NEW_WORLD);
  const [versions] = useResource(getVersions);
  const [version, setVersion] = useState<string | null>(null);
  const [loader, setLoader] = useState<Loader>("vanilla");
  const [name, setName] = useState("");
  const [difficulty, setDifficulty] = useState<Difficulty>("normal");
  const [gameMode, setGameMode] = useState<GameMode>("survival");
  const [seed, setSeed] = useState("");
  const [eula, setEula] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const instance = instances.find((candidate) => candidate.id === instanceId) ?? null;
  const worlds = useMemo(() => (home?.worlds ?? []).filter((candidate) => candidate.instanceId === instanceId), [home, instanceId]);
  // A world picked for another instance doesn't carry over.
  useEffect(() => {
    if (world !== NEW_WORLD && !worlds.some((candidate) => candidate.folder === world)) setWorld(NEW_WORLD);
  }, [worlds, world]);

  const releases = versions.status === "ready" ? versions.data.versions.filter((entry) => entry.kind === "release") : [];
  const latest = versions.status === "ready" ? versions.data.latestRelease : null;
  const shownVersion = version ?? latest;
  const versionOptions: SelectOption<string>[] = releases.map((entry) => ({ value: entry.id, label: entry.id, note: entry.id === latest ? "Newest" : undefined }));
  const worldOptions: SelectOption<string>[] = [{ value: NEW_WORLD, label: "A new world" }, ...worlds.map((candidate) => ({ value: candidate.folder, label: candidate.name }))];

  const picked = source === "instance" ? instance : null;
  const pickedWorld = source === "instance" ? (worlds.find((candidate) => candidate.folder === world) ?? null) : null;
  const suggestedName = pickedWorld?.name ?? picked?.name ?? (shownVersion ? `${loaderName(loader)} ${shownVersion} server` : "My server");
  const ready = eula && !pending && (source === "instance" ? picked !== null : shownVersion !== null);
  const newWorld = source === "fresh" || world === NEW_WORLD;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!ready) return;
    setPending(true);
    setError(null);
    const properties: [string, string][] = [
      ["difficulty", difficulty],
      ["gamemode", gameMode],
    ];
    if (newWorld && seed.trim()) properties.push(["level-seed", seed.trim()]);
    try {
      const server = await createHostedServer({
        name: name.trim() || suggestedName,
        gameVersion: picked?.gameVersion ?? shownVersion ?? "",
        loader: picked?.loader ?? loader,
        loaderVersion: picked?.loaderVersion ?? null,
        instanceId: picked?.id ?? null,
        world: picked && !newWorld ? { instanceId: picked.id, folder: world } : null,
        properties,
        eula,
      });
      onClose();
      openHosted(server.id);
      // Straight on: setting up the first time downloads the server's files.
      await startHostedServer(server.id).catch((failure: unknown) =>
        notify({ tone: "info", title: `Couldn't start ${server.name}`, body: failure instanceof Error ? failure.message : String(failure) }),
      );
    } catch (failure: unknown) {
      setError(failure instanceof Error ? failure.message : String(failure));
      setPending(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Host a server"
      description="It runs on this computer, and friends join it through Wisp. Nobody else hosts anything, and it costs nothing."
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            Cancel
          </Button>
          <Button type="submit" form={FORM_ID} icon={Server} disabled={!ready}>
            {pending ? "Making it…" : "Create and start"}
          </Button>
        </>
      }
    >
      <form id={FORM_ID} onSubmit={submit} className="flex flex-col gap-5">
        {instances.length > 0 && <Segmented label="Start" value={source} options={SOURCES} onChange={setSource} />}

        {source === "instance" ? (
          <div className="flex flex-col gap-4">
            <InstancePicker instances={instances} value={instanceId} onChange={setInstanceId} label="Play it with" />
            <p className="-mt-2 text-xs leading-relaxed text-mute">
              The server runs {picked ? `${loaderName(picked.loader)} ${picked.gameVersion}` : "the instance's version"} with the instance's mods (all but the ones that only run in the game). Friends who join get the same mods by themselves.
            </p>
            <Select label="World" value={world} options={worldOptions} onChange={setWorld} />
            {!newWorld && <p className="-mt-2 text-xs leading-relaxed text-mute">The server gets a copy: your singleplayer world stays as it is, and you keep your things when you join.</p>}
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <Segmented label="Loader" value={loader} options={LOADERS} onChange={setLoader} />
            <Select label="Minecraft version" value={shownVersion ?? ""} options={versionOptions.length > 0 ? versionOptions : [{ value: "", label: versions.status === "error" ? "Couldn't load the versions" : "Loading versions…" }]} onChange={setVersion} searchable />
          </div>
        )}

        <TextField label="Name" value={name} onChange={setName} placeholder={suggestedName} maxLength={48} />

        <div className="flex flex-col gap-2">
          <span className="text-[13px] font-semibold text-mute">Difficulty</span>
          <Chips label="Difficulty" value={difficulty} options={DIFFICULTIES} onChange={setDifficulty} />
        </div>
        <div className="flex flex-col gap-2">
          <span className="text-[13px] font-semibold text-mute">Game mode</span>
          <Chips label="Game mode" value={gameMode} options={GAME_MODES} onChange={setGameMode} />
        </div>
        {newWorld && <TextField label="Seed (optional)" value={seed} onChange={setSeed} placeholder="Leave empty for a random world" icon={<Globe2 size={15} className="text-mute" aria-hidden="true" />} maxLength={64} />}

        <div className="flex flex-col gap-2 rounded-[18px] bg-cream/4 p-3.5">
          <label className="flex cursor-pointer items-start gap-3">
            <input type="checkbox" checked={eula} onChange={(event) => setEula(event.target.checked)} className="mt-0.5 size-4 shrink-0 accent-[#f4eee4]" />
            <span className="text-sm leading-relaxed text-cream-200">
              I agree to the Minecraft End User License Agreement, which every server needs.{" "}
              <button type="button" onClick={() => void openExternal(EULA)} className="inline-flex items-center gap-1 font-bold text-cream underline-offset-4 hover:underline">
                Read it
                <ExternalLink size={12} aria-hidden="true" />
              </button>
            </span>
          </label>
          <p className={cn("flex items-start gap-2 text-xs leading-relaxed text-mute")}>
            <ShieldCheck size={14} className="mt-0.5 shrink-0" aria-hidden="true" />
            {canAddOfflineAccounts
              ? "Only people you let in can join: friends you invite, and anyone you add to its whitelist. Until Microsoft sign-in is in, players sign in through Wisp with offline accounts (and their Wisp skins), so share it only with friends."
              : "Only people you let in can join: friends you invite, and anyone you add to its whitelist."}
          </p>
        </div>
        {error && <p className="rounded-xl bg-coral/10 px-3.5 py-2.5 text-sm text-coral">{error}</p>}
      </form>
    </Dialog>
  );
}
