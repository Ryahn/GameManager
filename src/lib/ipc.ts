/**
 * Typed wrappers around the Rust commands (see src-tauri/src/commands.rs).
 * The webview never holds credentials; it only asks Rust to act.
 */
import { invoke } from "@tauri-apps/api/core";

export type AuthMode = "api_key" | "sso";

export interface AuthStatus {
  signed_in: boolean;
  mode: AuthMode | null;
  username: string | null;
}

export interface AppInfo {
  version: string;
  sso_configured: boolean;
  api_keys_url: string;
}

export interface Settings {
  onboarding_complete: boolean;
  auth_mode: AuthMode | null;
  username: string | null;
  library_dir: string | null;
  theme: "dark" | "light" | "system";
  accent: string;
  ui_scale: number;
  reduce_motion: boolean;
  blur_nsfw: boolean;
}

export type SettingsPatch = Partial<
  Pick<Settings, "onboarding_complete" | "library_dir" | "theme" | "accent" | "ui_scale" | "reduce_motion" | "blur_nsfw">
>;

export interface ThreadSummary {
  thread_id: number;
  url: string;
  title: string;
  creator: string;
  version: string;
  developer: string;
  category: string;
  last_promoted: number | null;
  first_post_ts: number | null;
  is_new: boolean;
  prefixes: string[];
  cover: string | null;
  banner: string | null;
  previews: string[];
}

export interface ThreadDetail extends ThreadSummary {
  overview: string | null;
  views: number | null;
  likes: number | null;
  genre: string | null;
  changelog: string | null;
  thread_updated: string | null;
  release_date: string | null;
  censored: string | null;
  os: string | null;
  language: string | null;
  sam_rating: number | null;
  tag_names: string[];
  external_links: { role: string; host: string; url: string; label: string }[];
}

export interface Handiwork {
  id: number;
  name: string;
  overview: string;
  authors: { name: string }[];
  category: string;
  cover: string | null;
  banner: string | null;
  previews: string[];
  last_thread_update: string | null;
  last_promoted: number | null;
  prefixes: string[];
  rating: { average: number | null; best: number | null; count: number | null };
  tags: string[];
  url: string;
  engine: string | null;
  status: string | null;
  version: string | null;
}

export interface SearchHit {
  thread_id: number;
  title: string;
  creator: string;
  url: string;
  category: string;
  score: number | null;
  cover: string | null;
}

export interface Bookmark {
  thread_id: number;
  has_update: boolean;
  flagged_at: string | null;
  last_seen_version: string;
  current_version: string;
  needs_manual_review: boolean;
  review_reason: string | null;
  thread: ThreadSummary | null;
}

export interface LatestQuery {
  category?: string;
  order?: "date" | "likes" | "views" | "title" | "rating";
  date?: 1 | 3 | 7 | 14 | 30 | 90 | 180 | 365;
  page?: number;
  limit?: number;
  included_tags?: string[];
  excluded_tags?: string[];
}

export type ApiError =
  | { kind: "not_signed_in" }
  | { kind: "unauthorized"; message: string }
  | { kind: "forbidden"; message: string }
  | { kind: "not_found" }
  | { kind: "rate_limited"; retry_in_secs: number }
  | { kind: "network"; message: string }
  | { kind: "server"; status: number; message: string }
  | { kind: "invalid"; message: string }
  | { kind: "internal"; message: string };

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  settings: () => invoke<Settings>("settings_get"),
  updateSettings: (patch: SettingsPatch) => invoke<Settings>("settings_update", { patch }),

  authStatus: () => invoke<AuthStatus>("auth_status"),
  connectApiKey: (key: string) => invoke<AuthStatus>("auth_use_api_key", { key }),
  ssoSignIn: () => invoke<AuthStatus>("auth_sso_sign_in"),
  signOut: () => invoke<AuthStatus>("auth_sign_out"),

  latest: (query: LatestQuery = {}) => invoke<Handiwork[]>("catalog_latest", { query }),
  search: (q: string, limit = 20) => invoke<{ query: string; hits: SearchHit[] }>("catalog_search", { q, limit }),
  thread: (id: number) => invoke<ThreadDetail>("catalog_thread", { id }),

  bookmarks: (updatedOnly = false, limit = 200, offset = 0) =>
    invoke<{ total: number; items: Bookmark[] }>("bookmarks_list", { updatedOnly, limit, offset }),
  addBookmark: (id: number) => invoke<Bookmark>("bookmarks_add", { id }),
  removeBookmark: (id: number) => invoke<void>("bookmarks_remove", { id }),
  ack: (id: number) => invoke<Bookmark>("bookmarks_ack", { id }),
  ackAll: () => invoke<{ acknowledged: number }>("bookmarks_ack_all"),
};
