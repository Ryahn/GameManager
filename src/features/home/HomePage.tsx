import { GameRow } from "@/components/game/GameRow";
import { fromBookmark, fromHandiwork } from "@/components/game/mappers";
import { useBookmarks, useLatest, useSettings } from "@/hooks/queries";

function greeting() {
  const h = new Date().getHours();
  if (h < 5) return "Up late";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

export function HomePage() {
  const settings = useSettings();
  const updates = useBookmarks(true);
  const latest = useLatest({ order: "date", limit: 18 });
  const topRated = useLatest({ order: "rating", date: 30, limit: 12 });
  const name = settings.data?.username;
  const updateGames = updates.data?.items.slice(0, 6).map(fromBookmark);

  return (
    <div className="animate-fade-up">
      <header className="mb-8">
        <h1 className="font-[family-name:var(--font-display)] text-[28px] font-semibold tracking-tight">
          {greeting()}
          {name ? `, ${name}` : ""}
        </h1>
        <p className="mt-1 text-sm text-muted">
          {updates.data?.total
            ? `${updates.data.total} game${updates.data.total === 1 ? "" : "s"} in your library ${updates.data.total === 1 ? "has" : "have"} an update.`
            : "Here's what's new in the catalog."}
        </p>
      </header>

      {!!updateGames?.length && <GameRow title="Updates for you" games={updateGames} seeAllTo="/updates" />}

      <GameRow
        title="Latest updates"
        subtitle="Freshly released versions across the catalog"
        games={latest.data?.map(fromHandiwork)}
        isLoading={latest.isLoading}
        error={latest.error}
        onRetry={() => latest.refetch()}
        seeAllTo="/discover"
        skeletons={12}
      />

      <GameRow
        title="Top rated this month"
        games={topRated.data?.map(fromHandiwork)}
        isLoading={topRated.isLoading}
        error={topRated.error}
        onRetry={() => topRated.refetch()}
        seeAllTo="/discover"
      />
    </div>
  );
}
