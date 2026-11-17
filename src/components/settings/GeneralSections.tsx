import { Gamepad2, LayoutDashboard, LogIn, Play, Power, Sparkles, UserRound } from "lucide-react";
import type { IntroMode, MotionPreference, StartPage, WhilePlaying } from "../../data/types";
import { modKey } from "../../lib/platform";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";
import { Panel } from "../ui/Page";
import { Segmented, type SegmentOption } from "../ui/Segmented";
import { Toggle } from "../ui/Toggle";
import { Choice, Divider, Row, type SectionProps } from "./parts";

const START_PAGES: { value: StartPage; label: string }[] = [
  { value: "home", label: "Home" },
  { value: "instances", label: "Instances" },
  { value: "discover", label: "Discover" },
  { value: "last", label: "Where you left off" },
];

const WHILE_PLAYING: { value: WhilePlaying; label: string }[] = [
  { value: "keep-open", label: "Keep it open" },
  { value: "minimize", label: "Minimize it" },
  { value: "hide", label: "Close it" },
];

const WHILE_PLAYING_NOTES: Record<WhilePlaying, string> = {
  "keep-open": "Wisp stays where it is, so you can watch a download or start another instance.",
  minimize: "Wisp tucks itself away while you play and comes back when the game closes.",
  hide: "Wisp closes while you play, leaving the game the memory it used, and opens again where you were when you quit, backing up and syncing your worlds. Its tray icon brings it back sooner.",
};

const INTRO_MODES: SegmentOption<IntroMode>[] = [
  { value: "full", label: "Full" },
  { value: "quick", label: "Quick" },
  { value: "off", label: "Off" },
];

const MOTIONS: { value: MotionPreference; label: string }[] = [
  { value: "system", label: "Match my computer" },
  { value: "reduced", label: "Reduced" },
  { value: "full", label: "Full" },
];

const MOTION_NOTES: Record<MotionPreference, string> = {
  system: "Follows your computer's “reduce motion” accessibility setting.",
  reduced: "Pages and panels appear without sliding or zooming.",
  full: "Every animation plays, even if your computer asks for less motion.",
};

export function GeneralSection({ settings, update }: SectionProps) {
  const { home, comingSoon } = useApp();
  return (
    <>
      <Panel title="Starting up" icon={<Power size={19} aria-hidden="true" />}>
        <Row label="Open Wisp on" description="The page you see each time Wisp starts." stacked>
          <Choice label="Open Wisp on" value={settings.startPage} options={START_PAGES} onChange={(startPage) => update({ startPage })} />
        </Row>
      </Panel>

      <Panel title="While you play" icon={<Gamepad2 size={19} aria-hidden="true" />}>
        <Row label="When a game starts" description={WHILE_PLAYING_NOTES[settings.whilePlaying]} stacked>
          <Choice label="When a game starts" value={settings.whilePlaying} options={WHILE_PLAYING} onChange={(whilePlaying) => update({ whilePlaying })} />
        </Row>
      </Panel>

      <Panel title="Account" icon={<UserRound size={19} aria-hidden="true" />}>
        <Row
          label={home?.account ? home.account.name : "Not signed in"}
          description={
            home?.account
              ? home.account.kind === "offline"
                ? "An offline account, for testing. It can't join online servers."
                : "Signed in with your Microsoft account."
              : "Sign in with the Microsoft account that owns Minecraft: Java Edition to play online. Offline play unlocks once you have."
          }
        >
          {!home?.account && (
            <Button icon={LogIn} onClick={() => comingSoon("Microsoft sign-in")}>
              Sign in
            </Button>
          )}
        </Row>
      </Panel>
    </>
  );
}

export function AppearanceSection({ settings, update }: SectionProps) {
  const { replayIntro } = useApp();
  const sections = settings.home;
  const setSection = (key: keyof typeof sections, value: boolean) => update((current) => ({ home: { ...current.home, [key]: value } }));
  return (
    <>
      <Panel title="Look and feel" icon={<Sparkles size={19} aria-hidden="true" />}>
        <Row label="Startup animation" description="Plays when Wisp opens. Any key or click skips it.">
          <div className="flex items-center gap-2">
            <Segmented label="Startup animation" value={settings.intro} options={INTRO_MODES} onChange={(intro) => update({ intro })} />
            <button
              type="button"
              onClick={replayIntro}
              disabled={settings.intro === "off"}
              className="flex h-9 items-center gap-2 rounded-full border border-cream/15 px-3.5 text-[13px] font-bold text-cream-300 transition-colors enabled:hover:border-cream/35 enabled:hover:bg-cream/8 enabled:hover:text-cream disabled:opacity-35"
            >
              <Play size={12} fill="currentColor" strokeWidth={0} aria-hidden="true" />
              Preview
            </button>
          </div>
        </Row>
        <Divider />
        <Row label="Motion" description={MOTION_NOTES[settings.motion]} stacked>
          <Choice label="Motion" value={settings.motion} options={MOTIONS} onChange={(motion) => update({ motion })} />
        </Row>
        <Divider />
        <Toggle
          label="Compact sidebar"
          description={`Show the sidebar as icons only. ${modKey}B switches it from anywhere.`}
          checked={settings.sidebarCollapsed}
          onChange={(sidebarCollapsed) => update({ sidebarCollapsed })}
        />
      </Panel>

      <Panel title="Home page" icon={<LayoutDashboard size={19} aria-hidden="true" />} description="Pick what Home shows below the hero and cloud sync.">
        <Toggle label="Your worlds" description="Worlds from every instance, newest first." checked={sections.worlds} onChange={(value) => setSection("worlds", value)} />
        <Divider />
        <Toggle label="Quick actions" description="Shortcuts for new instances, modpacks and sync." checked={sections.quickActions} onChange={(value) => setSection("quickActions", value)} />
        <Divider />
        <Toggle label="Featured modpacks" description="Popular modpacks from Modrinth." checked={sections.modpacks} onChange={(value) => setSection("modpacks", value)} />
        <Divider />
        <Toggle label="Minecraft news" description="Release notes from Mojang." checked={sections.news} onChange={(value) => setSection("news", value)} />
      </Panel>
    </>
  );
}
