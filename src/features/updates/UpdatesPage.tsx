import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { ArrowRight, BellOff, Check, CheckCheck, SkipForward, Undo2 } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Badge, EmptyState, ErrorPanel, PageHeader, Skeleton } from "@/components/ui/primitives";
import { Cover } from "@/components/game/GameCard";
import { keys, useAck, useLocalMap, useUpdateInbox } from "@/hooks/queries";
import { api, type Bookmark } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { cleanVersion } from "@/lib/utils";

export function UpdatesPage() {
  const inbox = useUpdateInbox();
  const updates = inbox.query;
  const ack = useAck();
  const qc = useQueryClient();
  const local = useLocalMap();
  const [showSkipped, setShowSkipped] = useState(false);
  const items = showSkipped ? [...inbox.items, ...inbox.skipped] : inbox.items;

  const skip = async (b: Bookmark, undo = false) => {
    try {
      await api.updateLocal(b.thread_id, { skipped_version: undo ? "" : b.current_version });
      qc.invalidateQueries({ queryKey: keys.local });
      if (!undo) {
        toast.success("Version skipped", {
          description: "You won't be reminded about it again. A newer version shows up as usual.",
          action: { label: "Undo", onClick: () => void skip(b, true) },
        });
      }
    } catch (e) {
      toast.error(friendlyError(e).title, { description: friendlyError(e).detail });
    }
  };

  const markAllSeen = async () => {
    try {
      await api.ackAll();
      toast.success("All updates marked as seen");
    } catch (e) {
      toast.error(friendlyError(e).title, { description: friendlyError(e).detail });
    } finally {
      qc.invalidateQueries({ queryKey: ["bookmarks"] });
    }
  };

  return (
    <div className="animate-fade-up">
      <PageHeader
        title="Updates"
        subtitle="New versions of games in your library. Checked on the server every 30 minutes, including stealth updates."
        actions={
          <>
            {inbox.skipped.length > 0 && (
              <Button variant="ghost" onClick={() => setShowSkipped((v) => !v)}>
                {showSkipped ? "Hide" : "Show"} {inbox.skipped.length} skipped
              </Button>
            )}
            {inbox.items.length > 1 && (
              <Button variant="outline" onClick={markAllSeen}>
                <CheckCheck /> Mark all seen
              </Button>
            )}
          </>
        }
      />

      {updates.error ? (
        <ErrorPanel error={updates.error} onRetry={() => updates.refetch()} />
      ) : !updates.data ? (
        <div className="space-y-3">
          {Array.from({ length: 4 }, (_, i) => (
            <Skeleton key={i} className="h-24" />
          ))}
        </div>
      ) : items.length === 0 ? (
        <EmptyState icon={BellOff} title="You're up to date">
          When a game in your library gets a new version, it shows up here and in your notifications.
        </EmptyState>
      ) : (
        <ul className="space-y-3">
          {items.map((b) => {
            const t = b.thread;
            const from = cleanVersion(b.last_seen_version);
            const to = cleanVersion(b.current_version || t?.version);
            const mine = local.get(b.thread_id);
            const installed = cleanVersion(mine?.installed_version);
            const skipped = !!mine?.skipped_version && mine.skipped_version === b.current_version;
            return (
              <li key={b.thread_id} className="flex items-center gap-4 rounded-[var(--radius-card)] border border-border bg-surface-1 p-3 pr-4">
                <Cover src={t?.cover} alt="" className="aspect-[16/9] w-36 shrink-0 rounded-lg" />
                <div className="min-w-0 flex-1">
                  <Link to="/game/$id" params={{ id: String(b.thread_id) }} className="block truncate font-semibold hover:underline">
                    {t?.title ?? `Thread ${b.thread_id}`}
                  </Link>
                  <p className="truncate text-sm text-muted">{t?.creator}</p>
                  <div className="mt-2 flex items-center gap-2 text-sm">
                    {from && <span className="text-faint line-through">v{from}</span>}
                    {from && <ArrowRight className="size-3.5 text-faint" />}
                    <Badge tone="accent">v{to || "?"}</Badge>
                    {b.needs_manual_review && <Badge tone="warning">Needs review</Badge>}
                    {installed && <span className="text-[12.5px] text-faint">You have v{installed}</span>}
                    {skipped && <Badge>Skipped</Badge>}
                  </div>
                </div>
                {skipped ? (
                  <Button variant="ghost" size="sm" onClick={() => skip(b, true)}>
                    <Undo2 /> Unskip
                  </Button>
                ) : (
                  <Button variant="ghost" size="sm" title="Hide this version and don't notify about it" onClick={() => skip(b)}>
                    <SkipForward /> Skip version
                  </Button>
                )}
                <Button variant="ghost" size="sm" loading={ack.isPending && ack.variables === b.thread_id} onClick={() => ack.mutate(b.thread_id)}>
                  <Check /> Mark seen
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
