import { useEffect, useMemo } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { toast } from "sonner";
import { api, type Events, type LatestQuery, type LocalGame, type LocalPatch, type SettingsPatch } from "@/lib/ipc";
import { formatPlaytime } from "@/lib/play";
import { friendlyError } from "@/lib/errors";

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

/** Listens for pushes from Rust and keeps the cache in step. Mount once. */
export function useBackendEvents() {
  const qc = useQueryClient();
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
    ];
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
