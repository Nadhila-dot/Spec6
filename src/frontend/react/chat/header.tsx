import { useEffect, useRef, useState } from "react";
import {
  IconChevronDown,
  IconMenu,
  IconMic,
  IconPencil,
  IconTrash,
} from "../components/icons";
import { cn } from "../lib/cn";
import type { Conversation } from "../types";

export function ChatHeader({
  conversation,
  onOpenMobile,
  onRename,
  onDelete,
  onOpenVoice,
}: {
  conversation: Conversation | null;
  onOpenMobile: () => void;
  onRename: (id: string, nextTitle: string) => void;
  onDelete: (id: string) => void;
  onOpenVoice?: () => void;
}) {
  return (
    <header className="flex h-12 shrink-0 items-center gap-1 bg-background px-2 sm:px-3">
      <button
        type="button"
        onClick={onOpenMobile}
        aria-label="Open sidebar"
        className="grid h-8 w-8 place-items-center rounded-lg text-muted-foreground/65 hover:bg-card/60 hover:text-foreground md:hidden"
      >
        <IconMenu size={16} />
      </button>
      <div className="min-w-0 flex-1">
        <TitleMenu conversation={conversation} onRename={onRename} onDelete={onDelete} />
      </div>
      {onOpenVoice && (
        <button
          type="button"
          onClick={onOpenVoice}
          title="Talk to Sentinel"
          className="group relative inline-flex h-8 items-center gap-1.5 overflow-hidden rounded-full bg-card px-3 text-[12px] font-semibold tracking-tight text-foreground/85 ring-1 ring-border/70 transition hover:text-foreground hover:ring-border"
        >
          <span
            aria-hidden
            className="pointer-events-none absolute inset-0 z-0 rounded-full opacity-[0.05] transition-opacity group-hover:opacity-[0.1]"
            style={{
              backgroundImage:
                "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
            }}
          />
          <span className="relative z-10 inline-flex items-center gap-1.5">
            <IconMic size={14} />
            <span className="hidden sm:inline">Voice</span>
          </span>
        </button>
      )}
    </header>
  );
}

function TitleMenu({
  conversation,
  onRename,
  onDelete,
}: {
  conversation: Conversation | null;
  onRename: (id: string, nextTitle: string) => void;
  onDelete: (id: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

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

  const disabled = !conversation;
  const title = conversation?.title ?? "New chat";

  const handleRename = () => {
    if (!conversation) return;
    setOpen(false);
    const next = window.prompt("Rename chat", conversation.title);
    if (next && next.trim() && next.trim() !== conversation.title)
      onRename(conversation.id, next.trim());
  };

  const handleDelete = () => {
    if (!conversation) return;
    setOpen(false);
    onDelete(conversation.id);
  };

  return (
    <div ref={wrapRef} className="relative inline-block max-w-full">
      <button
        type="button"
        disabled={disabled}
        onClick={() => setOpen((v) => !v)}
        className={cn(
          "group inline-flex h-8 max-w-full items-center gap-1.5 rounded-lg px-2 text-[13.5px] font-medium tracking-tight",
          disabled
            ? "cursor-default text-muted-foreground/65"
            : "text-foreground hover:bg-card/60",
        )}
      >
        <span className="truncate">{title}</span>
        {!disabled && (
          <IconChevronDown
            size={13}
            className={cn(
              "shrink-0 text-muted-foreground/55 transition-transform group-hover:text-foreground",
              open && "rotate-180",
            )}
          />
        )}
      </button>

      {open && conversation && (
        <div className="absolute left-0 top-full z-30 mt-1.5 w-[220px] overflow-hidden rounded-2xl border border-border/60 bg-popover p-2 shadow-[0_24px_56px_-10px_rgba(0,0,0,0.55),0_0_0_1px_rgba(255,255,255,0.04)]">
          <MenuItem onClick={handleRename} icon={<IconPencil size={14} />}>
            Rename
          </MenuItem>
          <div className="my-1 h-px bg-border/60" />
          <MenuItem onClick={handleDelete} icon={<IconTrash size={14} />} destructive>
            Delete chat
          </MenuItem>
        </div>
      )}
    </div>
  );
}

function MenuItem({
  children,
  icon,
  onClick,
  destructive,
}: {
  children: React.ReactNode;
  icon: React.ReactNode;
  onClick: () => void;
  destructive?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex h-9 w-full items-center gap-2.5 rounded-lg px-2.5 text-[13px] font-medium tracking-tight",
        destructive
          ? "text-red-400 hover:bg-red-500/[0.08]"
          : "text-foreground hover:bg-foreground/[0.08]",
      )}
    >
      <span className="text-current opacity-80">{icon}</span>
      <span>{children}</span>
    </button>
  );
}
