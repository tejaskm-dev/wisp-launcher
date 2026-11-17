import { cn } from "../../lib/cn";
import { loaderName } from "../../lib/format";

/** A Minecraft version in the pixel face: game things look like the game. */
export function VersionTag({ version, className }: { version: string; className?: string }) {
  return <span className={cn("font-pixel text-[1.04em] tracking-[0.02em]", className)}>{version}</span>;
}

/** "Fabric · 1.21.4", with the version in pixels. The plain-text form is `instanceLabel`. */
export function InstanceLabel({ instance, className }: { instance: { loader: string; gameVersion: string }; className?: string }) {
  return (
    <span className={className}>
      {loaderName(instance.loader)} · <VersionTag version={instance.gameVersion} />
    </span>
  );
}
