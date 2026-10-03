import * as Dialog from "@radix-ui/react-dialog";
import type { ComponentProps, ReactNode } from "react";
import { hueOf } from "../lib/format.js";

const cx = (...c: (string | false | undefined | null)[]): string => c.filter(Boolean).join(" ");

type Variant = "primary" | "secondary" | "ghost" | "danger";
const variants: Record<Variant, string> = {
  primary: "bg-accent text-on-accent hover:opacity-90 disabled:opacity-50",
  secondary: "bg-raised border border-edge hover:bg-panel disabled:opacity-50",
  ghost: "hover:bg-panel text-muted hover:text-ink disabled:opacity-50",
  danger: "bg-danger text-on-accent hover:opacity-90 disabled:opacity-50",
};

export function Button({
  variant = "secondary",
  small,
  className,
  ...p
}: ComponentProps<"button"> & { variant?: Variant; small?: boolean }): React.JSX.Element {
  return (
    <button
      {...p}
      className={cx(
        "inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors",
        small ? "px-2 py-1 text-xs" : "px-3 py-1.5 text-sm",
        variants[variant],
        className,
      )}
    />
  );
}

export const inputClass =
  "w-full rounded-md border border-edge bg-raised px-2.5 py-1.5 text-sm placeholder:text-muted/70 focus:border-accent focus:outline-none";

export function Input(p: ComponentProps<"input">): React.JSX.Element {
  return <input {...p} className={cx(inputClass, p.className)} />;
}
export function Textarea(p: ComponentProps<"textarea">): React.JSX.Element {
  return <textarea {...p} className={cx(inputClass, "resize-y", p.className)} />;
}
export function Select(p: ComponentProps<"select">): React.JSX.Element {
  return <select {...p} className={cx(inputClass, p.className)} />;
}

export function Field({ label, hint, children }: { label: string; hint?: string | undefined; children: ReactNode }): React.JSX.Element {
  return (
    <label className="block space-y-1">
      <span className="text-xs font-medium text-muted">{label}</span>
      {children}
      {hint && <span className="block text-xs text-muted">{hint}</span>}
    </label>
  );
}

export function Card({ className, ...p }: ComponentProps<"div">): React.JSX.Element {
  return <div {...p} className={cx("rounded-lg border border-edge bg-raised", className)} />;
}

export function Chip({ children, tone = "neutral", title }: { children: ReactNode; tone?: "neutral" | "good" | "warn" | "danger" | "accent"; title?: string }): React.JSX.Element {
  const tones = {
    neutral: "bg-panel text-muted",
    good: "bg-good/10 text-good",
    warn: "bg-warn-soft text-warn",
    danger: "bg-danger-soft text-danger",
    accent: "bg-accent-soft text-accent",
  } as const;
  return (
    <span title={title} className={cx("inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium", tones[tone])}>
      {children}
    </span>
  );
}

/** A role, the same colour wherever it appears. */
export function RoleChip({ name, model, state }: { name: string; model?: string | null | undefined; state?: "working" | "waiting" | "blocked" | "done" | undefined }): React.JSX.Element {
  const h = hueOf(name);
  return (
    <span
      className="role-chip inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium"
      style={{ "--h": h } as React.CSSProperties}
    >
      <span className="dot h-1.5 w-1.5 rounded-full" />
      {name}
      {model && <span className="opacity-60">· {model}</span>}
      {state && state !== "done" && <span className="opacity-70">· {state}</span>}
    </span>
  );
}

export function Spinner({ label }: { label?: string }): React.JSX.Element {
  return (
    <span role="status" className="inline-flex items-center gap-2 text-sm text-muted">
      <span className="h-3 w-3 animate-spin rounded-full border-2 border-edge border-t-accent" />
      {label}
    </span>
  );
}

export function Banner({ tone = "warn", children, action }: { tone?: "warn" | "danger" | "accent"; children: ReactNode; action?: ReactNode }): React.JSX.Element {
  const t = { warn: "bg-warn-soft text-ink border-warn/30", danger: "bg-danger-soft text-ink border-danger/30", accent: "bg-accent-soft text-ink border-accent/30" }[tone];
  return (
    <div role={tone === "danger" ? "alert" : "status"} className={cx("flex items-start justify-between gap-3 rounded-lg border px-3 py-2 text-sm", t)}>
      <div className="min-w-0">{children}</div>
      {action}
    </div>
  );
}

export function Empty({ title, children, action }: { title: string; children?: ReactNode; action?: ReactNode }): React.JSX.Element {
  return (
    <div className="mx-auto max-w-sm py-12 text-center">
      <p className="font-medium">{title}</p>
      {children && <p className="mt-1 text-sm text-muted">{children}</p>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function ErrorText({ children }: { children: ReactNode }): React.JSX.Element | null {
  return children ? <p role="alert" className="text-sm text-danger">{children}</p> : null;
}

export function Modal({
  open,
  onOpenChange,
  title,
  description,
  children,
  wide,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  title: string;
  description?: string;
  children: ReactNode;
  wide?: boolean;
}): React.JSX.Element {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/40" />
        <Dialog.Content
          className={cx(
            "fixed left-1/2 top-1/2 z-50 max-h-[85vh] w-[calc(100vw-2rem)] -translate-x-1/2 -translate-y-1/2 overflow-auto rounded-xl border border-edge bg-raised p-5 shadow-xl",
            wide ? "max-w-2xl" : "max-w-md",
          )}
        >
          <Dialog.Title className="text-base font-semibold">{title}</Dialog.Title>
          {description ? (
            <Dialog.Description className="mt-1 text-sm text-muted">{description}</Dialog.Description>
          ) : (
            <Dialog.Description className="sr-only">{title}</Dialog.Description>
          )}
          <div className="mt-4">{children}</div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** A destructive action that must be confirmed in words, not a reflexive click. */
export function Confirm({
  open,
  onOpenChange,
  title,
  body,
  confirmLabel,
  onConfirm,
  danger,
  pending,
  error,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  title: string;
  body: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  danger?: boolean;
  pending?: boolean;
  error?: string | null;
}): React.JSX.Element {
  return (
    <Modal open={open} onOpenChange={onOpenChange} title={title}>
      <div className="space-y-4 text-sm">
        <div>{body}</div>
        <ErrorText>{error}</ErrorText>
        <div className="flex justify-end gap-2">
          <Button onClick={() => onOpenChange(false)}>Cancel</Button>
          <Button variant={danger ? "danger" : "primary"} onClick={onConfirm} disabled={pending}>
            {confirmLabel}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
