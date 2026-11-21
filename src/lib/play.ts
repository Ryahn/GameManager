import type { PlayStatus } from "./ipc";

export const PLAY_STATUSES: { value: PlayStatus; label: string; dot: string }[] = [
  { value: "not_started", label: "Not started", dot: "bg-faint" },
  { value: "playing", label: "Playing", dot: "bg-accent" },
  { value: "on_hold", label: "On hold", dot: "bg-warning" },
  { value: "finished", label: "Finished", dot: "bg-success" },
  { value: "dropped", label: "Dropped", dot: "bg-danger" },
];

export const statusMeta = (s: PlayStatus) => PLAY_STATUSES.find((x) => x.value === s) ?? PLAY_STATUSES[0];

/** 3725 -> "1h 2m"; 42 -> "under a minute". */
export function formatPlaytime(secs: number | null | undefined): string {
  const s = Math.max(0, Math.floor(secs ?? 0));
  if (s < 60) return s === 0 ? "Not played" : "under a minute";
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h === 0) return `${m} min`;
  return m ? `${h}h ${m}m` : `${h}h`;
}

/** Compares versions loosely: "v0.9" vs "0.9" are the same. */
export function sameVersion(a: string | null | undefined, b: string | null | undefined): boolean {
  const n = (v: string | null | undefined) => (v ?? "").trim().toLowerCase().replace(/^v(?=\d)/, "");
  return !!n(a) && n(a) === n(b);
}
