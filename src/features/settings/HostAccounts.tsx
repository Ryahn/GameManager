/** Settings → File host accounts: Pixeldrain API key and MEGA sign-in. */
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CheckCircle2, ExternalLink, KeyRound, LogOut } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input, Section } from "@/components/ui/primitives";
import { api, type ApiError, type HostAccounts as Accounts } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";

const KEY = ["host-accounts"] as const;

function Connected({ who, onRemove, busy, label }: { who: string; onRemove: () => void; busy: boolean; label: string }) {
  return (
    <div className="flex items-center gap-3 rounded-lg border border-border bg-surface-2 px-3 py-2.5">
      <CheckCircle2 className="size-4 shrink-0 text-success" />
      <span className="min-w-0 flex-1 truncate text-sm">
        Signed in as <span className="font-medium">{who}</span>
      </span>
      <Button size="sm" variant="ghost" onClick={onRemove} loading={busy}>
        <LogOut /> {label}
      </Button>
    </div>
  );
}

export function HostAccounts() {
  const qc = useQueryClient();
  const accounts = useQuery({ queryKey: KEY, queryFn: api.hostAccounts, staleTime: Infinity });
  const [pdKey, setPdKey] = useState("");
  const [gfToken, setGfToken] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [mfa, setMfa] = useState("");
  const [needsMfa, setNeedsMfa] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);

  const run = async (what: string, fn: () => Promise<Accounts>, ok: string) => {
    setBusy(what);
    try {
      qc.setQueryData(KEY, await fn());
      qc.invalidateQueries({ queryKey: ["links"] });
      toast.success(ok);
      return true;
    } catch (e) {
      const err = e as ApiError;
      if (err?.kind === "invalid" && err.message === "mfa_required") {
        setNeedsMfa(true);
        toast("Enter the code from your authenticator app");
      } else {
        toast.error(friendlyError(e).title, { description: friendlyError(e).detail });
      }
      return false;
    } finally {
      setBusy(null);
    }
  };

  const a = accounts.data;

  return (
    <Section
      title="File host accounts"
      description="Optional. With your own accounts, these hosts download inside the app with fewer captchas and bigger quotas. Other hosts open in the F95 window, where the app picks up the file. Keys and sessions are stored in Windows Credential Manager."
    >
      <div className="space-y-5">
        <div>
          <div className="mb-2 flex items-center justify-between">
            <p className="text-sm font-medium">Pixeldrain</p>
            <button
              type="button"
              className="inline-flex items-center gap-1 text-[13px] text-accent hover:underline"
              onClick={() => openUrl("https://pixeldrain.com/user/api_keys")}
            >
              Get an API key <ExternalLink className="size-3" />
            </button>
          </div>
          {a?.pixeldrain ? (
            <Connected who={a.pixeldrain} label="Remove key" busy={busy === "pd-clear"} onRemove={() => run("pd-clear", api.clearPixeldrain, "Pixeldrain key removed")} />
          ) : (
            <form
              className="flex gap-2"
              onSubmit={async (e) => {
                e.preventDefault();
                if (await run("pd", () => api.setPixeldrainKey(pdKey), "Pixeldrain connected")) setPdKey("");
              }}
            >
              <div className="relative flex-1">
                <KeyRound className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-faint" />
                <Input
                  type="password"
                  autoComplete="off"
                  value={pdKey}
                  onChange={(e) => setPdKey(e.target.value)}
                  placeholder="Paste your Pixeldrain API key"
                  aria-label="Pixeldrain API key"
                  className="h-9 pl-9"
                />
              </div>
              <Button type="submit" disabled={!pdKey.trim()} loading={busy === "pd"}>
                Save key
              </Button>
            </form>
          )}
        </div>

        <div className="border-t border-border pt-5">
          <div className="mb-2 flex items-center justify-between">
            <p className="text-sm font-medium">
              Gofile <span className="font-normal text-faint">· Premium only</span>
            </p>
            <button type="button" className="inline-flex items-center gap-1 text-[13px] text-accent hover:underline" onClick={() => openUrl("https://gofile.io/api")}>
              Find your token <ExternalLink className="size-3" />
            </button>
          </div>
          {a?.gofile ? (
            <Connected who={a.gofile} label="Remove token" busy={busy === "gf-clear"} onRemove={() => run("gf-clear", api.clearGofile, "Gofile token removed")} />
          ) : (
            <form
              className="flex gap-2"
              onSubmit={async (e) => {
                e.preventDefault();
                if (await run("gf", () => api.setGofileToken(gfToken), "Gofile Premium connected")) setGfToken("");
              }}
            >
              <div className="relative flex-1">
                <KeyRound className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-faint" />
                <Input
                  type="password"
                  autoComplete="off"
                  value={gfToken}
                  onChange={(e) => setGfToken(e.target.value)}
                  placeholder="Paste your Gofile API token"
                  aria-label="Gofile API token"
                  className="h-9 pl-9"
                />
              </div>
              <Button type="submit" disabled={!gfToken.trim()} loading={busy === "gf"}>
                Save token
              </Button>
            </form>
          )}
          <p className="mt-1.5 text-[12.5px] text-faint">Gofile only allows API downloads for Premium accounts. Without one, Gofile links open in the F95 window.</p>
        </div>

        <div className="border-t border-border pt-5">
          <p className="mb-2 text-sm font-medium">MEGA</p>
          {a?.mega ? (
            <Connected who={a.mega} label="Sign out" busy={busy === "mega-out"} onRemove={() => run("mega-out", api.megaSignOut, "Signed out of MEGA")} />
          ) : (
            <form
              className="space-y-2"
              onSubmit={async (e) => {
                e.preventDefault();
                const ok = await run("mega", () => api.megaSignIn(email, password, needsMfa ? mfa : undefined), "Signed in to MEGA");
                if (ok) {
                  setPassword("");
                  setMfa("");
                  setNeedsMfa(false);
                }
              }}
            >
              <div className="grid gap-2 sm:grid-cols-2">
                <Input
                  type="email"
                  autoComplete="username"
                  value={email}
                  onChange={(e) => setEmail(e.target.value)}
                  placeholder="MEGA email"
                  aria-label="MEGA email"
                  className="h-9"
                />
                <Input
                  type="password"
                  autoComplete="current-password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  placeholder="MEGA password"
                  aria-label="MEGA password"
                  className="h-9"
                />
              </div>
              {needsMfa && (
                <Input
                  autoFocus
                  inputMode="numeric"
                  autoComplete="one-time-code"
                  value={mfa}
                  onChange={(e) => setMfa(e.target.value)}
                  placeholder="6-digit code from your authenticator"
                  aria-label="MEGA two-factor code"
                  className="h-9 max-w-xs"
                />
              )}
              <div className="flex items-center gap-3">
                <Button type="submit" disabled={!email.trim() || !password || (needsMfa && !mfa.trim())} loading={busy === "mega"}>
                  Sign in to MEGA
                </Button>
                <p className="text-[12.5px] text-faint">Your password is only used to sign in. Only the MEGA session is kept.</p>
              </div>
            </form>
          )}
        </div>
      </div>
    </Section>
  );
}
