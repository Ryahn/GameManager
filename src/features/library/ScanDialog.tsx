import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { ArrowRight, Check, FolderSearch, Loader2, Search, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge, Dialog, DialogContent, Input } from "@/components/ui/primitives";
import { Cover } from "@/components/game/GameCard";
import { keys, useSearch, useSettings } from "@/hooks/queries";
import { api, type Events, type MatchSuggestion, type ScanFolder, type ScanProgress } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { cn } from "@/lib/utils";

type Step = "pick" | "scanning" | "review";

interface Row {
  folder: ScanFolder;
  match: MatchSuggestion | null;
  selected: boolean;
}

function MatchPicker({ initial, onPick, onCancel }: { initial: string; onPick: (m: MatchSuggestion) => void; onCancel: () => void }) {
  const [q, setQ] = useState(initial);
  const [debounced, setDebounced] = useState(initial);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(q), 300);
    return () => clearTimeout(t);
  }, [q]);
  const search = useSearch(debounced);
  return (
    <div className="mt-2 rounded-lg border border-border bg-surface-2 p-2">
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-faint" />
          <Input autoFocus value={q} onChange={(e) => setQ(e.target.value)} className="h-8 pl-8 text-[13px]" aria-label="Search the catalog" />
        </div>
        <Button size="icon-sm" variant="ghost" onClick={onCancel} aria-label="Close search">
          <X />
        </Button>
      </div>
      <ul className="mt-1 max-h-48 overflow-y-auto">
        {search.isFetching && !search.data && <li className="px-2 py-2 text-[13px] text-faint">Searching…</li>}
        {search.data?.hits.slice(0, 8).map((h) => (
          <li key={h.thread_id}>
            <button
              type="button"
              onClick={() =>
                onPick({ thread_id: h.thread_id, title: h.title, creator: h.creator, cover: h.cover, confidence: 1, confident: true })
              }
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-surface-3"
            >
              <span className="flex-1 truncate text-text">{h.title}</span>
              <span className="truncate text-faint">{h.creator}</span>
            </button>
          </li>
        ))}
        {search.data && search.data.hits.length === 0 && <li className="px-2 py-2 text-[13px] text-faint">No matches.</li>}
      </ul>
    </div>
  );
}

export function ScanDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (v: boolean) => void }) {
  const settings = useSettings();
  const qc = useQueryClient();
  const [step, setStep] = useState<Step>("pick");
  const [progress, setProgress] = useState<ScanProgress | null>(null);
  const [rows, setRows] = useState<Row[]>([]);
  const [picking, setPicking] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    if (!open) {
      setStep("pick");
      setRows([]);
      setProgress(null);
      setPicking(null);
    }
  }, [open]);

  useEffect(() => {
    const un = listen<Events["scan-progress"]>("scan-progress", ({ payload }) => setProgress(payload));
    return () => void un.then((f) => f());
  }, []);

  const run = async (dir: string) => {
    setStep("scanning");
    setProgress({ phase: "finding", done: 0, total: 0 });
    try {
      const found = await api.scan(dir);
      setRows(
        found.map((f) => ({
          folder: f,
          match: f.suggestion,
          selected: !f.linked_thread_id && !!f.suggestion?.confident,
        })),
      );
      setStep("review");
    } catch (e) {
      toast.error("Scan failed", { description: friendlyError(e).detail });
      setStep("pick");
    }
  };

  const choose = async () => {
    const picked = await openDialog({ directory: true, multiple: false, title: "Choose a folder with games", defaultPath: settings.data?.library_dir ?? undefined });
    if (typeof picked === "string") void run(picked);
  };

  const update = (path: string, patch: Partial<Row>) => setRows((rs) => rs.map((r) => (r.folder.path === path ? { ...r, ...patch } : r)));

  const selected = rows.filter((r) => r.selected && r.match);
  const newRows = rows.filter((r) => !r.folder.linked_thread_id);
  const linkedCount = rows.length - newRows.length;
  const unmatched = newRows.filter((r) => !r.match).length;

  const importSelected = async () => {
    setImporting(true);
    try {
      const r = await api.importScan(
        selected.map((s) => ({ thread_id: s.match!.thread_id, path: s.folder.path, version: s.folder.version_guess })),
      );
      qc.invalidateQueries({ queryKey: keys.local });
      qc.invalidateQueries({ queryKey: ["bookmarks"] });
      toast.success(`Added ${r.linked} game${r.linked === 1 ? "" : "s"}`, {
        description: r.failed.length ? `${r.failed.length} couldn't be linked.` : "They're ready to play from your library.",
      });
      onOpenChange(false);
    } catch (e) {
      toast.error("Couldn't add the games", { description: friendlyError(e).detail });
    } finally {
      setImporting(false);
    }
  };

  const pct = progress && progress.total ? Math.round((progress.done / progress.total) * 100) : 0;
  const eta = progress && progress.total ? Math.ceil((progress.total - progress.done) / 60) : 0;

  return (
    <Dialog open={open} onOpenChange={(v) => (step === "scanning" ? undefined : onOpenChange(v))}>
      <DialogContent
        title="Find games on this PC"
        description="Scans a folder for installed games and matches them to the catalog. Nothing is moved or changed on disk."
        className="w-[min(94vw,880px)]"
      >
        {step === "pick" && (
          <div className="space-y-4">
            <div className="flex items-center gap-3 rounded-xl border border-border bg-surface-2 p-4">
              <FolderSearch className="size-5 shrink-0 text-accent" />
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium">Your games folder</p>
                <p className="truncate font-mono text-[12.5px] text-muted">{settings.data?.library_dir ?? "Not set"}</p>
              </div>
              {settings.data?.library_dir && (
                <Button variant="primary" onClick={() => run(settings.data!.library_dir!)}>
                  Scan this folder
                </Button>
              )}
            </div>
            <Button variant="outline" onClick={choose}>
              Choose another folder…
            </Button>
          </div>
        )}

        {step === "scanning" && (
          <div className="py-8 text-center">
            <Loader2 className="mx-auto size-7 animate-spin text-accent" />
            <p className="mt-4 font-medium">
              {progress?.phase === "matching" ? `Matching ${progress.done + 1} of ${progress.total}…` : "Looking for games…"}
            </p>
            {progress?.phase === "matching" && (
              <>
                <div className="mx-auto mt-4 h-1.5 w-72 overflow-hidden rounded-full bg-surface-3">
                  <div className="h-full rounded-full bg-accent transition-[width]" style={{ width: `${pct}%` }} />
                </div>
                <p className="mt-3 text-[13px] text-muted">
                  One catalog search per folder, paced to stay under the API limit
                  {eta > 1 ? ` (about ${eta} minutes left)` : ""}.
                </p>
              </>
            )}
            <Button variant="ghost" className="mt-5" onClick={() => api.cancelScan()}>
              Stop and review what's found
            </Button>
          </div>
        )}

        {step === "review" &&
          (rows.length === 0 ? (
            <div className="py-10 text-center">
              <p className="font-medium">No games found in that folder</p>
              <p className="mt-1 text-sm text-muted">Pick the folder that contains your game folders, or add games one at a time from their page.</p>
              <Button className="mt-5" variant="outline" onClick={() => setStep("pick")}>
                Try another folder
              </Button>
            </div>
          ) : (
            <>
              <p className="mb-3 text-sm text-muted">
                Found {rows.length} game folder{rows.length === 1 ? "" : "s"}
                {linkedCount ? ` · ${linkedCount} already linked` : ""}
                {unmatched ? ` · ${unmatched} need a match` : ""}. Tick the ones to add.
              </p>
              <ul className="max-h-[52vh] space-y-2 overflow-y-auto pr-1">
                {rows.map(({ folder: f, match, selected: sel }) => {
                  const linked = !!f.linked_thread_id;
                  return (
                    <li key={f.path} className={cn("rounded-xl border border-border bg-surface-2/60 p-3", linked && "opacity-55")}>
                      <div className="flex items-center gap-3">
                        <input
                          type="checkbox"
                          aria-label={`Add ${f.folder_name}`}
                          checked={sel}
                          disabled={linked || !match}
                          onChange={(e) => update(f.path, { selected: e.target.checked })}
                          className="size-4 shrink-0 accent-[var(--accent)]"
                        />
                        <div className="min-w-0 basis-[38%]">
                          <p className="truncate text-sm font-medium" title={f.path}>
                            {f.folder_name}
                          </p>
                          <p className="truncate text-[12px] text-faint">
                            {f.targets[0]?.label ?? "No executable"}
                            {f.version_guess ? ` · v${f.version_guess}` : ""}
                          </p>
                        </div>
                        <ArrowRight className="size-4 shrink-0 text-faint" />
                        <div className="flex min-w-0 flex-1 items-center gap-3">
                          {linked ? (
                            <span className="text-[13px] text-muted">Already in your library</span>
                          ) : match ? (
                            <>
                              <Cover src={match.cover} alt="" className="aspect-[16/9] w-16 shrink-0 rounded-md" />
                              <div className="min-w-0 flex-1">
                                <p className="truncate text-sm">{match.title}</p>
                                <p className="truncate text-[12px] text-faint">{match.creator}</p>
                              </div>
                              {!match.confident && <Badge tone="warning">Check this</Badge>}
                            </>
                          ) : (
                            <span className="flex-1 text-[13px] text-faint">No match found</span>
                          )}
                          {!linked && (
                            <Button size="sm" variant="ghost" onClick={() => setPicking(picking === f.path ? null : f.path)}>
                              {match ? "Change" : "Find"}
                            </Button>
                          )}
                        </div>
                      </div>
                      {picking === f.path && (
                        <MatchPicker
                          initial={f.title_guess}
                          onCancel={() => setPicking(null)}
                          onPick={(m) => {
                            update(f.path, { match: m, selected: true });
                            setPicking(null);
                          }}
                        />
                      )}
                    </li>
                  );
                })}
              </ul>
              <div className="mt-5 flex items-center justify-between gap-3">
                <p className="text-[13px] text-faint">Added games are also tracked for updates.</p>
                <div className="flex gap-2">
                  <Button variant="ghost" onClick={() => onOpenChange(false)}>
                    Cancel
                  </Button>
                  <Button variant="primary" disabled={!selected.length} loading={importing} onClick={importSelected}>
                    <Check /> Add {selected.length || ""} game{selected.length === 1 ? "" : "s"}
                  </Button>
                </div>
              </div>
            </>
          ))}
      </DialogContent>
    </Dialog>
  );
}
