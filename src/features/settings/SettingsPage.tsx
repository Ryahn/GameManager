import { useQueryClient } from "@tanstack/react-query";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ExternalLink, FolderOpen, Globe, KeyRound, LogOut, Moon, Monitor, RefreshCw, Sun, UserRound } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { PageHeader, Section, Segmented, Switch } from "@/components/ui/primitives";
import { keys, useAppInfo, useAuth, useCheckF95, useF95Status, useSettings, useUpdateSettings } from "@/hooks/queries";
import { api, type Settings } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { cn } from "@/lib/utils";
import { HostAccounts } from "./HostAccounts";
import { SystemSettings } from "./SystemSettings";
import { useOs } from "@/lib/platform";

const ACCENTS = ["violet", "blue", "teal", "amber", "rose"] as const;
const ACCENT_SWATCH: Record<(typeof ACCENTS)[number], string> = {
  violet: "#8b7cf6",
  blue: "#60a5fa",
  teal: "#2dd4bf",
  amber: "#f5b544",
  rose: "#f472a6",
};

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 border-t border-border py-3.5 first:border-t-0 first:pt-0 last:pb-0">
      <div>
        <p className="text-sm font-medium">{label}</p>
        {hint && <p className="mt-0.5 text-[13px] text-muted">{hint}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function SettingsPage() {
  const settings = useSettings();
  const auth = useAuth();
  const info = useAppInfo();
  const update = useUpdateSettings();
  const qc = useQueryClient();
  const f95 = useF95Status();
  const os = useOs();
  const checkF95 = useCheckF95();
  const s = settings.data;
  if (!s) return null;

  const set = (patch: Partial<Settings>) => update.mutate(patch, { onSuccess: () => toast.success("Saved", { duration: 1200 }) });

  const signOut = async () => {
    try {
      qc.setQueryData(keys.auth, await api.signOut());
      qc.removeQueries({ queryKey: ["bookmarks"] });
      qc.invalidateQueries({ queryKey: keys.settings });
      // Back to onboarding so the user can sign in again.
      update.mutate({ onboarding_complete: false });
    } catch (e) {
      toast.error(friendlyError(e).title);
    }
  };

  const chooseFolder = async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Choose your games folder" });
    if (typeof picked === "string") set({ library_dir: picked });
  };

  return (
    <div className="mx-auto max-w-3xl animate-fade-up">
      <PageHeader title="Settings" subtitle="Changes save automatically." />
      <div className="space-y-5">
        <Section title="Account">
          <div className="flex items-center gap-4">
            <div className="grid size-11 place-items-center rounded-full bg-accent/15 text-accent">
              {auth.data?.mode === "api_key" ? <KeyRound className="size-5" /> : <UserRound className="size-5" />}
            </div>
            <div className="min-w-0 flex-1">
              <p className="font-medium">{auth.data?.username ?? (auth.data?.mode === "api_key" ? "Signed in with an API key" : "Signed in")}</p>
              <p className="text-sm text-muted">
                {auth.data?.mode === "sso" ? "Signed in with F95 single sign-on" : `Key stored in ${os.keychain}`}
              </p>
            </div>
            <Button variant="danger" onClick={signOut}>
              <LogOut /> Sign out
            </Button>
          </div>
        </Section>

        <Section title="Games folder" description="Where new downloads are installed.">
          <div className="flex items-center gap-3">
            <div className="min-w-0 flex-1 truncate rounded-lg border border-border bg-surface-2 px-3 py-2 font-mono text-[13px] text-muted" data-selectable>
              {s.library_dir ?? "Not set"}
            </div>
            <Button onClick={chooseFolder}>
              <FolderOpen /> Change
            </Button>
          </div>
        </Section>

        <Section
          title="F95 account"
          description="Download links are read from threads with your own F95 sign-in, in a built-in browser window. The launcher never sees your F95 password."
        >
          <div className="flex items-center gap-4">
            <div className="grid size-11 place-items-center rounded-full bg-accent/15 text-accent">
              <Globe className="size-5" />
            </div>
            <div className="min-w-0 flex-1">
              <p className="font-medium">
                {f95.data == null
                  ? "Not checked yet"
                  : f95.data.logged_in
                    ? `Signed in${f95.data.username ? ` as ${f95.data.username}` : ""}`
                    : "Not signed in to F95"}
              </p>
              <p className="text-sm text-muted">Sign in once in the F95 window; it remembers you.</p>
            </div>
            <Button variant="ghost" onClick={() => checkF95.mutate()} loading={checkF95.isPending}>
              <RefreshCw /> Check
            </Button>
            <Button onClick={() => api.f95Open()}>
              <ExternalLink /> Open F95 window
            </Button>
          </div>
        </Section>

        <Section title="Downloads" description="Limits that keep your account and IP safe. The spacing between downloads and the daily warning can't be turned off.">
          <Row label="Downloads at a time" hint="Never more than one from the same file host.">
            <Segmented
              label="Downloads at a time"
              value={String(s.max_active_downloads) as "1" | "2" | "3"}
              onChange={(v) => set({ max_active_downloads: Number(v) })}
              options={[
                { value: "1", label: "1" },
                { value: "2", label: "2" },
                { value: "3", label: "3" },
              ]}
            />
          </Row>
          <Row label="Delete archives after installing" hint="Frees up space. Turn off to keep the zip/rar you downloaded.">
            <Switch label="Delete archives after installing" checked={s.delete_archives} onChange={(delete_archives) => set({ delete_archives })} />
          </Row>
        </Section>

        <HostAccounts />

        <Section title="Appearance">
          <Row label="Theme">
            <Segmented
              label="Theme"
              value={s.theme}
              onChange={(theme) => set({ theme })}
              options={[
                { value: "dark", label: <><Moon /> Dark</> },
                { value: "light", label: <><Sun /> Light</> },
                { value: "system", label: <><Monitor /> System</> },
              ]}
            />
          </Row>
          <Row label="Accent colour">
            <div role="radiogroup" aria-label="Accent colour" className="flex gap-2">
              {ACCENTS.map((a) => (
                <button
                  key={a}
                  type="button"
                  role="radio"
                  aria-checked={s.accent === a}
                  aria-label={a}
                  onClick={() => set({ accent: a })}
                  className={cn("size-7 rounded-full ring-offset-2 ring-offset-surface-1 transition", s.accent === a ? "ring-2 ring-text" : "hover:scale-110")}
                  style={{ background: ACCENT_SWATCH[a] }}
                />
              ))}
            </div>
          </Row>
          <Row label="Interface size" hint={`${Math.round(s.ui_scale * 100)}%`}>
            <input
              type="range"
              min={0.8}
              max={1.5}
              step={0.05}
              value={s.ui_scale}
              onChange={(e) => set({ ui_scale: Number(e.target.value) })}
              aria-label="Interface size"
              className="w-44 accent-[var(--accent)]"
            />
          </Row>
          <Row label="Reduce motion" hint="Turns off animations and transitions.">
            <Switch label="Reduce motion" checked={s.reduce_motion} onChange={(reduce_motion) => set({ reduce_motion })} />
          </Row>
        </Section>

        <Section title="Behaviour">
          <Row label="Keep running in the tray" hint="Closing the window keeps update checks and playtime tracking going. Quit from the tray icon.">
            <Switch label="Keep running in the tray" checked={s.close_to_tray} onChange={(close_to_tray) => set({ close_to_tray })} />
          </Row>
          <Row label="Update notifications" hint="A desktop notification when games in your library get a new version. Checked every 30 minutes.">
            <Switch label="Update notifications" checked={s.notify_updates} onChange={(notify_updates) => set({ notify_updates })} />
          </Row>
        </Section>

        <SystemSettings version={info.data?.version} />

        <Section title="Privacy">
          <Row label="Blur covers" hint="Covers stay blurred until you hover them. Handy on a shared screen.">
            <Switch label="Blur covers" checked={s.blur_nsfw} onChange={(blur_nsfw) => set({ blur_nsfw })} />
          </Row>
        </Section>

        <p className="pb-4 text-center text-xs text-faint">F95 Game Manager {info.data?.version}</p>
      </div>
    </div>
  );
}
