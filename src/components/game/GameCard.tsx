import { memo, useState } from "react";
import { Link } from "@tanstack/react-router";
import { Check, ImageOff, Play, Plus, X } from "lucide-react";
import { Badge } from "@/components/ui/primitives";
import { cn, cleanVersion, imageSrc, statusTone, timeAgo } from "@/lib/utils";
import type { PlayStatus } from "@/lib/ipc";
import { statusMeta } from "@/lib/play";

export interface GameCardData {
  id: number;
  title: string;
  creator?: string;
  version?: string | null;
  cover?: string | null;
  engine?: string | null;
  prefixes?: string[];
  updatedAt?: number | null;
  isNew?: boolean;
  hasUpdate?: boolean;
  rating?: number | null;
  /** Local state, when known. */
  installed?: boolean;
  /** Installed on this PC but not in the server library (no update checks). */
  untracked?: boolean;
  playStatus?: PlayStatus;
}

const TONE_TEXT = { success: "text-success", warning: "text-warning", danger: "text-danger", info: "text-info" } as const;

export function Cover({ src, alt, className }: { src?: string | null; alt: string; className?: string }) {
  const [state, setState] = useState<"loading" | "ok" | "error">(src ? "loading" : "error");
  return (
    <div data-cover-wrap className={cn("relative overflow-hidden bg-surface-2", className)}>
      {state === "loading" && <div className="skeleton absolute inset-0" aria-hidden />}
      {state === "error" ? (
        <div className="absolute inset-0 grid place-items-center text-faint">
          <ImageOff className="size-6" aria-hidden />
        </div>
      ) : (
        <img
          data-cover
          src={imageSrc(src)}
          alt={alt}
          loading="lazy"
          decoding="async"
          draggable={false}
          onLoad={() => setState("ok")}
          onError={() => setState("error")}
          className={cn("absolute inset-0 size-full object-cover transition-opacity duration-300", state === "ok" ? "opacity-100" : "opacity-0")}
        />
      )}
    </div>
  );
}

export const GameCard = memo(function GameCard({
  game,
  inLibrary,
  onToggleLibrary,
  onPlay,
  running,
}: {
  game: GameCardData;
  inLibrary?: boolean;
  onToggleLibrary?: (add: boolean) => void;
  onPlay?: () => void;
  running?: boolean;
}) {
  const status = statusTone(game.prefixes ?? []);
  const play = game.playStatus && game.playStatus !== "not_started" ? statusMeta(game.playStatus) : null;
  const version = cleanVersion(game.version);
  return (
    <article className="group relative flex flex-col overflow-hidden rounded-[var(--radius-card)] border border-border bg-surface-1 transition-[transform,border-color,box-shadow] duration-200 hover:-translate-y-0.5 hover:border-border-strong hover:shadow-card">
      <Link to="/game/$id" params={{ id: String(game.id) }} className="absolute inset-0 z-[1]" aria-label={game.title} />
      <div className="relative">
        <Cover src={game.cover} alt="" className="aspect-[16/9]" />
        <div className="absolute left-2 top-2 flex flex-wrap gap-1">
          {game.hasUpdate && <Badge tone="accent" className="bg-accent text-accent-fg">Update</Badge>}
          {game.isNew && <Badge tone="solid">New</Badge>}
          {game.untracked && (
            <Badge tone="solid" title="Installed on this PC, but not tracked for updates. Add it to your library to track it.">
              Not tracked
            </Badge>
          )}
          {status && <Badge tone="solid" className={TONE_TEXT[status.tone]}>{status.label}</Badge>}
        </div>
        {onToggleLibrary && (
          <button
            type="button"
            onClick={() => onToggleLibrary(!inLibrary)}
            aria-label={inLibrary ? `Remove ${game.title} from library` : `Add ${game.title} to library`}
            title={inLibrary ? "Remove from library…" : "Add to library"}
            className={cn(
              "group/lib absolute right-2 top-2 z-[2] grid size-8 place-items-center rounded-full border backdrop-blur-sm transition-all",
              inLibrary
                ? "border-accent/60 bg-accent text-accent-fg hover:border-danger hover:bg-danger hover:text-white"
                : "border-white/15 bg-black/55 text-white opacity-0 group-hover:opacity-100 focus-visible:opacity-100",
            )}
          >
            {inLibrary ? (
              <>
                <Check className="size-4 group-hover/lib:hidden" />
                <X className="hidden size-4 group-hover/lib:block" />
              </>
            ) : (
              <Plus className="size-4" />
            )}
          </button>
        )}
        {running ? (
          <span className="absolute bottom-2 right-2 z-[2] inline-flex h-8 items-center gap-1.5 rounded-full bg-success px-3 text-[12px] font-semibold text-black shadow-card">
            <span className="size-1.5 animate-pulse rounded-full bg-black/70" /> Playing
          </span>
        ) : (
          game.installed &&
          onPlay && (
            <button
              type="button"
              onClick={onPlay}
              aria-label={`Play ${game.title}`}
              title="Play"
              className="absolute bottom-2 right-2 z-[2] grid size-10 place-items-center rounded-full bg-accent text-accent-fg opacity-0 shadow-card transition-all hover:scale-105 hover:bg-accent-hover group-hover:opacity-100 focus-visible:opacity-100"
            >
              <Play className="size-4 fill-current" />
            </button>
          )
        )}
      </div>
      <div className="flex flex-1 flex-col gap-1 p-3">
        <h3 className="line-clamp-2 text-[14px] font-semibold leading-snug" title={game.title}>
          {game.title}
        </h3>
        {game.creator && <p className="truncate text-[12.5px] text-muted">{game.creator}</p>}
        {play && (
          <p className="flex items-center gap-1.5 text-[12px] text-muted">
            <span className={cn("size-1.5 rounded-full", play.dot)} aria-hidden />
            {play.label}
          </p>
        )}
        <div className="mt-auto flex items-center gap-1.5 pt-2 text-[12px] text-faint">
          {version && <span className="truncate rounded bg-surface-3 px-1.5 py-0.5 font-medium text-muted">v{version}</span>}
          {game.engine && <span className="truncate">{game.engine}</span>}
          {game.updatedAt ? <span className="ml-auto shrink-0">{timeAgo(game.updatedAt)}</span> : null}
        </div>
      </div>
    </article>
  );
});

export function GameCardSkeleton() {
  return (
    <div className="overflow-hidden rounded-[var(--radius-card)] border border-border bg-surface-1">
      <div className="skeleton aspect-[16/9]" />
      <div className="space-y-2 p-3">
        <div className="skeleton h-4 w-4/5 rounded" />
        <div className="skeleton h-3 w-2/5 rounded" />
        <div className="skeleton mt-3 h-3 w-1/3 rounded" />
      </div>
    </div>
  );
}

export function GameGrid({ children, min = 220 }: { children: React.ReactNode; min?: number }) {
  return (
    <div className="grid gap-4" style={{ gridTemplateColumns: `repeat(auto-fill, minmax(${min}px, 1fr))` }}>
      {children}
    </div>
  );
}
