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
  platform: "windows" | "macos" | "linux";
  steam_deck: boolean;
  pick_extensions: string[];
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
  close_to_tray: boolean;
  notify_updates: boolean;
  max_active_downloads: number;
  delete_archives: boolean;
  pixeldrain_user: string | null;
  mega_email: string | null;
  gofile_user: string | null;
  boss_key: string | null;
  wine_command: string | null;
  auto_update_check: boolean;
}

export type SettingsPatch = Partial<
  Pick<
    Settings,
    | "onboarding_complete"
    | "library_dir"
    | "theme"
    | "accent"
    | "ui_scale"
    | "reduce_motion"
    | "blur_nsfw"
    | "close_to_tray"
    | "notify_updates"
    | "max_active_downloads"
    | "delete_archives"
    | "wine_command"
    | "auto_update_check"
  >
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

// ------------------------------------------------------------ local library

export type PlayStatus = "not_started" | "playing" | "on_hold" | "finished" | "dropped";
export type Engine = "ren_py" | "rpg_maker" | "unity" | "unreal" | "godot" | "html" | "other";

export interface LaunchTarget {
  id: number;
  path: string;
  label: string;
  args: string;
  is_default: boolean;
}

export interface LocalGame {
  thread_id: number;
  in_library: boolean;
  play_status: PlayStatus;
  rating: number | null;
  notes: string;
  labels: string[];
  installed_version: string | null;
  finished_version: string | null;
  last_played_version: string | null;
  skipped_version: string | null;
  install_dir: string | null;
  added_at: number;
  last_played_at: number | null;
  playtime_secs: number;
  targets: LaunchTarget[];
  cached: {
    title: string;
    creator: string;
    version: string;
    cover: string | null;
    url: string;
    prefixes: string[];
    last_promoted: number | null;
  } | null;
}

export interface LocalPatch {
  play_status?: PlayStatus;
  /** 0 clears. */
  rating?: number;
  notes?: string;
  labels?: string[];
  /** Empty string clears. */
  installed_version?: string;
  finished_version?: string;
  skipped_version?: string;
  install_dir?: string;
}

export interface PlaySession {
  started_at: number;
  secs: number;
  version: string | null;
}

export interface Candidate {
  path: string;
  label: string;
  engine: Engine;
}

export interface MatchSuggestion {
  thread_id: number;
  title: string;
  creator: string;
  cover: string | null;
  confidence: number;
  confident: boolean;
}

export interface ScanFolder {
  path: string;
  folder_name: string;
  title_guess: string;
  version_guess: string | null;
  engine: Engine;
  targets: Candidate[];
  linked_thread_id: number | null;
  /** Linked here AND in the library: nothing to do for this folder. */
  linked_in_library: boolean;
  suggestion: MatchSuggestion | null;
}

export interface ScanProgress {
  phase: "finding" | "matching" | "done";
  done: number;
  total: number;
}

export interface ImportItem {
  thread_id: number;
  path: string;
  version: string | null;
}

// ---------------------------------------------------------------- downloads

export interface GameCtx {
  thread_id: number;
  title: string;
  creator: string;
  version: string;
}

export interface DownloadLink {
  host: string;
  url: string;
  label: string;
  /** F95-masked: needs the user to pass a captcha in the F95 window. */
  masked: boolean;
  /** Rust's decision: the app downloads it (true) or the F95 window does. */
  in_app: boolean;
}

export interface LinkGroup {
  label: string;
  os: ("windows" | "linux" | "mac" | "android")[];
  links: DownloadLink[];
}

export interface ThreadLinks {
  thread_id: number;
  version: string;
  fetched_at: number;
  from_cache: boolean;
  groups: LinkGroup[];
}

export interface F95Status {
  checked_at: number;
  logged_in: boolean;
  username: string | null;
}

export interface Preflight {
  queued: number;
  adding: number;
  daily: number;
  warn: boolean;
  needs_confirm: boolean;
  eta_secs: number;
}

export type JobState = "queued" | "downloading" | "extracting" | "installing" | "done" | "failed" | "needs_you" | "paused";

export interface DownloadJob {
  id: number;
  thread_id: number | null;
  title: string;
  creator: string;
  version: string;
  source: "direct" | "browser";
  host: string;
  page_url: string;
  file_name: string | null;
  path: string | null;
  state: JobState;
  bytes_done: number;
  bytes_total: number | null;
  error: string | null;
  attempts: number;
  retry_at: number | null;
  install_dir: string | null;
  created_at: number;
  finished_at: number | null;
  /** Queued jobs: when pacing lets this one start. */
  starts_at: number | null;
}

export interface DownloadSnapshot {
  jobs: DownloadJob[];
  daily: number;
  daily_limit: number;
  paused_for_game: boolean;
  max_active: number;
}

/** Events pushed from Rust. */
export interface Events {
  "game-running": { thread_id: number; running: boolean; secs: number | null };
  "updates-changed": { count: number };
  "scan-progress": ScanProgress;
  "downloads-changed": null;
  "download-captured": string;
  "game-installed": number;
  "f95-status": F95Status | null;
  "deep-link": DeepLink;
  "lock-changed": boolean;
}

/** Signed-in file-host accounts (display names only). */
export interface HostAccounts {
  pixeldrain: string | null;
  mega: string | null;
  gofile: string | null;
}

export interface SystemInfo {
  autostart: boolean;
  boss_key: string | null;
  pin_enabled: boolean;
  locked: boolean;
  updater_configured: boolean;
}

export interface F95CheckerImport {
  games: number;
  skipped_custom: number;
  executables_linked: number;
  executables_missing: number;
  added_to_library: number;
  not_in_catalog: number;
}

export interface DeepLink {
  action: "add" | "open";
  thread_id: number;
  added?: boolean;
}

/** Typed by the user to go past the daily download limit. */
export const CONFIRM_PHRASE = "download anyway";

export type ApiError =
  | { kind: "not_signed_in" }
  | { kind: "unauthorized"; message: string }
  | { kind: "forbidden"; message: string }
  | { kind: "not_found" }
  | { kind: "rate_limited"; retry_in_secs: number }
  | { kind: "network"; message: string }
  | { kind: "server"; status: number; message: string }
  | { kind: "invalid"; message: string }
  | { kind: "internal"; message: string }
  | { kind: "needs_browser"; reason: "login" | "challenge" | string; url: string }
  | { kind: "confirm_required"; daily: number; limit: number };

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

  library: () => invoke<{ total: number; items: Bookmark[] }>("bookmarks_all"),
  bookmarks: (updatedOnly = false, limit = 200, offset = 0) =>
    invoke<{ total: number; items: Bookmark[] }>("bookmarks_list", { updatedOnly, limit, offset }),
  addBookmark: (id: number) => invoke<Bookmark>("bookmarks_add", { id }),
  removeBookmark: (id: number) => invoke<void>("bookmarks_remove", { id }),
  ack: (id: number) => invoke<Bookmark>("bookmarks_ack", { id }),
  ackAll: () => invoke<{ acknowledged: number }>("bookmarks_ack_all"),

  localLibrary: () => invoke<LocalGame[]>("library_local"),
  localGame: (id: number) => invoke<LocalGame | null>("game_local", { id }),
  sessions: (id: number) => invoke<PlaySession[]>("game_sessions", { id }),
  updateLocal: (id: number, patch: LocalPatch) => invoke<LocalGame>("game_update", { id, patch }),
  addTarget: (id: number, path: string) => invoke<LocalGame>("target_add", { id, path }),
  removeTarget: (targetId: number) => invoke<LocalGame | null>("target_remove", { targetId }),
  setDefaultTarget: (targetId: number) => invoke<LocalGame | null>("target_set_default", { targetId }),
  detectFolder: (dir: string) => invoke<Candidate[]>("folder_detect", { dir }),
  linkFolder: (id: number, dir: string, version?: string | null) =>
    invoke<LocalGame>("game_link_folder", { id, dir, version: version ?? null }),
  play: (id: number, targetId?: number) => invoke<void>("game_play", { id, targetId: targetId ?? null }),
  running: () => invoke<number[]>("games_running"),
  openFolder: (id: number) => invoke<void>("game_open_folder", { id }),
  scan: (dir: string) => invoke<ScanFolder[]>("library_scan", { dir }),
  cancelScan: () => invoke<void>("library_scan_cancel"),
  importScan: (items: ImportItem[]) =>
    invoke<{ linked: number; added_to_library: number; failed: string[] }>("library_import", { items }),
  syncNow: () => invoke<void>("sync_now"),
  removeGame: (id: number, deleteFiles: boolean) => invoke<void>("library_remove", { id, deleteFiles }),

  f95Status: (force = false) => invoke<F95Status | null>("f95_status", { force }),
  f95Open: (url?: string) => invoke<void>("f95_open", { url: url ?? null }),
  links: (threadId: number, version: string, force = false) => invoke<ThreadLinks>("links_get", { threadId, version, force }),
  preflight: (hosts: string[]) => invoke<Preflight>("download_preflight", { hosts }),
  startDownload: (game: GameCtx, link: DownloadLink, confirm?: string) =>
    invoke<{ job_id: number | null; opened_browser: boolean }>("download_start", { game, link, confirm: confirm ?? null }),
  downloads: () => invoke<DownloadSnapshot>("downloads_list"),
  pauseDownload: (id: number) => invoke<void>("download_pause", { id }),
  resumeDownload: (id: number) => invoke<void>("download_resume", { id }),
  removeDownload: (id: number) => invoke<void>("download_remove", { id }),
  clearFinished: () => invoke<void>("downloads_clear_finished"),
  openDownloadFolder: (id: number) => invoke<void>("download_open_folder", { id }),
  openDownloadInBrowser: (id: number) => invoke<void>("download_open_in_browser", { id }),

  hostAccounts: () => invoke<HostAccounts>("host_accounts"),
  setPixeldrainKey: (key: string) => invoke<HostAccounts>("pixeldrain_set_key", { key }),
  clearPixeldrain: () => invoke<HostAccounts>("pixeldrain_clear"),
  megaSignIn: (email: string, password: string, mfa?: string) =>
    invoke<HostAccounts>("mega_sign_in", { email, password, mfa: mfa ?? null }),
  megaSignOut: () => invoke<HostAccounts>("mega_sign_out"),
  setGofileToken: (token: string) => invoke<HostAccounts>("gofile_set_token", { token }),
  clearGofile: () => invoke<HostAccounts>("gofile_clear"),

  systemInfo: () => invoke<SystemInfo>("system_info"),
  compatStatus: () => invoke<{ runner: { kind: "umu" | "wine" | "cross_over" | "custom"; program: string } | null }>("compat_status"),
  autostartSet: (enabled: boolean) => invoke<boolean>("autostart_set", { enabled }),
  bossKeySet: (shortcut: string | null) => invoke<void>("boss_key_set", { shortcut }),
  pinSet: (current: string | null, pin: string) => invoke<void>("pin_set", { current, pin }),
  pinClear: (current: string) => invoke<void>("pin_clear", { current }),
  pinUnlock: (pin: string) => invoke<boolean>("pin_unlock", { pin }),
  lockNow: () => invoke<void>("lock_app"),
  deepLinkTake: () => invoke<DeepLink | null>("deep_link_take"),
  updateCheck: () => invoke<{ version: string; notes: string | null } | null>("update_check"),
  updateInstall: () => invoke<void>("update_install"),
  updateStatus: () => invoke<{ version: string; notes: string | null } | null>("update_status"),
  f95checkerDefaultPath: () => invoke<string | null>("f95checker_default_path"),
  importF95checker: (path: string) => invoke<F95CheckerImport>("f95checker_import", { path }),
  backupExport: (path: string) => invoke<number>("backup_export", { path }),
  backupImport: (path: string) => invoke<{ games: number; sessions_added: number; added_to_library: number }>("backup_import", { path }),

  jdownloaderStatus: () => invoke<boolean>("jdownloader_status"),
  sendToJDownloader: (game: GameCtx, link: DownloadLink, confirm?: string) =>
    invoke<void>("jdownloader_send", { game, link, confirm: confirm ?? null }),
};
