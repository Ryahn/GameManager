import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

const isWindows = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);

/**
 * Routes remote F95 images through the Rust image cache (rate limited, disk
 * cached, host allow-listed) instead of letting the webview hit F95 directly.
 */
export function imageSrc(url: string | null | undefined): string | undefined {
  if (!url) return undefined;
  if (url.startsWith("data:")) return url;
  const encoded = encodeURIComponent(url);
  return isWindows ? `http://gmimg.localhost/${encoded}` : `gmimg://localhost/${encoded}`;
}

export function timeAgo(unixSeconds: number | null | undefined, now = Date.now()): string {
  if (!unixSeconds) return "";
  const diff = Math.max(0, now / 1000 - unixSeconds);
  const units: [number, string][] = [
    [60 * 60 * 24 * 365, "y"],
    [60 * 60 * 24 * 30, "mo"],
    [60 * 60 * 24 * 7, "w"],
    [60 * 60 * 24, "d"],
    [60 * 60, "h"],
    [60, "m"],
  ];
  for (const [secs, label] of units) {
    if (diff >= secs) return `${Math.floor(diff / secs)}${label} ago`;
  }
  return "just now";
}

/** Prefixes that describe development status, mapped to a tone. */
export function statusTone(prefixes: string[]): { label: string; tone: "success" | "warning" | "danger" | "info" } | null {
  const p = prefixes.map((x) => x.toLowerCase());
  if (p.includes("completed")) return { label: "Completed", tone: "success" };
  if (p.includes("onhold") || p.includes("on hold")) return { label: "On hold", tone: "warning" };
  if (p.includes("abandoned")) return { label: "Abandoned", tone: "danger" };
  return null;
}

/** Strips F95's version decorations for display ("v0.9.1" -> "0.9.1"). */
export function cleanVersion(v: string | null | undefined): string {
  if (!v) return "";
  return v.trim().replace(/^v(?=\d)/i, "");
}
