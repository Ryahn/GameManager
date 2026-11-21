import type { Bookmark, Handiwork, LocalGame, SearchHit, ThreadSummary } from "@/lib/ipc";
import type { GameCardData } from "./GameCard";
import { kindOf } from "@/lib/kinds";

export const fromHandiwork = (h: Handiwork): GameCardData => ({
  id: h.id,
  title: h.name,
  creator: h.authors[0]?.name,
  version: h.version,
  cover: h.cover,
  engine: h.engine,
  prefixes: h.prefixes,
  updatedAt: h.last_promoted,
  rating: h.rating?.average,
  category: kindOf(h.category),
});

export const fromThread = (t: ThreadSummary): GameCardData => ({
  id: t.thread_id,
  title: t.title,
  creator: t.creator || t.developer,
  version: t.version,
  cover: t.cover,
  prefixes: t.prefixes,
  updatedAt: t.last_promoted,
  isNew: t.is_new,
  category: kindOf(t.category),
});

export const fromBookmark = (b: Bookmark): GameCardData => ({
  ...(b.thread ? fromThread(b.thread) : { id: b.thread_id, title: `Thread ${b.thread_id}` }),
  version: b.current_version || b.thread?.version,
  hasUpdate: b.has_update,
});

export const fromHit = (h: SearchHit): GameCardData => ({
  id: h.thread_id,
  title: h.title,
  creator: h.creator,
  cover: h.cover,
  category: kindOf(h.category),
});

/** Adds local state (installed, play status) to a card. */
export const withLocal = (g: GameCardData, local: LocalGame | undefined): GameCardData => {
  if (!local) return g;
  const category = g.category ?? kindOf(local.cached?.category);
  // Games need something to launch; comics and animations just a folder.
  const installed = category === "games" ? local.targets.length > 0 : !!local.install_dir;
  return { ...g, category, installed, playStatus: local.play_status };
};

/** A card from the offline cache, for when the server can't be reached. */
export const fromLocal = (l: LocalGame): GameCardData =>
  withLocal(
    {
      id: l.thread_id,
      title: l.cached?.title || `Thread ${l.thread_id}`,
      creator: l.cached?.creator,
      version: l.cached?.version,
      cover: l.cached?.cover,
      prefixes: l.cached?.prefixes,
      updatedAt: l.cached?.last_promoted,
      category: kindOf(l.cached?.category),
    },
    l,
  );
