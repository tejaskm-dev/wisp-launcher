import { Check, Copy, Download, Globe2, LoaderCircle, Pencil, Radar, Router, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { checkServerAddress, getPlayit, linkPlayit, onPlayit, openExternal, setUpServerAddress, unlinkPlayit, updateHostedServer } from "../../data/api";
import type { AddressCheck, HostedView, PlayitView } from "../../data/types";
import { cn } from "../../lib/cn";
import { useApp } from "../../state/AppContext";
import { Button } from "../ui/Button";

const message = (failure: unknown) => (failure instanceof Error ? failure.message : String(failure));
const PLAYIT_DOWNLOAD = "https://playit.gg/download";

/** A value to copy, with what it is. */
export function CopyLine({ label, value, hint }: { label: string; value: string; hint?: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex items-center gap-3 rounded-xl bg-ink-900/60 py-2 pl-3.5 pr-2">
      <div className="min-w-0 flex-1">
        <p className="text-[11px] font-bold uppercase tracking-[0.13em] text-mute-2">{label}</p>
        <code className="block truncate font-mono text-sm text-cream-200">{value}</code>
        {hint && <p className="mt-0.5 text-xs text-mute">{hint}</p>}
      </div>
      <button
        type="button"
        onClick={() =>
          void navigator.clipboard.writeText(value).then(() => {
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1500);
          })
        }
        className="flex h-8 shrink-0 items-center gap-1.5 rounded-lg bg-cream px-3 text-xs font-bold text-ink-900"
      >
        {copied ? <Check size={13} aria-hidden="true" /> : <Copy size={13} aria-hidden="true" />}
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}

/**
 * How people reach a server: the same network, and an address that works from anywhere. Tester
 * builds get that address from playit by themselves (approve once in the browser); public builds
 * walk you through playit's own app, then check what you paste.
 */
export function AddressPanel({ server, onChange }: { server: HostedView; onChange: (server: HostedView) => void }) {
  const { notify } = useApp();
  const [playit, setPlayit] = useState<PlayitView | null>(null);
  const [step, setStep] = useState<"idle" | "approving" | "making">("idle");
  // Where linking is on playit's side, and since when it's been waiting for approval.
  const [claim, setClaim] = useState<{ step: "visit" | "approve" | "approved"; since: number } | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const [editing, setEditing] = useState(false);
  const [checking, setChecking] = useState(false);
  const [check, setCheck] = useState<AddressCheck | null>(null);
  const making = useRef(false);

  useEffect(() => {
    getPlayit().then(
      (found) => {
        setPlayit(found);
        // A link waiting for approval carries on in the background; show it after coming back.
        if (found.linking) setStep((current) => (current === "idle" ? "approving" : current));
      },
      () => setPlayit(null),
    );
  }, []);

  // While approving, the hint about a full playit account shows after a while.
  useEffect(() => {
    if (step !== "approving") return;
    const timer = window.setInterval(() => setNow(Date.now()), 5_000);
    return () => window.clearInterval(timer);
  }, [step]);

  const runCheck = async () => {
    setChecking(true);
    try {
      setCheck(await checkServerAddress(server.id));
    } catch (failure: unknown) {
      setCheck({ reachable: false, hint: message(failure) });
    } finally {
      setChecking(false);
    }
  };

  const makeAddress = async () => {
    if (making.current) return;
    making.current = true;
    setStep("making");
    try {
      onChange(await setUpServerAddress(server.id));
      setCheck(null);
      notify({ tone: "success", title: "The server has an address", body: "Anyone can join it from there while it runs. Friends with Wisp get the mods too." });
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't get an address from playit", body: message(failure) });
    } finally {
      making.current = false;
      setStep("idle");
    }
  };

  // Once you approve Wisp on playit's page, the address comes by itself.
  useEffect(
    () =>
      onPlayit((event) => {
        if (event.kind === "step") {
          setStep((current) => (current === "idle" ? "approving" : current));
          setClaim((current) => (current?.step === event.step ? current : { step: event.step, since: Date.now() }));
        } else if (event.kind === "linked") {
          setClaim(null);
          setPlayit((current) => (current ? { ...current, linked: true, linking: null, problem: null } : current));
          void makeAddress();
        } else {
          setClaim(null);
          setStep("idle");
          notify({ tone: "info", title: "playit wasn't linked", body: event.message });
        }
      }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [server.id],
  );

  /** playit.gg forgot this computer: start a fresh link. */
  const linkAgain = async () => {
    await unlinkPlayit().catch(() => undefined);
    setPlayit((current) => (current ? { ...current, linked: false, problem: null } : current));
    setStep("approving");
    try {
      await linkPlayit();
    } catch (failure: unknown) {
      setStep("idle");
      notify({ tone: "info", title: "Couldn't open playit", body: message(failure) });
    }
  };

  const getAddress = async () => {
    if (!playit?.linked) {
      setStep("approving");
      try {
        await linkPlayit();
      } catch (failure: unknown) {
        setStep("idle");
        notify({ tone: "info", title: "Couldn't open playit", body: message(failure) });
      }
      return;
    }
    await makeAddress();
  };

  const running = server.state === "running";

  return (
    <section className="wisp-surface flex flex-col gap-3 rounded-[24px] p-5">
      <h2 className="flex items-center gap-2 font-display text-lg font-bold">
        <Router size={18} className="text-ember-soft" aria-hidden="true" />
        Ways in
      </h2>
      <p className="text-sm leading-relaxed text-mute">Friends with Wisp just press Join. Wisp brings them the mods and their skin, whichever way below reaches you.</p>
      {server.code && (
        <CopyLine
          label="Wisp code"
          value={server.code}
          hint={server.address || server.publicAddress ? "Anyone with Wisp joins with it, from Servers, Join with a code." : "Anyone with Wisp joins with it (from anywhere once it has an address below; on your network now)."}
        />
      )}
      {server.lanAddress && <CopyLine label="Same network" value={server.lanAddress} hint="For people on your Wi-Fi, from the game's Multiplayer screen." />}
      {server.publicAddress && <CopyLine label="Through your router" value={server.publicAddress} hint="Your router opened the port." />}

      {server.address && !editing ? (
        <>
          <CopyLine label="From anywhere" value={server.address} hint="Works in any launcher. Only whitelisted players get in." />
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="secondary" className="h-9 px-4 text-xs" icon={checking ? undefined : Radar} onClick={() => void runCheck()} disabled={checking}>
              {checking ? "Checking…" : "Check it"}
            </Button>
            <button type="button" onClick={() => setEditing(true)} className="flex h-8 items-center gap-1.5 rounded-full px-2.5 text-xs font-bold text-mute transition-colors hover:text-cream">
              <Pencil size={12} aria-hidden="true" />
              Change
            </button>
          </div>
          {check && (
            <p className={cn("rounded-xl p-3 text-xs leading-relaxed", check.reachable ? "bg-moss/10 text-moss-soft" : "bg-amber/8 text-amber-soft")}>
              {check.reachable ? "It answers from the internet, as a game would see it." : check.hint}
            </p>
          )}
        </>
      ) : editing || playit === null ? (
        <OwnAddress server={server} onChange={onChange} onDone={() => setEditing(false)} canCancel={editing} />
      ) : playit.builtIn ? (
        <div className="flex flex-col gap-3 rounded-2xl border border-cream/8 bg-ink-900/40 p-4">
          <p className="text-sm leading-relaxed text-cream-300">
            Get a free address anyone can join from, through playit.gg. Wisp sets it all up{playit.linked ? "." : "; you approve it once in your browser."}
          </p>
          {playit.problem && step === "idle" && (
            <div className="flex flex-col gap-2 rounded-xl bg-amber/8 p-3 text-xs leading-relaxed text-amber-soft">
              <p>{playit.problem}</p>
              <button type="button" onClick={() => void linkAgain()} className="self-start font-bold text-cream underline-offset-2 hover:underline">
                Link again
              </button>
            </div>
          )}
          {step === "approving" ? (
            <div className="flex flex-col gap-2 rounded-xl bg-ember/8 p-3 text-xs leading-relaxed text-ember-soft">
              <p className="flex items-center gap-2 font-bold">
                <LoaderCircle size={13} className="animate-spin" aria-hidden="true" />
                {claim?.step === "approved"
                  ? "Approved. Finishing the link…"
                  : claim?.step === "visit"
                    ? "Open playit.gg's page in your browser to approve Wisp"
                    : "Approve Wisp on playit.gg in your browser"}
              </p>
              <p className="text-cream-300">Sign in there (a playit account keeps the address for good; a guest link can end), or carry on as a guest. Wisp finishes the rest by itself.</p>
              {claim?.step === "approve" && now - claim.since > 90_000 && (
                <p className="text-cream-300">Still waiting? If playit.gg says your account has no room for another agent, remove an old one there (Agents), then open the page again.</p>
              )}
              <button type="button" onClick={() => void linkPlayit()} className="self-start font-bold text-cream underline-offset-2 hover:underline">
                Open the page again
              </button>
            </div>
          ) : (
            <Button onClick={() => void getAddress()} disabled={step === "making"} icon={step === "making" ? undefined : Globe2} className="self-start">
              {step === "making" ? "Making the tunnel…" : "Get an address"}
            </Button>
          )}
          <button type="button" onClick={() => setEditing(true)} className="self-start text-xs font-bold text-mute transition-colors hover:text-cream">
            I have my own address
          </button>
        </div>
      ) : (
        <PlayitGuide server={server} onChange={onChange} onOwn={() => setEditing(true)} />
      )}
      {!running && server.address && <p className="text-xs text-mute-2">The address answers while the server runs.</p>}
    </section>
  );
}

/** Public builds: playit's own app, step by step, then the address it gives. */
function PlayitGuide({ server, onChange, onOwn }: { server: HostedView; onChange: (server: HostedView) => void; onOwn: () => void }) {
  return (
    <div className="flex flex-col gap-3 rounded-2xl border border-cream/8 bg-ink-900/40 p-4">
      <p className="text-sm leading-relaxed text-cream-300">For an address anyone can join from, even when your router is strict: playit.gg gives one free. Four steps, once.</p>
      <ol className="flex flex-col gap-3 text-sm">
        <GuideStep n={1} title="Get playit">
          <Button variant="secondary" icon={Download} onClick={() => void openExternal(PLAYIT_DOWNLOAD)} className="mt-1.5 h-9 self-start px-4 text-xs">
            Download playit
          </Button>
        </GuideStep>
        <GuideStep n={2} title="Open it and sign in">
          <p className="text-xs leading-relaxed text-mute">It opens playit.gg in your browser. A guest account works.</p>
        </GuideStep>
        <GuideStep n={3} title="Add a tunnel for this server">
          <p className="mb-1.5 text-xs leading-relaxed text-mute">Pick Minecraft Java, and point it at this computer:</p>
          <div className="flex flex-col gap-1.5">
            <CopyLine label="Local address" value="127.0.0.1" />
            <CopyLine label="Local port" value={String(server.port)} />
          </div>
        </GuideStep>
        <GuideStep n={4} title="Paste the address playit shows">
          <OwnAddress server={server} onChange={onChange} placeholder="name.gl.joinmc.link" compact />
        </GuideStep>
      </ol>
      <button type="button" onClick={onOwn} className="self-start text-xs font-bold text-mute transition-colors hover:text-cream">
        I have my own address
      </button>
    </div>
  );
}

function GuideStep({ n, title, children }: { n: number; title: string; children: React.ReactNode }) {
  return (
    <li className="flex gap-3">
      <span className="grid size-6 shrink-0 place-items-center rounded-full bg-cream/10 font-display text-xs font-bold text-cream-200">{n}</span>
      <div className="flex min-w-0 flex-1 flex-col">
        <p className="font-bold text-cream-200">{title}</p>
        {children}
      </div>
    </li>
  );
}

/** Typing in an address: playit's, or your own (a forwarded port, a domain). */
function OwnAddress({
  server,
  onChange,
  onDone,
  canCancel = false,
  placeholder = "play.example.com",
  compact = false,
}: {
  server: HostedView;
  onChange: (server: HostedView) => void;
  onDone?: () => void;
  canCancel?: boolean;
  placeholder?: string;
  compact?: boolean;
}) {
  const { notify } = useApp();
  const [value, setValue] = useState(server.address ?? "");
  const [saving, setSaving] = useState(false);
  const save = async (address: string) => {
    setSaving(true);
    try {
      onChange(await updateHostedServer(server.id, { address }));
      onDone?.();
    } catch (failure: unknown) {
      notify({ tone: "info", title: "Couldn't use that address", body: message(failure) });
    } finally {
      setSaving(false);
    }
  };
  return (
    <form
      className={cn("flex flex-col gap-2", !compact && "rounded-2xl border border-cream/8 bg-ink-900/40 p-4")}
      onSubmit={(event) => {
        event.preventDefault();
        void save(value.trim());
      }}
    >
      {!compact && <p className="text-sm leading-relaxed text-cream-300">The address people use to reach this computer: a playit tunnel's, a forwarded port, or your own domain.</p>}
      <div className={cn("flex gap-2", compact && "flex-col")}>
        <input
          value={value}
          onChange={(event) => setValue(event.target.value)}
          placeholder={placeholder}
          spellCheck={false}
          aria-label="The server's address"
          className="h-9 min-w-0 flex-1 rounded-xl border border-cream/10 bg-ink-900 px-3 font-mono text-sm text-cream outline-none transition-colors placeholder:text-mute-2 focus:border-cream/30"
        />
        <Button type="submit" icon={saving ? undefined : Check} className={cn("h-9 px-4 text-xs", compact && "self-start")} disabled={saving || value.trim() === (server.address ?? "")}>
          {saving ? "Saving…" : "Save"}
        </Button>
      </div>
      {(canCancel || server.address) && (
        <div className="flex gap-3">
          {canCancel && (
            <button type="button" onClick={onDone} className="text-xs font-bold text-mute transition-colors hover:text-cream">
              Cancel
            </button>
          )}
          {server.address && (
            <button type="button" onClick={() => void save("")} className="flex items-center gap-1 text-xs font-bold text-mute transition-colors hover:text-coral">
              <X size={12} aria-hidden="true" />
              Remove the address
            </button>
          )}
        </div>
      )}
    </form>
  );
}
