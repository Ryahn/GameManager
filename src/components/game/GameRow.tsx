import { Link } from "@tanstack/react-router";
import { ChevronRight } from "lucide-react";
import { ErrorPanel } from "@/components/ui/primitives";
import { useLibraryIds, useLibraryToggle, useLocalMap, usePlay, useRunning } from "@/hooks/queries";
import { GameCard, GameCardSkeleton, GameGrid, type GameCardData } from "./GameCard";
import { withLocal } from "./mappers";

/** A titled block of game cards with loading, error and empty handling. */
export function GameRow({
  title,
  subtitle,
  games,
  isLoading,
  error,
  onRetry,
  seeAllTo,
  skeletons = 6,
  min,
}: {
  title: string;
  subtitle?: string;
  games: GameCardData[] | undefined;
  isLoading?: boolean;
  error?: unknown;
  onRetry?: () => void;
  seeAllTo?: "/discover" | "/library" | "/updates";
  skeletons?: number;
  min?: number;
}) {
  const library = useLibraryIds();
  const toggle = useLibraryToggle();
  const local = useLocalMap();
  const running = useRunning();
  const play = usePlay();

  return (
    <section className="mb-10">
      <div className="mb-4 flex items-end justify-between gap-4">
        <div>
          <h2 className="text-[17px] font-semibold tracking-tight">{title}</h2>
          {subtitle && <p className="mt-0.5 text-sm text-muted">{subtitle}</p>}
        </div>
        {seeAllTo && (
          <Link to={seeAllTo} className="inline-flex items-center gap-0.5 text-sm font-medium text-muted hover:text-text">
            See all <ChevronRight className="size-4" />
          </Link>
        )}
      </div>
      {error ? (
        <ErrorPanel error={error} onRetry={onRetry} />
      ) : (
        <GameGrid min={min}>
          {isLoading || !games
            ? Array.from({ length: skeletons }, (_, i) => <GameCardSkeleton key={i} />)
            : games.map((g) => (
                <GameCard
                  key={g.id}
                  game={withLocal(g, local.get(g.id))}
                  inLibrary={library.has(g.id)}
                  onToggleLibrary={(add) => toggle(g, add)}
                  onPlay={() => play.mutate({ id: g.id, title: g.title })}
                  running={running.data?.has(g.id)}
                />
              ))}
        </GameGrid>
      )}
    </section>
  );
}
