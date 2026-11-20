import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { api, type LatestQuery, type SettingsPatch } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";

export const keys = {
  appInfo: ["app-info"] as const,
  settings: ["settings"] as const,
  auth: ["auth"] as const,
  latest: (q: LatestQuery) => ["latest", q] as const,
  search: (q: string) => ["search", q] as const,
  thread: (id: number) => ["thread", id] as const,
  bookmarks: (updatedOnly: boolean) => ["bookmarks", updatedOnly] as const,
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

/** Set of thread ids in the user's library (server-side bookmarks). */
export function useLibraryIds(): Set<number> {
  const signedIn = useSignedIn();
  const { data } = useBookmarks(false, signedIn);
  return new Set(data?.items.map((b) => b.thread_id) ?? []);
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
