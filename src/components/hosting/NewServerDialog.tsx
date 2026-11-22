import { ExternalLink, FileArchive, Globe2, Server, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState, type FormEvent } from "react";
import { canAddOfflineAccounts, createHostedServer, createServerFromModpack, getServerSoftwareVersions, getVersions, inspectImportFiles, openExternal, pickImportFiles, startHostedServer } from "../../data/api";
import type { InspectedFile, Loader, ServerSoftware } from "../../data/types";
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

type Source = "instance" | "fresh" | "pack";
const SOURCES: SegmentOption<Source>[] = [
  { value: "instance", label: "From an instance" },
  { value: "fresh", label: "From scratch" },
  { value: "pack", label: "From a modpack" },
];

/** What the server runs: plain Minecraft, a plugin server, or a mod loader. */
type Kind = Loader | ServerSoftware;
const SOFTWARE: ServerSoftware[] = ["paper", "purpur", "folia", "spigot"];
const isSoftware = (kind: Kind): kind is ServerSoftware => (SOFTWARE as string[]).includes(kind);
const KINDS: SelectOption<Kind>[] = [
  { value: "vanilla", label: "Vanilla", note: "Plain Minecraft" },
  { value: "paper", label: "Paper", note: "Plugins, and runs faster" },
  { value: "purpur", label: "Purpur", note: "Paper, with more to set" },
  { value: "folia", label: "Folia", note: "Paper for very busy servers" },
  { value: "spigot", label: "Spigot", note: "Plugins; built on first start" },
  { value: "fabric", label: "Fabric", note: "Mods" },
  { value: "quilt", label: "Quilt", note: "Mods" },
  { value: "forge", label: "Forge", note: "Mods" },
  { value: "neoforge", label: "NeoForge", note: "Mods" },
];
const KIND_NOTE: Partial<Record<Kind, string>> = {
  paper: "Runs Bukkit, Spigot and Paper plugins, which you add on its Plugins tab. Players join with plain Minecraft.",
  purpur: "Paper with extra settings, and the same plugins. Players join with plain Minecraft.",
  folia: "Spreads a busy server over many processor cores. Only plugins made for Folia run on it.",
  spigot: "Runs Bukkit and Spigot plugins. Wisp builds it from source the first time, which takes a few minutes (and Git, outside Windows). Paper runs the same plugins and starts at once.",
  fabric: "Add its mods on its Mods tab. Friends who join through Wisp get the ones they need by themselves.",
  quilt: "Add its mods on its Mods tab. Friends who join through Wisp get the ones they need by themselves.",
  forge: "Add its mods on its Mods tab. Friends who join through Wisp get the ones they need by themselves.",
  neoforge: "Add its mods on its Mods tab. Friends who join through Wisp get the ones they need by themselves.",
};

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
  const [kind, setKind] = useState<Kind>("vanilla");
  const loader: Loader = isSoftware(kind) ? "vanilla" : kind;
  // Paper and its kin list the versions they have builds for.
  const [softwareVersions, setSoftwareVersions] = useState<string[] | null>(null);
  useEffect(() => {
    if (!isSoftware(kind) || kind === "spigot") {
      setSoftwareVersions(null);
      return;
    }
    let active = true;
    setSoftwareVersions(null);
    getServerSoftwareVersions(kind).then(
      (found) => active && setSoftwareVersions(found),
      () => active && setSoftwareVersions([]),
    );
    return () => {
      active = false;
    };
  }, [kind]);
  const [name, setName] = useState("");
  const [difficulty, setDifficulty] = useState<Difficulty>("normal");
  const [gameMode, setGameMode] = useState<GameMode>("survival");
  const [seed, setSeed] = useState("");
  const [eula, setEula] = useState(false);
  // From a modpack file: the file, and what it makes.
  const [pack, setPack] = useState<InspectedFile | null>(null);
  const [packError, setPackError] = useState<string | null>(null);
  const choosePack = async () => {
    setPackError(null);
    try {
      const picked = (await pickImportFiles()).filter((path) => /\.(mrpack|zip)$/i.test(path));
      if (picked.length === 0) return;
      const [found] = await inspectImportFiles([picked[0]]);
      if (found?.kind === "modpack" && found.pack) setPack(found);
      else setPackError(found?.problem ?? `${found?.fileName ?? "That file"} isn't a modpack. Pick a .mrpack, or a CurseForge zip.`);
    } catch (failure: unknown) {
      setPackError(failure instanceof Error ? failure.message : String(failure));
    }
  };
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const instance = instances.find((candidate) => candidate.id === instanceId) ?? null;
  const worlds = useMemo(() => (home?.worlds ?? []).filter((candidate) => candidate.instanceId === instanceId), [home, instanceId]);
  // A world picked for another instance doesn't carry over.
  useEffect(() => {
    if (world !== NEW_WORLD && !worlds.some((candidate) => candidate.folder === world)) setWorld(NEW_WORLD);
  }, [worlds, world]);

  const releases = versions.status === "ready" ? versions.data.versions.filter((entry) => entry.kind === "release") : [];
  const latestRelease = versions.status === "ready" ? versions.data.latestRelease : null;
  const offered = softwareVersions ?? releases.map((entry) => entry.id);
  const latest = softwareVersions ? (softwareVersions[0] ?? null) : latestRelease;
  const shownVersion = version !== null && offered.includes(version) ? version : latest;
  const versionOptions: SelectOption<string>[] = offered.map((id) => ({ value: id, label: id, note: id === latest ? "Newest" : undefined }));
  const kindName = KINDS.find((option) => option.value === kind)?.label ?? loaderName(loader);
  const worldOptions: SelectOption<string>[] = [{ value: NEW_WORLD, label: "A new world" }, ...worlds.map((candidate) => ({ value: candidate.folder, label: candidate.name }))];

  const picked = source === "instance" ? instance : null;
  const pickedWorld = source === "instance" ? (worlds.find((candidate) => candidate.folder === world) ?? null) : null;
  const packName = source === "pack" && pack ? (pack.pack?.name ?? pack.fileName.replace(/\.(mrpack|zip)$/i, "")) : null;
  const suggestedName = packName ?? pickedWorld?.name ?? picked?.name ?? (shownVersion ? `${kindName} ${shownVersion} server` : "My server");
  const ready = eula && !pending && (source === "instance" ? picked !== null : source === "pack" ? pack !== null : shownVersion !== null);
  const newWorld = source !== "instance" || world === NEW_WORLD;

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
      if (source === "pack" && pack) {
        const made = await createServerFromModpack({ path: pack.path, name: name.trim() || suggestedName, properties, eula });
        onClose();
        openHosted(made.id);
        // Its first start sets the modpack up, then starts it.
        await startHostedServer(made.id).catch((failure: unknown) =>
          notify({ tone: "info", title: `Couldn't start ${made.name}`, body: failure instanceof Error ? failure.message : String(failure) }),
        );
        return;
      }
      const server = await createHostedServer({
        name: name.trim() || suggestedName,
        gameVersion: picked?.gameVersion ?? shownVersion ?? "",
        loader: picked?.loader ?? loader,
        loaderVersion: picked?.loaderVersion ?? null,
        instanceId: picked?.id ?? null,
        world: picked && !newWorld ? { instanceId: picked.id, folder: world } : null,
        properties,
        eula,
        software: picked || !isSoftware(kind) ? null : kind,
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
        <Segmented label="Start" value={source} options={instances.length > 0 ? SOURCES : SOURCES.filter((option) => option.value !== "instance")} onChange={setSource} />

        {source === "pack" ? (
          <div className="flex flex-col gap-3">
            <button
              type="button"
              onClick={() => void choosePack()}
              className="flex items-center gap-3 rounded-[18px] border border-dashed border-cream/20 bg-cream/4 p-4 text-left transition-colors hover:border-cream/40"
            >
              <FileArchive size={22} className="shrink-0 text-ember-soft" aria-hidden="true" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm font-bold">{pack ? (pack.pack?.name ?? pack.fileName) : "Choose a modpack file"}</span>
                <span className="block truncate text-xs text-mute">
                  {pack?.pack ? `${loaderName(pack.pack.loader)} ${pack.pack.gameVersion}${pack.pack.version ? ` · ${pack.pack.version}` : ""}` : "A .mrpack from Modrinth, or a CurseForge zip"}
                </span>
              </span>
              <span className="shrink-0 text-xs font-bold text-cream-300">{pack ? "Change" : "Browse"}</span>
            </button>
            {packError && <p className="rounded-xl bg-coral/10 px-3.5 py-2.5 text-sm text-coral">{packError}</p>}
            <p className="-mt-1 text-xs leading-relaxed text-mute">
              It becomes an instance too, which is what players join with: friends get its mods by themselves. The server runs the ones that work on a server, with the pack's own server files.
            </p>
          </div>
        ) : source === "instance" ? (
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
            <Select label="Runs" value={kind} options={KINDS} onChange={setKind} />
            {KIND_NOTE[kind] && <p className="-mt-2 text-xs leading-relaxed text-mute">{KIND_NOTE[kind]}</p>}
            <Select
              label="Minecraft version"
              value={shownVersion ?? ""}
              options={
                versionOptions.length > 0
                  ? versionOptions
                  : [{ value: "", label: versions.status === "error" || softwareVersions?.length === 0 ? "Couldn't load the versions" : "Loading versions…" }]
              }
              onChange={setVersion}
              searchable
            />
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
