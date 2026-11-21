/** Puts the app window in or out of full screen (the browser's, in dev). */
export async function setFullscreen(on: boolean) {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setFullscreen(on);
  } catch {
    // Outside the app shell (dev in a browser).
    if (on) await document.documentElement.requestFullscreen?.().catch(() => {});
    else if (document.fullscreenElement) await document.exitFullscreen().catch(() => {});
  }
}

/** A saved per-viewer preference; storage can be missing or blocked. */
export function pref<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(`gm.${key}`);
    return v == null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
}

export function setPref(key: string, value: unknown) {
  try {
    localStorage.setItem(`gm.${key}`, JSON.stringify(value));
  } catch {
    // Not saved; the default applies next time.
  }
}
