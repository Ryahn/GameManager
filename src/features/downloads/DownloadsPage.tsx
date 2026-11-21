import { useEffect, useState } from "react";
import { Link } from "@tanstack/react-router";
import { CheckCheck, Download, ExternalLink, FolderOpen, Gamepad2, Gauge, Hand, Pause, Play, RotateCcw, ShieldCheck, Timer, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, EmptyState, ErrorPanel, PageHeader, Skeleton, Tooltip } from "@/components/ui/primitives";
import { Cover } from "@/components/game/GameCard";
import { useDownloadAction, useDownloads, useLocalMap, usePlay } from "@/hooks/queries";
import { api, type DownloadJob, type JobState } from "@/lib/ipc";
import { cleanVersion, cn, timeAgo } from "@/lib/utils";
import { toast } from "sonner";
import { friendlyError } from "@/lib/errors";

/** Safety rules the download manager enforces (mirrors src-tauri limits). */
const RULES = [
  { icon: Gauge, title: "A few at a time", text: "2 downloads at once by default, and only one per file host." },
  { icon: Timer, title: "Spaced-out starts", text: "30-60 seconds between starts on the same host. Hosts that push back are left alone for 15 minutes or more." },
  { icon: Hand, title: "You stay in control", text: "Captchas and sign-ins open in the F95 window for you to finish. Nothing is automated." },
  { icon: ShieldCheck, title: "Warnings before bulk", text: "5 or more waiting shows a warning; more than 15 in a day asks you to type a confirmation." },
];

export function formatBytes(n: number | null | undefined): string {
  if (!n || n < 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  return `${(n / 1024 ** i).toFixed(i >= 2 ? 1 : 0)} ${units[i]}`;
}

function useNow(ms = 1000) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}

const SECTIONS: { title: string; states: JobState[] }[] = [
  { title: "Needs you", states: ["needs_you"] },
  { title: "In progress", states: ["downloading", "extracting", "installing"] },
  { title: "Up next", states: ["queued"] },
  { title: "Paused", states: ["paused"] },
  { title: "Didn't work", states: ["failed"] },
  { title: "Done", states: ["done"] },
];

function statusLine(j: DownloadJob, now: number): string {
  switch (j.state) {
    case "queued": {
      const wait = j.starts_at ? Math.ceil(j.starts_at - now / 1000) : 0;
      if (wait > 0) return wait > 90 ? `Starts in ${Math.ceil(wait / 60)} min` : `Starts in ${wait} s`;
      return "Starting soon";
    }
    case "downloading":
      return j.source === "browser" ? "Downloading in the F95 window" : "Downloading";
    case "extracting":
      return "Unpacking";
    case "installing":
      return "Installing";
    case "done":
      return j.install_dir ? `Installed ${timeAgo(j.finished_at)}` : `Saved ${timeAgo(j.finished_at)}`;
    case "paused":
      return "Paused";
    case "failed":
      return "Didn't work";
    case "needs_you":
      return "Waiting for you";
  }
}

function JobRow({ job: j, now }: { job: DownloadJob; now: number }) {
  const local = useLocalMap();
  const act = useDownloadAction();
  const play = usePlay();
  const cover = j.thread_id ? local.get(j.thread_id)?.cached?.cover : null;
  const pct = j.bytes_total ? Math.min(100, Math.round((j.bytes_done / j.bytes_total) * 100)) : null;
  const moving = ["downloading", "extracting", "installing"].includes(j.state);
  const installed = j.thread_id ? (local.get(j.thread_id)?.targets.length ?? 0) > 0 : false;

  return (
    <li className="flex items-center gap-4 rounded-[var(--radius-card)] border border-border bg-surface-1 p-3 pr-4">
      {cover ? (
        <Cover src={cover} alt="" className="aspect-[16/9] w-28 shrink-0 rounded-lg" />
      ) : (
        <div className="grid aspect-[16/9] w-28 shrink-0 place-items-center rounded-lg bg-surface-2 text-faint">
          <Gamepad2 className="size-6" />
        </div>
      )}
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          {j.thread_id ? (
            <Link to="/game/$id" params={{ id: String(j.thread_id) }} className="truncate font-semibold hover:underline">
              {j.title}
            </Link>
          ) : (
            <span className="truncate font-semibold">{j.title}</span>
          )}
          {j.version && <span className="shrink-0 text-sm text-muted">v{cleanVersion(j.version)}</span>}
        </div>
        <p className="mt-0.5 truncate text-[13px] text-muted">
          <span className={cn(j.state === "needs_you" && "font-medium text-warning", j.state === "failed" && "font-medium text-danger")}>{statusLine(j, now)}</span>
          {" · "}
          {j.host || "F95 window"}
          {j.bytes_done > 0 && ` · ${formatBytes(j.bytes_done)}${j.bytes_total ? ` of ${formatBytes(j.bytes_total)}` : ""}`}
        </p>
        {(moving || (j.state === "paused" && j.bytes_done > 0)) && (
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-surface-3">
            <div
              className={cn("h-full rounded-full transition-[width] duration-500", j.state === "paused" ? "bg-faint" : "bg-accent", pct === null && moving && "w-1/3 animate-pulse")}
              style={pct !== null ? { width: `${pct}%` } : undefined}
            />
          </div>
        )}
        {j.error && <p className={cn("mt-1.5 text-[12.5px]", j.state === "done" ? "text-faint" : "text-muted")}>{j.error}</p>}
      </div>

      <div className="flex shrink-0 items-center gap-1">
        {j.state === "needs_you" && (
          <Button size="sm" variant="primary" onClick={() => act.openInBrowser.mutate(j.id)}>
            <ExternalLink /> Open in F95 window
          </Button>
        )}
        {(j.state === "needs_you" || j.state === "failed") && (
          <Button size="sm" variant="ghost" onClick={() => act.resume.mutate(j.id)}>
            <RotateCcw /> Retry
          </Button>
        )}
        {j.state === "paused" && (
          <Button size="sm" variant="ghost" onClick={() => act.resume.mutate(j.id)}>
            <Play /> Resume
          </Button>
        )}
        {(j.state === "queued" || (j.state === "downloading" && j.source === "direct")) && (
          <Tooltip content="Pause (keeps what's downloaded)" side="top">
            <Button size="icon-sm" variant="ghost" aria-label="Pause" onClick={() => act.pause.mutate(j.id)}>
              <Pause />
            </Button>
          </Tooltip>
        )}
        {j.state === "done" && j.thread_id && installed && (
          <Button size="sm" variant="primary" onClick={() => play.mutate({ id: j.thread_id!, title: j.title })}>
            <Play className="fill-current" /> Play
          </Button>
        )}
        {j.state === "done" && (
          <Tooltip content="Show in folder" side="top">
            <Button
              size="icon-sm"
              variant="ghost"
              aria-label="Show in folder"
              onClick={() => api.openDownloadFolder(j.id).catch((e) => toast.error(friendlyError(e).detail))}
            >
              <FolderOpen />
            </Button>
          </Tooltip>
        )}
        {!moving && (
          <Tooltip content={j.state === "done" ? "Remove from list (the game stays installed)" : "Remove and delete the partial file"} side="top">
            <Button size="icon-sm" variant="ghost" aria-label="Remove" onClick={() => act.remove.mutate(j.id)}>
              <Trash2 />
            </Button>
          </Tooltip>
        )}
      </div>
    </li>
  );
}

export function DownloadsPage() {
  const q = useDownloads();
  const act = useDownloadAction();
  const now = useNow();
  const snap = q.data;
  const jobs = snap?.jobs ?? [];
  const hasDone = jobs.some((j) => j.state === "done");

  return (
    <div className="animate-fade-up">
      <PageHeader
        title="Downloads"
        subtitle={
          snap ? (
            <>
              {snap.daily} started in the last 24 hours
              <span className="text-faint"> · {snap.daily_limit} suggested per day · {snap.max_active} at a time</span>
            </>
          ) : (
            "Download, unpack and install games in one step."
          )
        }
        actions={
          <>
            <Button variant="outline" onClick={() => api.f95Open()}>
              <ExternalLink /> F95 window
            </Button>
            {hasDone && (
              <Button variant="ghost" onClick={() => act.clear.mutate()}>
                <CheckCheck /> Clear finished
              </Button>
            )}
          </>
        }
      />

      {snap?.paused_for_game && (
        <div role="status" className="mb-5 flex items-center gap-3 rounded-xl border border-accent/30 bg-accent/8 px-4 py-3 text-sm">
          <Pause className="size-4 shrink-0 text-accent" />
          <span>
            <span className="font-medium">Paused while you play.</span> <span className="text-muted">Downloads pick up where they left off when the game closes.</span>
          </span>
        </div>
      )}

      {snap && snap.daily >= snap.daily_limit && (
        <div role="status" className="mb-5 flex items-center gap-3 rounded-xl border border-warning/30 bg-warning/8 px-4 py-3 text-sm">
          <ShieldCheck className="size-4 shrink-0 text-warning" />
          <span className="text-muted">
            <span className="font-medium text-text">You've hit today's suggested limit.</span> More downloads ask you to confirm first. Spreading them over a
            few days keeps your account and IP safe.
          </span>
        </div>
      )}

      {q.error ? (
        <ErrorPanel error={q.error} onRetry={() => q.refetch()} />
      ) : !snap ? (
        <div className="space-y-3">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={i} className="h-24" />
          ))}
        </div>
      ) : jobs.length === 0 ? (
        <>
          <EmptyState icon={Download} title="No downloads yet">
            Open a game and press <span className="font-medium text-text">Download</span>. The launcher reads the links from the game's F95 thread using
            your own F95 sign-in, then installs it into your games folder.
          </EmptyState>
          <div className="mx-auto grid max-w-3xl gap-3 sm:grid-cols-2">
            {RULES.map((r) => (
              <Card key={r.title} className="flex gap-3 p-4">
                <r.icon className="mt-0.5 size-5 shrink-0 text-accent" />
                <div>
                  <p className="text-sm font-medium">{r.title}</p>
                  <p className="mt-0.5 text-sm text-muted">{r.text}</p>
                </div>
              </Card>
            ))}
          </div>
        </>
      ) : (
        SECTIONS.map((s) => {
          const list = jobs.filter((j) => s.states.includes(j.state));
          if (!list.length) return null;
          return (
            <section key={s.title} className="mb-8">
              <h2 className="mb-3 text-xs font-semibold uppercase tracking-wider text-faint">
                {s.title} <span className="ml-1 text-faint/70">{list.length}</span>
              </h2>
              <ul className="space-y-2.5">
                {list.map((j) => (
                  <JobRow key={j.id} job={j} now={now} />
                ))}
              </ul>
            </section>
          );
        })
      )}
    </div>
  );
}
