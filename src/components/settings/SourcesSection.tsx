import { CircleAlert, CircleCheck, ExternalLink, KeyRound, LoaderCircle, Store } from "lucide-react";
import { useState } from "react";
import { getCurseforgeCategories, getCurseforgeStatus, openExternal } from "../../data/api";
import { cn } from "../../lib/cn";
import { useResource } from "../../lib/useResource";
import { Panel } from "../ui/Page";
import { Note, Row, TextField, type SectionProps } from "./parts";

const CONSOLE = "https://console.curseforge.com/";

type Check = { state: "idle" } | { state: "checking" } | { state: "ok"; count: number } | { state: "error"; message: string };

/** Where Discover searches: Modrinth (open to all) and CurseForge (needs an API key). */
export function SourcesSection({ settings, update }: SectionProps) {
  const [status] = useResource(getCurseforgeStatus);
  const builtIn = status.status === "ready" && status.data.builtIn;
  const [check, setCheck] = useState<Check>({ state: "idle" });

  const test = async () => {
    setCheck({ state: "checking" });
    try {
      const categories = await getCurseforgeCategories("mod");
      setCheck({ state: "ok", count: categories.length });
    } catch (error) {
      setCheck({ state: "error", message: error instanceof Error ? error.message : String(error) });
    }
  };
  const usable = builtIn || settings.curseforgeApiKey !== "";

  return (
    <>
      <Panel title="Modrinth" icon={<Store size={19} aria-hidden="true" />} description="Open to everyone: no account or key needed. Discover searches it unless you switch to CurseForge.">
        {null}
      </Panel>

      <Panel title="CurseForge" icon={<KeyRound size={19} aria-hidden="true" />} description="CurseForge only answers apps that send an API key with each request.">
        {builtIn && <p className="mt-2 text-[13px] text-moss-soft">This copy of Wisp has a key built in. Paste your own to use it instead.</p>}
        <Row
          label="API key"
          description={
            <>
              Keys are free for projects like Wisp: sign in at{" "}
              <button type="button" onClick={() => void openExternal(CONSOLE)} className="inline-flex items-center gap-1 font-bold text-cream-300 underline decoration-cream/30 underline-offset-4 hover:text-cream">
                console.curseforge.com
                <ExternalLink size={11} aria-hidden="true" />
              </button>
              , create an API key, and paste it here. It stays on this computer.
            </>
          }
          stacked
        >
          <div className="flex gap-2">
            <TextField
              label="CurseForge API key"
              value={settings.curseforgeApiKey}
              placeholder={builtIn ? "Using the built-in key" : "Paste your key"}
              secret
              mono
              onCommit={(curseforgeApiKey) => {
                update({ curseforgeApiKey });
                setCheck({ state: "idle" });
              }}
            />
            <button
              type="button"
              disabled={!usable || check.state === "checking"}
              onClick={() => void test()}
              className="flex h-10 shrink-0 items-center gap-1.5 rounded-xl border border-cream/12 px-3.5 text-[13px] font-bold text-cream-300 transition-colors enabled:hover:border-cream/30 enabled:hover:text-cream disabled:opacity-40"
            >
              {check.state === "checking" && <LoaderCircle size={13} className="animate-spin" aria-hidden="true" />}
              Test
            </button>
          </div>
          {(check.state === "ok" || check.state === "error") && (
            <p className={cn("flex items-start gap-2 text-[13px]", check.state === "ok" ? "text-moss-soft" : "text-ember-soft")}>
              {check.state === "ok" ? <CircleCheck size={15} className="mt-0.5 shrink-0" aria-hidden="true" /> : <CircleAlert size={15} className="mt-0.5 shrink-0" aria-hidden="true" />}
              {check.state === "ok" ? `CurseForge answered: ${check.count} mod categories. Discover can search it now.` : check.message}
            </p>
          )}
        </Row>
        <Note>Some authors only allow downloads from CurseForge's own website. Wisp says when that happens, and for modpacks lists those files so you can fetch them yourself.</Note>
      </Panel>
    </>
  );
}
