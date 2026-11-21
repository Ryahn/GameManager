import { useEffect, useMemo } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { useNavigate } from "@tanstack/react-router";
import { api, type DeepLink, type Events, type LatestQuery, type LocalGame, type LocalPatch, type SettingsPatch } from "@/lib/ipc";
import { formatPlaytime } from "@/lib/play";
import { useUi } from "@/state/ui";
import { friendlyError } from "@/lib/errors";
import type { Kind } from "@/lib/kinds";

export const keys = {
  appInfo: ["app-info"] as const,
  settings: ["settings"] as const,
  auth: ["auth"] as const,
  latest: (q: LatestQuery) => ["latest", q] as const,
  search: (q: string) => ["search", q] as const,
  thread: (id: number) => ["thread", id] as const,
  bookmarks: (updatedOnly: boolean) => ["bookmarks", updatedOnly] as const,
  library: ["bookmarks", "all"] as const,
  local: ["local"] as const,
  localGame: (id: number) => ["local", id] as const,
  sessions: (id: number) => ["sessions", id] as const,
  running: ["running"] as const,
  downloads: ["downloads"] as const,
  f95: ["f95-status"] as const,
  links: (id: number, version: string) => ["links", id, version] as const,
};

export const useAppInfo = () => useQuery({ queryKey: keys.appInfo, queryFn: api.appInfo, staleTime: Infinity });
export const useSettings = () => useQuery({ queryKey: keys.settings, queryFn: api.settings, staleTime: Infinity });
export const useAuth = () => useQuery({ queryKey: keys.auth, queryFn: api.authStatus, staleTime: Infinity });

export function useUpdateSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: SettingsPatch) => api.updateSettings(patch),
    onSuccess: (s) => qc.setQueryData(keys.settings, s),
    onError: (e) => toast.error(friendlyError(e).title, { description: friendlyError(e).detail }),
  });
}

export function useSignedIn() {
  const auth = useAuth();
  return auth.data?.signed_in ?? false;
}

export const useLatest = (q: LatestQuery, enabled = true) =>
  useQuery({ queryKey: keys.latest(q), queryFn: () => api.latest(q), enabled, staleTime: 5 * 60_000 });

export const useSearch = (q: string) =>
  useQuery({
    queryKey: keys.search(q),
    queryFn: () => api.search(q),
    enabled: q.trim().length >= 2,
    staleTime: 60_000,
    placeholderData: (prev) => prev,
  });

export const useThread = (id: number) =>
  useQuery({ queryKey: keys.thread(id), queryFn: () => api.thread(id), staleTime: 10 * 60_000 });

export const useBookmarks = (updatedOnly = false, enabled = true) =>
  useQuery({
    queryKey: keys.bookmarks(updatedOnly),
    queryFn: () => api.bookmarks(updatedOnly),
    enabled,
    staleTime: 2 * 60_000,
  });

/** The whole library (all bookmark pages); also refreshes the offline cache. */
export const useLibrary = (enabled = true) =>
  useQuery({ queryKey: keys.library, queryFn: api.library, enabled, staleTime: 2 * 60_000 });

/** Set of thread ids in the user's library (server-side bookmarks). */
export function useLibraryIds(): Set<number> {
  const signedIn = useSignedIn();
  const { data } = useLibrary(signedIn);
  return useMemo(() => new Set(data?.items.map((b) => b.thread_id) ?? []), [data]);
}

export function useToggleLibrary() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ id, add }: { id: number; add: boolean; title?: string }) => {
      if (add) await api.addBookmark(id);
      else await api.removeBookmark(id);
    },
    onSuccess: (_d, v) => {
      qc.invalidateQueries({ queryKey: ["bookmarks"] });
      toast.success(v.add ? "Added to your library" : "Removed from your library", { description: v.title });
    },
    onError: (e) => toast.error(friendlyError(e).title, { description: friendlyError(e).detail }),
  });
}

/** Card/row handler: adding happens at once, removing asks first. */
export function useLibraryToggle() {
  const toggle = useToggleLibrary();
  return (g: { id: number; title: string }, add: boolean) =>
    add ? toggle.mutate({ id: g.id, add: true, title: g.title }) : useUi.getState().askRemove({ id: g.id, title: g.title });
}

export function useAck() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.ack(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["bookmarks"] }),
    onError: (e) => toast.error(friendlyError(e).title, { description: friendlyError(e).detail }),
  });
}

// ------------------------------------------------------------ local library

const toastError = (title: string) => (e: unknown) => toast.error(title, { description: friendlyError(e).detail });

export const useLocalLibrary = () => useQuery({ queryKey: keys.local, queryFn: api.localLibrary, staleTime: 30_000 });

/** Local state keyed by thread id. */
export function useLocalMap(): Map<number, LocalGame> {
  const { data } = useLocalLibrary();
  return useMemo(() => new Map(data?.map((g) => [g.thread_id, g]) ?? []), [data]);
}

export const useLocalGame = (id: number) =>
  useQuery({ queryKey: keys.localGame(id), queryFn: () => api.localGame(id), staleTime: 30_000 });

export const useSessions = (id: number, enabled = true) =>
  useQuery({ queryKey: keys.sessions(id), queryFn: () => api.sessions(id), enabled });

/** Stores a fresh LocalGame from any mutation and refreshes lists. */
function useLocalWriter() {
  const qc = useQueryClient();
  return (g: LocalGame | null | undefined) => {
    if (g) qc.setQueryData(keys.localGame(g.thread_id), g);
    qc.invalidateQueries({ queryKey: keys.local, exact: true });
  };
}

export function useUpdateLocal(id: number) {
  const write = useLocalWriter();
  return useMutation({
    mutationFn: (patch: LocalPatch) => api.updateLocal(id, patch),
    onSuccess: write,
    onError: toastError("Couldn't save"),
  });
}

export function useLocalActions(id: number) {
  const write = useLocalWriter();
  const onError = toastError("That didn't work");
  return {
    addTarget: useMutation({ mutationFn: (path: string) => api.addTarget(id, path), onSuccess: write, onError }),
    removeTarget: useMutation({ mutationFn: (targetId: number) => api.removeTarget(targetId), onSuccess: write, onError }),
    setDefault: useMutation({ mutationFn: (targetId: number) => api.setDefaultTarget(targetId), onSuccess: write, onError }),
    linkFolder: useMutation({
      mutationFn: ({ dir, version }: { dir: string; version?: string | null }) => api.linkFolder(id, dir, version),
      onSuccess: write,
      onError,
    }),
  };
}

export const useRunning = () =>
  useQuery({ queryKey: keys.running, queryFn: api.running, staleTime: Infinity, select: (ids) => new Set(ids) });

export function usePlay() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, targetId }: { id: number; targetId?: number; title?: string }) => api.play(id, targetId),
    onSuccess: (_d, v) => {
      qc.invalidateQueries({ queryKey: keys.local });
      toast.success(`Starting ${v.title ?? "game"}`, { duration: 2500 });
    },
    onError: toastError("Couldn't start the game"),
  });
}

/**
 * The primary action on an installed item: play a game, read a comic, open
 * an animation's folder.
 */
export function useOpenItem() {
  const play = usePlay();
  const navigate = useNavigate();
  return (item: { id: number; title?: string; category?: Kind }) => {
    if (item.category === "comics") void navigate({ to: "/read/$id", params: { id: String(item.id) } });
    else if (item.category === "animations") api.openFolder(item.id).catch(toastError("Couldn't open the folder"));
    else play.mutate({ id: item.id, title: item.title });
  };
}

export const useContinueReading = () =>
  useQuery({ queryKey: ["continue-reading"], queryFn: () => api.continueReading(12), staleTime: 10_000 });

/** Listens for pushes from Rust and keeps the cache in step. Mount once. */
export function useBackendEvents() {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const onDeepLink = (l: DeepLink) => {
    navigate({ to: "/game/$id", params: { id: String(l.thread_id) } });
    if (l.added) {
      qc.invalidateQueries({ queryKey: ["bookmarks"] });
      qc.invalidateQueries({ queryKey: keys.local });
      toast.success("Added to your library");
    }
  };
  useEffect(() => {
    const unlisten = [
      listen<Events["game-running"]>("game-running", ({ payload }) => {
        qc.setQueryData<number[]>(keys.running, (ids = []) =>
          payload.running ? [...new Set([...ids, payload.thread_id])] : ids.filter((x) => x !== payload.thread_id),
        );
        if (!payload.running) {
          qc.invalidateQueries({ queryKey: keys.local });
          qc.invalidateQueries({ queryKey: keys.sessions(payload.thread_id) });
          if (payload.secs && payload.secs >= 60) {
            const g = qc.getQueryData<LocalGame | null>(keys.localGame(payload.thread_id));
            toast(`Played ${formatPlaytime(payload.secs)}`, { description: g?.cached?.title });
          }
        }
      }),
      listen<Events["updates-changed"]>("updates-changed", () => {
        qc.invalidateQueries({ queryKey: ["bookmarks"] });
      }),
      // Progress arrives a few times a second; refetch at most every 400 ms.
      listen("downloads-changed", () => {
        if (downloadsTimer) return;
        downloadsTimer = setTimeout(() => {
          downloadsTimer = undefined;
          qc.invalidateQueries({ queryKey: keys.downloads });
        }, 400);
      }),
      listen<Events["game-installed"]>("game-installed", ({ payload }) => {
        qc.invalidateQueries({ queryKey: keys.local });
        qc.invalidateQueries({ queryKey: keys.localGame(payload) });
      }),
      listen<Events["download-captured"]>("download-captured", ({ payload }) => {
        toast.success("Added to your downloads", { description: payload });
        qc.invalidateQueries({ queryKey: keys.downloads });
      }),
      listen<Events["f95-status"]>("f95-status", ({ payload }) => qc.setQueryData(keys.f95, payload)),
      listen<Events["deep-link"]>("deep-link", ({ payload }) => onDeepLink(payload)),
    ];
    // A link that started the app arrived before this listener existed.
    void api.deepLinkTake().then((l) => l && onDeepLink(l));
    let downloadsTimer: ReturnType<typeof setTimeout> | undefined;
    return () => unlisten.forEach((p) => p.then((f) => f()));
  }, [qc]);
}

/**
 * The update inbox minus versions the user chose to skip. Used by the
 * sidebar badge, Home and the Updates page so they always agree.
 */
export function useUpdateInbox(enabled = true) {
  const q = useBookmarks(true, enabled);
  const local = useLocalMap();
  return useMemo(() => {
    const all = q.data?.items ?? [];
    const isSkipped = (b: (typeof all)[number]) => {
      const skip = local.get(b.thread_id)?.skipped_version;
      return !!skip && skip === b.current_version;
    };
    return { query: q, items: all.filter((b) => !isSkipped(b)), skipped: all.filter(isSkipped) };
  }, [q, local]);
}

// ---------------------------------------------------------------- downloads

export const useDownloads = () => useQuery({ queryKey: keys.downloads, queryFn: api.downloads, staleTime: 5_000 });

/** Downloads that need attention or are moving, for the sidebar badge. */
export function useDownloadBadge(): number {
  const { data } = useDownloads();
  return data?.jobs.filter((j) => ["needs_you", "downloading", "extracting", "installing", "queued"].includes(j.state)).length ?? 0;
}

export const useF95Status = () => useQuery({ queryKey: keys.f95, queryFn: () => api.f95Status(false), staleTime: Infinity });

export function useCheckF95() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.f95Status(true),
    onSuccess: (s) => qc.setQueryData(keys.f95, s),
    onError: toastError("Couldn't check your F95 sign-in"),
  });
}

export function useDownloadAction() {
  const qc = useQueryClient();
  const done = () => qc.invalidateQueries({ queryKey: keys.downloads });
  const onError = toastError("That didn't work");
  return {
    pause: useMutation({ mutationFn: api.pauseDownload, onSuccess: done, onError }),
    resume: useMutation({ mutationFn: api.resumeDownload, onSuccess: done, onError }),
    remove: useMutation({ mutationFn: api.removeDownload, onSuccess: done, onError }),
    clear: useMutation({ mutationFn: api.clearFinished, onSuccess: done, onError }),
    openInBrowser: useMutation({ mutationFn: api.openDownloadInBrowser, onSuccess: done, onError }),
  };
}
