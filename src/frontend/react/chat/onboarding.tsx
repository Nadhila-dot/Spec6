import { useEffect, useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Button } from "../components/button";
import { IconArrowRight, IconMenu, IconPlus, IconTrash } from "../components/icons";
import { cn } from "../lib/cn";
import type { ChatGroup, CompanyOverview } from "../types";
import {
  DEMO_COMPANY_NAME,
  DEMO_ONBOARDING,
  EMPTY_ONBOARDING,
  type CompanyOnboarding,
  isOnboardingEmpty,
  parseOnboarding,
  serializeOnboarding,
} from "./onboarding-format";

/* ─── GroupEditorPane ────────────────────────────────────────────────────── */

export function GroupEditorPane({
  group,
  overview,
  saving,
  onSave,
  onSaveAndStartThread,
  onDelete,
  onOpenMobile,
}: {
  group: ChatGroup;
  overview: CompanyOverview | null;
  saving: boolean;
  onSave: (name: string, data_text: string) => Promise<void> | void;
  onSaveAndStartThread: (name: string, data_text: string) => Promise<void> | void;
  onDelete: () => void;
  onOpenMobile: () => void;
}) {
  const [name, setName] = useState(group.name);
  const [fields, setFields] = useState<CompanyOnboarding>(() =>
    parseOnboarding(group.data_text),
  );

  // Re-sync when switching groups
  useEffect(() => {
    setName(group.name);
    setFields(parseOnboarding(group.data_text));
  }, [group.id]);

  const initial = useMemo(
    () => ({ name: group.name, data: parseOnboarding(group.data_text) }),
    [group.id, group.name, group.data_text],
  );

  const dirty =
    name.trim() !== initial.name.trim() ||
    serializeOnboarding(fields) !== serializeOnboarding(initial.data);

  const loadDemo = () => {
    setName(DEMO_COMPANY_NAME);
    setFields(DEMO_ONBOARDING);
  };

  const clearAll = () => {
    setName("");
    setFields({ ...EMPTY_ONBOARDING });
  };

  const handleSave = () => onSave(name.trim(), serializeOnboarding(fields));
  const handleSaveAndStart = () =>
    onSaveAndStartThread(name.trim(), serializeOnboarding(fields));

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {/* mini header */}
      <header className="flex h-12 shrink-0 items-center gap-2 bg-background px-3">
        <button
          type="button"
          onClick={onOpenMobile}
          aria-label="Open sidebar"
          className="grid h-8 w-8 place-items-center rounded-lg text-muted-foreground/65 hover:bg-card/60 hover:text-foreground md:hidden"
        >
          <IconMenu size={16} />
        </button>
        <span className="min-w-0 flex-1 truncate text-[13.5px] font-medium tracking-tight text-muted-foreground/70">
          Company
        </span>
        <button
          type="button"
          onClick={onDelete}
          className="grid h-8 w-8 place-items-center rounded-lg text-muted-foreground/55 hover:bg-red-500/10 hover:text-red-400"
          aria-label="Delete company"
        >
          <IconTrash size={15} />
        </button>
      </header>

      <div className="flex-1 overflow-y-auto px-4 py-6 sm:px-8">
        <div className="mx-auto max-w-4xl space-y-6">
          {/* hero card with title */}
          <div className="diagonal-line-card rounded-2xl border border-border p-3 shadow-sm">
            <div className="rounded-xl border border-border bg-card px-6 py-5">
              <p className="text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
                Company onboarding
              </p>
              <h1 className="mt-1 font-chillax text-[26px] font-semibold leading-[1.1] tracking-tight text-foreground sm:text-[30px]">
                Build the company context
              </h1>
              <p className="mt-1.5 text-[12.5px] text-muted-foreground/65">
                This becomes the private context used for competitor research, ratings, and every chat in this company.
              </p>
            </div>
          </div>

          {/* fields — 2-column grid */}
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
            <OnboardingField
              label="Company name"
              value={name}
              onChange={setName}
              placeholder="e.g. Puma"
              singleLine
            />
            <OnboardingField
              label="Company URL"
              value={fields.url}
              onChange={(v) => setFields({ ...fields, url: v })}
              placeholder="https://example.com"
              singleLine
            />
            <OnboardingField
              label="What do you specialize in?"
              value={fields.specialty}
              onChange={(v) => setFields({ ...fields, specialty: v })}
              placeholder="Athletic footwear, football kits…"
            />
            <OnboardingField
              label="Who are the customers?"
              value={fields.customers}
              onChange={(v) => setFields({ ...fields, customers: v })}
              placeholder="Style-aware sneaker buyers, football fans…"
            />
            <OnboardingField
              label="Known competitors"
              value={fields.competitors}
              onChange={(v) => setFields({ ...fields, competitors: v })}
              placeholder="Nike, Adidas, New Balance…"
            />
            <OnboardingField
              label="Additional notes"
              value={fields.notes}
              onChange={(v) => setFields({ ...fields, notes: v })}
              placeholder="Pressure-test customer trust, product quality…"
            />
          </div>

          {/* demo strip */}
          <div className="diagonal-line-card rounded-2xl border border-border p-3">
            <div className="flex flex-col items-start justify-between gap-3 rounded-xl border border-border bg-card px-5 py-4 sm:flex-row sm:items-center">
              <p className="text-[12px] text-muted-foreground/60">
                Demo content is prefilled so you can test the pipeline without retyping the same company every time.
              </p>
              <div className="flex shrink-0 items-center gap-2">
                {!isOnboardingEmpty(fields) && (
                  <button
                    type="button"
                    onClick={clearAll}
                    className="text-[11.5px] font-medium tracking-tight text-muted-foreground/55 hover:text-muted-foreground"
                  >
                    Clear
                  </button>
                )}
                <button
                  type="button"
                  onClick={loadDemo}
                  className={cn(
                    "group relative isolate inline-flex h-8 items-center gap-2 overflow-hidden rounded-full px-3.5",
                    "bg-card text-[12.5px] font-medium tracking-tight text-foreground",
                    "ring-1 ring-border shadow-[0_1px_2px_rgba(0,0,0,0.05)] hover:ring-foreground/30",
                  )}
                >
                  <span
                    aria-hidden
                    className="pointer-events-none absolute inset-0 z-0 rounded-full opacity-[0.18] transition-opacity group-hover:opacity-[0.26]"
                    style={{
                      backgroundImage:
                        "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
                    }}
                  />
                  <span className="relative z-10">Load demo content</span>
                </button>
              </div>
            </div>
          </div>

          {/* action row */}
          <div className="flex flex-wrap items-center gap-3">
            <Button
              type="button"
              onClick={handleSave}
              disabled={saving || !name.trim() || !dirty}
              className="rounded-xl"
            >
              {saving ? "Saving…" : "Save company"}
            </Button>
            <Button
              type="button"
              onClick={handleSaveAndStart}
              disabled={saving || !name.trim()}
              className="rounded-xl"
            >
              <span>Start thread</span>
              <IconArrowRight size={13} />
            </Button>
            {dirty && (
              <span className="text-[11.5px] font-medium tracking-tight text-amber-400/80">
                Unsaved onboarding changes
              </span>
            )}
          </div>

          {/* overview section */}
          {overview && <OverviewSection overview={overview} />}
        </div>
      </div>
    </div>
  );
}

/* ─── OnboardingField ────────────────────────────────────────────────────── */

function OnboardingField({
  label,
  value,
  onChange,
  placeholder,
  singleLine,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  singleLine?: boolean;
}) {
  return (
    <div className="diagonal-line-card rounded-2xl border border-border p-3">
      <div className="flex h-full flex-col rounded-xl border border-border bg-card px-4 py-3">
        <label className="text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
          {label}
        </label>
        {singleLine ? (
          <input
            type="text"
            value={value}
            onChange={(e) => onChange(e.target.value)}
            placeholder={placeholder}
            className={cn(
              "mt-1.5 w-full bg-transparent text-[14px] font-medium tracking-tight text-foreground",
              "placeholder:font-normal placeholder:text-muted-foreground/40 outline-none",
            )}
          />
        ) : (
          <textarea
            value={value}
            onChange={(e) => onChange(e.target.value)}
            rows={4}
            placeholder={placeholder}
            className={cn(
              "mt-1.5 min-h-[96px] w-full flex-1 resize-y bg-transparent text-[13.5px] leading-[1.55] text-foreground",
              "placeholder:text-muted-foreground/40 outline-none",
            )}
          />
        )}
      </div>
    </div>
  );
}

/* ─── OverviewSection ────────────────────────────────────────────────────── */

function OverviewSection({ overview }: { overview: CompanyOverview }) {
  if (overview.status === "queued" || overview.status === "running") {
    return (
      <div className="diagonal-line-card rounded-2xl border border-border p-3">
        <div className="rounded-xl border border-border bg-card px-5 py-4">
          <div className="flex items-center gap-3">
            <span className="h-2 w-2 animate-pulse rounded-full bg-amber-400" />
            <span className="text-[13px] font-medium text-foreground">
              {overview.status === "queued"
                ? "Overview queued — research starting…"
                : "Running competitor research…"}
            </span>
          </div>
          {overview.discovered_competitors.length > 0 && (
            <div className="mt-4 space-y-1.5">
              <p className="text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
                Found so far
              </p>
              {overview.discovered_competitors.map((comp, i) => (
                <div
                  key={i}
                  className="flex items-center gap-2 text-[12.5px] text-foreground/80"
                >
                  <span className="h-1.5 w-1.5 rounded-full bg-foreground/40" />
                  {comp.name}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    );
  }

  if (overview.status === "failed") {
    return (
      <div className="rounded-2xl border border-red-500/25 bg-red-500/[0.06] p-5">
        <p className="text-[13px] font-medium text-red-400">Overview failed</p>
        {overview.failure_reason && (
          <p className="mt-1 text-[12px] text-muted-foreground/70">
            {overview.failure_reason}
          </p>
        )}
      </div>
    );
  }

  if (overview.status === "completed" && overview.markdown_brief) {
    return (
      <div className="diagonal-line-card rounded-2xl border border-border p-3">
        <div className="rounded-xl border border-border bg-card px-5 py-4">
          <p className="mb-3 text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
            Company overview
          </p>
          <div className="assistant-markdown text-[13px] leading-relaxed">
            <ReactMarkdown remarkPlugins={[remarkGfm]}>
              {overview.markdown_brief}
            </ReactMarkdown>
          </div>
        </div>
      </div>
    );
  }

  return null;
}
