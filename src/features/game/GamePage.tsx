import { useState } from "react";
import { useParams, useRouter } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowLeft, Check, Download, ExternalLink, Eye, FolderOpen, Heart, Play, Plus, Star, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge, ErrorPanel, Select, Skeleton, Tooltip } from "@/components/ui/primitives";
import { Cover } from "@/components/game/GameCard";
import { BBCode } from "@/components/BBCode";
import { useLibraryIds, useLocalGame, usePlay, useRunning, useThread, useToggleLibrary, useUpdateLocal } from "@/hooks/queries";
import { useUi } from "@/state/ui";
import { PLAY_STATUSES, formatPlaytime, sameVersion } from "@/lib/play";
import { cleanVersion, cn, imageSrc, statusTone, timeAgo } from "@/lib/utils";
import { kindOf } from "@/lib/kinds";
import { api } from "@/lib/ipc";
import { FilesPanel, HistoryPanel, NotesPanel, useLocate } from "./YourGame";
import { DownloadDialog } from "./DownloadDialog";

type Tab = "overview" | "changelog" | "details" | "files" | "notes" | "history";

const TABS: { value: Tab; label: string }[] = [
  { value: "overview", label: "Overview" },
  { value: "changelog", label: "Changelog" },
  { value: "details", label: "Details" },
  { value: "files", label: "Files & launch" },
  { value: "notes", label: "Notes" },
  { value: "history", label: "Play history" },
];

export function GamePage() {
  const { id } = useParams({ strict: false }) as { id?: string };
  const threadId = Number(id);
  const router = useRouter();
  const thread = useThread(threadId);
  const library = useLibraryIds();
  const toggle = useToggleLibrary();
  const localQ = useLocalGame(threadId);
  const update = useUpdateLocal(threadId);
  const running = useRunning();
  const play = usePlay();
  const [tab, setTab] = useState<Tab>("overview");
  const [downloadOpen, setDownloadOpen] = useState(false);
  const locate = useLocate(threadId, () => setTab("files"));
  const [shot, setShot] = useState<string | null>(null);

  const back = (
    <Button variant="ghost" size="sm" className="-ml-2 mb-4" onClick={() => router.history.back()}>
      <ArrowLeft /> Back
    </Button>
  );

  if (thread.error) {
    return (
      <>
        {back}
        <ErrorPanel error={thread.error} onRetry={() => thread.refetch()} />
      </>
    );
  }

  const t = thread.data;
  if (!t) {
    return (
      <>
        {back}
        <Skeleton className="aspect-[21/7] w-full rounded-2xl" />
        <Skeleton className="mt-6 h-8 w-1/2" />
        <Skeleton className="mt-3 h-4 w-1/3" />
      </>
    );
  }

  const inLibrary = library.has(threadId);
  const status = statusTone(t.prefixes);
  const version = cleanVersion(t.version);
  const hero = t.banner || t.cover;
  const local = localQ.data;
  const kind = kindOf(t.category);
  const isGame = kind === "games";
  // Games need something to launch; comics and animations just their folder.
  const installed = isGame ? (local?.targets.length ?? 0) > 0 : !!local?.install_dir;
  const isRunning = running.data?.has(threadId) ?? false;
  const installedVersion = cleanVersion(local?.installed_version);
  const behind = !!installedVersion && !!version && !sameVersion(installedVersion, version);

  return (
    <div className="animate-fade-up">
      {back}

      <div className="relative overflow-hidden rounded-2xl border border-border">
        <Cover src={hero} alt="" className="aspect-[21/7] w-full" />
        <div className="absolute inset-0 bg-gradient-to-t from-bg via-bg/40 to-transparent" />
        <div className="absolute inset-x-0 bottom-0 flex flex-wrap items-end justify-between gap-4 p-6">
          <div className="min-w-0">
            <div className="mb-2 flex flex-wrap gap-1.5">
              {status && <Badge tone={status.tone}>{status.label}</Badge>}
              {t.prefixes
                .filter((p) => !["completed", "onhold", "abandoned"].includes(p.toLowerCase()))
                .slice(0, 4)
                .map((p) => (
                  <Badge key={p} tone="solid">
                    {p}
                  </Badge>
                ))}
            </div>
            <h1 className="font-[family-name:var(--font-display)] text-[30px] font-semibold leading-tight tracking-tight" data-selectable>
              {t.title}
            </h1>
            <p className="mt-1 text-muted">{t.creator || t.developer}</p>
          </div>
          <div className="flex items-center gap-2">
            {isRunning ? (
              <Button variant="primary" size="lg" disabled className="!opacity-100 !bg-success !text-black">
                <span className="size-2 animate-pulse rounded-full bg-black/70" /> Playing
              </Button>
            ) : installed ? (
              <>
                {behind && (
                  <Button variant="outline" size="lg" onClick={() => setDownloadOpen(true)}>
                    <Download /> Update to v{version}
                  </Button>
                )}
                {isGame ? (
                  <Button variant="primary" size="lg" loading={play.isPending} onClick={() => play.mutate({ id: threadId, title: t.title })}>
                    <Play className="fill-current" /> Play
                  </Button>
                ) : (
                  <Button variant="primary" size="lg" onClick={() => void api.openFolder(threadId)}>
                    <FolderOpen /> Open folder
                  </Button>
                )}
              </>
            ) : (
              <Button variant="primary" size="lg" onClick={() => setDownloadOpen(true)}>
                <Download /> Download
              </Button>
            )}
            <Button
              variant={inLibrary ? "secondary" : "outline"}
              size="lg"
              loading={toggle.isPending}
              title={inLibrary ? "Remove from library…" : "Add to library"}
              className={inLibrary ? "group/lib hover:border-danger/50 hover:bg-danger/15 hover:text-danger" : undefined}
              onClick={() =>
                inLibrary
                  ? useUi.getState().askRemove({ id: threadId, title: t.title })
                  : toggle.mutate({ id: threadId, add: true, title: t.title })
              }
            >
              {inLibrary ? (
                <>
                  <Check className="group-hover/lib:hidden" />
                  <Trash2 className="hidden group-hover/lib:block" />
                  <span className="group-hover/lib:hidden">In library</span>
                  <span className="hidden group-hover/lib:inline">Remove…</span>
                </>
              ) : (
                <>
                  <Plus /> Add to library
                </>
              )}
            </Button>
            <Tooltip content="Open the F95 thread" side="top">
              <Button variant="outline" size="icon" className="size-11" aria-label="Open F95 thread" onClick={() => openUrl(t.url)}>
                <ExternalLink />
              </Button>
            </Tooltip>
          </div>
        </div>
      </div>

      {/* Version strip */}
      <div className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-2 rounded-xl border border-border bg-surface-1 px-5 py-3 text-sm">
        <span>
          <span className="text-faint">Latest </span>
          <span className="font-semibold">{version ? `v${version}` : "Unknown"}</span>
        </span>
        <span className="flex items-center gap-1.5 text-faint">
          Installed{" "}
          {installedVersion ? (
            <span className={cn("font-medium", behind ? "text-warning" : "text-text")}>v{installedVersion}</span>
          ) : installed ? (
            <button type="button" className="text-accent hover:underline" onClick={() => setTab("files")}>
              set version
            </button>
          ) : (
            <>
              <span className="text-muted">No</span>
              <button type="button" className="text-accent hover:underline" onClick={locate.pickFolder}>
                · I have it
              </button>
            </>
          )}
          {behind && <Badge tone="warning">Update available</Badge>}
        </span>
        {local?.last_played_version && (
          <span className="text-faint">
            Played <span className="text-muted">v{cleanVersion(local.last_played_version)}</span>
          </span>
        )}
        {!!local?.playtime_secs && <span className="text-faint">{formatPlaytime(local.playtime_secs)} played</span>}
        {t.last_promoted && <span className="text-faint">Updated {timeAgo(t.last_promoted)}</span>}
        <span className="ml-auto flex items-center gap-4 text-muted">
          <Select
            label="Play status"
            value={local?.play_status ?? "not_started"}
            onChange={(play_status) => update.mutate({ play_status })}
            options={PLAY_STATUSES.map((s) => ({ value: s.value, label: s.label }))}
          />
          {t.sam_rating != null && (
            <span className="flex items-center gap-1">
              <Star className="size-4 text-warning" /> {t.sam_rating.toFixed(1)}
            </span>
          )}
          {t.likes != null && (
            <span className="flex items-center gap-1">
              <Heart className="size-4" /> {t.likes.toLocaleString()}
            </span>
          )}
          {t.views != null && (
            <span className="flex items-center gap-1">
              <Eye className="size-4" /> {t.views.toLocaleString()}
            </span>
          )}
        </span>
      </div>

      <div role="tablist" aria-label="Game sections" className="mt-6 flex gap-1 border-b border-border">
        {TABS.filter((x) => isGame || x.value !== "history").map(({ value: k, label: l }) => ({ value: k, label: !isGame && k === "files" ? "Files" : l })).map(({ value: k, label }) => (
          <button
            key={k}
            role="tab"
            aria-selected={tab === k}
            onClick={() => setTab(k)}
            className={cn(
              "-mb-px border-b-2 px-4 py-2.5 text-sm font-medium transition-colors",
              tab === k ? "border-accent text-text" : "border-transparent text-muted hover:text-text",
            )}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="py-6">
        {tab === "overview" && (
          <div className="grid gap-8 lg:grid-cols-[1fr_320px]">
            <div>
              {t.overview?.trim() ? (
                <BBCode text={t.overview.trim()} className="text-[15px] leading-relaxed text-muted" />
              ) : (
                <p className="text-[15px] text-muted">No overview yet.</p>
              )}
              {t.previews.length > 0 && (
                <div className="mt-8 grid grid-cols-2 gap-3 md:grid-cols-3">
                  {t.previews.slice(0, 9).map((p) => (
                    <button key={p} type="button" onClick={() => setShot(p)} className="overflow-hidden rounded-lg border border-border">
                      <Cover src={p} alt="Screenshot" className="aspect-[16/9] transition-transform hover:scale-[1.03]" />
                    </button>
                  ))}
                </div>
              )}
            </div>
            <aside>
              <h3 className="mb-3 text-xs font-semibold uppercase tracking-wider text-faint">Tags</h3>
              <div className="flex flex-wrap gap-1.5">
                {t.tag_names.length ? t.tag_names.map((tag) => <Badge key={tag}>{tag}</Badge>) : <span className="text-sm text-faint">No tags</span>}
              </div>
            </aside>
          </div>
        )}
        {tab === "changelog" && (
          t.changelog?.trim() ? (
            <BBCode text={t.changelog.trim()} className="max-w-4xl text-[14px] leading-relaxed text-muted" />
          ) : (
            <p className="text-[14px] text-muted">No changelog yet.</p>
          )
        )}
        {tab === "details" && (
          <dl className="grid max-w-2xl grid-cols-[160px_1fr] gap-x-6 gap-y-3 text-sm">
            {(
              [
                ["Developer", t.developer || t.creator],
                ["Version", version && `v${version}`],
                ["Thread updated", t.thread_updated],
                ["Release date", t.release_date],
                ["OS", t.os],
                ["Language", t.language],
                ["Censored", t.censored],
                ["Genre", t.genre],
              ] as const
            )
              .filter(([, v]) => v)
              .map(([k, v]) => (
                <div key={k} className="contents">
                  <dt className="text-faint">{k}</dt>
                  <dd className="text-text" data-selectable>
                    {v}
                  </dd>
                </div>
              ))}
            {t.external_links.length > 0 && (
              <>
                <dt className="text-faint">Links</dt>
                <dd className="flex flex-wrap gap-2">
                  {t.external_links.map((l) => (
                    <button key={l.url} type="button" className="text-accent hover:underline" onClick={() => openUrl(l.url)}>
                      {l.label || l.host}
                    </button>
                  ))}
                </dd>
              </>
            )}
          </dl>
        )}
        {tab === "files" && <FilesPanel id={threadId} local={local} latest={t.version} kind={kind} />}
        {tab === "notes" && <NotesPanel id={threadId} local={local} latest={t.version} />}
        {tab === "history" && <HistoryPanel id={threadId} local={local} />}
      </div>

      <DownloadDialog
        open={downloadOpen}
        onOpenChange={setDownloadOpen}
        game={{ thread_id: threadId, title: t.title, creator: t.creator || t.developer, version: t.version, category: kind }}
      />

      {shot && (
        <button type="button" aria-label="Close screenshot" className="fixed inset-0 z-50 grid place-items-center bg-overlay p-10 backdrop-blur" onClick={() => setShot(null)}>
          <img src={imageSrc(shot)} alt="Screenshot" className="max-h-full max-w-full rounded-xl shadow-card" />
        </button>
      )}
    </div>
  );
}
