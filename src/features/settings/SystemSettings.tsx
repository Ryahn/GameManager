/** Settings: startup & privacy, import & backup, updates. */
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { DatabaseBackup, Download, FileUp, Lock, RefreshCw, Upload } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input, Section, Select, Switch } from "@/components/ui/primitives";
import { keys } from "@/hooks/queries";
import { api } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";

const SYS = ["system-info"] as const;

const BOSS_KEYS = [
  { value: "", label: "Off" },
  { value: "Ctrl+Shift+H", label: "Ctrl + Shift + H" },
  { value: "Ctrl+Alt+Space", label: "Ctrl + Alt + Space" },
  { value: "Ctrl+Shift+F12", label: "Ctrl + Shift + F12" },
  { value: "Alt+`", label: "Alt + `" },
];

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

const fail = (title: string) => (e: unknown) => toast.error(title, { description: friendlyError(e).detail });

function PinForm({ enabled, onDone }: { enabled: boolean; onDone: () => void }) {
  const [current, setCurrent] = useState("");
  const [pin, setPin] = useState("");
  const [busy, setBusy] = useState(false);
  const run = async (fn: () => Promise<void>, ok: string) => {
    setBusy(true);
    try {
      await fn();
      toast.success(ok);
      setCurrent("");
      setPin("");
      onDone();
    } catch (e) {
      fail("That didn't work")(e);
    } finally {
      setBusy(false);
    }
  };
  const digits = (v: string) => v.replace(/\D/g, "").slice(0, 8);
  return (
    <form
      className="flex flex-wrap items-center gap-2"
      onSubmit={(e) => {
        e.preventDefault();
        void run(() => api.pinSet(enabled ? current : null, pin), enabled ? "PIN changed" : "PIN lock is on");
      }}
    >
      {enabled && (
        <Input
          type="password"
          inputMode="numeric"
          value={current}
          onChange={(e) => setCurrent(digits(e.target.value))}
          placeholder="Current PIN"
          aria-label="Current PIN"
          className="h-9 w-32"
        />
      )}
      <Input
        type="password"
        inputMode="numeric"
        value={pin}
        onChange={(e) => setPin(digits(e.target.value))}
        placeholder={enabled ? "New PIN" : "4-8 digits"}
        aria-label={enabled ? "New PIN" : "PIN"}
        className="h-9 w-32"
      />
      <Button type="submit" size="sm" disabled={pin.length < 4 || (enabled && current.length < 4)} loading={busy}>
        {enabled ? "Change" : "Turn on"}
      </Button>
      {enabled && (
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={current.length < 4}
          onClick={() => run(() => api.pinClear(current), "PIN lock is off")}
        >
          Turn off
        </Button>
      )}
    </form>
  );
}

export function SystemSettings({ version }: { version?: string }) {
  const qc = useQueryClient();
  const sys = useQuery({ queryKey: SYS, queryFn: api.systemInfo });
  const f95cPath = useQuery({ queryKey: ["f95checker-path"], queryFn: api.f95checkerDefaultPath, staleTime: Infinity });
  const [busy, setBusy] = useState<string | null>(null);
  const [update, setUpdate] = useState<{ version: string; notes: string | null } | null | undefined>(undefined);
  const s = sys.data;
  const refresh = () => qc.invalidateQueries({ queryKey: SYS });

  const busyRun = async <T,>(key: string, fn: () => Promise<T>, done: (r: T) => void, title: string) => {
    setBusy(key);
    try {
      done(await fn());
    } catch (e) {
      fail(title)(e);
    } finally {
      setBusy(null);
    }
  };

  const importF95c = async (path?: string | null) => {
    const file =
      path ??
      ((await openDialog({ multiple: false, title: "Choose F95Checker's db.sqlite3", filters: [{ name: "F95Checker database", extensions: ["sqlite3", "db"] }] })) as
        | string
        | null);
    if (!file) return;
    await busyRun(
      "f95c",
      () => api.importF95checker(file),
      (r) => {
        qc.invalidateQueries({ queryKey: keys.local });
        qc.invalidateQueries({ queryKey: ["bookmarks"] });
        toast.success(`Imported ${r.games} game${r.games === 1 ? "" : "s"} from F95Checker`, {
          description: [
            `${r.executables_linked} linked to their executables`,
            r.executables_missing ? `${r.executables_missing} executables not found on this PC` : "",
            r.not_in_catalog ? `${r.not_in_catalog} not in the catalog yet` : "",
            r.skipped_custom ? `${r.skipped_custom} custom games skipped` : "",
          ]
            .filter(Boolean)
            .join(" · "),
          duration: 9000,
        });
      },
      "Import failed",
    );
  };

  const exportBackup = async () => {
    const date = new Date().toISOString().slice(0, 10);
    const file = await saveDialog({ title: "Save library backup", defaultPath: `f95-game-manager-backup-${date}.json`, filters: [{ name: "Backup", extensions: ["json"] }] });
    if (!file) return;
    await busyRun("export", () => api.backupExport(file), (n) => toast.success(`Backed up ${n} games`), "Backup failed");
  };

  const restoreBackup = async () => {
    const file = (await openDialog({ multiple: false, title: "Choose a backup", filters: [{ name: "Backup", extensions: ["json"] }] })) as string | null;
    if (!file) return;
    await busyRun(
      "restore",
      () => api.backupImport(file),
      (r) => {
        qc.invalidateQueries({ queryKey: keys.local });
        qc.invalidateQueries({ queryKey: ["bookmarks"] });
        toast.success(`Restored ${r.games} games`, { description: `${r.sessions_added} play sessions added. Values you already had were kept.` });
      },
      "Restore failed",
    );
  };

  return (
    <>
      <Section title="Startup & privacy">
        <Row label="Start with Windows" hint="Opens in the tray when you sign in, so update checks run in the background.">
          <Switch
            label="Start with Windows"
            checked={!!s?.autostart}
            onChange={(v) => busyRun("autostart", () => api.autostartSet(v), refresh, "Couldn't change that")}
          />
        </Row>
        <Row label="Boss key" hint="One shortcut hides every launcher window. Press it again to bring the launcher back.">
          <Select
            label="Boss key"
            value={s?.boss_key ?? ""}
            onChange={(v) => busyRun("boss", () => api.bossKeySet(v || null), () => { refresh(); toast.success(v ? `Boss key set to ${v}` : "Boss key off"); }, "Couldn't set the boss key")}
            options={BOSS_KEYS}
            className="w-48"
          />
        </Row>
        <div className="border-t border-border py-3.5 last:pb-0">
          <div className="mb-2 flex items-center justify-between gap-6">
            <div>
              <p className="text-sm font-medium">PIN lock</p>
              <p className="mt-0.5 text-[13px] text-muted">
                Asks for a PIN when the launcher opens and after the boss key. A privacy screen for shared PCs; your files aren't encrypted.
              </p>
            </div>
            {s?.pin_enabled && (
              <Button size="sm" variant="ghost" onClick={() => api.lockNow()}>
                <Lock /> Lock now
              </Button>
            )}
          </div>
          <PinForm enabled={!!s?.pin_enabled} onDone={refresh} />
        </div>
      </Section>

      <Section title="Import & backup" description="Bring your library over from F95Checker, or keep a copy of everything stored on this PC.">
        <Row
          label="Import from F95Checker"
          hint={
            f95cPath.data
              ? `Found: ${f95cPath.data}. Close F95Checker first. Nothing in it is changed.`
              : "Pick F95Checker's db.sqlite3 (usually in %APPDATA%\\f95checker). Nothing in it is changed."
          }
        >
          <div className="flex gap-2">
            {f95cPath.data && (
              <Button size="sm" variant="primary" loading={busy === "f95c"} onClick={() => importF95c(f95cPath.data)}>
                <FileUp /> Import
              </Button>
            )}
            <Button size="sm" variant="outline" disabled={busy === "f95c"} onClick={() => importF95c(null)}>
              Choose file…
            </Button>
          </div>
        </Row>
        <Row label="Backup" hint="Play status, ratings, notes, labels, versions, launch settings and play history. No passwords or keys.">
          <div className="flex gap-2">
            <Button size="sm" variant="outline" loading={busy === "export"} onClick={exportBackup}>
              <Download /> Export…
            </Button>
            <Button size="sm" variant="ghost" loading={busy === "restore"} onClick={restoreBackup}>
              <Upload /> Restore…
            </Button>
          </div>
        </Row>
      </Section>

      <Section title="Updates">
        <Row
          label={`F95 Game Manager ${version ?? ""}`}
          hint={
            s && !s.updater_configured
              ? "Automatic updates aren't set up in this build."
              : update === null
                ? "You're on the latest version."
                : update
                  ? `Version ${update.version} is available.`
                  : "Updates are signed; the launcher only installs ones that match its key."
          }
        >
          {update ? (
            <Button size="sm" variant="primary" loading={busy === "install"} onClick={() => busyRun("install", api.updateInstall, () => undefined, "Update failed")}>
              <DatabaseBackup /> Install and restart
            </Button>
          ) : (
            <Button
              size="sm"
              variant="outline"
              disabled={!s?.updater_configured}
              loading={busy === "check"}
              onClick={() => busyRun("check", api.updateCheck, setUpdate, "Couldn't check for updates")}
            >
              <RefreshCw /> Check for updates
            </Button>
          )}
        </Row>
      </Section>
    </>
  );
}
