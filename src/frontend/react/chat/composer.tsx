import { useEffect, useRef, useState } from "react";
import { Button } from "../components/button";
import { IconChevronDown, IconSend } from "../components/icons";
import { cn } from "../lib/cn";
import type { PickerProps } from "./types";

export function ComposerBar({
  onSend,
  onStop,
  pendingReply,
  error,
  compact,
  body,
  onBodyChange,
  placeholder = "Reply to the conversation…",
  minRows = 1,
  picker,
}: {
  onSend: (body: string) => Promise<void> | void;
  onStop: () => void;
  pendingReply: boolean;
  error: string | null;
  compact?: boolean;
  body: string;
  onBodyChange: (next: string) => void;
  placeholder?: string;
  minRows?: number;
  picker: PickerProps;
}) {
  const taRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = taRef.current;
    if (!el) return;
    el.style.height = "";
    el.style.height = Math.min(220, el.scrollHeight) + "px";
  }, [body, minRows]);

  useEffect(() => {
    if (!body) return;
    const el = taRef.current;
    if (!el) return;
    if (document.activeElement !== el) {
      el.focus({ preventScroll: true });
      const len = el.value.length;
      el.setSelectionRange(len, len);
    }
  }, [body]);

  const submit = async (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    if (pendingReply) {
      onStop();
      return;
    }
    const trimmed = body.trim();
    if (!trimmed) return;
    onBodyChange("");
    await onSend(trimmed);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      submit();
    }
  };

  return (
    <div className={cn("w-full", compact ? "bg-background px-3 pb-4 pt-2 sm:px-5" : "")}>
      <form onSubmit={submit} className="mx-auto flex w-full max-w-4xl flex-col gap-2">
        {error && (
          <div className="rounded-lg bg-red-500/[0.08] px-3 py-2 text-[12px] text-red-400 ring-1 ring-red-500/25">
            {error}
          </div>
        )}
        <div
          className={cn(
            "diagonal-line-card rounded-2xl border border-border p-2",
            "shadow-[0_10px_32px_-14px_rgba(0,0,0,0.45)] transition-shadow duration-200",
            "focus-within:shadow-[0_18px_44px_-14px_rgba(0,0,0,0.6)]",
          )}
        >
          <div
            className={cn(
              "flex flex-col rounded-xl border border-border bg-card transition-[border-color]",
              "focus-within:border-foreground/20",
            )}
          >
            <textarea
              ref={taRef}
              value={body}
              onChange={(e) => onBodyChange(e.target.value)}
              onKeyDown={onKeyDown}
              rows={minRows}
              placeholder={placeholder}
              className={cn(
                "block w-full resize-none bg-transparent",
                "px-4 pt-4 pb-2 text-[14.5px] leading-[1.55] tracking-tight text-foreground",
                "placeholder:text-muted-foreground/45 outline-none min-h-[56px]",
              )}
            />
            <div className="flex items-center justify-between gap-3 px-2.5 pb-2.5 pt-1">
              <ModelPickerPill {...picker} />
              <Button
                type="submit"
                size="default"
                className="rounded-xl"
                disabled={!pendingReply && body.trim().length === 0}
              >
                <IconSend size={13} />
                <span>{pendingReply ? "Cancel" : "Send"}</span>
              </Button>
            </div>
          </div>
        </div>
      </form>
    </div>
  );
}

export function ModelPickerPill({
  catalog,
  catalogLoading,
  selectedProviderId,
  selectedModelId,
  onProviderChange,
  onModelChange,
}: PickerProps) {
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

  const availableProviders = catalog?.providers.filter((p) => p.available) ?? [];
  const selectedProvider = availableProviders.find((p) => p.id === selectedProviderId);
  const selectedModel = selectedProvider?.models.find((m) => m.id === selectedModelId);

  if (catalogLoading) {
    return (
      <span className="text-[11px] tracking-tight text-muted-foreground/55">
        Loading models…
      </span>
    );
  }
  if (availableProviders.length === 0) {
    return (
      <span className="text-[11px] tracking-tight text-red-400/90">
        No models configured
      </span>
    );
  }

  const rawLabel = selectedModel?.label ?? "Select model";
  const slashIdx = rawLabel.lastIndexOf("/");
  const displayLabel = slashIdx === -1 ? rawLabel : rawLabel.slice(slashIdx + 1);

  return (
    <div ref={wrapRef} className="relative min-w-0">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        title={rawLabel}
        className={cn(
          "group/pill relative isolate inline-flex h-8 max-w-[240px] items-center gap-2 overflow-hidden rounded-full px-3 text-[12px] tracking-tight",
          "bg-foreground/[0.04] text-foreground ring-1 ring-border/60",
          "shadow-[0_1px_2px_rgba(0,0,0,0.04)] transition-colors hover:bg-foreground/[0.07] hover:ring-border",
        )}
      >
        <span
          aria-hidden
          className="pointer-events-none absolute inset-0 z-0 rounded-full opacity-[0.05] transition-opacity group-hover/pill:opacity-[0.09]"
          style={{
            backgroundImage:
              "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
          }}
        />
        <span className="relative z-10 inline-flex min-w-0 items-center gap-1.5">
          <span className="shrink-0 text-[10px] font-bold uppercase tracking-[0.13em] text-muted-foreground/55">
            Model
          </span>
          <span className="truncate font-medium text-foreground/95">{displayLabel}</span>
          <IconChevronDown
            size={11}
            className={cn(
              "shrink-0 text-muted-foreground/55 transition-transform",
              open && "rotate-180",
            )}
          />
        </span>
      </button>

      {open && (
        <div className="absolute left-0 bottom-full z-30 mb-2 w-[280px] overflow-hidden rounded-2xl border border-border/60 bg-popover p-2 shadow-[0_24px_56px_-10px_rgba(0,0,0,0.55),0_0_0_1px_rgba(255,255,255,0.04)]">
          <div className="max-h-[320px] overflow-y-auto">
            {availableProviders.map((provider, idx) => (
              <div key={provider.id} className={cn(idx > 0 && "mt-1")}>
                <div className="px-2.5 pt-1.5 pb-1 text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
                  {provider.label}
                </div>
                {provider.models.map((model) => {
                  const isSelected =
                    provider.id === selectedProviderId && model.id === selectedModelId;
                  const sIdx = model.label.lastIndexOf("/");
                  const cleanLabel = sIdx === -1 ? model.label : model.label.slice(sIdx + 1);
                  return (
                    <button
                      key={model.id}
                      type="button"
                      onClick={() => {
                        if (provider.id !== selectedProviderId) onProviderChange(provider.id);
                        onModelChange(model.id);
                        setOpen(false);
                      }}
                      title={model.label}
                      className={cn(
                        "flex w-full items-center justify-between gap-2 rounded-lg px-2.5 py-1.5 text-left text-[12.5px] tracking-tight",
                        isSelected
                          ? "bg-foreground/[0.08] text-foreground"
                          : "text-muted-foreground/85 hover:bg-foreground/[0.05] hover:text-foreground",
                      )}
                    >
                      <span className="truncate">{cleanLabel}</span>
                      {isSelected && (
                        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-foreground/80" />
                      )}
                    </button>
                  );
                })}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
