import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft,
  ArrowRight,
  Bell,
  CheckCircle2,
  ExternalLink,
  Eye,
  EyeOff,
  FolderOpen,
  KeyRound,
  Library,
  LogIn,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/primitives";
import { Logo } from "@/app/Sidebar";
import { api, type AuthStatus } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { keys, useAppInfo, useAuth, useSettings, useUpdateSettings } from "@/hooks/queries";
import { cn } from "@/lib/utils";
import { useOs } from "@/lib/platform";

type Step = "welcome" | "account" | "folder" | "done";
const STEPS: Step[] = ["welcome", "account", "folder", "done"];

export function Onboarding() {
  const auth = useAuth();
  const [step, setStep] = useState<Step>(auth.data?.signed_in ? "folder" : "welcome");
  const idx = STEPS.indexOf(step);

  return (
    <div className="flex h-full">
      <BrandPanel />
      <div className="flex min-w-0 flex-1 flex-col">
        <div className="flex h-16 items-center justify-end px-8">
          <ol className="flex items-center gap-2" aria-label="Setup progress">
            {STEPS.map((s, i) => (
              <li
                key={s}
                aria-current={i === idx ? "step" : undefined}
                className={cn("h-1.5 rounded-full transition-all", i === idx ? "w-8 bg-accent" : i < idx ? "w-4 bg-accent/50" : "w-4 bg-surface-3")}
              />
            ))}
          </ol>
        </div>
        <div className="flex flex-1 items-center justify-center overflow-y-auto px-8 pb-16">
          <div key={step} className="w-full max-w-[460px] animate-fade-up">
            {step === "welcome" && <Welcome onNext={() => setStep("account")} />}
            {step === "account" && <Account onBack={() => setStep("welcome")} onNext={() => setStep("folder")} />}
            {step === "folder" && <Folder onBack={() => setStep("account")} onNext={() => setStep("done")} />}
            {step === "done" && <Done />}
          </div>
        </div>
      </div>
    </div>
  );
}

function BrandPanel() {
  const features = [
    { icon: Library, title: "Your whole library in one place", text: "Track every game, version and hour played." },
    { icon: Bell, title: "Updates come to you", text: "Including stealth updates, checked every 30 minutes." },
    { icon: Sparkles, title: "Discover what's new", text: "Fresh releases and hidden gems from the catalog." },
    { icon: ShieldCheck, title: "Safe by default", text: "Downloads are paced so your accounts don't get flagged." },
  ];
  return (
    <aside className="relative hidden w-[420px] shrink-0 flex-col overflow-hidden border-r border-border bg-surface-1 p-10 lg:flex">
      <div
        aria-hidden
        className="pointer-events-none absolute -left-32 -top-32 size-[420px] rounded-full opacity-30 blur-3xl"
        style={{ background: "radial-gradient(circle, var(--accent), transparent 70%)" }}
      />
      <Logo />
      <div className="relative mt-auto space-y-6">
        {features.map((f) => (
          <div key={f.title} className="flex gap-4">
            <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent/12 text-accent">
              <f.icon className="size-5" />
            </div>
            <div>
              <p className="font-medium">{f.title}</p>
              <p className="mt-0.5 text-sm text-muted">{f.text}</p>
            </div>
          </div>
        ))}
      </div>
    </aside>
  );
}

function StepTitle({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <div className="mb-8">
      <h1 className="font-[family-name:var(--font-display)] text-[28px] font-semibold tracking-tight">{title}</h1>
      {children && <p className="mt-2 text-[15px] leading-relaxed text-muted">{children}</p>}
    </div>
  );
}

function Welcome({ onNext }: { onNext: () => void }) {
  return (
    <>
      <StepTitle title="Welcome to F95 Game Manager">
        The official launcher for the f95api.xyz catalog. Setup takes about a minute: sign in, pick where your games live, and
        you're done.
      </StepTitle>
      <Button variant="primary" size="lg" className="w-full" onClick={onNext} autoFocus>
        Get started <ArrowRight />
      </Button>
    </>
  );
}

function Account({ onBack, onNext }: { onBack: () => void; onNext: () => void }) {
  const { keychain } = useOs();
  const qc = useQueryClient();
  const info = useAppInfo();
  const ssoReady = info.data?.sso_configured ?? false;
  const [showKey, setShowKey] = useState(!ssoReady);
  const [key, setKey] = useState("");
  const [reveal, setReveal] = useState(false);
  const [busy, setBusy] = useState<"sso" | "key" | null>(null);
  const [error, setError] = useState<unknown>(null);

  const finish = (s: AuthStatus) => {
    qc.setQueryData(keys.auth, s);
    qc.invalidateQueries({ queryKey: keys.settings });
    onNext();
  };

  const run = async (kind: "sso" | "key") => {
    setBusy(kind);
    setError(null);
    try {
      finish(kind === "sso" ? await api.ssoSignIn() : await api.connectApiKey(key));
    } catch (e) {
      setError(e);
    } finally {
      setBusy(null);
    }
  };

  const err = error ? friendlyError(error) : null;

  return (
    <>
      <StepTitle title="Sign in">
        Your account connects the launcher to the catalog and keeps your library and update list in sync.
      </StepTitle>

      <Button
        variant="primary"
        size="lg"
        className="w-full"
        disabled={!ssoReady || busy === "key"}
        loading={busy === "sso"}
        onClick={() => run("sso")}
      >
        <LogIn /> {busy === "sso" ? "Waiting for your browser…" : "Sign in with F95"}
      </Button>
      <p className="mt-2 text-center text-xs text-faint">
        {ssoReady ? "Opens your browser. Your password is never seen by the launcher." : "Sign-in with F95 is coming soon. Use an API key for now."}
      </p>

      <div className="my-6 flex items-center gap-3 text-xs text-faint">
        <div className="h-px flex-1 bg-border" />
        or
        <div className="h-px flex-1 bg-border" />
      </div>

      {!showKey ? (
        <Button variant="ghost" className="w-full" onClick={() => setShowKey(true)}>
          <KeyRound /> Use an API key instead
        </Button>
      ) : (
        <form
          className="space-y-4 rounded-xl border border-border bg-surface-1 p-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (key.trim()) run("key");
          }}
        >
          <ol className="space-y-1.5 text-sm text-muted">
            <li>
              1. Open{" "}
              <button
                type="button"
                className="inline-flex items-center gap-1 font-medium text-accent hover:underline"
                onClick={() => info.data && openUrl(info.data.api_keys_url)}
              >
                your API keys <ExternalLink className="size-3.5" />
              </button>{" "}
              and sign in with F95zone.
            </li>
            <li>2. Click <span className="font-medium text-text">Create</span> and copy the key (it's shown once).</li>
            <li>3. Paste it below.</li>
          </ol>
          <div className="relative">
            <Input
              aria-label="API key"
              type={reveal ? "text" : "password"}
              autoComplete="off"
              spellCheck={false}
              placeholder="Paste your API key"
              value={key}
              onChange={(e) => setKey(e.target.value)}
              className="pr-10 font-mono"
              autoFocus={!ssoReady}
            />
            <button
              type="button"
              onClick={() => setReveal((r) => !r)}
              aria-label={reveal ? "Hide key" : "Show key"}
              className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-faint hover:text-text"
            >
              {reveal ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
            </button>
          </div>
          <Button type="submit" className="w-full" disabled={!key.trim() || busy === "sso"} loading={busy === "key"}>
            Connect
          </Button>
          <p className="text-xs text-faint">The key is stored in {keychain}, not in a plain file.</p>
        </form>
      )}

      {err && (
        <div role="alert" className="mt-4 rounded-lg border border-danger/30 bg-danger/8 px-3 py-2.5 text-sm">
          <p className="font-medium text-danger">{err.title}</p>
          <p className="mt-0.5 text-muted">{err.detail}</p>
        </div>
      )}

      <div className="mt-8">
        <Button variant="ghost" onClick={onBack}>
          <ArrowLeft /> Back
        </Button>
      </div>
    </>
  );
}

function Folder({ onBack, onNext }: { onBack: () => void; onNext: () => void }) {
  const settings = useSettings();
  const update = useUpdateSettings();
  const dir = settings.data?.library_dir ?? null;

  const choose = async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Choose your games folder" });
    if (typeof picked === "string") update.mutate({ library_dir: picked });
  };

  return (
    <>
      <StepTitle title="Where do your games live?">
        New downloads are installed here as <span className="font-mono text-[13px] text-text">Creator / Game / Version</span>. You can change
        this any time in Settings.
      </StepTitle>
      <button
        type="button"
        onClick={choose}
        className="flex w-full items-center gap-4 rounded-xl border border-dashed border-border-strong bg-surface-1 p-5 text-left transition-colors hover:border-accent hover:bg-accent/5"
      >
        <div className="grid size-11 shrink-0 place-items-center rounded-xl bg-accent/12 text-accent">
          <FolderOpen className="size-5" />
        </div>
        <div className="min-w-0">
          <p className="font-medium">{dir ? "Games folder" : "Choose a folder"}</p>
          <p className="truncate text-sm text-muted" data-selectable>
            {dir ?? "For example D:\\Games"}
          </p>
        </div>
        {dir && <CheckCircle2 className="ml-auto size-5 shrink-0 text-success" />}
      </button>
      <div className="mt-8 flex items-center justify-between">
        <Button variant="ghost" onClick={onBack}>
          <ArrowLeft /> Back
        </Button>
        <Button variant="primary" onClick={onNext}>
          {dir ? "Continue" : "Skip for now"} <ArrowRight />
        </Button>
      </div>
    </>
  );
}

function Done() {
  const update = useUpdateSettings();
  return (
    <>
      <div className="mb-6 grid size-14 place-items-center rounded-2xl bg-success/15 text-success">
        <CheckCircle2 className="size-7" />
      </div>
      <StepTitle title="You're all set">
        Add games from Discover or search with <span className="font-medium text-text">Ctrl K</span>. When you download your first game,
        we'll ask you to sign in to F95zone once in the built-in browser.
      </StepTitle>
      <Button variant="primary" size="lg" className="w-full" autoFocus loading={update.isPending} onClick={() => update.mutate({ onboarding_complete: true })}>
        Open my library <ArrowRight />
      </Button>
    </>
  );
}
