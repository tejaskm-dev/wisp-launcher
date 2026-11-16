import { m } from "motion/react";
import { useState } from "react";
import heroArt from "../assets/art/hero-moon-shrine.jpg";
import { Hero, type HeroIntro } from "../components/hero/Hero";
import { describeLaunch } from "../components/launch/describe";
import { ModpacksSection } from "../components/ModpacksSection";
import { NewsPanel } from "../components/NewsPanel";
import { QuickActions, type QuickActionId } from "../components/QuickActions";
import { SyncPanel } from "../components/SyncPanel";
import { WorldsSection, type WorldLaunch } from "../components/WorldsSection";
import { worldLaunch } from "../components/worlds/launch";
import type { WorldSummary } from "../data/types";
import { stagger } from "../lib/motion";
import { useApp } from "../state/AppContext";

interface HomePageProps {
  /** False while the startup intro covers the screen. */
  revealed: boolean;
  heroIntro: HeroIntro;
}

export function HomePage({ revealed, heroIntro }: HomePageProps) {
  const { home, now, navigate, comingSoon, openNewInstance, launches, launchWorlds, play, stop, settings, openWorld, importWorlds, requestInstall } = useApp();
  const sections = settings?.home ?? { worlds: true, quickActions: true, modpacks: true, news: true };
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [instanceId, setInstanceId] = useState<string | null>(null);
  const instance = home?.instances.find((candidate) => candidate.id === instanceId) ?? home?.instances[0] ?? null;
  const worlds = home?.worlds ?? null;
  // The most recently played world, unless one was picked.
  const selected = worlds?.find((world) => world.id === selectedId) ?? worlds?.[0] ?? null;

  const viewOf = (world: WorldSummary): WorldLaunch => worldLaunch(world, launches, launchWorlds, now);
  // While the world's instance is busy with something else, the hero shows (and stops) that.
  const heroLaunch = selected
    ? (() => {
        const own = viewOf(selected);
        return own.blocked ? { view: describeLaunch(launches[selected.instanceId], now), blocked: true } : own;
      })()
    : null;

  const onQuickAction = (action: QuickActionId) => {
    if (action === "import") importWorlds();
    else if (action === "instance") openNewInstance();
    else if (action === "modpacks") navigate("discover");
    else navigate("sync");
  };

  return (
    <m.div
      variants={stagger}
      initial="hidden"
      animate={revealed ? "show" : "hidden"}
      exit={{ opacity: 0, transition: { duration: 0.15 } }}
      className="grid grid-cols-1 gap-x-6 gap-y-7 px-7 pb-10 lg:grid-cols-[minmax(0,1fr)_minmax(280px,320px)]"
    >
      <Hero
        art={heroArt}
        home={home}
        world={selected}
        worldLaunch={heroLaunch}
        now={now}
        revealed={revealed}
        intro={heroIntro}
        instance={instance}
        instanceView={describeLaunch(instance ? launches[instance.id] : undefined, now)}
        onSelectInstance={setInstanceId}
        onPlayInstance={() => instance && play(instance.id)}
        onStopInstance={() => instance && stop(instance.id)}
        onSelectWorld={setSelectedId}
        onPlay={() => selected && play(selected.instanceId, selected.folder)}
        onStop={() => selected && stop(selected.instanceId)}
        onImport={() => importWorlds()}
        onNewWorld={() => (instance ? play(instance.id) : openNewInstance())}
      />
      <SyncPanel home={home} onSetUp={(provider) => comingSoon(`Syncing with ${provider}`)} />
      {sections.worlds && (
        <WorldsSection
          worlds={worlds}
          selectedId={selected?.id ?? null}
          now={now}
          viewOf={viewOf}
          onSelect={(world) => (world.id === selected?.id ? openWorld(world.id) : setSelectedId(world.id))}
          onPlay={(world) => play(world.instanceId, world.folder)}
          onStop={(world) => stop(world.instanceId)}
          onImport={() => importWorlds()}
          onNewWorld={() => (instance ? play(instance.id) : openNewInstance())}
          onSeeAll={() => navigate("worlds")}
        />
      )}
      {sections.quickActions && <QuickActions onAction={onQuickAction} />}
      {sections.modpacks && (
        <ModpacksSection
          onInstall={(pack) => requestInstall({ projectId: pack.id, title: pack.title, projectType: "modpack", iconUrl: pack.iconUrl })}
          onBrowse={() => navigate("discover")}
        />
      )}
      {sections.news && <NewsPanel />}
    </m.div>
  );
}
