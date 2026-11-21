# F95 Game Manager

The official desktop launcher for the [f95api.xyz](https://f95api.xyz) catalog. It's a game library, an update tracker and (soon) a downloader in one app, built with **Tauri 2 (Rust) + React + TypeScript + Tailwind**.

It brings together the best ideas from F95Checker, wLib and XLibrary:
- an easy-to-read dark UI with a light theme
- one primary action per game
- the Ctrl+K command palette
- server-side update detection, including stealth updates
- downloads paced so users don't get banned

## Platforms

| | Windows | Linux / Steam Deck | macOS |
| --- | --- | --- | --- |
| Installer | `.msi`, `.exe` (NSIS) | `.AppImage`, `.deb`, `.rpm` | `.dmg` (universal) |
| Windows games | run directly | Proton via [umu-launcher](https://github.com/Open-Wine-Components/umu-launcher), else Wine | CrossOver, else Wine |
| Native games | — | `.sh`, `.x86_64`, AppImage, Linux binaries | `.app` bundles |
| Secrets stored in | Credential Manager | Secret Service (KWallet/GNOME Keyring), else a 0600 file | Keychain |

On Steam Deck, install the AppImage in Desktop Mode and add it to Steam as a non-Steam game if you want it in Game Mode. Game Mode has no keyring, so sign-in details go to a file only your user can read. Windows games run through umu-launcher, and the app downloads GE-Proton the first time if you haven't chosen a Proton build yourself. You can pick a different program under Settings → Windows games.

## Quick start

```bash
npm install
npm run tauri dev      # the real app (needs Rust + MSVC build tools + WebView2)
npm run dev            # UI only, in a browser, with a fake backend (src/dev/mockIpc.ts)
```

Tests and checks:

```bash
npm test               # vitest
npm run typecheck      # tsc
cd src-tauri && cargo test && cargo clippy --all-targets
```

## How it fits together

```
React UI ──invoke()──▶ Rust commands ──▶ f95api.xyz          (catalog, bookmarks, update inbox)
   │                        │
   └─ gmimg:// images ──────┴──▶ F95 image hosts                (allow-listed, 2 req/s, disk cache)
```

- **The webview never holds credentials.** All network traffic goes through Rust (`src-tauri/src/api/client.rs`). There is one shared rate limiter (`src-tauri/src/limiter.rs`) set to 90 requests per minute, below the API's limit of 120. After a 429 there is a cooldown, and requests fail fast during it instead of piling up.
- **Your library lives on the server as bookmarks.** The server checks them every 30 minutes, including stealth updates. `GET /v1/bookmarks?updated_only=true` is the Updates inbox, and "Mark seen" calls `/ack`.
- **Your copy of each game lives in a local SQLite database** (`library.db` in the app data folder). It stores the install folder, launch targets, playtime, notes, labels, rating and play status, plus a cached copy of the catalog fields so the library opens offline. The database is copied aside before any schema migration, and the last three copies are kept.
- **Background update checks** read the small `updated_only` inbox every 30 minutes in the limiter's background lane. That's about two requests an hour, and each new version triggers one Windows notification.
- **Download links never come from the API.** They're for approved partners only. The launcher reads them from the thread's first post on the user's machine, with the user's own F95 sign-in in a built-in browser window:
  - A hidden window loads the thread and a parser (`src-tauri/src/f95/op_parser.js`) reads the DOWNLOAD section. Only one thread is read at a time, about 5 s apart. Pages are only read when the user presses Download, and links are cached per version.
  - F95 windows aren't in any capability, so F95 pages can't call the app. The launcher never sees the F95 password.
  - Masked links, captchas, Cloudflare checks and hosts without a public API open in the visible F95 window, for the user to click through. Whatever they download there is picked up, installed and attributed to the game.
- **Downloads** (`src-tauri/src/downloads/`):
  - The defaults are 2 at a time, 1 per host, and 30-60 s between starts on a host.
  - A host that returns 429 or 403 is left alone for 15 min, then 30, 60 and 120.
  - Downloads stop while a game runs and resume from `.part` files.
  - Queueing 5 or more shows a warning. More than 15 starts in 24 hours needs the user to type "download anyway". These limits are enforced in Rust.
  - Hosts the app downloads itself, using only documented APIs:
    - Pixeldrain, with your own API key if you add one.
    - MEGA, decrypted locally and signed in optionally (only the session is stored).
    - Gofile, only with your own Premium token. Gofile's API is Premium-only.
    - Direct archive links.
  - Every other approved host opens in the F95 window, where you press the host's own download button and the app picks up the file. As of November 2026, none of them documented a way for apps to download someone else's file.
  - Optional **Send to JDownloader** hands a link to your own running JDownloader through its Click'n'Load listener (`127.0.0.1:9666/flash/add`).
- **Install** (`src-tauri/src/install.rs`):
  - zip, 7z and rar are unpacked with Rust libraries, so no 7-Zip program ships.
  - Archives are unpacked into a staging folder, and entries that would land outside it (`..`, absolute paths, symlinks) are skipped.
  - The game is moved to `{games folder}/{creator}/{title}/{version}`.
  - Ren'Py and RPG Maker saves are copied from the previous version, and the old launch targets are swapped for the new ones. The old version's folder is kept.

| Path | What it does |
| --- | --- |
| `src-tauri/src/config.rs` | Public build-time settings: API base, OIDC issuer and client id, rate budgets |
| `src-tauri/src/limiter.rs` | Sliding-window limiter with priorities, jitter and the 429 circuit (unit-tested) |
| `src-tauri/src/api/` | Typed API client and models, mapped to friendly errors |
| `src-tauri/src/auth/` | API key or Authentik SSO (PKCE + loopback). Secrets are kept in the OS keyring |
| `src-tauri/src/images.rs` | Cover cache behind the `gmimg://` scheme |
| `src-tauri/src/db/` | Local library database and migrations |
| `src-tauri/src/library/` | Finds the executable in a game folder, parses folder names, scans for installed games and matches them to the catalog |
| `src-tauri/src/launch.rs` | Starts games and tracks playtime across the game folder's processes |
| `src-tauri/src/sync.rs`, `tray.rs`, `notify.rs` | 30-minute update check, tray menu (recently played, check now, quit), notifications |
| `src/app/` | Shell, sidebar, Ctrl+K palette, router, onboarding gate |
| `src/features/*` | Home, Library, Discover, Updates, Downloads, Settings, Game page, Onboarding |
| `src/styles/index.css` | Design tokens: every colour, both themes, and the accent presets |

## Secrets: what may and may not ship

The installer is public. Anyone can unpack it, so **it must contain no secrets**.

- **Safe to ship** (public values in `config.rs`):
  - the API base URL
  - the OIDC issuer URL
  - the OAuth **client id**
  - the updater **public** key
- **Never ship or commit any of these:**
  - an OAuth client secret (the launcher is a *public* client, so it doesn't have one)
  - `X-Worker-Token` / `/internal/*` credentials
  - Authentik admin tokens
  - the server's F95 cookies
  - the updater private key
  - the code-signing certificate
  The last two belong only in CI secrets.
- Each user's API key or SSO refresh token is stored in **Windows Credential Manager** (`keyring`). It never goes into files or the webview.

## Turning on "Sign in with F95" (Authentik)

SSO stays hidden until a client id is compiled in. To set it up:

1. In Authentik at `auth.zonies.xyz`, create an **OAuth2/OpenID Provider**:
   - Client type: **Public**. It has no secret.
   - Redirect URIs (regex): `^http://127\.0\.0\.1:\d+/callback$`
   - Scopes: `openid profile offline_access`
   - Access token about 10 min. Refresh token about 30 days, with rotation turned on.
   - Bind it to an Application named `f95gamemanager` with a policy that only lets approved users in.
2. Have f95api.xyz accept `Authorization: Bearer <JWT>`. It should verify the token with Authentik's JWKS (`iss`, `aud = client_id`, `exp`), map `sub` to the user, and apply the same per-user limits. Downloads stay off for these tokens.
3. Build with the public values:
   ```bash
   F95GM_OIDC_CLIENT_ID=<client id> F95GM_OIDC_ISSUER=https://auth.zonies.xyz/application/o/f95gamemanager/ npm run tauri build
   ```

## Releases: updater and code signing

GitHub Actions does the work:
- `.github/workflows/ci.yml` runs the type check, frontend tests, `cargo clippy` and `cargo test` on Windows, Linux and macOS for every push and pull request.
- `.github/workflows/release.yml` runs when you push a tag like `v0.2.0`. It builds installers for all three systems into a **draft** GitHub release, which you publish yourself.

Signing needs secrets, so they live only in the repository settings and are never committed.

1. **Updater keys:** run `npm run tauri signer generate -- -w ~/.tauri/f95gm.key` once.
   - Keep the **private** key and its password in CI secrets, as `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
   - The **public** key is safe to ship.
2. **Turn on signed updates:** in the GitHub repository settings, add:
   - the secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
   - the variable `F95GM_UPDATER_PUBKEY`

   Release builds then make signed update files plus `latest.json`, and the app checks this repository's latest release.

   To build by hand instead:
   ```bash
   F95GM_UPDATER_PUBKEY="<public key>" F95GM_UPDATE_URL="https://f95api.xyz/launcher/latest.json" npm run tauri build -- --config '{"bundle":{"createUpdaterArtifacts":true}}'
   ```
   Then host the generated `latest.json` and the signed installer at that URL. Builds without these values never check for updates, and Settings says so.
3. **Code signing:** sign the installer in CI, for example with Azure Trusted Signing, so Windows SmartScreen doesn't warn. The certificate never goes in the repo.

## Links and the browser extension

- `f95gm://add/<thread id>` adds a game and opens it. `f95gm://game/<thread id>` opens it.
- The installer registers the scheme, and `npm run tauri dev` registers it for the dev build.
- `extension/` holds a tiny MV3 extension for Chrome, Edge and Firefox. It puts an "Add to F95 Game Manager" button on F95 threads using those links, with no permissions and no network requests. See `extension/README.md`.

## Roadmap

- [x] **Phase 0:** shell and design system, onboarding, API key + SSO sign-in, API client and limiter, Home / Discover / Library / Updates / Game / Settings
- [x] **Phase 1:** local SQLite library (installed, finished and played versions, notes, labels, rating, play status), launching and playtime, folder scan with catalog matching, board view, skip-a-version, background update checks, tray and notifications, offline library
- [x] **Phase 2:** F95 session window, first-post link reader with a per-version cache, download queue and anti-ban limits, mass-download warning and typed confirm, Pixeldrain and direct links, browser-window capture for everything else, unpack and install, updates that keep saves. *Still needs end-to-end testing with a real F95 account.*
- [x] **Phase 3:**
  - start with Windows (opens in the tray)
  - F95Checker import
  - JSON backup and restore
  - `f95gm://` links and the browser extension
  - boss key and PIN lock
  - signed auto-updater, which turns on once a public key and update URL are built in

  *Still to do: the CI release pipeline and code signing.*
- [x] **Phase 4:**
  - Linux, Steam Deck and macOS builds
  - native game detection
  - Proton, Wine and CrossOver for Windows games
  - executable permissions after unpacking
  - per-OS credential storage
  - CI and release workflows

  *Still to do: "Add to Steam" for individual games, a controller-friendly layout for Game Mode, and macOS notarization.*
