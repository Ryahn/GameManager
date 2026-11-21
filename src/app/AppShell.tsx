import { Outlet, useNavigate } from "@tanstack/react-router";
import { Search, UserRound } from "lucide-react";
import { Sidebar } from "./Sidebar";
import { CommandPalette } from "./CommandPalette";
import { Kbd } from "@/components/ui/primitives";
import { useAuth, useBackendEvents } from "@/hooks/queries";
import { useUi } from "@/state/ui";

function Topbar() {
  const setPaletteOpen = useUi((s) => s.setPaletteOpen);
  const auth = useAuth();
  const navigate = useNavigate();
  const who = auth.data?.signed_in ? (auth.data.username ?? (auth.data.mode === "api_key" ? "API key" : "Signed in")) : "Signed out";

  return (
    <div className="flex h-16 shrink-0 items-center gap-3 border-b border-border px-6">
      <button
        type="button"
        onClick={() => setPaletteOpen(true)}
        className="flex h-10 w-full max-w-[520px] items-center gap-3 rounded-xl border border-border bg-surface-1 px-3.5 text-left text-sm text-faint transition-colors hover:border-border-strong hover:text-muted"
      >
        <Search className="size-4" />
        <span className="flex-1">Search games, pages and actions…</span>
        <Kbd>Ctrl K</Kbd>
      </button>
      <div className="flex-1" />
      <button
        type="button"
        onClick={() => navigate({ to: "/settings" })}
        className="flex h-9 items-center gap-2 rounded-full border border-border bg-surface-1 pl-1 pr-3 text-[13px] text-muted hover:text-text"
        title="Account settings"
      >
        <span className="grid size-7 place-items-center rounded-full bg-accent/15 text-accent">
          <UserRound className="size-4" />
        </span>
        <span className="max-w-[160px] truncate">{who}</span>
        <span className={`size-2 rounded-full ${auth.data?.signed_in ? "bg-success" : "bg-faint"}`} aria-hidden />
      </button>
    </div>
  );
}

export function AppShell() {
  useBackendEvents();
  return (
    <div className="flex h-full">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <Topbar />
        <main className="min-h-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[1480px] px-6 py-6 lg:px-8">
            <Outlet />
          </div>
        </main>
      </div>
      <CommandPalette />
    </div>
  );
}
