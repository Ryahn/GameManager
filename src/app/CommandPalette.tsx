import { useEffect, useState } from "react";
import { Command } from "cmdk";
import { useNavigate } from "@tanstack/react-router";
import { Dialog as D } from "radix-ui";
import { Compass, Download, Gamepad2, Home, Library, Loader2, Moon, RefreshCcw, Search, Settings, Sun } from "lucide-react";
import { useSearch, useSettings, useSignedIn, useUpdateSettings } from "@/hooks/queries";
import { useUi } from "@/state/ui";
import { Kbd } from "@/components/ui/primitives";

function useDebounced<T>(value: T, ms: number) {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

const groupCls =
  "[&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:py-2 [&_[cmdk-group-heading]]:text-[11px] [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-wider [&_[cmdk-group-heading]]:text-faint";

const itemCls =
  "flex h-10 cursor-default items-center gap-3 rounded-lg px-3 text-sm text-muted data-[selected=true]:bg-surface-3 data-[selected=true]:text-text [&_svg]:size-4";

export function CommandPalette() {
  const open = useUi((s) => s.paletteOpen);
  const setOpen = useUi((s) => s.setPaletteOpen);
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const debounced = useDebounced(query, 250);
  const signedIn = useSignedIn();
  const search = useSearch(signedIn ? debounced : "");
  const settings = useSettings();
  const update = useUpdateSettings();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen(!useUi.getState().paletteOpen);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setOpen]);

  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  const go = (fn: () => void) => {
    setOpen(false);
    fn();
  };

  const pages = [
    { label: "Home", icon: Home, to: "/" as const },
    { label: "Library", icon: Library, to: "/library" as const },
    { label: "Discover", icon: Compass, to: "/discover" as const },
    { label: "Updates", icon: RefreshCcw, to: "/updates" as const },
    { label: "Downloads", icon: Download, to: "/downloads" as const },
    { label: "Settings", icon: Settings, to: "/settings" as const },
  ];
  const dark = (settings.data?.theme ?? "dark") !== "light";
  const ql = query.trim().toLowerCase();
  const matchedPages = pages.filter((p) => !ql || p.label.toLowerCase().includes(ql));
  const showThemeAction = !ql || "switch theme dark light".includes(ql);

  return (
    <D.Root open={open} onOpenChange={setOpen}>
      <D.Portal>
        <D.Overlay className="fixed inset-0 z-40 bg-overlay backdrop-blur-[2px]" />
        <D.Content
          aria-describedby={undefined}
          className="fixed left-1/2 top-[14vh] z-50 w-[min(92vw,640px)] -translate-x-1/2 overflow-hidden rounded-2xl border border-border bg-surface-1 shadow-card animate-fade-up"
        >
          <D.Title className="sr-only">Command palette</D.Title>
          <Command shouldFilter={false} loop label="Command palette">
            <div className="flex items-center gap-3 border-b border-border px-4">
              <Search className="size-[18px] text-faint" />
              <Command.Input
                value={query}
                onValueChange={setQuery}
                placeholder="Search games, pages and actions…"
                className="h-14 flex-1 bg-transparent text-[15px] text-text placeholder:text-faint focus:outline-none focus-visible:outline-none"
              />
              {search.isFetching && <Loader2 className="size-4 animate-spin text-faint" />}
              <Kbd>Esc</Kbd>
            </div>
            <Command.List className="max-h-[56vh] overflow-y-auto p-2">
              <Command.Empty className="px-3 py-8 text-center text-sm text-faint">
                {query.trim().length < 2 ? "Type to search the catalog." : "No matches."}
              </Command.Empty>

              {!!search.data?.hits.length && (
                <Command.Group heading="Games" className={groupCls}>
                  {search.data.hits.slice(0, 8).map((h) => (
                    <Command.Item
                      key={h.thread_id}
                      value={`game-${h.thread_id}`}
                      onSelect={() => go(() => navigate({ to: "/game/$id", params: { id: String(h.thread_id) } }))}
                      className={itemCls}
                    >
                      <Gamepad2 />
                      <span className="flex-1 truncate text-text">{h.title}</span>
                      <span className="truncate text-xs text-faint">{h.creator}</span>
                    </Command.Item>
                  ))}
                </Command.Group>
              )}

              {matchedPages.length > 0 && (
              <Command.Group heading="Go to" className={groupCls}>
                {matchedPages
                  .map((p) => (
                    <Command.Item key={p.to} value={`page-${p.label}`} onSelect={() => go(() => navigate({ to: p.to }))} className={itemCls}>
                      <p.icon />
                      {p.label}
                    </Command.Item>
                  ))}
              </Command.Group>
              )}

              {showThemeAction && (
              <Command.Group heading="Actions" className={groupCls}>
                {(
                  <Command.Item
                    value="action-theme"
                    onSelect={() => go(() => update.mutate({ theme: dark ? "light" : "dark" }))}
                    className={itemCls}
                  >
                    {dark ? <Sun /> : <Moon />}
                    Switch to {dark ? "light" : "dark"} theme
                  </Command.Item>
                )}
              </Command.Group>
              )}
            </Command.List>
          </Command>
        </D.Content>
      </D.Portal>
    </D.Root>
  );
}
