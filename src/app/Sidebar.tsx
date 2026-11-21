import { Link, useRouterState } from "@tanstack/react-router";
import { Compass, Download, Home, Library, PanelLeftClose, PanelLeftOpen, RefreshCcw, Settings, type LucideIcon } from "lucide-react";
import { Tooltip } from "@/components/ui/primitives";
import { useAuth, useDownloadBadge, useUpdateInbox } from "@/hooks/queries";
import { useUi } from "@/state/ui";
import { cn } from "@/lib/utils";

interface NavItem {
  to: "/" | "/library" | "/discover" | "/updates" | "/downloads" | "/settings";
  label: string;
  icon: LucideIcon;
  badge?: number;
}

export function Logo({ compact }: { compact?: boolean }) {
  return (
    <div className="flex items-center gap-2.5">
      <div className="grid size-8 shrink-0 place-items-center rounded-[10px] bg-gradient-to-br from-accent to-accent-hover text-[13px] font-black text-accent-fg shadow-sm">
        F
      </div>
      {!compact && (
        <div className="leading-tight">
          <div className="text-[14px] font-semibold tracking-tight">Game Manager</div>
          <div className="text-[11px] text-faint">for F95zone</div>
        </div>
      )}
    </div>
  );
}

export function Sidebar() {
  const collapsed = useUi((s) => s.sidebarCollapsed);
  const toggle = useUi((s) => s.toggleSidebar);
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const auth = useAuth();
  const signedIn = auth.data?.signed_in ?? false;
  const updateCount = useUpdateInbox(signedIn).items.length;
  const downloadCount = useDownloadBadge();

  const items: NavItem[] = [
    { to: "/", label: "Home", icon: Home },
    { to: "/library", label: "Library", icon: Library },
    { to: "/discover", label: "Discover", icon: Compass },
    { to: "/updates", label: "Updates", icon: RefreshCcw, badge: updateCount },
    { to: "/downloads", label: "Downloads", icon: Download, badge: downloadCount },
  ];

  const isActive = (to: string) => (to === "/" ? pathname === "/" : pathname.startsWith(to));

  const renderItem = (item: NavItem) => {
    const active = isActive(item.to);
    const link = (
      <Link
        key={item.to}
        to={item.to}
        aria-current={active ? "page" : undefined}
        className={cn(
          "relative flex h-10 items-center gap-3 rounded-lg px-3 text-[14px] font-medium transition-colors",
          active ? "bg-surface-3 text-text" : "text-muted hover:bg-surface-2 hover:text-text",
          collapsed && "justify-center px-0",
        )}
      >
        {active && <span className="absolute left-0 top-2 bottom-2 w-[3px] rounded-r bg-accent" aria-hidden />}
        <item.icon className={cn("size-[18px] shrink-0", active && "text-accent")} />
        {!collapsed && <span className="flex-1 truncate">{item.label}</span>}
        {!!item.badge && (
          <span
            className={cn(
              "grid min-w-5 place-items-center rounded-full bg-accent px-1.5 text-[11px] font-bold leading-5 text-accent-fg",
              collapsed && "absolute right-1 top-1 min-w-4 px-1 text-[10px] leading-4",
            )}
          >
            {item.badge > 99 ? "99+" : item.badge}
          </span>
        )}
      </Link>
    );
    return collapsed ? (
      <Tooltip key={item.to} content={item.label}>
        {link}
      </Tooltip>
    ) : (
      link
    );
  };

  return (
    <aside
      className={cn(
        "flex h-full shrink-0 flex-col border-r border-border bg-surface-1 transition-[width] duration-200",
        collapsed ? "w-[68px]" : "w-[232px]",
      )}
    >
      <div className={cn("flex h-16 items-center", collapsed ? "justify-center" : "px-4")}>
        <Logo compact={collapsed} />
      </div>
      <nav aria-label="Main" className="flex flex-1 flex-col gap-1 px-3 pt-2">
        {items.map(renderItem)}
      </nav>
      <div className="flex flex-col gap-1 border-t border-border px-3 py-3">
        {renderItem({ to: "/settings", label: "Settings", icon: Settings })}
        <button
          type="button"
          onClick={toggle}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className={cn(
            "flex h-9 items-center gap-3 rounded-lg px-3 text-[13px] text-faint transition-colors hover:bg-surface-2 hover:text-text",
            collapsed && "justify-center px-0",
          )}
        >
          {collapsed ? <PanelLeftOpen className="size-[18px]" /> : <PanelLeftClose className="size-[18px]" />}
          {!collapsed && "Collapse"}
        </button>
      </div>
    </aside>
  );
}
