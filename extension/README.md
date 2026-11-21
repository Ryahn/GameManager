# F95 Game Manager browser extension

Adds an **Add to F95 Game Manager** button to F95zone game threads. Clicking it opens an `f95gm://add/<thread id>` link. The launcher then adds the game to your library and shows it.

- It needs no permissions, makes no network requests and doesn't read your F95 session.
- The first time you click, your browser asks whether to open F95 Game Manager.
- The launcher has to be installed, because the installer registers the `f95gm://` links. In development, `npm run tauri dev` registers them for the dev build.

## Install (unpacked)

**Chrome, Edge, Brave, Opera**
1. Open `chrome://extensions` (or `edge://extensions`).
2. Turn on **Developer mode**.
3. Choose **Load unpacked** and pick this `extension` folder.

**Firefox**
1. Open `about:debugging#/runtime/this-firefox`.
2. Choose **Load Temporary Add-on…** and pick `manifest.json`.
3. For a permanent install, sign it on addons.mozilla.org. The add-on id is already set in the manifest.

## Links the launcher understands

| Link | What happens |
| --- | --- |
| `f95gm://add/<thread id>` | Adds the game to your library and opens its page. |
| `f95gm://game/<thread id>` | Opens the game's page. |

The website can use the same links for an "Open in launcher" button.
