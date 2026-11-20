import { Download, Gauge, Hand, ShieldCheck, Timer } from "lucide-react";
import { Card, EmptyState, PageHeader } from "@/components/ui/primitives";

/** Safety rules the download manager enforces (mirrors src-tauri limits). */
const RULES = [
  { icon: Gauge, title: "2 downloads at a time", text: "And only one per file host, so no host sees a burst from you." },
  { icon: Timer, title: "Spaced-out starts", text: "About 45 seconds between starts on the same host, with a little randomness." },
  { icon: Hand, title: "You stay in control", text: "Captchas and sign-ins open in a window for you to finish. Nothing is automated." },
  { icon: ShieldCheck, title: "Warnings before bulk", text: "Queuing 5 or more games, or more than 15 in a day, asks you to confirm first." },
];

export function DownloadsPage() {
  return (
    <div className="animate-fade-up">
      <PageHeader title="Downloads" subtitle="Download, extract and install games in one step." />
      <EmptyState icon={Download} title="No downloads yet">
        Open a game and press <span className="font-medium text-text">Download</span>. The launcher reads the links from the game's F95 thread
        using your own F95 sign-in, then installs it into your games folder.
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
    </div>
  );
}
