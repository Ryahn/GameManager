/**
 * "Remove from library": one dialog used everywhere (cards, list rows, the
 * game page). Opened with `useUi.getState().askRemove(...)`.
 */
import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent } from "@/components/ui/primitives";
import { keys, useLocalGame } from "@/hooks/queries";
import { api } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { useUi } from "@/state/ui";

export function RemoveGameDialog() {
  const target = useUi((s) => s.removeTarget);
  const close = useUi((s) => s.closeRemove);
  const qc = useQueryClient();
  const local = useLocalGame(target?.id ?? 0);
  const [deleteFiles, setDeleteFiles] = useState(false);
  const [busy, setBusy] = useState(false);
  const installDir = target ? local.data?.install_dir : null;

  useEffect(() => setDeleteFiles(!!target?.deleteFiles), [target]);

  const remove = async () => {
    if (!target) return;
    setBusy(true);
    try {
      await api.removeGame(target.id, deleteFiles && !!installDir);
      qc.invalidateQueries({ queryKey: ["bookmarks"] });
      qc.invalidateQueries({ queryKey: keys.local });
      qc.invalidateQueries({ queryKey: keys.localGame(target.id) });
      toast.success(`Removed ${target.title}`, {
        description: deleteFiles && installDir ? "Its files are in the Recycle Bin." : "Its files are still on this PC.",
      });
      close();
    } catch (e) {
      toast.error("Couldn't remove the game", { description: friendlyError(e).detail });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={!!target} onOpenChange={(v) => !v && close()}>
      <DialogContent title={`Remove ${target?.title ?? "game"} from your library?`} description="It stops being tracked for updates and leaves your library.">
        <p className="text-sm text-muted">Your notes, rating and play history stay saved on this PC in case you add it back.</p>
        {installDir && (
          <label className="mt-4 flex cursor-default items-start gap-3 rounded-xl border border-border bg-surface-2 p-3">
            <input
              type="checkbox"
              checked={deleteFiles}
              onChange={(e) => setDeleteFiles(e.target.checked)}
              className="mt-0.5 size-4 shrink-0 accent-[var(--accent)]"
            />
            <span className="min-w-0">
              <span className="block text-sm font-medium">Also delete the game's files</span>
              <span className="mt-0.5 block truncate font-mono text-[12px] text-faint" title={installDir}>
                {installDir}
              </span>
              <span className="mt-1 block text-[12.5px] text-muted">Moved to the Recycle Bin, so you can restore them. Saves inside the folder go too.</span>
            </span>
          </label>
        )}
        <div className="mt-6 flex justify-end gap-2">
          <Button variant="ghost" onClick={close}>
            Cancel
          </Button>
          <Button variant="danger" loading={busy} onClick={remove}>
            <Trash2 /> {deleteFiles && installDir ? "Remove and delete files" : "Remove from library"}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
