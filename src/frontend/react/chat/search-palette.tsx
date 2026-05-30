import { useEffect, useMemo, useRef, useState } from "react";
import { IconChat, IconSearch } from "../components/icons";
import { cn } from "../lib/cn";
import type { Conversation } from "../types";

export function SearchPalette({
  conversations,
  activeId,
  onSelect,
  onClose,
}: {
  conversations: Conversation[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return conversations.slice(0, 20);
    return conversations
      .filter((c) => c.title.toLowerCase().includes(q))
      .slice(0, 20);
  }, [conversations, query]);

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center px-4 pt-[15vh]">
      <button
        type="button"
        className="absolute inset-0 bg-black/40 backdrop-blur-[2px]"
        onClick={onClose}
        aria-label="Close"
      />
      <div className="relative z-10 w-full max-w-[520px] overflow-hidden rounded-2xl border border-border/60 bg-popover shadow-[0_24px_56px_-10px_rgba(0,0,0,0.6),0_0_0_1px_rgba(255,255,255,0.04)]">
        <div className="flex items-center gap-3 border-b border-border/50 px-4 py-3">
          <IconSearch size={15} className="shrink-0 text-muted-foreground/50" />
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search conversations…"
            className="min-w-0 flex-1 bg-transparent text-[13.5px] text-foreground placeholder:text-muted-foreground/45 outline-none"
          />
          <kbd className="shrink-0 rounded border border-border/60 bg-muted/50 px-1.5 py-0.5 font-mono text-[10px] text-muted-foreground/50">
            esc
          </kbd>
        </div>
        <div className="max-h-[320px] overflow-y-auto p-2">
          {filtered.length === 0 ? (
            <p className="px-3 py-4 text-center text-[12.5px] text-muted-foreground/45">
              No conversations found.
            </p>
          ) : (
            filtered.map((c) => (
              <button
                key={c.id}
                type="button"
                onClick={() => onSelect(c.id)}
                className={cn(
                  "flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left text-[13px] transition-colors",
                  c.id === activeId
                    ? "bg-foreground/[0.08] text-foreground"
                    : "text-foreground/80 hover:bg-foreground/[0.05]",
                )}
              >
                <IconChat size={14} className="shrink-0 text-muted-foreground/50" />
                <span className="min-w-0 flex-1 truncate">{c.title}</span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
