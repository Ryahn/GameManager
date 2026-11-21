/** The parts of the game page that are about *your* copy: files, notes, history. */
import { useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Check, FolderOpen, FolderSearch, Play, Plus, Star, Trash2, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge, Input, Section, Tooltip } from "@/components/ui/primitives";
import { useLocalActions, useLocalLibrary, usePlay, useRunning, useSessions, useUpdateLocal } from "@/hooks/queries";
import { useUi } from "@/state/ui";
import { useOs } from "@/lib/platform";
import { kindMeta, type Kind } from "@/lib/kinds";
import { api, type LocalGame } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { formatPlaytime, sameVersion } from "@/lib/play";
import { cleanVersion, cn } from "@/lib/utils";

/** Pick a folder and link it; falls back to picking the exe. */
export function useLocate(id: number, onDone?: () => void) {
  const { linkFolder, addTarget } = useLocalActions(id);
  const os = useOs();
  const pickExe = async () => {
    const picked = await openDialog({
      multiple: false,
      title: "Choose the game's program",
      // Linux game binaries often have no extension, so allow any file there.
      filters: [{ name: "Games", extensions: os.platform === "linux" ? [...os.pickExtensions, "*"] : os.pickExtensions }],
    });
    if (typeof picked === "string") addTarget.mutate(picked, { onSuccess: onDone });
  };
  const pickFolder = async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Choose the game's folder" });
    if (typeof picked !== "string") return;
    try {
      await linkFolder.mutateAsync({ dir: picked });
      toast.success("Game linked", { description: "Press Play to start it." });
      onDone?.();
    } catch (e) {
      toast.error("No game found in that folder", {
        description: friendlyError(e).detail,
        action: { label: "Pick the program", onClick: () => void pickExe() },
      });
    }
  };
  return { pickFolder, pickExe, busy: linkFolder.isPending || addTarget.isPending };
}

// ------------------------------------------------------------ Files & launch

export function FilesPanel({
  id,
  local,
  latest,
  kind = "games",
}: {
  id: number;
  local: LocalGame | null | undefined;
  latest: string;
  kind?: Kind;
}) {
  const isGame = kind === "games";
  const { removeTarget, setDefault } = useLocalActions(id);
  const update = useUpdateLocal(id);
  const play = usePlay();
  const running = useRunning();
  const locate = useLocate(id);
  const os = useOs();
  const [version, setVersion] = useState(local?.installed_version ?? "");
  useEffect(() => setVersion(local?.installed_version ?? ""), [local?.installed_version]);

  const targets = local?.targets ?? [];
  const isRunning = running.data?.has(id);
  const saveVersion = (v: string) => {
    if (v.trim() !== (local?.installed_version ?? "")) update.mutate({ installed_version: v.trim() });
  };

  if (!targets.length && !local?.install_dir) {
    return (
      <div className="mx-auto max-w-lg py-10 text-center">
        <div className="mx-auto mb-4 grid size-14 place-items-center rounded-2xl bg-accent/12 text-accent">
          <FolderSearch className="size-7" />
        </div>
        <h3 className="text-lg font-semibold">Already have this {kindMeta(kind).one}?</h3>
        <p className="mt-2 text-sm text-muted">
          {isGame
            ? "Point to its folder and the launcher finds the executable, tracks your playtime and tells you when your copy is out of date."
            : "Point to its folder (or file) and the launcher remembers where it is and tells you when your copy is out of date."}
        </p>
        <div className="mt-6 flex justify-center gap-2">
          <Button variant="primary" onClick={locate.pickFolder} loading={locate.busy}>
            <FolderOpen /> Choose {isGame ? "game " : ""}folder
          </Button>
          {isGame && (
            <Button variant="ghost" onClick={locate.pickExe}>
              Pick the program instead
            </Button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="grid max-w-3xl gap-5">
      {isGame && (
      <Section
        title="Launch"
        description="The default is used by Play, the tray and the library."
        actions={
          <Button size="sm" variant="outline" onClick={locate.pickExe}>
            <Plus /> Add executable
          </Button>
        }
      >
        {targets.length === 0 ? (
          <p className="text-sm text-muted">No executable found yet. Add one to play from the launcher.</p>
        ) : (
          <ul className="divide-y divide-border">
            {targets.map((t) => (
              <li key={t.id} className="flex items-center gap-3 py-2.5 first:pt-0 last:pb-0">
                <div className="min-w-0 flex-1">
                  <p className="flex items-center gap-2 text-sm font-medium">
                    {t.label}
                    {t.is_default && <Badge tone="accent">Default</Badge>}
                  </p>
                  <p className="truncate font-mono text-[12px] text-faint" title={t.path} data-selectable>
                    {t.path}
                  </p>
                </div>
                {!t.is_default && (
                  <Button size="sm" variant="ghost" onClick={() => setDefault.mutate(t.id)}>
                    Make default
                  </Button>
                )}
                <Tooltip content={`Play with ${t.label}`} side="top">
                  <Button
                    size="icon-sm"
                    variant="ghost"
                    aria-label={`Play with ${t.label}`}
                    disabled={isRunning}
                    onClick={() => play.mutate({ id, targetId: t.id, title: local?.cached?.title })}
                  >
                    <Play />
                  </Button>
                </Tooltip>
                <Tooltip content="Remove from the launcher (the file stays on disk)" side="top">
                  <Button size="icon-sm" variant="ghost" aria-label={`Remove ${t.label}`} onClick={() => removeTarget.mutate(t.id)}>
                    <Trash2 />
                  </Button>
                </Tooltip>
              </li>
            ))}
          </ul>
        )}
      </Section>
      )}

      <Section title="Installed copy">
        <div className="space-y-4">
          <div>
            <p className="mb-1.5 text-[13px] text-muted">Folder</p>
            <div className="flex items-center gap-2">
              <div className="min-w-0 flex-1 truncate rounded-lg border border-border bg-surface-2 px-3 py-2 font-mono text-[12.5px] text-muted" data-selectable>
                {local?.install_dir ?? "Unknown"}
              </div>
              <Button size="sm" variant="outline" onClick={() => api.openFolder(id).catch((e) => toast.error(friendlyError(e).detail))}>
                <FolderOpen /> Open
              </Button>
              <Button size="sm" variant="ghost" onClick={locate.pickFolder}>
                Change
              </Button>
            </div>
          </div>
          <div>
            <label htmlFor="installed-version" className="mb-1.5 block text-[13px] text-muted">
              Installed version
            </label>
            <div className="flex items-center gap-2">
              <Input
                id="installed-version"
                value={version}
                placeholder="e.g. 0.9.1"
                onChange={(e) => setVersion(e.target.value)}
                onBlur={(e) => saveVersion(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
                className="h-9 max-w-48"
              />
              {latest && !sameVersion(version, latest) && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => {
                    setVersion(latest);
                    saveVersion(latest);
                  }}
                >
                  I have v{cleanVersion(latest)}
                </Button>
              )}
            </div>
            <p className="mt-1.5 text-[12.5px] text-faint">Used to tell you when your copy is behind the latest release.</p>
          </div>
        </div>
      </Section>

      <Section title="Uninstall" description={`Removes the game from your library and moves its folder to the ${os.trash}.`}>
        <Button
          variant="danger"
          disabled={isRunning}
          onClick={() => useUi.getState().askRemove({ id, title: local?.cached?.title ?? "this game", deleteFiles: true })}
        >
          <Trash2 /> Uninstall…
        </Button>
      </Section>
    </div>
  );
}

// -------------------------------------------------------------------- Notes

function Stars({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  const [hover, setHover] = useState(0);
  const shown = hover || value;
  return (
    <div role="radiogroup" aria-label="Your rating" className="flex items-center gap-0.5" onMouseLeave={() => setHover(0)}>
      {[1, 2, 3, 4, 5].map((n) => (
        <button
          key={n}
          type="button"
          role="radio"
          aria-checked={value === n}
          aria-label={`${n} star${n === 1 ? "" : "s"}`}
          onMouseEnter={() => setHover(n)}
          onClick={() => onChange(value === n ? 0 : n)}
          className="rounded p-0.5 text-faint transition-colors hover:text-warning"
        >
          <Star className={cn("size-5", n <= shown && "fill-warning text-warning")} />
        </button>
      ))}
    </div>
  );
}

export function NotesPanel({ id, local, latest }: { id: number; local: LocalGame | null | undefined; latest: string }) {
  const update = useUpdateLocal(id);
  const all = useLocalLibrary();
  const [notes, setNotes] = useState(local?.notes ?? "");
  const [saved, setSaved] = useState(true);
  const [newLabel, setNewLabel] = useState("");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const labels = local?.labels ?? [];
  const known = useMemo(
    () => [...new Set(all.data?.flatMap((g) => g.labels) ?? [])].filter((l) => !labels.includes(l)).sort(),
    [all.data, labels],
  );

  useEffect(() => {
    setNotes(local?.notes ?? "");
    // Only when switching games; typing shouldn't be overwritten by refetches.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);

  const saveNotes = (v: string) => {
    clearTimeout(timer.current);
    update.mutate({ notes: v }, { onSuccess: () => setSaved(true) });
  };
  const onNotes = (v: string) => {
    setNotes(v);
    setSaved(false);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => saveNotes(v), 900);
  };
  useEffect(() => () => clearTimeout(timer.current), []);

  const addLabel = (l: string) => {
    const v = l.trim();
    if (!v || labels.some((x) => x.toLowerCase() === v.toLowerCase())) return setNewLabel("");
    update.mutate({ labels: [...labels, v] });
    setNewLabel("");
  };

  const finished = local?.finished_version;

  return (
    <div className="grid max-w-3xl gap-5">
      <Section title="Your rating and progress">
        <div className="flex flex-wrap items-center gap-x-10 gap-y-4">
          <div>
            <p className="mb-1.5 text-[13px] text-muted">Rating</p>
            <Stars value={local?.rating ?? 0} onChange={(rating) => update.mutate({ rating })} />
          </div>
          <div>
            <p className="mb-1.5 text-[13px] text-muted">Finished up to</p>
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium">{finished ? `v${cleanVersion(finished)}` : "Not finished"}</span>
              {latest && !sameVersion(finished, latest) && (
                <Button size="sm" variant="ghost" onClick={() => update.mutate({ finished_version: latest })}>
                  <Check /> I finished v{cleanVersion(latest)}
                </Button>
              )}
              {finished && (
                <Button size="icon-sm" variant="ghost" aria-label="Clear finished version" onClick={() => update.mutate({ finished_version: "" })}>
                  <X />
                </Button>
              )}
            </div>
          </div>
        </div>
      </Section>

      <Section title="Labels" description="Your own tags. Filter the library by them.">
        <div className="flex flex-wrap items-center gap-1.5">
          {labels.map((l) => (
            <span key={l} className="inline-flex h-7 items-center gap-1 rounded-full border border-accent/40 bg-accent/10 pl-3 pr-1.5 text-[12.5px] font-medium text-accent">
              {l}
              <button
                type="button"
                aria-label={`Remove label ${l}`}
                onClick={() => update.mutate({ labels: labels.filter((x) => x !== l) })}
                className="rounded-full p-0.5 hover:bg-accent/20"
              >
                <X className="size-3" />
              </button>
            </span>
          ))}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              addLabel(newLabel);
            }}
          >
            <Input
              list="known-labels"
              value={newLabel}
              onChange={(e) => setNewLabel(e.target.value)}
              placeholder={labels.length ? "Add another" : "Add a label, e.g. Favourite"}
              aria-label="New label"
              maxLength={40}
              className="h-7 w-48 rounded-full text-[12.5px]"
            />
            <datalist id="known-labels">
              {known.map((l) => (
                <option key={l} value={l} />
              ))}
            </datalist>
          </form>
        </div>
      </Section>

      <Section title="Notes" description={saved ? "Saved on this computer." : "Saving…"}>
        <textarea
          value={notes}
          onChange={(e) => onNotes(e.target.value)}
          onBlur={() => !saved && saveNotes(notes)}
          placeholder="Choices you made, where you left off, walkthrough links…"
          aria-label="Notes"
          rows={8}
          className="w-full resize-y rounded-lg border border-border bg-surface-2 p-3 text-sm leading-relaxed text-text placeholder:text-faint focus:border-accent focus:outline-none focus:ring-2 focus:ring-accent/30"
        />
      </Section>
    </div>
  );
}

// ------------------------------------------------------------------ History

export function HistoryPanel({ id, local }: { id: number; local: LocalGame | null | undefined }) {
  const sessions = useSessions(id);
  const list = sessions.data ?? [];
  const fmt = (ts: number) =>
    new Date(ts * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  return (
    <div className="max-w-3xl">
      <div className="mb-5 grid grid-cols-3 gap-3">
        {[
          ["Total playtime", formatPlaytime(local?.playtime_secs)],
          ["Sessions", String(list.length)],
          ["Last played", local?.last_played_at ? fmt(local.last_played_at) : "Never"],
        ].map(([k, v]) => (
          <div key={k} className="rounded-xl border border-border bg-surface-1 px-4 py-3">
            <p className="text-[12px] text-faint">{k}</p>
            <p className="mt-0.5 font-semibold">{v}</p>
          </div>
        ))}
      </div>
      {list.length === 0 ? (
        <p className="py-8 text-center text-sm text-muted">Play from the launcher and your sessions show up here.</p>
      ) : (
        <ul className="overflow-hidden rounded-[var(--radius-card)] border border-border bg-surface-1">
          {list.map((s, i) => (
            <li key={s.started_at} className={cn("flex items-center gap-4 px-4 py-2.5 text-sm", i && "border-t border-border")}>
              <span className="flex-1 text-muted">{fmt(s.started_at)}</span>
              {s.version && <span className="text-faint">v{cleanVersion(s.version)}</span>}
              <span className="w-24 text-right font-medium">{s.secs ? formatPlaytime(s.secs) : "Opened"}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
