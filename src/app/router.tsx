import { useEffect } from "react";
import { Outlet, createHashHistory, createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { Loader2 } from "lucide-react";
import { AppShell } from "./AppShell";
import { Onboarding } from "@/features/onboarding/Onboarding";
import { HomePage } from "@/features/home/HomePage";
import { LibraryPage } from "@/features/library/LibraryPage";
import { DiscoverPage } from "@/features/discover/DiscoverPage";
import { UpdatesPage } from "@/features/updates/UpdatesPage";
import { DownloadsPage } from "@/features/downloads/DownloadsPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { GamePage } from "@/features/game/GamePage";
import { ReaderPage } from "@/features/reader/ReaderPage";
import { PlayerPage } from "@/features/player/PlayerPage";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { useAuth, useSettings } from "@/hooks/queries";
import { api } from "@/lib/ipc";
import { LockScreen } from "./LockScreen";
import type { Settings } from "@/lib/ipc";

function applyAppearance(s: Settings) {
  const root = document.documentElement;
  const theme = s.theme === "system" ? (matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark") : s.theme;
  root.dataset.theme = theme;
  root.dataset.accent = s.accent;
  root.dataset.reduceMotion = String(s.reduce_motion);
  root.style.setProperty("--ui-scale", String(s.ui_scale));
  root.classList.toggle("blur-nsfw", s.blur_nsfw);
}

function Root() {
  const settings = useSettings();
  const auth = useAuth();
  const qc = useQueryClient();
  const sys = useQuery({ queryKey: ["system-info"], queryFn: api.systemInfo });

  useEffect(() => {
    const un = listen<boolean>("lock-changed", () => qc.invalidateQueries({ queryKey: ["system-info"] }));
    return () => void un.then((f) => f());
  }, [qc]);

  useEffect(() => {
    if (!settings.data) return;
    applyAppearance(settings.data);
    if (settings.data.theme !== "system") return;
    const mq = matchMedia("(prefers-color-scheme: light)");
    const onChange = () => settings.data && applyAppearance(settings.data);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, [settings.data]);

  if (!settings.data || !auth.data) {
    return (
      <div className="grid h-full place-items-center text-faint">
        <Loader2 className="size-6 animate-spin" aria-label="Loading" />
      </div>
    );
  }

  if (sys.data?.locked) return <LockScreen />;

  // Signed out, or setup not finished: show the wizard instead of the app.
  if (!settings.data.onboarding_complete || !auth.data.signed_in) return <Onboarding />;
  return <Outlet />;
}

const rootRoute = createRootRoute({ component: Root });
const appRoute = createRoute({ getParentRoute: () => rootRoute, id: "app", component: AppShell });

const routeTree = rootRoute.addChildren([
  appRoute.addChildren([
    createRoute({ getParentRoute: () => appRoute, path: "/", component: HomePage }),
    createRoute({ getParentRoute: () => appRoute, path: "/library", component: LibraryPage }),
    createRoute({ getParentRoute: () => appRoute, path: "/discover", component: DiscoverPage }),
    createRoute({ getParentRoute: () => appRoute, path: "/updates", component: UpdatesPage }),
    createRoute({ getParentRoute: () => appRoute, path: "/downloads", component: DownloadsPage }),
    createRoute({ getParentRoute: () => appRoute, path: "/settings", component: SettingsPage }),
    createRoute({ getParentRoute: () => appRoute, path: "/game/$id", component: GamePage }),
  ]),
  // Full screen, outside the sidebar layout (the lock screen still applies).
  createRoute({ getParentRoute: () => rootRoute, path: "/read/$id", component: ReaderPage }),
  createRoute({ getParentRoute: () => rootRoute, path: "/watch/$id", component: PlayerPage }),
]);

export const router = createRouter({ routeTree, history: createHashHistory(), defaultPreload: false });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
