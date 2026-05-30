import { useEffect, useRef, useState } from "react";
import { cn } from "../lib/cn";
import { HatchedChip } from "./diagonal";
import { IconSignOut } from "./icons";
import type { AuthUser } from "../types";

/**
 * Sidebar-footer identity card. Click to open an upward dropdown with the
 * sign-out row. Modelled on the Cntrl Panel workspace pattern.
 *
 * `compact` shrinks the trigger to just the hatched avatar chip — used in the
 * collapsed-sidebar rail.
 */
export function WorkspaceCard({
  user,
  compact = false,
}: {
  user: AuthUser;
  compact?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const initials = initialsOf(user.display_name || user.username);

  // Close on outside click / Escape.
  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (!wrapRef.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onClick);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onClick);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        title={compact ? `${user.display_name} (@${user.username})` : undefined}
        className={cn(
          "group flex items-center text-left ring-1 ring-border/60 hover:bg-card hover:ring-border",
          compact
            ? "mx-auto h-10 w-10 justify-center rounded-lg bg-card/60"
            : "w-full gap-2.5 rounded-lg bg-card/60 p-1.5",
        )}
      >
        <HatchedChip size={compact ? 28 : 32}>
          <span className="text-[10.5px] font-bold tracking-tight">
            {initials || "·"}
          </span>
        </HatchedChip>
        {!compact && (
          <>
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-[12.5px] font-semibold leading-4 text-foreground">
                {user.display_name}
              </span>
              <span className="truncate text-[10.5px] leading-4 text-muted-foreground/65">
                @{user.username}
              </span>
            </div>
            <ChevronIcon
              className={cn(
                "text-muted-foreground/55 transition-transform",
                open && "rotate-180",
              )}
            />
          </>
        )}
      </button>

      {open && (
        <div
          className={cn(
            "absolute bottom-full z-30 mb-2 w-[240px] overflow-hidden rounded-2xl border border-border/60 bg-popover p-2",
            "shadow-[0_24px_56px_-10px_rgba(0,0,0,0.55),0_0_0_1px_rgba(255,255,255,0.04)]",
            compact ? "left-0" : "left-0 right-0 w-auto",
          )}
        >
          <div className="flex items-center gap-2.5 rounded-xl bg-card/60 p-2.5 ring-1 ring-border/60">
            <HatchedChip size={36} className="rounded-[10.5px]">
              <span className="text-[11px] font-bold tracking-tight">
                {initials || "·"}
              </span>
            </HatchedChip>
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="truncate text-[13px] font-semibold leading-4 text-foreground">
                {user.display_name}
              </span>
              <span className="truncate text-[10.5px] leading-4 text-muted-foreground/65">
                @{user.username}
              </span>
            </div>
          </div>

          <div className="my-2 h-px bg-border/60" />

          <SignOutRow />
        </div>
      )}
    </div>
  );
}

function SignOutRow() {
  const onClick = async () => {
    await fetch("/api/auth/logout", {
      method: "POST",
      credentials: "include",
    });
    window.location.href = "/login";
  };
  return (
    <button
      onClick={onClick}
      className={cn(
        "flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-[13px] font-semibold",
        "text-red-400 hover:bg-red-500/[0.08]",
      )}
    >
      <IconSignOut size={16} />
      <span>Sign out</span>
    </button>
  );
}

function initialsOf(name: string): string {
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}

function ChevronIcon({ className }: { className?: string }) {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      className={className}
    >
      <path d="m6 15 6-6 6 6" />
    </svg>
  );
}
