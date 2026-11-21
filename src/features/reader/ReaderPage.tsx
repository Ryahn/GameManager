import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useRouter } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  BookOpen,
  Columns2,
  FileText,
  FolderOpen,
  Grid3x3,
  Loader2,
  Maximize,
  Minimize,
  Pause,
  Play,
  Rows3,
  Square,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { EmptyState, Kbd } from "@/components/ui/primitives";
import { useLocalGame } from "@/hooks/queries";
import { api, type Book, type FitMode, type ReadingProgress, type ReadMode } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { cn, mediaSrc } from "@/lib/utils";
import { flatten, pageLabel, spreadOf, spreads } from "./book";
import { toast } from "sonner";

const MODES: { value: ReadMode; label: string; icon: typeof Square }[] = [
  { value: "single", label: "Single page", icon: Square },
  { value: "double", label: "Two-page spreads", icon: Columns2 },
  { value: "scroll", label: "Vertical scroll", icon: Rows3 },
];
const FITS: { value: FitMode; label: string }[] = [
  { value: "height", label: "Fit height" },
  { value: "width", label: "Fit width" },
  { value: "original", label: "Original size" },
];
const SLIDE_SECONDS = [3, 5, 8];

/** Full-screen comic reader and gallery for a comic in the library. */
export function ReaderPage() {
  const { id } = useParams({ strict: false }) as { id: string };
  const threadId = Number(id);
  const book = useQuery({
    queryKey: ["book", threadId],
    queryFn: () => api.openBook(threadId),
    staleTime: Infinity,
    gcTime: 0,
    retry: false,
  });
  const local = useLocalGame(threadId);
  const title = local.data?.cached?.title || "Reader";
  const back = useBack(threadId);

  if (book.isLoading) {
    return (
      <Shell>
        <div className="grid h-full place-items-center text-white/60">
          <div className="flex flex-col items-center gap-3 text-sm">
            <Loader2 className="size-6 animate-spin" aria-hidden />
            Opening {title}…
          </div>
        </div>
      </Shell>
    );
  }
  if (book.error || !book.data) {
    return (
      <Shell>
        <EmptyState icon={BookOpen} title="Couldn't open this comic" action={<Button onClick={back}>Back</Button>}>
          {friendlyError(book.error).detail}
        </EmptyState>
      </Shell>
    );
  }
  return <Reader threadId={threadId} title={title} book={book.data.book} progress={book.data.progress} onBack={back} />;
}

function useBack(threadId: number) {
  const router = useRouter();
  const navigate = useNavigate();
  return useCallback(() => {
    if (window.history.length > 1) router.history.back();
    else void navigate({ to: "/game/$id", params: { id: String(threadId) } });
  }, [router, navigate, threadId]);
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div data-theme="dark" className="fixed inset-0 z-40 select-none bg-black text-white">
      {children}
    </div>
  );
}

async function setFullscreen(on: boolean) {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setFullscreen(on);
  } catch {
    // Outside the app shell (dev in a browser).
    if (on) await document.documentElement.requestFullscreen?.().catch(() => {});
    else if (document.fullscreenElement) await document.exitFullscreen().catch(() => {});
  }
}

function Reader({
  threadId,
  title,
  book,
  progress,
  onBack,
}: {
  threadId: number;
  title: string;
  book: Book;
  progress: ReadingProgress | null;
  onBack: () => void;
}) {
  const qc = useQueryClient();
  const { pages, starts } = useMemo(() => flatten(book), [book]);
  const total = pages.length;
  const [index, setIndex] = useState(() => Math.min(Math.max(progress?.page ?? 0, 0), Math.max(total - 1, 0)));
  const [mode, setMode] = useState<ReadMode>(progress?.mode ?? "single");
  const [fit, setFit] = useState<FitMode>(progress?.fit ?? "height");
  const [rtl, setRtl] = useState(progress?.rtl ?? false);
  const [view, setView] = useState<"read" | "gallery">("read");
  const [slideshow, setSlideshow] = useState<number | null>(null);
  const [fullscreen, setFs] = useState(false);
  const [chrome, setChrome] = useState(true);
  const [wide, setWide] = useState<Set<number>>(() => new Set());
  const stageRef = useRef<HTMLDivElement>(null);

  const spreadList = useMemo(() => spreads(pages, wide), [pages, wide]);
  const frame = mode === "double" ? (spreadList[spreadOf(spreadList, index)] ?? [index]) : [index];
  const chapter = pages[index]?.chapter ?? 0;

  // ---- navigation
  const step = useCallback(
    (dir: 1 | -1) => {
      if (mode === "double") {
        const s = spreadOf(spreadList, index) + dir;
        if (s >= 0 && s < spreadList.length) setIndex(spreadList[s][0]);
        return s >= 0 && s < spreadList.length;
      }
      const n = index + dir;
      if (n >= 0 && n < total) setIndex(n);
      return n >= 0 && n < total;
    },
    [mode, spreadList, index, total],
  );
  const next = useCallback(() => step(1), [step]);
  const prev = useCallback(() => step(-1), [step]);
  // Left/right on screen follow the reading direction.
  const left = rtl ? next : prev;
  const right = rtl ? prev : next;

  const goto = useCallback(
    (i: number) => {
      setIndex(Math.min(Math.max(i, 0), Math.max(total - 1, 0)));
      if (mode === "scroll") {
        requestAnimationFrame(() =>
          stageRef.current?.querySelector(`[data-page="${i}"]`)?.scrollIntoView({ block: "start" }),
        );
      }
    },
    [total, mode],
  );

  // Back to the top of the page when it changes (fit width / original).
  useEffect(() => {
    if (mode !== "scroll") stageRef.current?.scrollTo({ top: 0, left: rtl ? stageRef.current.scrollWidth : 0 });
  }, [index, mode, rtl]);

  // Entering scroll mode starts at the current page.
  useEffect(() => {
    if (mode === "scroll" && view === "read") {
      stageRef.current?.querySelector(`[data-page="${index}"]`)?.scrollIntoView({ block: "start" });
    }
    // Only when switching modes or views, not on every page change.
  }, [mode, view]);

  // In scroll mode the page in the middle of the screen is the current one.
  useEffect(() => {
    if (mode !== "scroll" || view !== "read" || !stageRef.current) return;
    const root = stageRef.current;
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) setIndex(Number((e.target as HTMLElement).dataset.page));
        }
      },
      { root, rootMargin: "-50% 0px -50% 0px" },
    );
    root.querySelectorAll("[data-page]").forEach((el) => io.observe(el));
    return () => io.disconnect();
  }, [mode, view, total]);

  // Load the next few pages ahead of time.
  useEffect(() => {
    if (mode === "scroll") return;
    for (const i of [index + 1, index + 2, index + 3, index - 1]) {
      if (i >= 0 && i < total) new Image().src = mediaSrc(pages[i].src);
    }
  }, [index, mode, pages, total]);

  // Slideshow.
  useEffect(() => {
    if (!slideshow || view !== "read") return;
    const t = setInterval(() => {
      if (!step(1)) setSlideshow(null);
    }, slideshow * 1000);
    return () => clearInterval(t);
  }, [slideshow, view, step]);

  // ---- saving progress (debounced, and once more on the way out)
  const latest = useRef<ReadingProgress | null>(null);
  latest.current = total
    ? { thread_id: threadId, chapter, page: index, pages_total: total, mode, rtl, fit, updated_at: 0 }
    : null;
  useEffect(() => {
    const t = setTimeout(() => latest.current && void api.saveProgress(latest.current).catch(() => {}), 700);
    return () => clearTimeout(t);
  }, [index, mode, rtl, fit]);
  useEffect(
    () => () => {
      if (latest.current) void api.saveProgress(latest.current).catch(() => {});
      // Let the save land before Home and the game page re-read it.
      setTimeout(() => {
        qc.invalidateQueries({ queryKey: ["continue-reading"] });
        qc.invalidateQueries({ queryKey: ["reading-progress", threadId] });
      }, 150);
      void setFullscreen(false);
    },
    [qc, threadId],
  );

  // ---- chrome auto-hide while reading
  const hideTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const poke = useCallback(() => {
    setChrome(true);
    clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setChrome(false), 2500);
  }, []);
  useEffect(() => {
    poke();
    return () => clearTimeout(hideTimer.current);
  }, [poke]);
  const showChrome = chrome || view === "gallery" || total === 0;

  const toggleFullscreen = useCallback(() => {
    setFs((f) => {
      void setFullscreen(!f);
      return !f;
    });
  }, []);

  const cycleMode = useCallback(() => setMode((m) => MODES[(MODES.findIndex((x) => x.value === m) + 1) % MODES.length].value), []);

  // ---- keyboard
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t?.closest?.("input, select, textarea") && e.key !== "Escape") return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const paged = view === "read" && mode !== "scroll";
      const k = e.key;
      const handled = (() => {
        if (k === "Escape") {
          if (view === "gallery") setView("read");
          else if (slideshow) setSlideshow(null);
          else if (fullscreen) toggleFullscreen();
          else onBack();
          return true;
        }
        if (k === "f" || k === "F") return toggleFullscreen(), true;
        if (k === "m" || k === "M") return cycleMode(), true;
        if (k === "g" || k === "G") return setView((v) => (v === "gallery" ? "read" : "gallery")), true;
        if (k === "r" || k === "R") return setRtl((v) => !v), true;
        if (k === "s" || k === "S") return setSlideshow((s) => (s ? null : SLIDE_SECONDS[0])), true;
        if (k === "Home") return goto(0), true;
        if (k === "End") return goto(total - 1), true;
        if (!paged) return false;
        if (k === "ArrowLeft") return left(), true;
        if (k === "ArrowRight") return right(), true;
        if (k === "PageDown" || (k === " " && !e.shiftKey)) return next(), true;
        if (k === "PageUp" || (k === " " && e.shiftKey)) return prev(), true;
        if (fit === "height" && k === "ArrowDown") return next(), true;
        if (fit === "height" && k === "ArrowUp") return prev(), true;
        return false;
      })();
      if (handled) {
        e.preventDefault();
        poke();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view, mode, fit, slideshow, fullscreen, total, left, right, next, prev, goto, onBack, toggleFullscreen, cycleMode, poke]);

  // Mouse wheel turns pages when nothing scrolls (fit height).
  const wheelAt = useRef(0);
  const onWheel = (e: React.WheelEvent) => {
    if (mode === "scroll" || fit !== "height" || Math.abs(e.deltaY) < 4) return;
    const now = Date.now();
    if (now - wheelAt.current < 220) return;
    wheelAt.current = now;
    if (e.deltaY > 0) next();
    else prev();
  };

  const markWide = (i: number) => (e: React.SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    if (img.naturalWidth > img.naturalHeight * 1.1 && !wide.has(i)) setWide((w) => new Set(w).add(i));
  };

  const imgFit = (count: number) =>
    fit === "height"
      ? cn("max-h-full object-contain", count === 2 ? "max-w-[50%]" : "max-w-full")
      : fit === "width"
        ? cn("h-auto", count === 2 ? "w-1/2" : "w-full")
        : "max-w-none";

  if (total === 0) {
    return (
      <Shell>
        <TopBar title={title} onBack={onBack} />
        <EmptyState
          icon={BookOpen}
          title="No pages found"
          action={
            <div className="flex flex-wrap justify-center gap-2">
              {book.other_files.map((f) => (
                <Button key={f} variant="primary" onClick={() => openExternal(f)}>
                  <FileText /> Open {baseName(f)}
                </Button>
              ))}
              <Button onClick={() => void api.openFolder(threadId)}>
                <FolderOpen /> Open folder
              </Button>
            </div>
          }
        >
          {book.other_files.length
            ? "This comic is a PDF, which opens in your PDF app."
            : "There are no images or comic archives (.cbz, .cbr, .cb7) in this comic's folder."}
        </EmptyState>
      </Shell>
    );
  }

  return (
    <Shell>
      <div className="flex h-full flex-col" onMouseMove={view === "read" ? poke : undefined}>
        {/* ---- top bar */}
        <div
          className={cn(
            "absolute inset-x-0 top-0 z-20 transition-opacity duration-200",
            showChrome ? "opacity-100" : "pointer-events-none opacity-0",
          )}
        >
          <TopBar title={title} subtitle={book.chapters.length > 1 ? book.chapters[chapter]?.name : undefined} onBack={onBack}>
            {book.chapters.length > 1 && (
              <select
                aria-label="Chapter"
                value={chapter}
                onChange={(e) => {
                  const v = e.target.value;
                  if (v.startsWith("pdf:")) openExternal(v.slice(4));
                  else goto(starts[Number(v)]);
                }}
                className="h-8 max-w-[220px] truncate rounded-lg border border-white/15 bg-black/60 px-2 text-[13px] text-white focus:border-accent focus:outline-none"
              >
                {book.chapters.map((c, i) => (
                  <option key={i} value={i} className="bg-neutral-900">
                    {c.name}
                  </option>
                ))}
                {book.other_files.length > 0 && (
                  <optgroup label="Open in another app">
                    {book.other_files.map((f) => (
                      <option key={f} value={`pdf:${f}`} className="bg-neutral-900">
                        {baseName(f)}
                      </option>
                    ))}
                  </optgroup>
                )}
              </select>
            )}
            {book.chapters.length <= 1 &&
              book.other_files.map((f) => (
                <ToolButton key={f} label={`Open ${baseName(f)} in another app`} onClick={() => openExternal(f)}>
                  <FileText />
                </ToolButton>
              ))}
            <div role="radiogroup" aria-label="Reading mode" className="inline-flex rounded-lg border border-white/15 bg-black/50 p-0.5">
              {MODES.map((m) => (
                <button
                  key={m.value}
                  type="button"
                  role="radio"
                  aria-checked={mode === m.value}
                  title={`${m.label} (M)`}
                  aria-label={m.label}
                  onClick={() => setMode(m.value)}
                  className={cn(
                    "grid h-7 w-8 place-items-center rounded-md transition-colors [&_svg]:size-4",
                    mode === m.value ? "bg-white/15 text-white" : "text-white/60 hover:text-white",
                  )}
                >
                  <m.icon />
                </button>
              ))}
            </div>
            <select
              aria-label="Page fit"
              value={fit}
              onChange={(e) => setFit(e.target.value as FitMode)}
              className="h-8 rounded-lg border border-white/15 bg-black/60 px-2 text-[13px] text-white focus:border-accent focus:outline-none"
            >
              {FITS.map((f) => (
                <option key={f.value} value={f.value} className="bg-neutral-900">
                  {f.label}
                </option>
              ))}
            </select>
            <button
              type="button"
              aria-pressed={rtl}
              title="Right-to-left, for manga (R)"
              onClick={() => setRtl((v) => !v)}
              className={cn(
                "h-8 rounded-lg border px-2.5 text-[12.5px] font-semibold transition-colors",
                rtl ? "border-accent bg-accent text-accent-fg" : "border-white/15 bg-black/50 text-white/70 hover:text-white",
              )}
            >
              {rtl ? "Right to left" : "Left to right"}
            </button>
            <ToolButton
              label={view === "gallery" ? "Back to reading (G)" : "Gallery (G)"}
              active={view === "gallery"}
              onClick={() => setView((v) => (v === "gallery" ? "read" : "gallery"))}
            >
              <Grid3x3 />
            </ToolButton>
            <SlideshowButton
              value={slideshow}
              onChange={(s) => {
                setSlideshow(s);
                if (s) setView("read");
              }}
            />
            <ToolButton label={fullscreen ? "Exit full screen (F)" : "Full screen (F)"} onClick={toggleFullscreen}>
              {fullscreen ? <Minimize /> : <Maximize />}
            </ToolButton>
          </TopBar>
        </div>

        {/* ---- pages */}
        {view === "gallery" ? (
          <Gallery book={book} pages={pages} starts={starts} current={index} onPick={(i) => (goto(i), setView("read"))} />
        ) : mode === "scroll" ? (
          <div ref={stageRef} className="h-full overflow-y-auto">
            <div className="mx-auto flex flex-col items-center">
              {pages.map((p, i) => (
                <img
                  key={p.src}
                  data-page={i}
                  src={mediaSrc(p.src)}
                  alt={`Page ${i + 1}`}
                  loading="lazy"
                  decoding="async"
                  draggable={false}
                  className={cn(
                    "block min-h-[40vh] bg-white/[0.03]",
                    fit === "width" ? "h-auto w-full" : fit === "height" ? "h-screen w-auto max-w-full object-contain" : "max-w-none",
                  )}
                  onLoad={(e) => e.currentTarget.classList.remove("min-h-[40vh]")}
                />
              ))}
            </div>
          </div>
        ) : (
          <div ref={stageRef} className="relative h-full overflow-auto" onWheel={onWheel}>
            <div
              className={cn(
                "flex min-h-full items-center justify-center",
                fit === "height" && "h-full",
                rtl && "flex-row-reverse",
              )}
            >
              {frame.map((i) => (
                <img
                  key={pages[i].src}
                  src={mediaSrc(pages[i].src)}
                  alt={`Page ${i + 1}`}
                  decoding="async"
                  draggable={false}
                  onLoad={markWide(i)}
                  className={cn("block", imgFit(frame.length))}
                />
              ))}
            </div>
            {/* Click the left or right side to turn; the middle shows the controls. */}
            <button type="button" aria-label="Previous page side" tabIndex={-1} onClick={left} className="fixed inset-y-16 left-0 z-10 w-2/5 cursor-w-resize" />
            <button type="button" aria-label="Next page side" tabIndex={-1} onClick={right} className="fixed inset-y-16 right-0 z-10 w-2/5 cursor-e-resize" />
          </div>
        )}

        {/* ---- bottom bar */}
        {view === "read" && (
          <div
            className={cn(
              "absolute inset-x-0 bottom-0 z-20 bg-gradient-to-t from-black/85 to-transparent px-5 pb-4 pt-8 transition-opacity duration-200",
              showChrome ? "opacity-100" : "pointer-events-none opacity-0",
            )}
          >
            <div className="mx-auto flex max-w-3xl items-center gap-4">
              <input
                type="range"
                aria-label="Page"
                min={0}
                max={total - 1}
                value={index}
                dir={rtl ? "rtl" : "ltr"}
                onChange={(e) => goto(Number(e.target.value))}
                className="h-1 flex-1 cursor-pointer accent-[var(--accent)]"
              />
              <span className="w-24 text-right text-[13px] tabular-nums text-white/80">
                {frame.length === 2 ? `${frame[0] + 1}–${frame[1] + 1} / ${total}` : pageLabel(index, total)}
              </span>
            </div>
            <p className="mt-2 hidden text-center text-[11.5px] text-white/45 md:block">
              <Kbd>←</Kbd> <Kbd>→</Kbd> turn · <Kbd>M</Kbd> mode · <Kbd>R</Kbd> direction · <Kbd>G</Kbd> gallery · <Kbd>S</Kbd> slideshow ·{" "}
              <Kbd>F</Kbd> full screen · <Kbd>Esc</Kbd> close
            </p>
          </div>
        )}
      </div>
    </Shell>
  );
}

const baseName = (p: string) => p.split(/[\\/]/).pop() ?? p;

function openExternal(path: string) {
  api.openExternal(path).catch((e) => toast.error("Couldn't open it", { description: friendlyError(e).detail }));
}

function TopBar({
  title,
  subtitle,
  onBack,
  children,
}: {
  title: string;
  subtitle?: string;
  onBack: () => void;
  children?: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-3 bg-gradient-to-b from-black/85 to-transparent px-3 pb-6 pt-3">
      <ToolButton label="Close (Esc)" onClick={onBack}>
        <ArrowLeft />
      </ToolButton>
      <div className="min-w-0 flex-1">
        <h1 className="truncate text-[14px] font-semibold">{title}</h1>
        {subtitle && <p className="truncate text-[12px] text-white/60">{subtitle}</p>}
      </div>
      <div className="flex flex-wrap items-center justify-end gap-2">{children}</div>
    </div>
  );
}

function ToolButton({
  label,
  onClick,
  active,
  children,
}: {
  label: string;
  onClick: () => void;
  active?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={active}
      onClick={onClick}
      className={cn(
        "grid size-8 place-items-center rounded-lg border transition-colors [&_svg]:size-4",
        active ? "border-accent bg-accent text-accent-fg" : "border-white/15 bg-black/50 text-white/80 hover:text-white",
      )}
    >
      {children}
    </button>
  );
}

function SlideshowButton({ value, onChange }: { value: number | null; onChange: (s: number | null) => void }) {
  return (
    <div className="inline-flex items-center rounded-lg border border-white/15 bg-black/50">
      <button
        type="button"
        title={value ? "Stop slideshow (S)" : "Slideshow (S)"}
        aria-label={value ? "Stop slideshow" : "Start slideshow"}
        onClick={() => onChange(value ? null : SLIDE_SECONDS[0])}
        className={cn("grid size-8 place-items-center rounded-lg [&_svg]:size-4", value ? "text-accent" : "text-white/80 hover:text-white")}
      >
        {value ? <Pause /> : <Play />}
      </button>
      {value && (
        <button
          type="button"
          title="Seconds per page"
          onClick={() => onChange(SLIDE_SECONDS[(SLIDE_SECONDS.indexOf(value) + 1) % SLIDE_SECONDS.length])}
          className="pr-2.5 text-[12px] font-semibold tabular-nums text-white/80 hover:text-white"
        >
          {value}s
        </button>
      )}
    </div>
  );
}

function Gallery({
  book,
  pages,
  starts,
  current,
  onPick,
}: {
  book: Book;
  pages: ReturnType<typeof flatten>["pages"];
  starts: number[];
  current: number;
  onPick: (i: number) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.querySelector(`[data-thumb="${current}"]`)?.scrollIntoView({ block: "center" });
  }, [current]);
  return (
    <div ref={ref} className="h-full overflow-y-auto px-5 pb-10 pt-20">
      {book.chapters.map((c, ci) => (
        <section key={ci} className="mb-8">
          {book.chapters.length > 1 && <h2 className="mb-3 text-[13px] font-semibold text-white/70">{c.name}</h2>}
          <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))" }}>
            {c.pages.map((_, pi) => {
              const i = starts[ci] + pi;
              return (
                <button
                  key={pages[i].src}
                  type="button"
                  data-thumb={i}
                  onClick={() => onPick(i)}
                  title={pages[i].name}
                  className={cn(
                    "group relative overflow-hidden rounded-lg border bg-white/[0.04] text-left transition-colors",
                    i === current ? "border-accent ring-2 ring-accent/50" : "border-white/10 hover:border-white/40",
                  )}
                >
                  <div data-cover-wrap className="relative aspect-[2/3] overflow-hidden">
                    <img
                      data-cover
                      src={mediaSrc(pages[i].src)}
                      alt=""
                      loading="lazy"
                      decoding="async"
                      draggable={false}
                      className="absolute inset-0 size-full object-cover"
                    />
                  </div>
                  <span className="absolute bottom-1 right-1 rounded bg-black/70 px-1.5 py-0.5 text-[11px] tabular-nums text-white/90">
                    {i + 1}
                  </span>
                </button>
              );
            })}
          </div>
        </section>
      ))}
    </div>
  );
}
