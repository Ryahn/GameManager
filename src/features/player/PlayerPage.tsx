import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams, useRouter } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  ArrowLeft,
  Check,
  ExternalLink,
  Film,
  ListVideo,
  Loader2,
  Maximize,
  Minimize,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { EmptyState, Kbd } from "@/components/ui/primitives";
import { clock } from "@/components/game/mappers";
import { useLocalGame } from "@/hooks/queries";
import { api, type Playlist, type Video, type WatchProgress } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { pref, setFullscreen, setPref } from "@/lib/fullscreen";
import { cn, mediaSrc } from "@/lib/utils";

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];
/** Watched this far counts as finished. */
const DONE = 0.95;

type Item = Video & { group: number };

/** Full-screen video player with a playlist, for an animation in the library. */
export function PlayerPage() {
  const { id } = useParams({ strict: false }) as { id: string };
  const threadId = Number(id);
  const data = useQuery({
    queryKey: ["playlist", threadId],
    queryFn: () => api.openPlaylist(threadId),
    staleTime: Infinity,
    gcTime: 0,
    retry: false,
  });
  const local = useLocalGame(threadId);
  const title = local.data?.cached?.title || "Player";
  const router = useRouter();
  const navigate = useNavigate();
  const back = useCallback(() => {
    if (window.history.length > 1) router.history.back();
    else void navigate({ to: "/game/$id", params: { id: String(threadId) } });
  }, [router, navigate, threadId]);

  if (data.isLoading) {
    return (
      <Shell>
        <div className="grid h-full place-items-center text-sm text-white/60">
          <Loader2 className="size-6 animate-spin" aria-label="Loading" />
        </div>
      </Shell>
    );
  }
  if (data.error || !data.data) {
    return (
      <Shell>
        <EmptyState icon={Film} title="Couldn't open this" action={<Button onClick={back}>Back</Button>}>
          {friendlyError(data.error).detail}
        </EmptyState>
      </Shell>
    );
  }
  return <Player threadId={threadId} title={title} playlist={data.data.playlist} progress={data.data.progress} onBack={back} />;
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div data-theme="dark" className="fixed inset-0 z-40 bg-black text-white">
      {children}
    </div>
  );
}

const finished = (p: WatchProgress | undefined) => !!p && p.duration > 0 && p.position >= p.duration * DONE;

/** Where to start: the last video watched, or the one after it if that was finished. */
export function startIndex(items: { file: string }[], progress: WatchProgress[]) {
  const last = progress[0];
  const i = last ? items.findIndex((v) => v.file === last.file) : -1;
  if (i < 0) return 0;
  return finished(last) && i + 1 < items.length ? i + 1 : i;
}

function Player({
  threadId,
  title,
  playlist,
  progress,
  onBack,
}: {
  threadId: number;
  title: string;
  playlist: Playlist;
  progress: WatchProgress[];
  onBack: () => void;
}) {
  const qc = useQueryClient();
  const items = useMemo<Item[]>(
    () => playlist.groups.flatMap((g, group) => g.videos.map((v) => ({ ...v, group }))),
    [playlist],
  );
  const saved = useRef(new Map(progress.map((p) => [p.file, p])));
  const [index, setIndex] = useState(() => startIndex(items, progress));
  const current = items[index] as Item | undefined;
  const videoRef = useRef<HTMLVideoElement>(null);

  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(() => pref("player.volume", 1));
  const [muted, setMuted] = useState(() => pref("player.muted", false));
  const [rate, setRate] = useState(() => pref("player.rate", 1));
  const [autoplay, setAutoplay] = useState(() => pref("player.autoplay", true));
  const [listOpen, setListOpen] = useState(() => pref("player.list", true));
  const [failed, setFailed] = useState(false);
  const [fullscreen, setFs] = useState(false);
  const [chrome, setChrome] = useState(true);

  useEffect(() => setPref("player.volume", volume), [volume]);
  useEffect(() => setPref("player.muted", muted), [muted]);
  useEffect(() => setPref("player.rate", rate), [rate]);
  useEffect(() => setPref("player.autoplay", autoplay), [autoplay]);
  useEffect(() => setPref("player.list", listOpen), [listOpen]);

  // Keep the element in step with the controls.
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    v.volume = volume;
    v.muted = muted;
    v.playbackRate = rate;
  });

  // ---- saving where each video was left
  const lastSave = useRef(0);
  // The latest position, kept outside React so leaving the page can save it
  // (the video element is gone by the time cleanup runs).
  const pos = useRef<{ file: string; position: number; duration: number } | null>(null);
  const persist = useCallback(
    (file: string, position: number, length: number) => {
      if (!Number.isFinite(length) || length <= 0) return;
      saved.current.set(file, { file, position, duration: length, updated_at: Math.floor(Date.now() / 1000) });
      lastSave.current = Date.now();
      void api.saveWatch(threadId, file, position, length).catch(() => {});
    },
    [threadId],
  );
  const save = useCallback(
    (position: number, length: number) => {
      if (!current) return;
      pos.current = { file: current.file, position, duration: length };
      persist(current.file, position, length);
    },
    [current, persist],
  );
  const saveNow = useCallback(() => {
    const v = videoRef.current;
    if (v && v.duration) save(v.currentTime, v.duration);
  }, [save]);

  // Save once more when leaving.
  useEffect(
    () => () => {
      const p = pos.current;
      if (p) persist(p.file, p.position, p.duration);
      setTimeout(() => {
        qc.invalidateQueries({ queryKey: ["continue-watching"] });
        qc.invalidateQueries({ queryKey: ["watch-last", threadId] });
      }, 150);
      void setFullscreen(false);
    },
    [qc, threadId, persist],
  );

  // Pause when the boss key hides the window.
  useEffect(() => {
    const un = listen("boss-key", () => videoRef.current?.pause());
    return () => void un.then((f) => f()).catch(() => {});
  }, []);

  // A new video: clear the old state.
  useEffect(() => {
    setFailed(false);
    setTime(0);
    setDuration(0);
  }, [index]);

  const go = useCallback(
    (i: number) => {
      if (i < 0 || i >= items.length || i === index) return;
      saveNow();
      setIndex(i);
    },
    [items.length, index, saveNow],
  );
  const next = useCallback(() => go(index + 1), [go, index]);
  const prev = useCallback(() => {
    const v = videoRef.current;
    if (v && v.currentTime > 5) v.currentTime = 0;
    else go(index - 1);
  }, [go, index]);

  const toggle = useCallback(() => {
    const v = videoRef.current;
    if (!v || failed || !current?.playable) return;
    if (v.paused) void v.play().catch(() => {});
    else v.pause();
  }, [failed, current]);

  const seekBy = useCallback((secs: number) => {
    const v = videoRef.current;
    if (v && v.duration) v.currentTime = Math.min(Math.max(v.currentTime + secs, 0), v.duration - 0.1);
  }, []);

  const toggleFullscreen = useCallback(() => {
    setFs((f) => {
      void setFullscreen(!f);
      return !f;
    });
  }, []);

  const openExternal = useCallback(() => {
    if (!current) return;
    videoRef.current?.pause();
    api.openExternal(current.path).catch((e) => toast.error("Couldn't open it", { description: friendlyError(e).detail }));
  }, [current]);

  // ---- controls auto-hide while playing
  const hideTimer = useRef<ReturnType<typeof setTimeout>>(undefined);
  const poke = useCallback(() => {
    setChrome(true);
    clearTimeout(hideTimer.current);
    hideTimer.current = setTimeout(() => setChrome(false), 2800);
  }, []);
  useEffect(() => () => clearTimeout(hideTimer.current), []);
  const showChrome = chrome || !playing || failed || !current?.playable;

  // ---- keyboard
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t?.closest?.("input, select, textarea") && e.key !== "Escape") return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
      const actions: Record<string, () => void> = {
        " ": toggle,
        k: toggle,
        ArrowLeft: () => seekBy(-5),
        ArrowRight: () => seekBy(5),
        j: () => seekBy(-10),
        l: () => seekBy(10),
        ArrowUp: () => (setVolume((v) => Math.min(1, +(v + 0.1).toFixed(2))), setMuted(false)),
        ArrowDown: () => setVolume((v) => Math.max(0, +(v - 0.1).toFixed(2))),
        m: () => setMuted((m) => !m),
        f: toggleFullscreen,
        n: next,
        p: prev,
        Escape: () => (fullscreen ? toggleFullscreen() : onBack()),
      };
      if (/^[0-9]$/.test(k) && videoRef.current?.duration) {
        videoRef.current.currentTime = (videoRef.current.duration * Number(k)) / 10;
      } else if (actions[k]) {
        actions[k]();
      } else {
        return;
      }
      e.preventDefault();
      poke();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggle, seekBy, toggleFullscreen, next, prev, fullscreen, onBack, poke]);

  if (!current) {
    return (
      <Shell>
        <div className="flex items-center gap-3 p-3">
          <IconButton label="Close (Esc)" onClick={onBack}>
            <ArrowLeft />
          </IconButton>
          <h1 className="truncate text-[14px] font-semibold">{title}</h1>
        </div>
        <EmptyState
          icon={Film}
          title="No videos found"
          action={
            <Button onClick={() => void api.openFolder(threadId)}>
              <ExternalLink /> Open folder
            </Button>
          }
        >
          There are no video files in this folder.
        </EmptyState>
      </Shell>
    );
  }

  const group = playlist.groups[current.group];
  const VolumeIcon = muted || volume === 0 ? VolumeX : volume < 0.5 ? Volume1 : Volume2;

  return (
    <Shell>
      <div className="flex h-full">
        <div className="relative min-w-0 flex-1" onMouseMove={poke}>
          {current.playable && !failed ? (
            <video
              key={`${index}:${current.src}`}
              ref={videoRef}
              src={mediaSrc(current.src)}
              autoPlay
              playsInline
              className="size-full bg-black object-contain"
              onClick={toggle}
              onDoubleClick={toggleFullscreen}
              onPlay={() => setPlaying(true)}
              onPause={() => {
                setPlaying(false);
                saveNow();
              }}
              onLoadedMetadata={(e) => {
                const v = e.currentTarget;
                setDuration(v.duration);
                const s = saved.current.get(current.file);
                if (s && s.position > 5 && !finished(s)) {
                  v.currentTime = s.position;
                  toast(`Picked up at ${clock(s.position)}`, {
                    duration: 4000,
                    action: { label: "Start over", onClick: () => videoRef.current && (videoRef.current.currentTime = 0) },
                  });
                }
                save(v.currentTime, v.duration);
              }}
              onTimeUpdate={(e) => {
                const v = e.currentTarget;
                setTime(v.currentTime);
                pos.current = { file: current.file, position: v.currentTime, duration: v.duration };
                if (Date.now() - lastSave.current > 5000) save(v.currentTime, v.duration);
              }}
              onEnded={(e) => {
                save(e.currentTarget.duration, e.currentTarget.duration);
                if (autoplay && index + 1 < items.length) next();
              }}
              onError={() => setFailed(true)}
            />
          ) : (
            <div className="grid size-full place-items-center p-6">
              <div className="max-w-md text-center">
                <div className="mx-auto mb-4 grid size-14 place-items-center rounded-2xl bg-white/10">
                  <Film className="size-7 text-white/80" />
                </div>
                <h2 className="text-lg font-semibold">{current.name}</h2>
                <p className="mt-2 text-sm text-white/65">
                  {current.playable
                    ? "This video uses a format the app can't play. Your own video player can."
                    : `.${current.file.split(".").pop()} videos play in your own video player.`}
                </p>
                <div className="mt-5 flex flex-wrap justify-center gap-2">
                  <Button variant="primary" onClick={openExternal}>
                    <ExternalLink /> Open in your video player
                  </Button>
                  {index + 1 < items.length && (
                    <Button variant="outline" onClick={next}>
                      <SkipForward /> Next video
                    </Button>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* ---- top bar */}
          <div
            className={cn(
              "absolute inset-x-0 top-0 flex items-center gap-3 bg-gradient-to-b from-black/85 to-transparent px-3 pb-8 pt-3 transition-opacity duration-200",
              showChrome ? "opacity-100" : "pointer-events-none opacity-0",
            )}
          >
            <IconButton label="Close (Esc)" onClick={onBack}>
              <ArrowLeft />
            </IconButton>
            <div className="min-w-0 flex-1">
              <h1 className="truncate text-[14px] font-semibold">{current.name}</h1>
              <p className="truncate text-[12px] text-white/60">
                {title}
                {playlist.groups.length > 1 && group ? ` · ${group.name}` : ""}
              </p>
            </div>
            <IconButton label="Open in your video player" onClick={openExternal}>
              <ExternalLink />
            </IconButton>
            <IconButton label={listOpen ? "Hide playlist" : "Show playlist"} active={listOpen} onClick={() => setListOpen((o) => !o)}>
              <ListVideo />
            </IconButton>
          </div>

          {/* ---- controls */}
          {current.playable && !failed && (
            <div
              className={cn(
                "absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/90 to-transparent px-4 pb-3 pt-10 transition-opacity duration-200",
                showChrome ? "opacity-100" : "pointer-events-none opacity-0",
              )}
            >
              <input
                type="range"
                aria-label="Seek"
                min={0}
                max={duration || 0}
                step={0.1}
                value={Math.min(time, duration || 0)}
                onChange={(e) => {
                  const v = videoRef.current;
                  if (v) v.currentTime = Number(e.target.value);
                }}
                className="h-1 w-full cursor-pointer accent-[var(--accent)]"
              />
              <div className="mt-2 flex items-center gap-1.5">
                <IconButton label="Previous (P)" onClick={prev} plain>
                  <SkipBack />
                </IconButton>
                <IconButton label={playing ? "Pause (Space)" : "Play (Space)"} onClick={toggle} plain>
                  {playing ? <Pause className="fill-current" /> : <Play className="fill-current" />}
                </IconButton>
                <IconButton label="Next (N)" onClick={next} plain>
                  <SkipForward />
                </IconButton>
                <span className="ml-1 text-[12.5px] tabular-nums text-white/80">
                  {clock(time)} / {clock(duration)}
                </span>
                <div className="ml-2 flex items-center gap-1">
                  <IconButton label={muted ? "Unmute (M)" : "Mute (M)"} onClick={() => setMuted((m) => !m)} plain>
                    <VolumeIcon />
                  </IconButton>
                  <input
                    type="range"
                    aria-label="Volume"
                    min={0}
                    max={1}
                    step={0.05}
                    value={muted ? 0 : volume}
                    onChange={(e) => {
                      setVolume(Number(e.target.value));
                      setMuted(false);
                    }}
                    className="h-1 w-20 cursor-pointer accent-[var(--accent)]"
                  />
                </div>
                <div className="flex-1" />
                <label className="flex items-center gap-1.5 text-[12.5px] text-white/75" title="Play the next video when one ends">
                  <input type="checkbox" checked={autoplay} onChange={(e) => setAutoplay(e.target.checked)} className="accent-[var(--accent)]" />
                  Autoplay
                </label>
                <select
                  aria-label="Speed"
                  value={rate}
                  onChange={(e) => setRate(Number(e.target.value))}
                  className="h-8 rounded-lg border border-white/15 bg-black/60 px-2 text-[12.5px] text-white focus:border-accent focus:outline-none"
                >
                  {SPEEDS.map((s) => (
                    <option key={s} value={s} className="bg-neutral-900">
                      {s === 1 ? "Normal" : `${s}×`}
                    </option>
                  ))}
                </select>
                <IconButton label={fullscreen ? "Exit full screen (F)" : "Full screen (F)"} onClick={toggleFullscreen} plain>
                  {fullscreen ? <Minimize /> : <Maximize />}
                </IconButton>
              </div>
              <p className="mt-1 hidden text-center text-[11px] text-white/40 lg:block">
                <Kbd>Space</Kbd> play · <Kbd>←</Kbd> <Kbd>→</Kbd> 5s · <Kbd>J</Kbd> <Kbd>L</Kbd> 10s · <Kbd>N</Kbd> <Kbd>P</Kbd> next/previous ·{" "}
                <Kbd>M</Kbd> mute · <Kbd>F</Kbd> full screen
              </p>
            </div>
          )}
        </div>

        {listOpen && (
          <PlaylistPanel
            playlist={playlist}
            items={items}
            current={index}
            saved={saved.current}
            livePosition={{ file: current.file, position: time, duration }}
            onPick={go}
          />
        )}
      </div>
    </Shell>
  );
}

function PlaylistPanel({
  playlist,
  items,
  current,
  saved,
  livePosition,
  onPick,
}: {
  playlist: Playlist;
  items: Item[];
  current: number;
  saved: Map<string, WatchProgress>;
  livePosition: { file: string; position: number; duration: number };
  onPick: (i: number) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.querySelector(`[data-item="${current}"]`)?.scrollIntoView({ block: "nearest" });
  }, [current]);
  let i = 0;
  return (
    <aside className="flex w-80 shrink-0 flex-col border-l border-white/10 bg-neutral-950">
      <div className="border-b border-white/10 px-4 py-3">
        <h2 className="text-[13px] font-semibold">Playlist</h2>
        <p className="text-[12px] text-white/50">
          {current + 1} of {items.length}
        </p>
      </div>
      <div ref={ref} className="flex-1 overflow-y-auto py-2">
        {playlist.groups.map((g, gi) => (
          <section key={gi} className="mb-2">
            {playlist.groups.length > 1 && (
              <h3 className="px-4 pb-1 pt-2 text-[11.5px] font-semibold uppercase tracking-wide text-white/45">{g.name}</h3>
            )}
            {g.videos.map((v) => {
              const n = i++;
              const p = v.file === livePosition.file && livePosition.duration ? livePosition : saved.get(v.file);
              const frac = p && p.duration ? Math.min(p.position / p.duration, 1) : 0;
              const done = frac >= DONE;
              return (
                <button
                  key={v.src}
                  type="button"
                  data-item={n}
                  onClick={() => onPick(n)}
                  aria-current={n === current ? "true" : undefined}
                  className={cn(
                    "flex w-full items-start gap-3 px-4 py-2 text-left transition-colors",
                    n === current ? "bg-white/10" : "hover:bg-white/5",
                  )}
                >
                  <span className="mt-0.5 grid w-5 shrink-0 place-items-center text-[12px] tabular-nums text-white/45">
                    {n === current ? (
                      <Play className="size-3.5 fill-current text-accent" />
                    ) : done ? (
                      <Check className="size-3.5 text-success" aria-label="Watched" />
                    ) : (
                      n + 1
                    )}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className={cn("block truncate text-[13px]", n === current ? "font-semibold text-white" : "text-white/85")}>{v.name}</span>
                    {!v.playable && <span className="text-[11.5px] text-white/45">Opens in your video player</span>}
                    {frac > 0 && !done && (
                      <span className="mt-1.5 block h-0.5 overflow-hidden rounded bg-white/15">
                        <span className="block h-full bg-accent" style={{ width: `${Math.round(frac * 100)}%` }} />
                      </span>
                    )}
                  </span>
                </button>
              );
            })}
          </section>
        ))}
      </div>
    </aside>
  );
}

function IconButton({
  label,
  onClick,
  active,
  plain,
  children,
}: {
  label: string;
  onClick: () => void;
  active?: boolean;
  plain?: boolean;
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
        "grid size-8 shrink-0 place-items-center rounded-lg transition-colors [&_svg]:size-4",
        plain
          ? "text-white/85 hover:bg-white/10 hover:text-white"
          : active
            ? "border border-accent bg-accent text-accent-fg"
            : "border border-white/15 bg-black/50 text-white/80 hover:text-white",
      )}
    >
      {children}
    </button>
  );
}
