/**
 * Dev-only stand-in for the Rust commands, used when the frontend runs in a
 * plain browser (`npm run dev`) instead of inside Tauri. Lets you review the
 * UI without an API key. Never bundled into production builds.
 */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";

const palette = ["#8b7cf6", "#f472a6", "#2dd4bf", "#f5b544", "#60a5fa", "#f27474"];
const cover = (i: number, title: string) => {
  const a = palette[i % palette.length];
  const b = palette[(i + 2) % palette.length];
  const svg = `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 640 360'><defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'><stop offset='0' stop-color='${a}'/><stop offset='1' stop-color='${b}' stop-opacity='.55'/></linearGradient></defs><rect width='640' height='360' fill='#14171d'/><rect width='640' height='360' fill='url(#g)' opacity='.85'/><text x='32' y='320' font-family='Segoe UI,sans-serif' font-size='40' font-weight='700' fill='white' opacity='.9'>${title.replace(/[<&'"]/g, "")}</text></svg>`;
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`;
};

const words = ["Summer", "Night", "Academy", "Echoes", "Crimson", "Haven", "Lust", "Legacy", "Starlight", "Hollow", "Desire", "Island", "Code", "Fate", "Secret", "Garden"];
const engines = ["Ren'Py", "Unity", "RPGM", "HTML", "Unreal Engine"];
const statuses = [[], [], [], ["Completed"], ["Onhold"], ["Abandoned"]];
const now = Math.floor(Date.now() / 1000);

const games = Array.from({ length: 90 }, (_, i) => {
  const title = `${words[i % words.length]} ${words[(i * 7 + 3) % words.length]}${i % 4 === 0 ? ": Reborn" : ""}`;
  return {
    id: 100000 + i,
    name: title,
    overview:
      "You return to a [B]small seaside town[/B] after years away, only to find that nothing is quite how you left it. Build relationships, uncover the town's secrets, and decide who you want to become.\n\n[CENTER][SIZE=5]Features[/SIZE][/CENTER]\n[LIST]\n[*]A branching story with [U]multiple endings[/U]\n[*]Fully animated scenes\n[/LIST]\n[QUOTE=Developer]Thanks for playing &amp; supporting the game![/QUOTE]",
    authors: [{ name: ["Moonlit Studio", "PixelHeart", "Velvet Dev", "Nightcap Games"][i % 4] }],
    category: "games",
    cover: cover(i, title),
    banner: cover(i + 1, title),
    previews: [cover(i + 3, "Screenshot 1"), cover(i + 4, "Screenshot 2"), cover(i + 5, "Screenshot 3")],
    last_thread_update: null,
    last_promoted: now - i * 3600 * 5,
    prefixes: [engines[i % engines.length], ...(statuses[i % statuses.length] as string[])],
    rating: { average: 3 + ((i * 37) % 20) / 10, best: 5, count: 100 + i },
    tags: ["3dcg", "male protagonist", "romance", "sandbox", "animated"].slice(0, 2 + (i % 4)),
    url: `https://f95zone.to/threads/${100000 + i}/`,
    engine: engines[i % engines.length],
    status: null,
    version: `0.${(i % 12) + 1}.${i % 3}`,
  };
});

const toSummary = (g: (typeof games)[number]) => ({
  thread_id: g.id,
  url: g.url,
  title: g.name,
  creator: g.authors[0].name,
  version: `v${g.version}`,
  developer: g.authors[0].name,
  category: "games",
  last_promoted: g.last_promoted,
  first_post_ts: g.last_promoted,
  is_new: g.id % 11 === 0,
  prefixes: g.prefixes,
  cover: g.cover,
  banner: g.banner,
  previews: g.previews,
});

let settings = {
  onboarding_complete: false,
  auth_mode: null as string | null,
  username: null as string | null,
  library_dir: null as string | null,
  theme: "dark",
  accent: "violet",
  ui_scale: 1,
  reduce_motion: false,
  blur_nsfw: false,
  close_to_tray: true,
  notify_updates: true,
  max_active_downloads: 2,
  delete_archives: true,
  pixeldrain_user: null as string | null,
  mega_email: null as string | null,
  gofile_user: null as string | null,
};
let signedIn = false;
const library = new Map<number, { has_update: boolean; last_seen_version: string }>([
  [100002, { has_update: true, last_seen_version: "v0.2.1" }],
  [100005, { has_update: true, last_seen_version: "v0.5.0" }],
  [100009, { has_update: false, last_seen_version: "" }],
  [100014, { has_update: false, last_seen_version: "" }],
]);

// ---- local library (installed games, playtime, notes)
type Target = { id: number; path: string; label: string; args: string; is_default: boolean };
type Local = {
  play_status: string;
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
  targets: Target[];
  sessions: { started_at: number; secs: number; version: string | null }[];
};
let nextTarget = 1;
const local = new Map<number, Local>();
const blankLocal = (): Local => ({
  play_status: "not_started",
  rating: null,
  notes: "",
  labels: [],
  installed_version: null,
  finished_version: null,
  last_played_version: null,
  skipped_version: null,
  install_dir: null,
  added_at: now,
  last_played_at: null,
  playtime_secs: 0,
  targets: [],
  sessions: [],
});
const ensure = (id: number) => {
  if (!local.has(id)) local.set(id, blankLocal());
  return local.get(id)!;
};
const addTarget = (id: number, path: string) => {
  const l = ensure(id);
  if (l.targets.some((t) => t.path === path)) return;
  const label = path.split(/[\\/]/).pop()!.replace(/\.exe$/i, "");
  l.targets.push({ id: nextTarget++, path, label: label === "Game" ? "Play" : label, args: "", is_default: l.targets.length === 0 });
  l.install_dir ??= path.replace(/[\\/][^\\/]+$/, "");
};
// Seed: two installed games with history.
{
  const a = ensure(100002);
  addTarget(100002, "D:\\Games\\Night Academy\\NightAcademy.exe");
  Object.assign(a, {
    installed_version: "v0.2.1",
    play_status: "playing",
    playtime_secs: 4 * 3600 + 12 * 60,
    last_played_at: now - 7200,
    rating: 4,
    labels: ["Favourite"],
    last_played_version: "v0.2.1",
  });
  a.sessions = [
    { started_at: now - 9000, secs: 1800, version: "v0.2.1" },
    { started_at: now - 90000, secs: 3600 * 2, version: "v0.2.0" },
  ];
  const b = ensure(100009);
  addTarget(100009, "D:\\Games\\Fate Garden\\Game.exe");
  Object.assign(b, {
    installed_version: "v0.10.0",
    play_status: "finished",
    playtime_secs: 9 * 3600,
    last_played_at: now - 86400 * 3,
    finished_version: "v0.10.0",
    labels: ["Favourite", "Replay later"],
  });
}
const running = new Set<number>();
let scanCancelled = false;

const localGame = (id: number) => {
  const l = local.get(id);
  if (!l) return null;
  const g = games.find((x) => x.id === id);
  const { sessions: _sessions, ...rest } = l;
  return {
    thread_id: id,
    in_library: library.has(id),
    ...rest,
    cached: g
      ? { title: g.name, creator: g.authors[0].name, version: `v${g.version}`, cover: g.cover, url: g.url, prefixes: g.prefixes, last_promoted: g.last_promoted }
      : null,
  };
};

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const authStatus = () => ({ signed_in: signedIn, mode: signedIn ? settings.auth_mode : null, username: signedIn ? settings.username : null });

const bookmark = (id: number) => {
  const g = games.find((x) => x.id === id)!;
  const b = library.get(id)!;
  return {
    thread_id: id,
    has_update: b.has_update,
    flagged_at: null,
    last_seen_version: b.last_seen_version,
    current_version: `v${g.version}`,
    needs_manual_review: false,
    review_reason: null,
    thread: toSummary(g),
  };
};

// ---- downloads
let f95LoggedIn = false;
let nextJob = 1;
let daily = 3;
type Job = Record<string, any>;
const jobs: Job[] = [];
const jobsChanged = () => void emit("downloads-changed", null);
const linkFixture = (id: number) => [
  {
    label: "Win/Linux",
    os: ["windows", "linux"],
    links: [
      { host: "mega.nz", url: `https://f95zone.to/masked/mega.nz/1/${id}`, label: "MEGA", masked: true },
      { host: "mega.nz", url: `https://mega.nz/file/Ab${id}#k3y${id}`, label: "MEGA (unmasked)", masked: false },
      { host: "pixeldrain.com", url: `https://pixeldrain.com/u/pd${id}`, label: "PIXELDRAIN", masked: false },
      { host: "gofile.io", url: `https://gofile.io/d/go${id}`, label: "GOFILE", masked: false },
      { host: "datanodes.to", url: `https://datanodes.to/dn${id}/Game-${id}-pc.zip`, label: "DATANODES", masked: false },
    ],
  },
  { label: "Mac", os: ["mac"], links: [{ host: "mega.nz", url: `https://f95zone.to/masked/mega.nz/2/${id}`, label: "MEGA", masked: true }] },
  { label: "Android", os: ["android"], links: [{ host: "pixeldrain.com", url: `https://pixeldrain.com/u/and${id}`, label: "PIXELDRAIN", masked: false }] },
];
const runJob = (job: Job) => {
  const total = 734_003_200;
  const step = () => {
    if (!jobs.includes(job) || job.state === "paused") return;
    if (job.state === "queued") {
      job.state = "downloading";
      job.starts_at = null;
      job.bytes_total = total;
      daily++;
    } else if (job.state === "downloading") {
      job.bytes_done = Math.min(total, job.bytes_done + total / 8);
      if (job.bytes_done >= total) job.state = "extracting";
    } else if (job.state === "extracting") job.state = "installing";
    else if (job.state === "installing") {
      job.state = "done";
      job.finished_at = Math.floor(Date.now() / 1000);
      job.install_dir = `D:\Games\${job.creator}\${job.title}\${job.version.replace(/^v/, "")}`;
      addTarget(job.thread_id, `${job.install_dir}\Game.exe`);
      ensure(job.thread_id).install_dir = job.install_dir;
      ensure(job.thread_id).installed_version = job.version;
      library.set(job.thread_id, library.get(job.thread_id) ?? { has_update: false, last_seen_version: "" });
      void emit("game-installed", job.thread_id);
      jobsChanged();
      return;
    }
    jobsChanged();
    setTimeout(step, 600);
  };
  setTimeout(step, job.starts_at ? Math.max(600, job.starts_at * 1000 - Date.now()) : 900);
};

const scanFixtures = () => [
  { name: "SecretGarden-0.4.1-pc", title: "Secret Garden", version: "0.4.1", match: games[14], confident: true, linked: false },
  { name: "Crimson_Haven_v0.12", title: "Crimson Haven", version: "0.12", match: games[20], confident: true, linked: false },
  { name: "EchoesLegacy-1.0-pc", title: "Echoes Legacy", version: "1.0", match: games[3], confident: false, linked: false },
  { name: "MysteryThing", title: "Mystery Thing", version: null, match: null, confident: false, linked: false },
  { name: "Night Academy", title: "Night Academy", version: null, match: games[2], confident: true, linked: true },
];

export function installMockIpc() {
  mockWindows("main");
  mockIPC(
    async (cmd, raw) => {
      const args = (raw ?? {}) as Record<string, any>;
      await delay(cmd.startsWith("catalog") ? 450 : 120);
      switch (cmd) {
        case "app_info":
          return { version: "0.1.0-dev", sso_configured: false, api_keys_url: "https://f95api.xyz/panel/keys" };
        case "settings_get":
          return settings;
        case "settings_update":
          settings = { ...settings, ...Object.fromEntries(Object.entries(args.patch).filter(([, v]) => v !== undefined)) };
          return settings;
        case "auth_status":
          return authStatus();
        case "auth_use_api_key":
          await delay(600);
          if (String(args.key).trim().length < 16) throw { kind: "invalid", message: "That doesn't look like an API key. Copy the full key from the panel." };
          if (String(args.key).startsWith("bad")) throw { kind: "unauthorized", message: "Invalid API key" };
          signedIn = true;
          settings = { ...settings, auth_mode: "api_key", username: null };
          return authStatus();
        case "auth_sso_sign_in":
          throw { kind: "invalid", message: "SSO not configured in mock" };
        case "auth_sign_out":
          signedIn = false;
          settings = { ...settings, auth_mode: null };
          return authStatus();
        case "catalog_latest": {
          const q = args.query ?? {};
          const list = [...games];
          if (q.order === "rating") list.sort((a, b) => (b.rating.average ?? 0) - (a.rating.average ?? 0));
          const limit = q.limit ?? 30;
          const page = q.page ?? 1;
          return list.slice((page - 1) * limit, page * limit);
        }
        case "catalog_search": {
          const q = String(args.q).toLowerCase();
          return {
            query: q,
            hits: games
              .filter((g) => g.name.toLowerCase().includes(q))
              .slice(0, 20)
              .map((g) => ({ thread_id: g.id, title: g.name, creator: g.authors[0].name, url: g.url, category: "games", score: 1, cover: g.cover })),
          };
        }
        case "catalog_thread": {
          const g = games.find((x) => x.id === args.id);
          if (!g) throw { kind: "not_found" };
          return {
            ...toSummary(g),
            overview: g.overview,
            views: 120000 + g.id,
            likes: 3400,
            genre: null,
            changelog:
              `[B]v${g.version}[/B]\n[LIST]\n[*]Added [COLOR=#f5b544]3 new scenes[/COLOR] with [I]Emily[/I]\n[*]Fixed a save bug on the beach map\n[*][S]Removed[/S] Reworked the old gallery\n[/LIST]\n` +
              `[SPOILER=Older versions]\n[B]v0.1.0[/B]\n[LIST]\n[*]Initial release\n[/LIST]\n[/SPOILER]\n` +
              `Full notes on [URL=https://example.com/devlog]the devlog[/URL] or https://example.com/patch. [v0.2] tags and [Ren'Py] stay as typed.`,
            thread_updated: "2026-11-10",
            release_date: "2025-03-02",
            censored: "No",
            os: "Windows, Linux, Mac",
            language: "English",
            sam_rating: g.rating.average,
            tag_names: g.tags,
            external_links: [{ role: "developer", host: "patreon.com", url: "https://patreon.com", label: "Patreon" }],
          };
        }
        case "bookmarks_all":
          return { total: library.size, items: [...library.keys()].map(bookmark) };
        case "bookmarks_list": {
          const ids = [...library.keys()].filter((id) => !args.updatedOnly || library.get(id)!.has_update);
          return { total: ids.length, items: ids.map(bookmark) };
        }
        case "bookmarks_add":
          library.set(args.id, { has_update: false, last_seen_version: "" });
          return bookmark(args.id);
        case "bookmarks_remove":
          library.delete(args.id);
          return null;
        case "bookmarks_ack":
          library.get(args.id)!.has_update = false;
          return bookmark(args.id);
        case "bookmarks_ack_all":
          library.forEach((b) => (b.has_update = false));
          return { acknowledged: library.size };

        // ---- local library
        case "library_local":
          return [...local.keys()].filter((id) => library.has(id) || local.get(id)!.targets.length).map(localGame);
        case "game_local":
          return localGame(args.id);
        case "game_sessions":
          return local.get(args.id)?.sessions ?? [];
        case "game_update": {
          const l = ensure(args.id) as Record<string, any>;
          for (const [k, v] of Object.entries(args.patch as Record<string, unknown>)) {
            if (v === undefined) continue;
            if (k === "rating") l.rating = (v as number) || null;
            else if (typeof v === "string" && k !== "notes" && k !== "play_status") l[k] = v.trim() || null;
            else l[k] = v;
          }
          return localGame(args.id);
        }
        case "target_add":
          addTarget(args.id, args.path);
          return localGame(args.id);
        case "target_remove":
        case "target_set_default": {
          for (const [id, l] of local) {
            const t = l.targets.find((x) => x.id === args.targetId);
            if (!t) continue;
            if (cmd === "target_remove") {
              l.targets = l.targets.filter((x) => x !== t);
              if (t.is_default && l.targets[0]) l.targets[0].is_default = true;
            } else l.targets.forEach((x) => (x.is_default = x === t));
            return localGame(id);
          }
          return null;
        }
        case "folder_detect":
          return [{ path: `${args.dir}\\Game.exe`, label: "Play", engine: "ren_py" }];
        case "game_link_folder":
          addTarget(args.id, `${args.dir}\\Game.exe`);
          ensure(args.id).install_dir = args.dir;
          if (args.version) ensure(args.id).installed_version = args.version;
          return localGame(args.id);
        case "games_running":
          return [...running];
        case "game_play": {
          const l = local.get(args.id);
          if (!l?.targets.length) throw { kind: "invalid", message: "Add the game's executable first." };
          if (running.has(args.id)) throw { kind: "invalid", message: "This game is already running." };
          running.add(args.id);
          const started = Math.floor(Date.now() / 1000);
          void emit("game-running", { thread_id: args.id, running: true, secs: null });
          setTimeout(() => {
            running.delete(args.id);
            const secs = 95;
            l.playtime_secs += secs;
            l.last_played_at = started + secs;
            l.last_played_version = l.installed_version;
            l.sessions.unshift({ started_at: started, secs, version: l.installed_version });
            if (l.play_status === "not_started") l.play_status = "playing";
            void emit("game-running", { thread_id: args.id, running: false, secs });
          }, 6000);
          return null;
        }
        case "game_open_folder":
          return null;
        case "library_scan": {
          scanCancelled = false;
          const found = scanFixtures();
          await emit("scan-progress", { phase: "finding", done: 0, total: 0 });
          await delay(700);
          for (let i = 0; i < 4 && !scanCancelled; i++) {
            await emit("scan-progress", { phase: "matching", done: i, total: 4 });
            await delay(450);
          }
          await emit("scan-progress", { phase: "done", done: 4, total: 4 });
          return found.map((f) => ({
            path: `D:\\Games\\${f.name}`,
            folder_name: f.name,
            title_guess: f.title,
            version_guess: f.version,
            engine: "ren_py",
            targets: [{ path: `D:\\Games\\${f.name}\\Game.exe`, label: "Play", engine: "ren_py" }],
            linked_thread_id: f.linked || (f.match && local.get(f.match.id)?.install_dir && !library.has(f.match.id)) ? f.match!.id : null,
          linked_in_library: !!f.linked && library.has(f.match!.id),
            suggestion:
              f.match && !(f.linked && library.has(f.match.id))
                ? { thread_id: f.match.id, title: f.match.name, creator: f.match.authors[0].name, cover: f.match.cover, confidence: f.confident ? 0.95 : 0.6, confident: f.confident }
                : null,
          }));
        }
        case "library_scan_cancel":
          scanCancelled = true;
          return null;
        case "library_import":
          for (const it of args.items) {
            addTarget(it.thread_id, `${it.path}\\Game.exe`);
            ensure(it.thread_id).installed_version = it.version;
            library.set(it.thread_id, { has_update: false, last_seen_version: "" });
          }
          return { linked: args.items.length, added_to_library: args.items.length, failed: [] };
        case "sync_now":
          return null;
        case "library_remove": {
          library.delete(args.id);
          const l = local.get(args.id);
          if (l) Object.assign(l, { targets: [], install_dir: null, installed_version: null });
          return null;
        }

        case "f95_status":
          return { checked_at: Math.floor(Date.now() / 1000), logged_in: f95LoggedIn, username: f95LoggedIn ? "Tester" : null };
        case "f95_open":
          f95LoggedIn = true;
          void emit("f95-status", { checked_at: Math.floor(Date.now() / 1000), logged_in: true, username: "Tester" });
          return null;
        case "links_get":
          await delay(1200);
          if (!f95LoggedIn) throw { kind: "needs_browser", reason: "login", url: "https://f95zone.to/login/" };
          return {
            thread_id: args.threadId,
            version: args.version,
            fetched_at: Math.floor(Date.now() / 1000),
            from_cache: false,
            groups: linkFixture(args.threadId).map((g) => ({
              ...g,
              links: g.links.map((l) => ({
                ...l,
                in_app: !l.masked && (l.host === "pixeldrain.com" || l.host === "mega.nz" || (l.host === "gofile.io" && !!settings.gofile_user)),
              })),
            })),
          };
        case "download_preflight": {
          const queued = jobs.filter((j) => j.state === "queued").length;
          const adding = args.hosts.length;
          return { queued, adding, daily, warn: queued + adding >= 5, needs_confirm: daily + adding > 15, eta_secs: queued * 45 };
        }
        case "download_start": {
          if (daily + 1 > 15 && String(args.confirm ?? "").trim().toLowerCase() !== "download anyway") throw { kind: "confirm_required", daily, limit: 15 };
          const link = args.link;
          const inApp = !link.masked && (link.host === "pixeldrain.com" || link.host === "mega.nz" || (link.host === "gofile.io" && !!settings.gofile_user));
          if (!inApp) return { job_id: null, opened_browser: true };
          const busy = jobs.some((j) => j.host === link.host && ["downloading", "queued"].includes(j.state));
          const job: Job = {
            id: nextJob++, thread_id: args.game.thread_id, title: args.game.title, creator: args.game.creator, version: args.game.version,
            source: "direct", host: link.host, page_url: link.url, file_name: `${args.game.title}.zip`, path: null, state: "queued",
            bytes_done: 0, bytes_total: null, error: null, attempts: 0, retry_at: null, install_dir: null,
            created_at: Math.floor(Date.now() / 1000), finished_at: null, starts_at: busy ? Math.floor(Date.now() / 1000) + 42 : null,
          };
          jobs.unshift(job);
          runJob(job);
          jobsChanged();
          return { job_id: job.id, opened_browser: false };
        }
        case "downloads_list":
          return { jobs: [...jobs], daily, daily_limit: 15, paused_for_game: running.size > 0, max_active: settings.max_active_downloads };
        case "download_pause": {
          const j = jobs.find((x) => x.id === args.id);
          if (j) j.state = "paused";
          jobsChanged();
          return null;
        }
        case "download_resume": {
          const j = jobs.find((x) => x.id === args.id);
          if (j) {
            j.state = "queued";
            j.error = null;
            runJob(j);
          }
          jobsChanged();
          return null;
        }
        case "download_remove": {
          const i = jobs.findIndex((x) => x.id === args.id);
          if (i >= 0) jobs.splice(i, 1);
          jobsChanged();
          return null;
        }
        case "downloads_clear_finished":
          for (let i = jobs.length - 1; i >= 0; i--) if (jobs[i].state === "done") jobs.splice(i, 1);
          jobsChanged();
          return null;
        case "download_open_folder":
        case "download_open_in_browser":
          return null;

        case "host_accounts":
          return { pixeldrain: settings.pixeldrain_user, mega: settings.mega_email, gofile: settings.gofile_user };
        case "gofile_set_token":
          await delay(500);
          if (String(args.token).startsWith("free")) throw { kind: "invalid", message: "This Gofile account isn't Premium. Gofile only lets apps download through its API with a Premium account; without one, Gofile links open in the F95 window." };
          settings.gofile_user = "me@example.com";
          return { pixeldrain: settings.pixeldrain_user, mega: settings.mega_email, gofile: settings.gofile_user };
        case "gofile_clear":
          settings.gofile_user = null;
          return { pixeldrain: settings.pixeldrain_user, mega: settings.mega_email, gofile: null };
        case "jdownloader_status":
          return true;
        case "jdownloader_send":
          await delay(300);
          daily++;
          return null;
        case "pixeldrain_set_key":
          await delay(500);
          if (String(args.key).length < 8) throw { kind: "invalid", message: "That doesn't look like a Pixeldrain API key." };
          settings.pixeldrain_user = "tester";
          return { pixeldrain: settings.pixeldrain_user, mega: settings.mega_email, gofile: settings.gofile_user };
        case "pixeldrain_clear":
          settings.pixeldrain_user = null;
          return { pixeldrain: null, mega: settings.mega_email, gofile: settings.gofile_user };
        case "mega_sign_in":
          await delay(700);
          if (!args.mfa) throw { kind: "invalid", message: "mfa_required" };
          settings.mega_email = String(args.email).toLowerCase();
          return { pixeldrain: settings.pixeldrain_user, mega: settings.mega_email, gofile: settings.gofile_user };
        case "mega_sign_out":
          settings.mega_email = null;
          return { pixeldrain: settings.pixeldrain_user, mega: null, gofile: settings.gofile_user };

        case "plugin:dialog|open":
          return args.options?.directory ? "D:\\Games" : "D:\\Games\\Some Game\\Some Game.exe";
        case "plugin:opener|open_url":
          return null;
        default:
          console.warn("[mock ipc] unhandled", cmd, args);
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
