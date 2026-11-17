import { Coffee, Gamepad2, HardDrive, Info, NotebookPen, Paintbrush, Plug, Settings2, ShieldCheck, SquareTerminal, type LucideIcon } from "lucide-react";
import { AnimatePresence, m } from "motion/react";
import { useState, type ComponentType } from "react";
import { GameSection, HooksSection, JavaSection } from "../components/settings/GameSections";
import { AppearanceSection, GeneralSection } from "../components/settings/GeneralSections";
import { JournalSection } from "../components/settings/JournalSection";
import type { SectionProps } from "../components/settings/parts";
import { SourcesSection } from "../components/settings/SourcesSection";
import { AboutSection, PrivacySection, StorageSection } from "../components/settings/SystemSections";
import { Page } from "../components/ui/Page";
import { PageHeader } from "../components/ui/PageHeader";
import { Loading, Skeleton } from "../components/ui/Skeleton";
import { cn } from "../lib/cn";
import { fadeUp } from "../lib/motion";
import { useApp } from "../state/AppContext";
import { takeRequestedSettingsSection } from "../state/settingsSection";
import { Sparkle } from "../components/brand/Sparkle";

type SectionId = "general" | "appearance" | "game" | "java" | "hooks" | "journal" | "sources" | "storage" | "privacy" | "about";

interface Section {
  id: SectionId;
  label: string;
  icon: LucideIcon;
  view: ComponentType<SectionProps>;
}

const GROUPS: { title: string; sections: Section[] }[] = [
  {
    title: "Wisp",
    sections: [
      { id: "general", label: "General", icon: Settings2, view: GeneralSection },
      { id: "appearance", label: "Appearance", icon: Paintbrush, view: AppearanceSection },
    ],
  },
  {
    title: "Playing",
    sections: [
      { id: "game", label: "Game", icon: Gamepad2, view: GameSection },
      { id: "java", label: "Java", icon: Coffee, view: JavaSection },
      { id: "hooks", label: "Launch hooks", icon: SquareTerminal, view: HooksSection },
      { id: "journal", label: "Journal", icon: NotebookPen, view: JournalSection },
    ],
  },
  {
    title: "System",
    sections: [
      { id: "sources", label: "Mod sources", icon: Plug, view: SourcesSection },
      { id: "storage", label: "Downloads & storage", icon: HardDrive, view: StorageSection },
      { id: "privacy", label: "Privacy", icon: ShieldCheck, view: PrivacySection },
      { id: "about", label: "About", icon: Info, view: AboutSection },
    ],
  },
];

const SECTIONS = GROUPS.flatMap((group) => group.sections);

// The open section, remembered while the app runs.
let rememberedSection: SectionId = "general";

export function SettingsPage() {
  const { settings, updateSettings } = useApp();
  // Another page may ask for a section (Discover's "Add a CurseForge key", say).
  const [current, setCurrent] = useState<SectionId>(() => {
    const requested = takeRequestedSettingsSection();
    const found = SECTIONS.find((section) => section.id === requested);
    if (found) rememberedSection = found.id;
    return rememberedSection;
  });
  const open = (id: SectionId) => {
    rememberedSection = id;
    setCurrent(id);
  };
  const section = SECTIONS.find((candidate) => candidate.id === current) ?? SECTIONS[0];
  const View = section.view;

  return (
    <Page className="max-w-[1040px]">
      <PageHeader title="Settings" subtitle="How Wisp runs Minecraft on this computer. Changes save as you make them." />

      <m.div variants={fadeUp} className="grid items-start gap-6 md:grid-cols-[208px_minmax(0,1fr)]">
        <nav aria-label="Settings sections" className="flex flex-col gap-4 md:sticky md:top-24">
          {GROUPS.map((group) => (
            <div key={group.title}>
              <p className="mb-1.5 px-3 text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">{group.title}</p>
              <ul className="flex flex-col gap-0.5">
                {group.sections.map(({ id, label, icon: Icon }) => {
                  const active = id === section.id;
                  return (
                    <li key={id}>
                      <button
                        type="button"
                        aria-current={active ? "page" : undefined}
                        onClick={() => open(id)}
                        className={cn(
                          "flex h-10 w-full items-center gap-3 rounded-xl px-3 text-left text-sm font-semibold transition-colors duration-150",
                          active ? "bg-cream/10 text-cream" : "lit text-mute hover:bg-cream/5 hover:text-cream",
                        )}
                      >
                        <Icon size={17} aria-hidden="true" className={cn("shrink-0", active ? "text-cream" : "text-mute-2")} />
                        <span className="min-w-0 flex-1 truncate">{label}</span>
                        {active && <Sparkle className="size-2.5 shrink-0 text-ember" />}
                      </button>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </nav>

        {settings === null ? (
          <Loading label="Loading settings" className="flex flex-col gap-5">
            {[0, 1].map((index) => (
              <div key={index} className="flex flex-col gap-4 rounded-[24px] border border-cream/8 bg-ink-800 p-6">
                <div className="flex items-center gap-3.5">
                  <Skeleton className="size-10 rounded-[13px]" />
                  <Skeleton className="h-4 w-32 rounded-full" />
                </div>
                <Skeleton className="h-3.5 w-full rounded-full" />
                <Skeleton className="h-3.5 w-2/3 rounded-full" />
              </div>
            ))}
          </Loading>
        ) : (
          <AnimatePresence mode="wait" initial={false}>
            <m.div
              key={section.id}
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4, transition: { duration: 0.1 } }}
              transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
              className="flex min-w-0 flex-col gap-5"
            >
              <h2 className="sr-only">{section.label}</h2>
              <View settings={settings} update={updateSettings} />
            </m.div>
          </AnimatePresence>
        )}
      </m.div>
    </Page>
  );
}
