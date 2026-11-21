import { useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import { Columns3, Compass, FolderSearch, LayoutGrid, Library, List, Play, Search, Trash2, WifiOff, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge, EmptyState, ErrorPanel, Input, PageHeader, Segmented, Select } from "@/components/ui/primitives";
import { Cover, GameCard, GameCardSkeleton, GameGrid, type GameCardData } from "@/components/game/GameCard";
import { fromBookmark, fromLocal, withLocal } from "@/components/game/mappers";
import { keys, useLibrary, useLibraryToggle, useLocalLibrary, useOpenItem, useRunning } from "@/hooks/queries";
import { useUi } from "@/state/ui";
import { api, type LocalGame, type PlayStatus } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { PLAY_STATUSES, formatPlaytime, statusMeta } from "@/lib/play";
import { cleanVersion, cn, timeAgo } from "@/lib/utils";
import { KINDS, countLabel, type Kind } from "@/lib/kinds";
import { ScanDialog } from "./ScanDialog";

type Show = "all" | "updates" | "installed";
type Sort = "recent" | "played" | "title" | "playtime" | "rating";
type StatusFilter = "any" | PlayStatus;

interface Entry {
  card: GameCardData;
  local?: LocalGame;
}

const SORTS: { value: Sort; label: string }[] = [
  { value: "recent", label: "Recently updated" },
  { value: "played", label: "Recently played" },
  { value: "title", label: "Title" },
  { value: "playtime", label: "Most played" },
  { value: "rating", label: "My rating" },
];

function BoardColumn({
  status,
  entries,
  onDrop,
  onPlay,
  running,
}: {
  status: (typeof PLAY_STATUSES)[number];
  entries: Entry[];
  onDrop: (id: number) => void;
  onPlay: (e: Entry) => void;
  running: Set<number> | undefined;
}) {
  const [over, setOver] = useState(false);
  return (
    <section
      aria-label={status.label}
      onDragOver={(e) => {
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        const id = Number(e.dataTransfer.getData("text/plain"));
        if (id) onDrop(id);
      }}
      className={cn(
        "flex min-h-[320px] min-w-[180px] flex-1 flex-col rounded-[var(--radius-card)] border bg-surface-1/60 transition-colors",
        over ? "border-accent bg-accent/5" : "border-border",
      )}
    >
      <header className="flex items-center gap-2 px-3 py-2.5">
        <span className={cn("size-2 rounded-full", status.dot)} aria-hidden />
        <h3 className="text-[13px] font-semibold">{status.label}</h3>
        <span className="ml-auto text-xs text-faint">{entries.length}</span>
      </header>
      <ul className="flex flex-1 flex-col gap-2 overflow-y-auto px-2 pb-2">
        {entries.map((e) => (
          <li
            key={e.card.id}
            draggable
            onDragStart={(ev) => {
              ev.dataTransfer.setData("text/plain", String(e.card.id));
              ev.dataTransfer.effectAllowed = "move";
            }}
            className="group relative flex items-center gap-2.5 rounded-lg border border-border bg-surface-2 p-2 hover:border-border-strong"
          >
            <Cover src={e.card.cover} alt="" className="aspect-[16/9] w-16 shrink-0 rounded" />
            <Link to="/game/$id" params={{ id: String(e.card.id) }} className="min-w-0 flex-1" draggable={false}>
              <p className="line-clamp-2 text-[13px] font-medium leading-snug">{e.card.title}</p>
              {e.card.hasUpdate && <Badge tone="accent" className="mt-1">Update</Badge>}
            </Link>
            {e.card.installed &&
              (running?.has(e.card.id) ? (
                <span className="size-2 shrink-0 animate-pulse rounded-full bg-success" title="Playing" />
              ) : (
                <button
                  type="button"
                  onClick={() => onPlay(e)}
                  aria-label={`Play ${e.card.title}`}
                  className="grid size-7 shrink-0 place-items-center rounded-full bg-accent text-accent-fg opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
                >
                  <Play className="size-3 fill-current" />
                </button>
              ))}
          </li>
        ))}
        {entries.length === 0 && <li className="px-2 py-6 text-center text-xs text-faint">Drag games here</li>}
      </ul>
    </section>
  );
}

export function LibraryPage() {
  const library = useLibrary();
  const localQ = useLocalLibrary();
  const toggle = useLibraryToggle();
  const running = useRunning();
  const qc = useQueryClient();
  const view = useUi((s) => s.libraryView);
  const setView = useUi((s) => s.setLibraryView);
  const cardSize = useUi((s) => s.cardSize);
  const setCardSize = useUi((s) => s.setCardSize);
  const [text, setText] = useState("");
  const [show, setShow] = useState<Show>("all");
  const [kind, setKind] = useState<Kind>("games");
  const [status, setStatus] = useState<StatusFilter>("any");
  const [label, setLabel] = useState<string | null>(null);
  const [sort, setSort] = useState<Sort>("recent");
  const [scanOpen, setScanOpen] = useState(false);

  const localMap = useMemo(() => new Map(localQ.data?.map((g) => [g.thread_id, g]) ?? []), [localQ.data]);
  // Can't reach the server: fall back to the copy saved on this computer.
  const offline = !!library.error && !!localQ.data?.some((g) => g.in_library);

  const everything: Entry[] = useMemo(() => {
    if (library.data) {
      const tracked = new Set(library.data.items.map((b) => b.thread_id));
      const fromServer = library.data.items.map((b) => {
        const local = localMap.get(b.thread_id);
        return { card: withLocal(fromBookmark(b), local), local };
      });
      // Installed on this computer but not in the server library: still listed,
      // so nothing on disk is ever "missing" from the library.
      const installedOnly = (localQ.data ?? [])
        .filter((l) => l.targets.length > 0 && !tracked.has(l.thread_id))
        .map((l) => ({ card: { ...fromLocal(l), untracked: true }, local: l }));
      return [...fromServer, ...installedOnly];
    }
    if (offline) return (localQ.data ?? []).filter((g) => g.in_library).map((l) => ({ card: fromLocal(l), local: l }));
    return [];
  }, [library.data, localMap, offline, localQ.data]);
  const kindCounts = useMemo(() => {
    const c: Record<Kind, number> = { games: 0, comics: 0, animations: 0 };
    for (const e of everything) c[e.card.category ?? "games"]++;
    return c;
  }, [everything]);
  const all = useMemo(() => everything.filter((e) => (e.card.category ?? "games") === kind), [everything, kind]);

  const labels = useMemo(() => [...new Set(all.flatMap((e) => e.local?.labels ?? []))].sort((a, b) => a.localeCompare(b)), [all]);
  const counts = {
    updates: all.filter((e) => e.card.hasUpdate).length,
    installed: all.filter((e) => e.card.installed).length,
  };

  const entries = useMemo(() => {
    let list = all;
    const t = text.trim().toLowerCase();
    if (t) list = list.filter((e) => e.card.title.toLowerCase().includes(t) || e.card.creator?.toLowerCase().includes(t));
    if (show === "updates") list = list.filter((e) => e.card.hasUpdate);
    if (show === "installed") list = list.filter((e) => e.card.installed);
    if (status !== "any") list = list.filter((e) => (e.local?.play_status ?? "not_started") === status);
    if (label) list = list.filter((e) => e.local?.labels.includes(label));
    const by: Record<Sort, (a: Entry, b: Entry) => number> = {
      recent: (a, b) => (b.card.updatedAt ?? 0) - (a.card.updatedAt ?? 0),
      played: (a, b) => (b.local?.last_played_at ?? 0) - (a.local?.last_played_at ?? 0),
      title: (a, b) => a.card.title.localeCompare(b.card.title),
      playtime: (a, b) => (b.local?.playtime_secs ?? 0) - (a.local?.playtime_secs ?? 0),
      rating: (a, b) => (b.local?.rating ?? 0) - (a.local?.rating ?? 0),
    };
    return [...list].sort(by[sort]);
  }, [all, text, show, status, label, sort]);

  const open = useOpenItem();
  const onPlay = (e: Entry) => open(e.card);

  const moveTo = async (id: number, to: PlayStatus) => {
    try {
      await api.updateLocal(id, { play_status: to });
      qc.invalidateQueries({ queryKey: keys.local });
    } catch (err) {
      toast.error("Couldn't move the game", { description: friendlyError(err).detail });
    }
  };

  const total = all.length;
  const loading = !library.data && !offline && !library.error;
  const scan = <ScanDialog open={scanOpen} onOpenChange={setScanOpen} kind={kind} />;

  if (library.error && !offline) return <ErrorPanel error={library.error} onRetry={() => library.refetch()} />;

  if (library.data && everything.length === 0) {
    return (
      <>
        <EmptyState
          icon={Library}
          title="Your library is empty"
          action={
            <div className="flex flex-wrap justify-center gap-2">
              <Button variant="primary" onClick={() => setScanOpen(true)}>
                <FolderSearch /> Find games on this computer
              </Button>
              <Link to="/discover">
                <Button variant="outline">
                  <Compass /> Browse Discover
                </Button>
              </Link>
            </div>
          }
        >
          Already have games installed? Scan a folder and they're added in one go. Or add games from Discover. Everything you add is tracked for
          updates, including stealth updates.
        </EmptyState>
        {scan}
      </>
    );
  }

  return (
    <div className="animate-fade-up">
      <PageHeader
        title="Library"
        subtitle={
          loading
            ? "Loading…"
            : `${countLabel(total, kind)}${counts.installed ? ` · ${counts.installed} installed` : ""}${
                counts.updates ? ` · ${counts.updates} with updates` : ""
              }`
        }
        actions={
          <Button variant="outline" onClick={() => setScanOpen(true)}>
            <FolderSearch /> Find {KINDS.find((k) => k.value === kind)!.many} on this computer
          </Button>
        }
      />

      <div className="mb-5">
        <Segmented
          label="Type"
          value={kind}
          onChange={(k) => {
            setKind(k);
            setLabel(null);
          }}
          options={KINDS.map((k) => ({ value: k.value, label: `${k.label}${kindCounts[k.value] ? ` (${kindCounts[k.value]})` : ""}` }))}
        />
      </div>

      {offline && (
        <div role="status" className="mb-5 flex items-center gap-3 rounded-xl border border-warning/30 bg-warning/8 px-4 py-3 text-sm">
          <WifiOff className="size-4 shrink-0 text-warning" />
          <span className="flex-1">
            <span className="font-medium">Offline.</span> <span className="text-muted">Showing the copy saved on this computer. You can still play installed games.</span>
          </span>
          <Button size="sm" variant="ghost" onClick={() => library.refetch()} loading={library.isFetching}>
            Try again
          </Button>
        </div>
      )}

      <div className="mb-4 flex flex-wrap items-center gap-3">
        <div className="relative min-w-[200px] max-w-xs flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-faint" />
          <Input aria-label="Filter library" placeholder="Filter by title or creator" value={text} onChange={(e) => setText(e.target.value)} className="pl-9" />
        </div>
        <Segmented
          label="Show"
          value={show}
          onChange={setShow}
          options={[
            { value: "all", label: "All" },
            { value: "installed", label: `Installed${counts.installed ? ` (${counts.installed})` : ""}` },
            { value: "updates", label: `Updates${counts.updates ? ` (${counts.updates})` : ""}` },
          ]}
        />
        {view !== "board" && (
          <Select
            label="Play status"
            value={status}
            onChange={setStatus}
            options={[{ value: "any", label: "Any status" }, ...PLAY_STATUSES.map((s) => ({ value: s.value, label: s.label }))]}
          />
        )}
        <Select label="Sort" value={sort} onChange={setSort} options={SORTS} />
        <div className="ml-auto flex items-center gap-3">
          {view === "grid" && (
            <input
              type="range"
              min={180}
              max={340}
              step={20}
              value={cardSize}
              onChange={(e) => setCardSize(Number(e.target.value))}
              aria-label="Card size"
              className="w-24 accent-[var(--accent)]"
            />
          )}
          <Segmented
            label="View"
            value={view}
            onChange={setView}
            options={[
              { value: "grid", label: <LayoutGrid />, title: "Grid" },
              { value: "list", label: <List />, title: "List" },
              { value: "board", label: <Columns3 />, title: "Board by play status" },
            ]}
          />
        </div>
      </div>

      {labels.length > 0 && (
        <div className="mb-5 flex flex-wrap items-center gap-1.5" role="group" aria-label="Labels">
          {labels.map((l) => (
            <button
              key={l}
              type="button"
              aria-pressed={label === l}
              onClick={() => setLabel(label === l ? null : l)}
              className={cn(
                "inline-flex h-7 items-center gap-1 rounded-full border px-3 text-[12.5px] font-medium transition-colors",
                label === l ? "border-accent bg-accent/15 text-accent" : "border-border text-muted hover:border-border-strong hover:text-text",
              )}
            >
              {l}
              {label === l && <X className="size-3" />}
            </button>
          ))}
        </div>
      )}

      {loading ? (
        <GameGrid min={cardSize}>
          {Array.from({ length: 12 }, (_, i) => (
            <GameCardSkeleton key={i} />
          ))}
        </GameGrid>
      ) : view === "board" ? (
        <div className="-mx-1 flex gap-3 overflow-x-auto px-1 pb-3">
          {PLAY_STATUSES.map((s) => (
            <BoardColumn
              key={s.value}
              status={s}
              entries={entries.filter((e) => (e.local?.play_status ?? "not_started") === s.value)}
              onDrop={(id) => moveTo(id, s.value)}
              onPlay={onPlay}
              running={running.data}
            />
          ))}
        </div>
      ) : entries.length === 0 ? (
        <p className="py-16 text-center text-sm text-muted">No games match these filters.</p>
      ) : view === "grid" ? (
        <GameGrid min={cardSize}>
          {entries.map((e) => (
            <GameCard
              key={e.card.id}
              game={e.card}
              inLibrary={!e.card.untracked}
              onToggleLibrary={offline ? undefined : (add) => toggle(e.card, add)}
              onPlay={() => onPlay(e)}
              running={running.data?.has(e.card.id)}
            />
          ))}
        </GameGrid>
      ) : (
        <div className="overflow-hidden rounded-[var(--radius-card)] border border-border bg-surface-1">
          <div className="flex items-center gap-4 border-b border-border px-4 py-2 text-[11px] font-semibold uppercase tracking-wider text-faint">
            <span className="w-24 shrink-0" />
            <span className="flex-1">Game</span>
            <span className="w-28">Status</span>
            <span className="w-24 text-right">Played</span>
            <span className="w-32 text-right">Version</span>
            <span className="w-20 text-right">Updated</span>
            <span className="w-[76px]" />
          </div>
          {entries.map((e, i) => {
            const g = e.card;
            const st = statusMeta(e.local?.play_status ?? "not_started");
            const installed = cleanVersion(e.local?.installed_version);
            const latest = cleanVersion(g.version);
            return (
              <div key={g.id} className={cn("group flex items-center gap-4 px-4 py-2.5 transition-colors hover:bg-surface-2", i && "border-t border-border")}>
                <Cover src={g.cover} alt="" className="aspect-[16/9] w-24 shrink-0 rounded-md" />
                <Link to="/game/$id" params={{ id: String(g.id) }} className="min-w-0 flex-1">
                  <p className="flex items-center gap-2 truncate font-medium">
                    <span className="truncate">{g.title}</span>
                    {g.hasUpdate && <Badge tone="accent">Update</Badge>}
                    {g.untracked && <Badge title="Installed on this computer, but not tracked for updates">Not tracked</Badge>}
                  </p>
                  <p className="truncate text-sm text-muted">{g.creator}</p>
                </Link>
                <span className="flex w-28 items-center gap-1.5 text-[13px] text-muted">
                  <span className={cn("size-1.5 rounded-full", st.dot)} aria-hidden />
                  {st.label}
                </span>
                <span className="w-24 text-right text-[13px] text-muted">{e.local?.playtime_secs ? formatPlaytime(e.local.playtime_secs) : "—"}</span>
                <span className="w-32 truncate text-right text-[13px] text-muted" title={installed ? `Installed v${installed} · Latest v${latest}` : undefined}>
                  {installed && installed !== latest ? (
                    <>
                      <span className="text-faint">v{installed} → </span>v{latest}
                    </>
                  ) : latest ? (
                    `v${latest}`
                  ) : (
                    ""
                  )}
                </span>
                <span className="w-20 text-right text-xs text-faint">{timeAgo(g.updatedAt)}</span>
                {!offline && (
                  <Button
                    size="icon-sm"
                    variant="ghost"
                    title={g.untracked ? "Remove from this computer's list…" : "Remove from library…"}
                    aria-label={`Remove ${g.title} from library`}
                    className="opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100"
                    onClick={() => toggle(g, false)}
                  >
                    <Trash2 />
                  </Button>
                )}
                <span className="grid w-9 place-items-center">
                  {g.installed &&
                    (running.data?.has(g.id) ? (
                      <span className="size-2 animate-pulse rounded-full bg-success" title="Playing" />
                    ) : (
                      <Button size="icon-sm" variant="ghost" aria-label={`Play ${g.title}`} onClick={() => onPlay(e)}>
                        <Play className="fill-current" />
                      </Button>
                    ))}
                </span>
              </div>
            );
          })}
        </div>
      )}
      {scan}
    </div>
  );
}
