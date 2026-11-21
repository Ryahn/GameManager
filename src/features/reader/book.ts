import type { Book } from "@/lib/ipc";

export interface FlatPage {
  src: string;
  name: string;
  chapter: number;
}

/** All pages in reading order, plus the index where each chapter starts. */
export function flatten(book: Book): { pages: FlatPage[]; starts: number[] } {
  const pages: FlatPage[] = [];
  const starts: number[] = [];
  book.chapters.forEach((c, chapter) => {
    starts.push(pages.length);
    for (const p of c.pages) pages.push({ src: p.src, name: p.name, chapter });
  });
  return { pages, starts };
}

/**
 * Two-page spreads. A chapter's first page (its cover) and wide pages
 * (already a double-page scan) stand alone; everything else pairs up.
 */
export function spreads(pages: FlatPage[], wide: ReadonlySet<number>): number[][] {
  const out: number[][] = [];
  let i = 0;
  while (i < pages.length) {
    const first = i === 0 || pages[i - 1].chapter !== pages[i].chapter;
    const next = i + 1;
    const canPair =
      !first && !wide.has(i) && next < pages.length && pages[next].chapter === pages[i].chapter && !wide.has(next);
    out.push(canPair ? [i, next] : [i]);
    i += canPair ? 2 : 1;
  }
  return out;
}

/** Which spread holds page `page`. */
export function spreadOf(list: number[][], page: number): number {
  const i = list.findIndex((s) => s.includes(page));
  return i < 0 ? 0 : i;
}

/** "Page 12 of 240". */
export const pageLabel = (page: number, total: number) => `${Math.min(page + 1, total)} / ${total}`;
