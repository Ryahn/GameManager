/**
 * Picking a download link: links come from the thread's first post (read
 * with the user's own F95 sign-in), then either go in the queue or open in
 * the F95 window for the user to click through.
 */
import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AlertTriangle, ArrowDownToLine, ExternalLink, Globe, Loader2, RefreshCw, Send, ShieldAlert } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge, Dialog, DialogContent, ErrorPanel, Input } from "@/components/ui/primitives";
import { keys } from "@/hooks/queries";
import { api, CONFIRM_PHRASE, type ApiError, type DownloadLink, type GameCtx, type LinkGroup, type Preflight } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { cleanVersion, cn, timeAgo } from "@/lib/utils";

function pickRecommended(groups: LinkGroup[]): number {
  const win = groups.findIndex((g) => g.os.includes("windows"));
  return win >= 0 ? win : 0;
}

const fmtEta = (s: number) => (s < 90 ? `${s} seconds` : `${Math.round(s / 60)} minutes`);

type Step = { kind: "pick" } | { kind: "warn"; link: DownloadLink; pre: Preflight } | { kind: "confirm"; link: DownloadLink; daily: number; limit: number };

export function DownloadDialog({
  open,
  onOpenChange,
  game,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  game: GameCtx;
}) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [step, setStep] = useState<Step>({ kind: "pick" });
  const [busy, setBusy] = useState<string | null>(null);
  const [phrase, setPhrase] = useState("");
  const links = useQuery({
    queryKey: keys.links(game.thread_id, game.version),
    queryFn: () => api.links(game.thread_id, game.version),
    enabled: open,
    staleTime: Infinity,
    retry: false,
  });
  const [refreshing, setRefreshing] = useState(false);
  const jd = useQuery({ queryKey: ["jdownloader"], queryFn: api.jdownloaderStatus, enabled: open, staleTime: 30_000, retry: false });

  const sendToJd = async (link: DownloadLink, confirm?: string) => {
    setBusy(`jd:${link.url}`);
    try {
      await api.sendToJDownloader(game, link, confirm);
      toast.success("Sent to JDownloader", { description: "It handles the download there. Install it with Find games on this PC when it's done." });
    } catch (e) {
      const err = e as ApiError;
      if (err?.kind === "confirm_required") setStep({ kind: "confirm", link, daily: err.daily, limit: err.limit });
      else toast.error(friendlyError(e).title, { description: friendlyError(e).detail });
    } finally {
      setBusy(null);
    }
  };
  const recommended = useMemo(() => pickRecommended(links.data?.groups ?? []), [links.data]);

  const refresh = async () => {
    setRefreshing(true);
    try {
      qc.setQueryData(keys.links(game.thread_id, game.version), await api.links(game.thread_id, game.version, true));
    } catch (e) {
      toast.error(friendlyError(e).title, { description: friendlyError(e).detail });
    } finally {
      setRefreshing(false);
    }
  };

  const close = () => {
    onOpenChange(false);
    setStep({ kind: "pick" });
    setPhrase("");
  };

  const start = async (link: DownloadLink, confirm?: string) => {
    setBusy(link.url);
    try {
      const r = await api.startDownload(game, link, confirm);
      qc.invalidateQueries({ queryKey: keys.downloads });
      close();
      if (r.opened_browser) {
        toast("Finish in the F95 window", {
          description: link.masked
            ? "Complete the captcha, then press the host's download button. The launcher picks up the file and installs it."
            : "Press the host's download button. The launcher picks up the file and installs it.",
          duration: 9000,
        });
      } else {
        toast.success("Added to your downloads", {
          description: `${game.title} installs automatically when it's done.`,
          action: { label: "View", onClick: () => navigate({ to: "/downloads" }) },
        });
      }
    } catch (e) {
      const err = e as ApiError;
      if (err?.kind === "confirm_required") setStep({ kind: "confirm", link, daily: err.daily, limit: err.limit });
      else toast.error(friendlyError(e).title, { description: friendlyError(e).detail });
    } finally {
      setBusy(null);
    }
  };

  const choose = async (link: DownloadLink) => {
    try {
      const pre = await api.preflight([link.host]);
      if (pre.needs_confirm) {
        setStep({ kind: "confirm", link, daily: pre.daily, limit: 15 });
        return;
      }
      if (pre.warn) {
        setStep({ kind: "warn", link, pre });
        return;
      }
    } catch {
      /* Rust re-checks on start anyway */
    }
    void start(link);
  };

  const err = links.error as ApiError | null;

  return (
    <Dialog open={open} onOpenChange={(v) => (v ? onOpenChange(v) : close())}>
      <DialogContent
        title={`Download ${game.title}`}
        description={game.version ? `Version ${cleanVersion(game.version)}` : undefined}
        className="w-[min(94vw,640px)]"
      >
        {step.kind === "warn" && (
          <div>
            <div className="flex gap-3 rounded-xl border border-warning/30 bg-warning/8 p-4">
              <AlertTriangle className="mt-0.5 size-5 shrink-0 text-warning" />
              <div className="text-sm leading-relaxed">
                <p className="font-semibold">Lots of downloads at once</p>
                <p className="mt-1 text-muted">
                  You'll have {step.pre.queued + step.pre.adding} downloads waiting. File hosts block IPs that download too much, and F95 can flag
                  accounts that open many masked links. The launcher spaces them out, so this queue takes about{" "}
                  <span className="font-medium text-text">{fmtEta(step.pre.eta_secs)}</span>.
                </p>
              </div>
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setStep({ kind: "pick" })}>
                Back
              </Button>
              <Button variant="primary" loading={!!busy} onClick={() => start(step.link)}>
                Queue it anyway
              </Button>
            </div>
          </div>
        )}

        {step.kind === "confirm" && (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void start(step.link, phrase);
            }}
          >
            <div className="flex gap-3 rounded-xl border border-danger/30 bg-danger/8 p-4">
              <ShieldAlert className="mt-0.5 size-5 shrink-0 text-danger" />
              <div className="text-sm leading-relaxed">
                <p className="font-semibold">You've started {step.daily} downloads in the last 24 hours</p>
                <p className="mt-1 text-muted">
                  That's more than the {step.limit} we suggest. Heavy downloading is how accounts get flagged and IPs get blocked by hosts. If you're
                  sure, type <span className="font-mono text-text">{CONFIRM_PHRASE}</span> below.
                </p>
              </div>
            </div>
            <Input
              autoFocus
              value={phrase}
              onChange={(e) => setPhrase(e.target.value)}
              placeholder={CONFIRM_PHRASE}
              aria-label="Type the confirmation"
              className="mt-4"
            />
            <div className="mt-5 flex justify-end gap-2">
              <Button variant="ghost" onClick={() => setStep({ kind: "pick" })}>
                Back
              </Button>
              <Button type="submit" variant="danger" disabled={phrase.trim().toLowerCase() !== CONFIRM_PHRASE} loading={!!busy}>
                Download
              </Button>
            </div>
          </form>
        )}

        {step.kind === "pick" &&
          (links.isLoading ? (
            <div className="py-10 text-center">
              <Loader2 className="mx-auto size-6 animate-spin text-accent" />
              <p className="mt-3 font-medium">Reading the first post…</p>
              <p className="mt-1 text-[13px] text-muted">With your F95 sign-in, one thread at a time, to keep your account safe.</p>
            </div>
          ) : err?.kind === "needs_browser" ? (
            <div className="py-4">
              <div className="flex gap-3 rounded-xl border border-border bg-surface-2 p-4">
                <Globe className="mt-0.5 size-5 shrink-0 text-accent" />
                <div className="text-sm">
                  <p className="font-semibold">{friendlyError(err).title}</p>
                  <p className="mt-1 text-muted">{friendlyError(err).detail}</p>
                </div>
              </div>
              <div className="mt-5 flex justify-end gap-2">
                <Button variant="outline" onClick={() => api.f95Open(err.url)}>
                  <ExternalLink /> Open F95 window
                </Button>
                <Button variant="primary" onClick={() => links.refetch()} loading={links.isFetching}>
                  <RefreshCw /> Try again
                </Button>
              </div>
            </div>
          ) : links.error ? (
            <ErrorPanel error={links.error} onRetry={() => links.refetch()} />
          ) : links.data && links.data.groups.length === 0 ? (
            <div className="py-8 text-center">
              <p className="font-medium">No download links in the first post</p>
              <p className="mt-1 text-sm text-muted">The developer may link elsewhere. Open the thread to check.</p>
              <Button className="mt-5" variant="outline" onClick={() => api.f95Open(`https://f95zone.to/threads/${game.thread_id}/`)}>
                <ExternalLink /> Open thread
              </Button>
            </div>
          ) : (
            links.data && (
              <>
                <div className="max-h-[56vh] space-y-3 overflow-y-auto pr-1">
                  {links.data.groups.map((g, gi) => (
                    <section key={g.label + gi} className={cn("rounded-xl border p-3", gi === recommended ? "border-accent/50 bg-accent/5" : "border-border")}>
                      <header className="mb-2 flex items-center gap-2">
                        <h3 className="text-sm font-semibold">{g.label}</h3>
                        {gi === recommended && <Badge tone="accent">For this PC</Badge>}
                      </header>
                      <ul className="space-y-1.5">
                        {g.links.map((l) => {
                          const inApp = l.in_app;
                          return (
                            <li key={l.url} className="flex items-center gap-3 rounded-lg bg-surface-2/70 px-3 py-2">
                              <div className="min-w-0 flex-1">
                                <p className="truncate text-sm font-medium">{l.label}</p>
                                <p className="truncate text-[12px] text-faint">
                                  {l.host}
                                  {" · "}
                                  {inApp ? "downloads in the app" : l.masked ? "opens in the F95 window (captcha)" : "opens in the F95 window"}
                                </p>
                              </div>
                              {jd.data && !inApp && !l.masked && (
                                <Button
                                  size="icon-sm"
                                  variant="ghost"
                                  title="Send to JDownloader"
                                  aria-label={`Send ${l.label} to JDownloader`}
                                  loading={busy === `jd:${l.url}`}
                                  disabled={!!busy}
                                  onClick={() => sendToJd(l)}
                                >
                                  <Send />
                                </Button>
                              )}
                              <Button size="sm" variant={inApp ? "primary" : "outline"} loading={busy === l.url} disabled={!!busy} onClick={() => choose(l)}>
                                {inApp ? <ArrowDownToLine /> : <ExternalLink />}
                                {inApp ? "Download" : "Open"}
                              </Button>
                            </li>
                          );
                        })}
                      </ul>
                    </section>
                  ))}
                </div>
                <p className="mt-4 flex items-center gap-2 text-[12.5px] text-faint">
                  <span className="flex-1">
                    Read from the first post {links.data.from_cache ? timeAgo(links.data.fetched_at) : "just now"}. Captchas are always yours to solve.
                  </span>
                  <button type="button" onClick={refresh} disabled={refreshing} className="inline-flex items-center gap-1 text-muted hover:text-text disabled:opacity-50">
                    <RefreshCw className={cn("size-3.5", refreshing && "animate-spin")} /> Re-read
                  </button>
                </p>
              </>
            )
          ))}
      </DialogContent>
    </Dialog>
  );
}
