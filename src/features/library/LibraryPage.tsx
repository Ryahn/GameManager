import { useMemo, useState } from "react";
import { Link } from "@tanstack/react-router";
import { Compass, LayoutGrid, Library, List, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge, EmptyState, ErrorPanel, Input, PageHeader, Segmented } from "@/components/ui/primitives";
import { Cover, GameCard, GameCardSkeleton, GameGrid } from "@/components/game/GameCard";
import { fromBookmark } from "@/components/game/mappers";
import { useBookmarks, useToggleLibrary } from "@/hooks/queries";
import { useUi } from "@/state/ui";
import { cleanVersion, timeAgo } from "@/lib/utils";

type Filter = "all" | "updates";
type Sort = "recent" | "title";

export function LibraryPage() {
  const bookmarks = useBookmarks(false);
  const toggle = useToggleLibrary();
  const view = useUi((s) => s.libraryView);
  const setView = useUi((s) => s.setLibraryView);
  const cardSize = useUi((s) => s.cardSize);
  const setCardSize = useUi((s) => s.setCardSize);
  const [text, setText] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [sort, setSort] = useState<Sort>("recent");

  const games = useMemo(() => {
    let list = (bookmarks.data?.items ?? []).map(fromBookmark);
    const t = text.trim().toLowerCase();
    if (t) list = list.filter((g) => g.title.toLowerCase().includes(t) || g.creator?.toLowerCase().includes(t));
    if (filter === "updates") list = list.filter((g) => g.hasUpdate);
    list.sort(sort === "title" ? (a, b) => a.title.localeCompare(b.title) : (a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0));
    return list;
  }, [bookmarks.data, text, filter, sort]);

  const total = bookmarks.data?.total ?? 0;
  const updates = bookmarks.data?.items.filter((b) => b.has_update).length ?? 0;

  if (bookmarks.error) return <ErrorPanel error={bookmarks.error} onRetry={() => bookmarks.refetch()} />;

  if (bookmarks.data && total === 0) {
    return (
      <EmptyState
        icon={Library}
        title="Your library is empty"
        action={
          <Link to="/discover">
            <Button variant="primary">
              <Compass /> Browse Discover
            </Button>
          </Link>
        }
      >
        Add games from Discover or search with Ctrl K. Games you add are tracked for updates automatically, including stealth updates.
      </EmptyState>
    );
  }

  return (
    <div className="animate-fade-up">
      <PageHeader
        title="Library"
        subtitle={bookmarks.data ? `${total} game${total === 1 ? "" : "s"}${updates ? ` · ${updates} with updates` : ""}` : "Loading…"}
      />

      <div className="mb-6 flex flex-wrap items-center gap-3">
        <div className="relative w-full max-w-xs">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-faint" />
          <Input aria-label="Filter library" placeholder="Filter by title or creator" value={text} onChange={(e) => setText(e.target.value)} className="pl-9" />
        </div>
        <Segmented
          label="Show"
          value={filter}
          onChange={setFilter}
          options={[
            { value: "all", label: "All" },
            { value: "updates", label: `Has update${updates ? ` (${updates})` : ""}` },
          ]}
        />
        <Segmented
          label="Sort"
          value={sort}
          onChange={setSort}
          options={[
            { value: "recent", label: "Recently updated" },
            { value: "title", label: "Title" },
          ]}
        />
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
              className="w-28 accent-[var(--accent)]"
            />
          )}
          <Segmented
            label="View"
            value={view}
            onChange={setView}
            options={[
              { value: "grid", label: <LayoutGrid />, title: "Grid" },
              { value: "list", label: <List />, title: "List" },
            ]}
          />
        </div>
      </div>

      {!bookmarks.data ? (
        <GameGrid min={cardSize}>
          {Array.from({ length: 12 }, (_, i) => (
            <GameCardSkeleton key={i} />
          ))}
        </GameGrid>
      ) : games.length === 0 ? (
        <p className="py-16 text-center text-sm text-muted">No games match these filters.</p>
      ) : view === "grid" ? (
        <GameGrid min={cardSize}>
          {games.map((g) => (
            <GameCard key={g.id} game={g} inLibrary onToggleLibrary={(add) => toggle.mutate({ id: g.id, add, title: g.title })} />
          ))}
        </GameGrid>
      ) : (
        <div className="overflow-hidden rounded-[var(--radius-card)] border border-border bg-surface-1">
          {games.map((g, i) => (
            <Link
              key={g.id}
              to="/game/$id"
              params={{ id: String(g.id) }}
              className={`flex items-center gap-4 px-4 py-2.5 transition-colors hover:bg-surface-2 ${i ? "border-t border-border" : ""}`}
            >
              <Cover src={g.cover} alt="" className="aspect-[16/9] w-24 shrink-0 rounded-md" />
              <div className="min-w-0 flex-1">
                <p className="truncate font-medium">{g.title}</p>
                <p className="truncate text-sm text-muted">{g.creator}</p>
              </div>
              {g.hasUpdate && <Badge tone="accent">Update</Badge>}
              <span className="w-24 truncate text-right text-sm text-muted">{g.version ? `v${cleanVersion(g.version)}` : ""}</span>
              <span className="w-20 text-right text-xs text-faint">{timeAgo(g.updatedAt)}</span>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
