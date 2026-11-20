/**
 * Dev-only stand-in for the Rust commands, used when the frontend runs in a
 * plain browser (`npm run dev`) instead of inside Tauri. Lets you review the
 * UI without an API key. Never bundled into production builds.
 */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

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
      "You return to a small seaside town after years away, only to find that nothing is quite how you left it. Build relationships, uncover the town's secrets, and decide who you want to become.\n\nFeatures a branching story, multiple endings and fully animated scenes.",
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
};
let signedIn = false;
const library = new Map<number, { has_update: boolean; last_seen_version: string }>([
  [100002, { has_update: true, last_seen_version: "v0.2.1" }],
  [100005, { has_update: true, last_seen_version: "v0.5.0" }],
  [100009, { has_update: false, last_seen_version: "" }],
  [100014, { has_update: false, last_seen_version: "" }],
]);

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

export function installMockIpc() {
  mockWindows("main");
  mockIPC(async (cmd, raw) => {
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
        return { query: q, hits: games.filter((g) => g.name.toLowerCase().includes(q)).slice(0, 20).map((g) => ({ thread_id: g.id, title: g.name, creator: g.authors[0].name, url: g.url, category: "games", score: 1, cover: g.cover })) };
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
          changelog: `v${g.version}\n- Added 3 new scenes\n- Fixed a save bug on the beach map\n- Improved translations\n\nv0.1.0\n- Initial release`,
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
      case "plugin:dialog|open":
        return "D:\\Games";
      case "plugin:opener|open_url":
        return null;
      default:
        console.warn("[mock ipc] unhandled", cmd, args);
        return null;
    }
  });
}
