import { useMemo } from "react";
import { GameRow } from "@/components/game/GameRow";
import { fromBookmark, fromContinue, fromHandiwork, fromLocal, fromWatch } from "@/components/game/mappers";
import { useContinueReading, useContinueWatching, useLatest, useLocalLibrary, useSettings, useUpdateInbox } from "@/hooks/queries";

function greeting() {
  const h = new Date().getHours();
  if (h < 5) return "Up late";
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

export function HomePage() {
  const settings = useSettings();
  const inbox = useUpdateInbox();
  const local = useLocalLibrary();
  const reading = useContinueReading();
  const watching = useContinueWatching();
  const latest = useLatest({ order: "date", limit: 18 });
  const topRated = useLatest({ order: "rating", date: 30, limit: 12 });
  const name = settings.data?.username;
  const updateGames = inbox.query.data ? inbox.items.slice(0, 6).map(fromBookmark) : undefined;
  const updateCount = inbox.items.length;
  const continuePlaying = useMemo(
    () =>
      (local.data ?? [])
        .filter((g) => g.last_played_at && g.targets.length && g.play_status !== "finished" && g.play_status !== "dropped")
        .sort((a, b) => (b.last_played_at ?? 0) - (a.last_played_at ?? 0))
        .slice(0, 6)
        .map(fromLocal),
    [local.data],
  );

  return (
    <div className="animate-fade-up">
      <header className="mb-8">
        <h1 className="font-[family-name:var(--font-display)] text-[28px] font-semibold tracking-tight">
          {greeting()}
          {name ? `, ${name}` : ""}
        </h1>
        <p className="mt-1 text-sm text-muted">
          {updateCount
            ? `${updateCount} game${updateCount === 1 ? "" : "s"} in your library ${updateCount === 1 ? "has" : "have"} an update.`
            : "Here's what's new in the catalog."}
        </p>
      </header>

      {continuePlaying.length > 0 && <GameRow title="Continue playing" games={continuePlaying} seeAllTo="/library" />}

      {!!reading.data?.length && <GameRow title="Continue reading" games={reading.data.slice(0, 6).map(fromContinue)} seeAllTo="/library" />}

      {!!watching.data?.length && <GameRow title="Continue watching" games={watching.data.slice(0, 6).map(fromWatch)} seeAllTo="/library" />}

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
