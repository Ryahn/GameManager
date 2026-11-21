import { useEffect, useState } from "react";
import { ChevronLeft, ChevronRight, Search, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input, PageHeader, Segmented } from "@/components/ui/primitives";
import { GameRow } from "@/components/game/GameRow";
import { fromHandiwork, fromHit } from "@/components/game/mappers";
import { useLatest, useSearch } from "@/hooks/queries";
import type { LatestQuery } from "@/lib/ipc";
import { KINDS, kindMeta, kindOf, type Kind } from "@/lib/kinds";

type Order = NonNullable<LatestQuery["order"]>;
type Window = "any" | "7" | "30" | "90" | "365";

const ORDERS: { value: Order; label: string }[] = [
  { value: "date", label: "Latest" },
  { value: "rating", label: "Top rated" },
  { value: "likes", label: "Most liked" },
  { value: "views", label: "Most viewed" },
];

const WINDOWS: { value: Window; label: string }[] = [
  { value: "any", label: "All time" },
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
  { value: "365", label: "Year" },
];

const PAGE_SIZE = 30;

export function DiscoverPage() {
  const [kind, setKind] = useState<Kind>("games");
  const [order, setOrder] = useState<Order>("date");
  const [win, setWin] = useState<Window>("any");
  const [page, setPage] = useState(1);
  const [text, setText] = useState("");
  const [q, setQ] = useState("");

  useEffect(() => {
    const t = setTimeout(() => setQ(text.trim()), 300);
    return () => clearTimeout(t);
  }, [text]);
  useEffect(() => setPage(1), [order, win, kind]);

  const searching = q.length >= 2;
  const query: LatestQuery = {
    category: kind,
    order,
    date: win === "any" ? undefined : (Number(win) as LatestQuery["date"]),
    page,
    limit: PAGE_SIZE,
  };
  const latest = useLatest(query, !searching);
  const search = useSearch(q);

  return (
    <div className="animate-fade-up">
      <PageHeader
        title="Discover"
        subtitle={`Browse the whole catalog. Click + on any ${kindMeta(kind).one} to add it to your library.`}
        actions={<Segmented label="Type" value={kind} onChange={setKind} options={KINDS.map((k) => ({ value: k.value, label: k.label }))} />}
      />

      <div className="sticky top-0 z-10 -mx-2 mb-6 flex flex-wrap items-center gap-3 rounded-xl bg-bg/85 px-2 py-2 backdrop-blur">
        <div className="relative w-full max-w-sm">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-faint" />
          <Input
            aria-label="Search the catalog"
            placeholder="Search titles, creators, DLsite or Steam IDs…"
            value={text}
            onChange={(e) => setText(e.target.value)}
            className="pl-9 pr-9"
          />
          {text && (
            <button
              type="button"
              aria-label="Clear search"
              onClick={() => setText("")}
              className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-faint hover:text-text"
            >
              <X className="size-4" />
            </button>
          )}
        </div>
        {!searching && (
          <>
            <Segmented label="Sort" value={order} onChange={setOrder} options={ORDERS} />
            <Segmented label="Time window" value={win} onChange={setWin} options={WINDOWS} />
          </>
        )}
      </div>

      {searching ? (
        <GameRow
          title={`Results for "${q}"`}
          games={search.data?.hits.filter((h) => kindOf(h.category) === kind).map(fromHit)}
          isLoading={search.isLoading}
          error={search.error}
          onRetry={() => search.refetch()}
          skeletons={12}
        />
      ) : (
        <>
          <GameRow
            title={ORDERS.find((o) => o.value === order)!.label}
            subtitle={`Page ${page}`}
            games={latest.data?.map(fromHandiwork)}
            isLoading={latest.isLoading || latest.isPlaceholderData}
            error={latest.error}
            onRetry={() => latest.refetch()}
            skeletons={PAGE_SIZE}
          />
          <div className="flex items-center justify-center gap-2 pb-4">
            <Button variant="outline" disabled={page <= 1} onClick={() => setPage((p) => p - 1)}>
              <ChevronLeft /> Previous
            </Button>
            <span className="min-w-16 text-center text-sm text-muted">Page {page}</span>
            <Button variant="outline" disabled={(latest.data?.length ?? 0) < PAGE_SIZE} onClick={() => setPage((p) => p + 1)}>
              Next <ChevronRight />
            </Button>
          </div>
        </>
      )}
    </div>
  );
}
