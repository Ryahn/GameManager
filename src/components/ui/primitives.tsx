/** Small presentational building blocks shared across features. */
import { forwardRef, type HTMLAttributes, type InputHTMLAttributes } from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { AlertTriangle, RefreshCw, type LucideIcon } from "lucide-react";
import { Dialog as D, Tooltip as T } from "radix-ui";
import { X } from "lucide-react";
import { cn } from "@/lib/utils";
import { friendlyError } from "@/lib/errors";
import { Button } from "./button";

// ------------------------------------------------------------------ Input

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement>>(({ className, ...props }, ref) => (
  <input
    ref={ref}
    className={cn(
      "h-10 w-full rounded-lg border border-border bg-surface-1 px-3 text-sm text-text placeholder:text-faint",
      "transition-colors hover:border-border-strong focus:border-accent focus:outline-none focus-visible:outline-none focus:ring-2 focus:ring-accent/30",
      "disabled:opacity-50",
      className,
    )}
    {...props}
  />
));
Input.displayName = "Input";

// ------------------------------------------------------------------ Badge

const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-md px-1.5 py-0.5 text-[11px] font-semibold leading-tight tracking-wide",
  {
    variants: {
      tone: {
        neutral: "bg-surface-3 text-muted",
        accent: "bg-accent/15 text-accent",
        success: "bg-success/15 text-success",
        warning: "bg-warning/15 text-warning",
        danger: "bg-danger/15 text-danger",
        info: "bg-info/15 text-info",
        solid: "bg-black/65 text-white backdrop-blur-sm",
      },
    },
    defaultVariants: { tone: "neutral" },
  },
);

export function Badge({ className, tone, ...props }: HTMLAttributes<HTMLSpanElement> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ tone }), className)} {...props} />;
}

// ---------------------------------------------------------------- Skeleton

export function Skeleton({ className }: { className?: string }) {
  return <div aria-hidden className={cn("skeleton rounded-lg", className)} />;
}

// --------------------------------------------------------------------- Kbd

export function Kbd({ children, className }: { children: React.ReactNode; className?: string }) {
  return (
    <kbd className={cn("rounded border border-border bg-surface-2 px-1.5 py-0.5 font-sans text-[11px] text-faint", className)}>
      {children}
    </kbd>
  );
}

// -------------------------------------------------------------------- Card

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("rounded-[var(--radius-card)] border border-border bg-surface-1", className)} {...props} />;
}

export function Section({
  title,
  description,
  children,
  actions,
}: {
  title: string;
  description?: React.ReactNode;
  children: React.ReactNode;
  actions?: React.ReactNode;
}) {
  return (
    <Card className="p-5">
      <div className="mb-4 flex items-start justify-between gap-4">
        <div>
          <h3 className="text-[15px] font-semibold">{title}</h3>
          {description && <p className="mt-0.5 text-sm text-muted">{description}</p>}
        </div>
        {actions}
      </div>
      {children}
    </Card>
  );
}

// ----------------------------------------------------------------- Tooltip

export function Tooltip({
  content,
  children,
  side = "right",
}: {
  content: React.ReactNode;
  children: React.ReactNode;
  side?: "top" | "right" | "bottom" | "left";
}) {
  return (
    <T.Root delayDuration={300}>
      <T.Trigger asChild>{children}</T.Trigger>
      <T.Portal>
        <T.Content
          side={side}
          sideOffset={8}
          className="z-50 rounded-md border border-border bg-surface-3 px-2 py-1 text-xs text-text shadow-card animate-fade-up"
        >
          {content}
        </T.Content>
      </T.Portal>
    </T.Root>
  );
}

export const TooltipProvider = T.Provider;

// ------------------------------------------------------------------ Dialog

export const Dialog = D.Root;
export const DialogClose = D.Close;

export function DialogContent({
  className,
  children,
  title,
  description,
}: {
  className?: string;
  children: React.ReactNode;
  title: string;
  description?: string;
}) {
  return (
    <D.Portal>
      <D.Overlay className="fixed inset-0 z-40 bg-overlay backdrop-blur-[2px]" />
      <D.Content
        className={cn(
          "fixed left-1/2 top-1/2 z-50 w-[min(92vw,520px)] -translate-x-1/2 -translate-y-1/2 rounded-2xl border border-border bg-surface-1 p-6 shadow-card focus:outline-none",
          className,
        )}
      >
        <D.Title className="text-lg font-semibold">{title}</D.Title>
        <D.Description className={description ? "mt-1 text-sm text-muted" : "sr-only"}>{description ?? title}</D.Description>
        <div className="mt-4">{children}</div>
        <D.Close className="absolute right-4 top-4 rounded-md p-1 text-faint hover:bg-surface-3 hover:text-text" aria-label="Close">
          <X className="size-4" />
        </D.Close>
      </D.Content>
    </D.Portal>
  );
}

// ------------------------------------------------------------- EmptyState

export function EmptyState({
  icon: Icon,
  title,
  children,
  action,
}: {
  icon: LucideIcon;
  title: string;
  children?: React.ReactNode;
  action?: React.ReactNode;
}) {
  return (
    <div className="mx-auto flex max-w-md flex-col items-center px-6 py-16 text-center animate-fade-up">
      <div className="mb-4 grid size-14 place-items-center rounded-2xl bg-accent/12 text-accent">
        <Icon className="size-7" />
      </div>
      <h3 className="text-lg font-semibold">{title}</h3>
      {children && <div className="mt-2 text-sm leading-relaxed text-muted">{children}</div>}
      {action && <div className="mt-6">{action}</div>}
    </div>
  );
}

// -------------------------------------------------------------- ErrorPanel

export function ErrorPanel({ error, onRetry }: { error: unknown; onRetry?: () => void }) {
  const f = friendlyError(error);
  return (
    <div role="alert" className="flex items-start gap-3 rounded-xl border border-danger/30 bg-danger/8 p-4">
      <AlertTriangle className="mt-0.5 size-5 shrink-0 text-danger" />
      <div className="min-w-0 flex-1">
        <p className="font-medium">{f.title}</p>
        <p className="mt-0.5 text-sm text-muted">{f.detail}</p>
      </div>
      {onRetry && f.action !== "wait" && (
        <Button size="sm" variant="ghost" onClick={onRetry}>
          <RefreshCw /> Retry
        </Button>
      )}
    </div>
  );
}

// ------------------------------------------------------------- PageHeader

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: React.ReactNode; actions?: React.ReactNode }) {
  return (
    <header className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="font-[family-name:var(--font-display)] text-[26px] font-semibold tracking-tight">{title}</h1>
        {subtitle && <p className="mt-1 text-sm text-muted">{subtitle}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </header>
  );
}

// ------------------------------------------------------- SegmentedControl

export function Segmented<T extends string>({
  value,
  onChange,
  options,
  label,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: React.ReactNode; title?: string }[];
  label: string;
}) {
  return (
    <div role="radiogroup" aria-label={label} className="inline-flex rounded-lg border border-border bg-surface-1 p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          title={o.title}
          onClick={() => onChange(o.value)}
          className={cn(
            "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-[13px] font-medium transition-colors [&_svg]:size-4",
            value === o.value ? "bg-surface-3 text-text shadow-sm" : "text-muted hover:text-text",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

// ------------------------------------------------------------------ Switch

export function Switch({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cn(
        "relative inline-flex h-6 w-10 shrink-0 items-center rounded-full border transition-colors",
        checked ? "border-accent bg-accent" : "border-border-strong bg-surface-3",
      )}
    >
      <span
        className={cn(
          "inline-block size-4 rounded-full bg-white shadow transition-transform",
          checked ? "translate-x-[19px]" : "translate-x-[3px]",
        )}
      />
    </button>
  );
}
