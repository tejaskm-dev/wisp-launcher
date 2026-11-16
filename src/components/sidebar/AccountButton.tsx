import { Cloud, LogIn, Shirt, SlidersHorizontal, Trash2, UserRoundPlus } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { canAddOfflineAccounts, getSkins, removeAccount } from "../../data/api";
import type { Account } from "../../data/types";
import { cn } from "../../lib/cn";
import { modKey } from "../../lib/platform";
import { useApp } from "../../state/AppContext";
import { Ghost } from "../brand/Ghost";
import { SkinFace } from "../skins/SkinDoll";
import { skinSource } from "../skins/skinSource";
import { makeDefaultSkin } from "../skins/skinTexture";
import { MenuItem, Popover } from "../ui/Popover";
import { Loading, Skeleton } from "../ui/Skeleton";

let wispSkin: HTMLCanvasElement | null = null;
/** Wisp's own skin, worn until you add one (the Skins page shows it too). */
const defaultSkin = () => (wispSkin ??= makeDefaultSkin(false));

/** The skin you wear (Skins, then Wear) as a picture: null only while it loads. */
function useWornSkin(): string | HTMLCanvasElement | null {
  const [skin, setSkin] = useState<string | HTMLCanvasElement | null>(null);
  useEffect(() => {
    let active = true;
    const load = async () => {
      let source: string | HTMLCanvasElement;
      try {
        const library = await getSkins();
        const worn = library.skins.find((candidate) => candidate.id === library.active);
        source = worn ? await skinSource(worn) : defaultSkin();
      } catch {
        source = defaultSkin();
      }
      if (active) setSkin(source);
    };
    void load();
    window.addEventListener("wisp:skins-changed", load);
    return () => {
      active = false;
      window.removeEventListener("wisp:skins-changed", load);
    };
  }, []);
  return skin;
}

/** The worn skin's face, pixel sharp. */
function Avatar({ skin }: { skin: string | HTMLCanvasElement | null }) {
  return <span className="grid size-10 shrink-0 place-items-center overflow-hidden rounded-xl bg-ink-700">{skin && <SkinFace src={skin} className="size-10" />}</span>;
}

/** The account row at the foot of the sidebar; opens the account menu. */
export function AccountButton({ collapsed }: { collapsed: boolean }) {
  const { home, page } = useApp();
  // Skins and Cloud sync open from this menu, so it shows as where you are on those pages.
  const here = page === "skins" || page === "sync";
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const skin = useWornSkin();

  if (home === null) {
    return (
      <Loading label="Loading account" className={cn("flex items-center gap-3 p-1.5", collapsed ? "flex-none" : "min-w-0 flex-1")}>
        <Skeleton className="size-10 shrink-0 rounded-xl" />
        <div className={cn("flex flex-1 flex-col gap-2", collapsed && "hidden")}>
          <Skeleton className="h-3.5 w-16 rounded-full" />
          <Skeleton className="h-3 w-24 rounded-full" />
        </div>
      </Loading>
    );
  }

  const account = home.account;
  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={collapsed ? (account ? `${account.name}, account menu` : "Sign in") : undefined}
        aria-current={here ? "page" : undefined}
        onClick={() => setOpen((isOpen) => !isOpen)}
        className={cn(
          "flex items-center gap-2.5 rounded-2xl p-1.5 text-left transition-colors duration-200",
          open || here ? "bg-cream/8" : "hover:bg-cream/5",
          // Collapsed, only the avatar shows; the text stays laid out (clipped) so it can fade back in.
          collapsed ? "w-[52px] flex-none overflow-hidden" : "min-w-0 flex-1",
        )}
      >
        <Avatar skin={skin} />
        <span className={cn("flex min-w-0 flex-1 flex-col whitespace-nowrap transition-opacity duration-200", collapsed && "opacity-0")}>
          <span className="truncate text-sm font-bold">{account ? account.name : "Sign in"}</span>
          <span className="truncate text-xs text-mute">{account ? (account.kind === "offline" ? "Offline account" : "Microsoft account") : "to play"}</span>
        </span>
      </button>

      <Popover
        open={open}
        anchorRef={buttonRef}
        placement={collapsed ? "right-end" : "top-start"}
        onClose={() => setOpen(false)}
        label="Account"
        className="w-[288px]"
      >
        <AccountMenu account={account} skin={skin} onClose={() => setOpen(false)} />
      </Popover>
    </>
  );
}

function AccountMenu({ account, skin, onClose }: { account: Account | null; skin: string | HTMLCanvasElement | null; onClose: () => void }) {
  const { navigate, comingSoon, addAccount, refreshHome, notify } = useApp();
  const run = (action: () => void) => () => {
    onClose();
    action();
  };

  return (
    <>
      <div className="flex items-start gap-3 px-3 pb-3 pt-2.5">
        {account ? <Avatar skin={skin} /> : <Ghost size={38} sleeping className="mt-0.5" />}
        <div className="min-w-0">
          <p className="truncate text-sm font-bold">{account ? account.name : "Not signed in"}</p>
          <p className="mt-0.5 text-xs leading-snug text-mute">
            {account
              ? account.kind === "offline"
                ? "Offline account: singleplayer and offline servers."
                : "Signed in with Microsoft."
              : "Sign in to play. Your password goes to Microsoft, never to Wisp."}
          </p>
        </div>
      </div>
      <div role="menu" aria-label="Account" className="flex flex-col border-t border-cream/8 pt-1.5">
        <MenuItem icon={<LogIn size={16} aria-hidden="true" />} label="Sign in with Microsoft" hint="Coming soon" onSelect={run(() => comingSoon("Microsoft sign-in"))} />
        <MenuItem
          icon={<UserRoundPlus size={16} aria-hidden="true" />}
          label={account ? "Use a different offline name" : "Add offline account"}
          hint={canAddOfflineAccounts ? "Developer build: for testing" : "Needs a Microsoft account that owns Minecraft"}
          disabled={!canAddOfflineAccounts}
          onSelect={run(addAccount)}
        />
        {account && (
          <MenuItem
            icon={<Trash2 size={16} aria-hidden="true" />}
            label={`Remove ${account.name}`}
            onSelect={run(() =>
              removeAccount(account.id).then(refreshHome, (failure: unknown) =>
                notify({ tone: "info", title: "Couldn't remove the account", body: failure instanceof Error ? failure.message : String(failure) }),
              ),
            )}
          />
        )}
        <div className="mx-2 my-1.5 h-px bg-cream/8" />
        <MenuItem icon={<Shirt size={16} aria-hidden="true" />} label="Skins" onSelect={run(() => navigate("skins"))} />
        <MenuItem icon={<Cloud size={16} aria-hidden="true" />} label="Cloud sync" onSelect={run(() => navigate("sync"))} />
        <MenuItem
          icon={<SlidersHorizontal size={16} aria-hidden="true" />}
          label="Settings"
          shortcut={`${modKey},`}
          onSelect={run(() => navigate("settings"))}
        />
      </div>
    </>
  );
}
