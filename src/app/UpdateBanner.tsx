/** "Update available" bar at the top of the app, from the background check. */
import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { ArrowUpCircle, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/primitives";
import { useRunning } from "@/hooks/queries";
import { api } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";

const KEY = ["update-status"] as const;

export function UpdateBanner() {
  const qc = useQueryClient();
  const status = useQuery({ queryKey: KEY, queryFn: api.updateStatus, staleTime: Infinity });
  const running = useRunning();
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [progress, setProgress] = useState<{ downloaded: number; total: number | null } | null>(null);

  useEffect(() => {
    const subs = [
      listen<{ version: string; notes: string | null }>("update-available", ({ payload }) => qc.setQueryData(KEY, payload)),
      listen<{ downloaded: number; total: number | null }>("update-progress", ({ payload }) => setProgress(payload)),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, [qc]);

  const u = status.data;
  if (!u || dismissed === u.version) return null;

  const playing = (running.data?.size ?? 0) > 0;
  const pct = progress?.total ? Math.round((progress.downloaded / progress.total) * 100) : null;

  const install = async () => {
    setProgress({ downloaded: 0, total: null });
    try {
      await api.updateInstall(); // restarts the app when done
    } catch (e) {
      setProgress(null);
      toast.error("Update didn't install", { description: friendlyError(e).detail });
    }
  };

  return (
    <div role="status" className="flex items-center gap-3 border-b border-accent/30 bg-accent/10 px-6 py-2.5 text-sm">
      <ArrowUpCircle className="size-4 shrink-0 text-accent" />
      <span className="min-w-0 flex-1 truncate">
        <span className="font-medium">F95 Game Manager {u.version} is ready.</span>{" "}
        {progress ? (
          <span className="text-muted">{pct !== null ? `Downloading… ${pct}%` : "Downloading…"} The app restarts when it's done.</span>
        ) : u.notes ? (
          <Tooltip content={<span className="block max-w-sm whitespace-pre-line">{u.notes}</span>} side="bottom">
            <button type="button" className="text-accent underline-offset-2 hover:underline">
              What's new
            </button>
          </Tooltip>
        ) : null}
      </span>
      {!progress && (
        <>
          <Button size="sm" variant="primary" disabled={playing} title={playing ? "Close your game first" : undefined} onClick={install}>
            Install and restart
          </Button>
          <Button size="icon-sm" variant="ghost" aria-label="Remind me later" title="Remind me later" onClick={() => setDismissed(u.version)}>
            <X />
          </Button>
        </>
      )}
    </div>
  );
}
