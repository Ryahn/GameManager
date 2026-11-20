import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { Toaster } from "sonner";
import { TooltipProvider } from "@/components/ui/primitives";
import { router } from "@/app/router";
import type { ApiError } from "@/lib/ipc";
import "@/styles/index.css";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      // Never hammer the API: retry only transient failures, and never while rate limited.
      retry: (count, error) => {
        const kind = (error as unknown as ApiError)?.kind;
        return count < 2 && (kind === "network" || kind === "server");
      },
      retryDelay: (n) => 2000 * 2 ** n,
    },
  },
});

async function boot() {
  // Running in a plain browser during development: fake the Rust side.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    const { installMockIpc } = await import("@/dev/mockIpc");
    installMockIpc();
  }

  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <TooltipProvider>
          <RouterProvider router={router} />
          <Toaster
            position="bottom-right"
            theme="system"
            toastOptions={{
              className: "!bg-surface-2 !border-border !text-text",
              descriptionClassName: "!text-muted",
            }}
          />
        </TooltipProvider>
      </QueryClientProvider>
    </React.StrictMode>,
  );
}

void boot();
