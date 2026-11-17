import { ArrowRight, Box } from "lucide-react";
import { m } from "motion/react";
import { getFeaturedModpacks } from "../data/api";
import type { Project } from "../data/types";
import { formatCount, hexColor, loaderName } from "../lib/format";
import { fadeUp, rise, stagger } from "../lib/motion";
import { useResource } from "../lib/useResource";
import { ErrorState } from "./ui/ErrorState";
import { SectionHeader } from "./ui/SectionHeader";
import { Loading, Skeleton } from "./ui/Skeleton";

const GRID = "grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-4";

interface ModpacksSectionProps {
  onInstall: (pack: Project) => void;
  onBrowse: () => void;
}

export function ModpacksSection({ onInstall, onBrowse }: ModpacksSectionProps) {
  const [packs, retry] = useResource(getFeaturedModpacks);

  return (
    <m.section variants={rise} aria-labelledby="packs-title" className="flex min-w-0 flex-col gap-3.5">
      <SectionHeader
        id="packs-title"
        title="Featured modpacks"
        icon={<Box size={18} strokeWidth={2} className="text-ember" aria-hidden="true" />}
        action={
          <>
            <span className="mr-2 text-sm text-mute-2">from Modrinth</span>
            <button type="button" onClick={onBrowse} className="group flex items-center gap-1.5 text-sm font-semibold text-cream-200 hover:text-cream">
              Browse all
              <ArrowRight size={16} className="transition-transform duration-300 group-hover:translate-x-0.5" aria-hidden="true" />
            </button>
          </>
        }
      />

      {packs.status === "loading" && (
        <Loading label="Loading modpacks" className={GRID}>
          {[0, 1, 2].map((index) => (
            <ModpackSkeleton key={index} />
          ))}
        </Loading>
      )}

      {packs.status === "error" && (
        <ErrorState title="Couldn't load modpacks" body="Check your internet connection and try again." onRetry={retry} />
      )}

      {packs.status === "ready" &&
        (packs.data.length === 0 ? (
          <p className="rounded-[20px] border border-dashed border-cream/15 p-5 text-sm text-mute">No modpacks to show right now.</p>
        ) : (
          <m.ul initial="hidden" animate="show" variants={stagger} className={GRID}>
            {packs.data.map((pack) => (
              <m.li key={pack.id} variants={fadeUp} className="min-w-0">
                <ModpackCard pack={pack} onInstall={() => onInstall(pack)} />
              </m.li>
            ))}
          </m.ul>
        ))}
    </m.section>
  );
}

function ModpackCard({ pack, onInstall }: { pack: Project; onInstall: () => void }) {
  const details = [pack.loaders.map(loaderName).join(", "), pack.gameVersion].filter(Boolean).join(" · ");

  return (
    <m.article
      whileHover={{ y: -4 }}
      transition={{ type: "spring", stiffness: 320, damping: 24 }}
      className="wisp-card wag lit group flex h-full flex-col"
    >
      <div
        className="relative h-28 overflow-hidden rounded-t-[22px] bg-ink-700"
        style={pack.color !== null ? { backgroundColor: hexColor(pack.color) } : undefined}
      >
        {pack.imageUrl && (
          <img
            src={pack.imageUrl}
            alt=""
            loading="lazy"
            decoding="async"
            className="size-full object-cover transition-transform duration-700 ease-out-expo group-hover:scale-[1.08]"
          />
        )}
        <div className="absolute inset-0 bg-linear-to-t from-ink-900/75 via-ink-900/10 to-transparent" />
        <button
          type="button"
          onClick={onInstall}
          aria-label={`Install ${pack.title}`}
          className="absolute bottom-2.5 right-2.5 flex h-8 items-center rounded-full border border-cream/25 bg-ink-900/70 px-3.5 text-[13px] font-bold text-cream backdrop-blur-sm transition-colors duration-200 hover:border-cream hover:bg-cream hover:text-ink-900"
        >
          Install
        </button>
      </div>
      <div className="relative flex flex-1 flex-col gap-1 px-3.5 pb-3.5">
        {pack.iconUrl ? (
          <img
            src={pack.iconUrl}
            alt=""
            loading="lazy"
            decoding="async"
            className="relative z-10 -mt-6 mb-1 size-11 rounded-xl bg-ink-700 object-cover shadow-lg ring-2 ring-ink-800"
          />
        ) : (
          <div className="h-3" />
        )}
        <h3 className="line-clamp-2 text-[15px] font-bold leading-snug">{pack.title}</h3>
        <p className="truncate text-xs text-mute">
          by {pack.author} · {formatCount(pack.downloads)} downloads
        </p>
        {details && <p className="truncate text-xs text-mute-2">{details}</p>}
      </div>
    </m.article>
  );
}

function ModpackSkeleton() {
  return (
    <div className="wisp-card">
      <Skeleton className="h-28 w-full rounded-t-[22px]" />
      <div className="flex flex-col gap-2 px-3.5 pb-3.5">
        <Skeleton className="-mt-6 mb-1 size-11 rounded-xl ring-2 ring-ink-800" />
        <Skeleton className="h-4 w-4/5 rounded-full" />
        <Skeleton className="h-3 w-3/5 rounded-full" />
        <Skeleton className="h-3 w-2/5 rounded-full" />
      </div>
    </div>
  );
}
