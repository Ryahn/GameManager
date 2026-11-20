import { useState } from "react";
import { useParams, useRouter } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowLeft, Check, Download, ExternalLink, Eye, Heart, Plus, Star } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge, ErrorPanel, Skeleton, Tooltip } from "@/components/ui/primitives";
import { Cover } from "@/components/game/GameCard";
import { useLibraryIds, useThread, useToggleLibrary } from "@/hooks/queries";
import { cleanVersion, cn, imageSrc, statusTone, timeAgo } from "@/lib/utils";

type Tab = "overview" | "changelog" | "details";

export function GamePage() {
  const { id } = useParams({ strict: false }) as { id?: string };
  const threadId = Number(id);
  const router = useRouter();
  const thread = useThread(threadId);
  const library = useLibraryIds();
  const toggle = useToggleLibrary();
  const [tab, setTab] = useState<Tab>("overview");
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
            <Tooltip content="Downloads arrive in the next update" side="top">
              <span>
                <Button variant="primary" size="lg" disabled>
                  <Download /> Download
                </Button>
              </span>
            </Tooltip>
            <Button
              variant={inLibrary ? "secondary" : "outline"}
              size="lg"
              loading={toggle.isPending}
              onClick={() => toggle.mutate({ id: threadId, add: !inLibrary, title: t.title })}
            >
              {inLibrary ? <Check /> : <Plus />} {inLibrary ? "In library" : "Add to library"}
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
        <span className="text-faint">
          Installed <span className="text-muted">Not installed</span>
        </span>
        {t.last_promoted && <span className="text-faint">Updated {timeAgo(t.last_promoted)}</span>}
        <span className="ml-auto flex items-center gap-4 text-muted">
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
        {(["overview", "changelog", "details"] as Tab[]).map((k) => (
          <button
            key={k}
            role="tab"
            aria-selected={tab === k}
            onClick={() => setTab(k)}
            className={cn(
              "-mb-px border-b-2 px-4 py-2.5 text-sm font-medium capitalize transition-colors",
              tab === k ? "border-accent text-text" : "border-transparent text-muted hover:text-text",
            )}
          >
            {k}
          </button>
        ))}
      </div>

      <div className="py-6">
        {tab === "overview" && (
          <div className="grid gap-8 lg:grid-cols-[1fr_320px]">
            <div>
              <p className="whitespace-pre-line text-[15px] leading-relaxed text-muted" data-selectable>
                {t.overview?.trim() || "No overview yet."}
              </p>
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
          <pre className="whitespace-pre-wrap font-sans text-[14px] leading-relaxed text-muted" data-selectable>
            {t.changelog?.trim() || "No changelog yet."}
          </pre>
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
      </div>

      {shot && (
        <button type="button" aria-label="Close screenshot" className="fixed inset-0 z-50 grid place-items-center bg-overlay p-10 backdrop-blur" onClick={() => setShot(null)}>
          <img src={imageSrc(shot)} alt="Screenshot" className="max-h-full max-w-full rounded-xl shadow-card" />
        </button>
      )}
    </div>
  );
}
