import { useEffect, useMemo, useRef, useState, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { DiagonalAccent } from "../components/diagonal";
import {
  IconArrowRight,
  IconBook,
  IconChevronDown,
  IconClose,
  IconCode,
  IconCompass,
  IconFolder,
  IconLightbulb,
  IconPencil,
} from "../components/icons";
import { cn } from "../lib/cn";
import type { AuthUser, ChatGroup, ChatMessage, CompanyOverview } from "../types";
import { ComposerBar } from "./composer";
import { AgentWorkCard, CanvasDrawer, InlineCanvasMap } from "./canvas-drawer";
import {
  buildGreeting,
  extractMapPins,
  extractTrends,
  formatElapsed,
  isCogneeTool,
  sourceTypeLabel,
  splitThinking,
  stripFakeToolNarration,
} from "./utils";
import { type ParsedTrend, TrendChart, parseSpec6Trend } from "./trend-chart";
import { AutonomyBanner } from "./autonomy-banner";
import type {
  OverviewActivityEvent,
  PickerProps,
  ToolCallItem,
  ToolCallsByMessage,
} from "./types";

export function ThreadArea({
  activeId,
  messages,
  loading,
  pendingReply,
  onSend,
  onStop,
  error,
  user,
  composerBody,
  onComposerBodyChange,
  picker,
  chatGroups,
  activeCompany,
  activeCompanyOverview,
  activeCompanyActivity,
  liveToolCalls,
  toolCallsByMessage,
  onSelectGroup,
}: {
  activeId: string | null;
  messages: ChatMessage[];
  loading: boolean;
  pendingReply: boolean;
  onSend: (body: string) => Promise<void> | void;
  onStop: () => void;
  error: string | null;
  user: AuthUser;
  composerBody: string;
  onComposerBodyChange: (next: string) => void;
  picker: PickerProps;
  chatGroups: ChatGroup[];
  activeCompany: ChatGroup | null;
  activeCompanyOverview: CompanyOverview | null;
  activeCompanyActivity: OverviewActivityEvent[];
  liveToolCalls: ToolCallItem[];
  toolCallsByMessage: ToolCallsByMessage;
  onSelectGroup: (groupId: string) => void;
}) {
  const scrollerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = scrollerRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [messages.length, messages[messages.length - 1]?.body, pendingReply]);

  if (!activeId && messages.length === 0) {
    return (
      <div className="flex flex-1 flex-col overflow-hidden">
        <div className="flex flex-1 items-center justify-center px-4 py-6 sm:px-8">
          <EmptyHero
            user={user}
            onSend={onSend}
            onStop={onStop}
            pendingReply={pendingReply}
            error={error}
            body={composerBody}
            onBodyChange={onComposerBodyChange}
            picker={picker}
            chatGroups={chatGroups}
            onSelectGroup={onSelectGroup}
          />
        </div>
      </div>
    );
  }

  // Empty conversation with a company context — show starter prompt chips
  if (activeId && messages.length === 0 && !loading) {
    const overviewResearching =
      !!activeCompany &&
      (activeCompanyOverview?.status === "queued" ||
        activeCompanyOverview?.status === "running");
    if (!overviewResearching) {
      return (
        <div className="flex flex-1 flex-col overflow-hidden">
          {activeCompany && (
            <div className="flex shrink-0 items-center gap-2 border-b border-border/40 bg-background px-4 py-1.5">
              <IconFolder size={12} className="text-muted-foreground/50" />
              <span className="text-[11.5px] font-medium tracking-tight text-muted-foreground/60">
                {activeCompany.name}
              </span>
              <OverviewStatusBadge overview={activeCompanyOverview} />
              {activeCompanyOverview?.status === "completed" && (
                <OverviewDrawerTrigger overview={activeCompanyOverview} />
              )}
            </div>
          )}
          <div className="flex flex-1 items-center justify-center px-4 py-6 sm:px-8">
            <ConversationEmptyState
              company={activeCompany}
              onSend={onSend}
              onStop={onStop}
              pendingReply={pendingReply}
              error={error}
              body={composerBody}
              onBodyChange={onComposerBodyChange}
              picker={picker}
            />
          </div>
        </div>
      );
    }
  }

  // While a company's overview is still researching, replace the chat UI
  // entirely with a live "researching" card so the user can see what we're
  // looking up. Once it flips to completed/failed the chat unlocks.
  const overviewIsResearching =
    !!activeCompany &&
    (activeCompanyOverview?.status === "queued" ||
      activeCompanyOverview?.status === "running");

  if (overviewIsResearching && activeCompany) {
    return (
      <div className="flex flex-1 flex-col overflow-hidden">
        <div className="flex shrink-0 items-center gap-2 border-b border-border/40 bg-background px-4 py-1.5">
          <IconFolder size={12} className="text-muted-foreground/50" />
          <span className="text-[11.5px] font-medium tracking-tight text-muted-foreground/60">
            {activeCompany.name}
          </span>
          <OverviewStatusBadge overview={activeCompanyOverview} />
        </div>
        <div className="flex-1 overflow-y-auto px-4 py-8 sm:px-8">
          <div className="mx-auto max-w-2xl">
            <ResearchingCard
              company={activeCompany}
              overview={activeCompanyOverview}
              activity={activeCompanyActivity}
            />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-1 flex-col overflow-hidden">
      {activeCompany && (
        <div className="flex shrink-0 items-center gap-2 border-b border-border/40 bg-background px-4 py-1.5">
          <IconFolder size={12} className="text-muted-foreground/50" />
          <span className="text-[11.5px] font-medium tracking-tight text-muted-foreground/60">
            {activeCompany.name}
          </span>
          <OverviewStatusBadge overview={activeCompanyOverview} />
          {activeCompanyOverview?.status === "completed" && (
            <OverviewDrawerTrigger overview={activeCompanyOverview} />
          )}
        </div>
      )}
      <div ref={scrollerRef} className="flex-1 overflow-y-auto">
        <div className="mx-auto flex w-full max-w-4xl flex-col gap-7 px-4 pb-10 pt-8 sm:px-6">
          {loading && messages.length === 0 && <LoadingRow />}
          {messages.map((m) => (
            <MessageBubble
              key={m.id}
              message={m}
              toolCalls={toolCallsByMessage[m.id] ?? []}
              toolsRunning={
                m.id.startsWith("tmp-assistant-") && pendingReply ? liveToolCalls : []
              }
            />
          ))}
          {pendingReply &&
            !messages.some(
              (m) => m.role === "assistant" && m.id.startsWith("tmp-assistant-"),
            ) && <ThinkingRow />}
        </div>
      </div>

      <ComposerBar
        onSend={onSend}
        onStop={onStop}
        pendingReply={pendingReply}
        error={error}
        compact
        body={composerBody}
        onBodyChange={onComposerBodyChange}
        picker={picker}
      />
    </div>
  );
}

/* ─── empty state ────────────────────────────────────────────────────────── */

function EmptyHero({
  user,
  onSend,
  onStop,
  pendingReply,
  error,
  body,
  onBodyChange,
  picker,
  chatGroups,
  onSelectGroup,
}: {
  user: AuthUser;
  onSend: (body: string) => Promise<void> | void;
  onStop: () => void;
  pendingReply: boolean;
  error: string | null;
  body: string;
  onBodyChange: (next: string) => void;
  picker: PickerProps;
  chatGroups: ChatGroup[];
  onSelectGroup: (groupId: string) => void;
}) {
  const greeting = useMemo(
    () => buildGreeting(user.display_name || user.username),
    [user.display_name, user.username],
  );

  const chips: { label: string; prefill: string; icon: React.ReactNode }[] = [
    { label: "Write",    prefill: "Draft ",              icon: <IconPencil size={13} /> },
    { label: "Research", prefill: "Research ",           icon: <IconBook size={13} /> },
    { label: "Code",     prefill: "Write code that ",    icon: <IconCode size={13} /> },
    { label: "Analyze",  prefill: "Analyze ",            icon: <IconLightbulb size={13} /> },
    { label: "Plan",     prefill: "Outline a plan for ", icon: <IconCompass size={13} /> },
  ];

  return (
    <div className="flex w-full max-w-3xl flex-col items-center gap-8 text-center">
      <h1 className="font-chillax text-[34px] font-semibold leading-[1.05] tracking-tight text-foreground sm:text-[40px]">
        {greeting}
      </h1>

      <AutonomyBanner />

      {chatGroups.length > 0 && (
        <div className="flex flex-wrap items-center justify-center gap-2">
          {chatGroups.map((g) => (
            <CompanyPill key={g.id} name={g.name} onClick={() => onSelectGroup(g.id)} />
          ))}
        </div>
      )}

      <ComposerBar
        onSend={onSend}
        onStop={onStop}
        pendingReply={pendingReply}
        error={error}
        body={body}
        onBodyChange={onBodyChange}
        placeholder="Ask Spec6 anything."
        minRows={3}
        picker={picker}
      />

      <div className="flex flex-wrap items-center justify-center gap-2">
        {chips.map((c) => (
          <ChipButton
            key={c.label}
            icon={c.icon}
            label={c.label}
            onClick={() => onBodyChange(c.prefill)}
          />
        ))}
      </div>
    </div>
  );
}

/* ─── per-conversation empty state ──────────────────────────────────────── */

const COMPANY_PROMPTS: ((name: string) => string)[] = [
  (n) => `What's going wrong for ${n}?`,
  (n) => `Who are ${n}'s biggest competitors?`,
  (n) => `How do customers feel about ${n}?`,
  (n) => `What triggers should I set for ${n}?`,
  (n) => `What changed for ${n} in the last 7 days?`,
  (n) => `Which alerts for ${n} should go to Slack or Discord?`,
  (n) => `Where can ${n} improve?`,
  (n) => `What's ${n}'s market position right now?`,
  (n) => `Find bad reviews for ${n}`,
];

const GENERIC_PROMPTS = [
  "What's going wrong for this company?",
  "Who are the biggest competitors here?",
  "How do customers feel about this brand?",
  "What triggers should I set up for this company?",
  "What changed in the market this week?",
  "Where can they improve?",
];

function ConversationEmptyState({
  company,
  onSend,
  onStop,
  pendingReply,
  error,
  body,
  onBodyChange,
  picker,
}: {
  company: ChatGroup | null;
  onSend: (body: string) => Promise<void> | void;
  onStop: () => void;
  pendingReply: boolean;
  error: string | null;
  body: string;
  onBodyChange: (next: string) => void;
  picker: PickerProps;
}) {
  const prompts = company
    ? COMPANY_PROMPTS.map((fn) => fn(company.name))
    : GENERIC_PROMPTS;

  return (
    <div className="flex w-full max-w-3xl flex-col items-center gap-7 text-center">
      {company ? (
        <div className="flex flex-col items-center gap-1.5">
          <div className="inline-flex items-center gap-2 rounded-full bg-foreground/[0.04] px-3 py-1 ring-1 ring-border/50">
            <IconFolder size={12} className="text-muted-foreground/55" />
            <span className="text-[11.5px] font-medium tracking-tight text-muted-foreground/70">
              {company.name}
            </span>
          </div>
          <h2 className="font-chillax text-[28px] font-semibold leading-[1.1] tracking-tight text-foreground sm:text-[34px]">
            What do you want to<br />know about {company.name}?
          </h2>
        </div>
      ) : (
        <h2 className="font-chillax text-[28px] font-semibold leading-[1.1] tracking-tight text-foreground sm:text-[34px]">
          Start the conversation.
        </h2>
      )}

      <div className="flex flex-wrap items-center justify-center gap-2">
        {prompts.map((p) => (
          <button
            key={p}
            type="button"
            onClick={() => onSend(p)}
            className={cn(
              "group relative isolate inline-flex h-auto items-center gap-1.5 overflow-hidden rounded-xl px-3.5 py-2.5",
              "bg-card/60 text-left text-[12.5px] font-medium leading-[1.4] tracking-tight text-muted-foreground/80",
              "ring-1 ring-border/60 transition-colors hover:bg-card hover:text-foreground hover:ring-border",
            )}
          >
            <span
              aria-hidden
              className="pointer-events-none absolute inset-0 z-0 rounded-[inherit] opacity-[0.04] transition-opacity group-hover:opacity-[0.08]"
              style={{
                backgroundImage:
                  "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
              }}
            />
            <span className="relative z-10">{p}</span>
          </button>
        ))}
      </div>

      <ComposerBar
        onSend={onSend}
        onStop={onStop}
        pendingReply={pendingReply}
        error={error}
        compact
        body={body}
        onBodyChange={onBodyChange}
        placeholder={company ? `Ask about ${company.name}…` : "Ask anything…"}
        picker={picker}
      />
    </div>
  );
}

function CompanyPill({ name, onClick }: { name: string; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "group relative isolate inline-flex h-8 items-center gap-2 overflow-hidden rounded-full px-3.5",
        "bg-card/60 text-[12.5px] font-medium tracking-tight text-muted-foreground/80",
        "ring-1 ring-border/60 transition-colors hover:bg-card hover:text-foreground hover:ring-border",
      )}
    >
      <span
        aria-hidden
        className="pointer-events-none absolute inset-0 z-0 rounded-full opacity-[0.04] transition-opacity group-hover:opacity-[0.08]"
        style={{
          backgroundImage:
            "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
        }}
      />
      <span className="relative z-10 inline-flex items-center gap-1.5">
        <IconFolder size={12} className="opacity-70" />
        {name}
        <IconArrowRight size={11} className="opacity-50" />
      </span>
    </button>
  );
}

function ChipButton({
  icon,
  label,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "group relative isolate inline-flex h-8 items-center gap-1.5 overflow-hidden rounded-full px-3",
        "bg-card/60 text-[12.5px] font-medium tracking-tight text-muted-foreground/80",
        "ring-1 ring-border/60 transition-colors hover:bg-card hover:text-foreground hover:ring-border",
      )}
    >
      <span
        aria-hidden
        className="pointer-events-none absolute inset-0 z-0 rounded-full opacity-[0.04] transition-opacity group-hover:opacity-[0.08]"
        style={{
          backgroundImage:
            "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
        }}
      />
      <span className="relative z-10 inline-flex items-center gap-1.5">
        <span className="opacity-80">{icon}</span>
        {label}
      </span>
    </button>
  );
}

/* ─── MessageBubble + tool calls ─────────────────────────────────────────── */

function MessageBubble({
  message,
  toolCalls,
  toolsRunning,
}: {
  message: ChatMessage;
  toolCalls: ToolCallItem[];
  toolsRunning: ToolCallItem[];
}) {
  const isUser = message.role === "user";
  const isStreamingAssistant =
    !isUser && message.id.startsWith("tmp-assistant-") && message.body.length === 0;

  if (isUser) {
    return (
      <article className="flex justify-end">
        <div className="relative max-w-[80%] overflow-hidden rounded-2xl bg-card px-4 py-3 ring-1 ring-border/70">
          <DiagonalAccent
            className="rounded-2xl text-foreground"
            opacity={0.03}
            spacing={7}
          />
          <p className="relative whitespace-pre-wrap break-words text-[14px] leading-[1.6] text-foreground">
            {message.body}
          </p>
        </div>
      </article>
    );
  }

  const allToolCalls = [...toolsRunning, ...toolCalls];
  const displayBody = stripFakeToolNarration(message.body);
  const splitOut = splitThinking(displayBody);
  // Fallback: some open-source models put their entire analyst response
  // inside <think> tags. If that's the only substantive content we have,
  // promote it to the answer so the user sees something useful.
  const promotedThinking =
    splitOut.answer.trim().length < 24 && splitOut.thinking.trim().length > 24;
  const thinking = promotedThinking ? "" : splitOut.thinking;
  const answer = promotedThinking ? splitOut.thinking : splitOut.answer;
  const pins = useMemo(() => extractMapPins(message.body), [message.body]);
  const trends = useMemo(
    () =>
      extractTrends(message.body)
        .map(parseSpec6Trend)
        .filter((t): t is ParsedTrend => t !== null),
    [message.body],
  );
  const [canvasOpen, setCanvasOpen] = useState(false);
  const openCanvas = useCallback(() => setCanvasOpen(true), []);
  const closeCanvas = useCallback(() => setCanvasOpen(false), []);

  return (
    <article className="w-full">
      {isStreamingAssistant && allToolCalls.length === 0 ? (
        <ThinkingRow />
      ) : (
        <div className="text-[14.5px] leading-[1.72] text-foreground">
          {thinking && (
            <details className="mb-3 rounded-lg border border-border/50 bg-muted/30 px-3 py-2 text-[12.5px] text-muted-foreground/70">
              <summary className="cursor-pointer text-[11px] font-bold uppercase tracking-[0.12em]">
                Thinking
              </summary>
              <div className="mt-2 whitespace-pre-wrap">{thinking}</div>
            </details>
          )}
          {answer && <AssistantMarkdown body={answer} />}
          {trends.map((t, i) => (
            <TrendChart key={i} trend={t} />
          ))}
        </div>
      )}

      {/* Geographic signal first, then a compact lower card showing how the
          agents are working. Both open the full canvas drawer. */}
      {pins.length > 0 && <InlineCanvasMap pins={pins} onOpen={openCanvas} />}
      {allToolCalls.length > 0 && (
        <AgentWorkCard calls={allToolCalls} onOpen={openCanvas} />
      )}
      {(allToolCalls.length > 0 || pins.length > 0) && (
        <CanvasDrawer
          open={canvasOpen}
          calls={allToolCalls}
          pins={pins}
          onClose={closeCanvas}
        />
      )}
    </article>
  );
}

function ToolCallsCard({ calls }: { calls: ToolCallItem[] }) {
  return (
    <div className="mb-4 rounded-xl border border-border/60 bg-card/50 p-3">
      <p className="mb-2 text-[10px] font-bold uppercase tracking-[0.14em] text-muted-foreground/50">
        Sources
      </p>
      <div className="space-y-1.5">
        {calls.map((call) => (
          <ToolCallRow key={call.callId} call={call} />
        ))}
      </div>
    </div>
  );
}

function ToolCallRow({ call }: { call: ToolCallItem }) {
  const elapsed = useElapsed(call.startedAt, call.status === "running");
  const isCognee = isCogneeTool(call.toolName);
  return (
    <div className="flex items-center gap-2.5 text-[12px]">
      {call.status === "running" ? (
        <span className={cn(
          "h-1.5 w-1.5 animate-pulse rounded-full",
          isCognee ? "bg-violet-400/80" : "bg-amber-400/80",
        )} />
      ) : (
        <span className={cn(
          "h-1.5 w-1.5 rounded-full",
          isCognee ? "bg-violet-400/70" : "bg-emerald-400/70",
        )} />
      )}
      <span className={cn(
        "font-medium",
        isCognee ? "text-violet-400/80" : "text-muted-foreground/70",
      )}>
        {sourceTypeLabel(call.toolName)}
      </span>
      <span className="min-w-0 flex-1 truncate text-muted-foreground/50">{call.query}</span>
      <span className="shrink-0 tabular-nums text-[10.5px] text-muted-foreground/35">
        {call.status === "running"
          ? formatElapsed(elapsed)
          : call.endedAt
            ? formatElapsed(call.endedAt - call.startedAt)
            : ""}
      </span>
    </div>
  );
}

function useElapsed(startedAt: number, running: boolean): number {
  const [elapsed, setElapsed] = useState(() => Date.now() - startedAt);
  useEffect(() => {
    if (!running) return;
    const id = setInterval(() => setElapsed(Date.now() - startedAt), 100);
    return () => clearInterval(id);
  }, [running, startedAt]);
  return elapsed;
}

function AssistantMarkdown({ body }: { body: string }) {
  return (
    <div className="assistant-markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={{
          a: ({ children, ...props }) => (
            <a {...props} target="_blank" rel="noreferrer">
              {children}
            </a>
          ),
        }}
      >
        {body}
      </ReactMarkdown>
    </div>
  );
}

function ThinkingRow() {
  return (
    <article className="w-full">
      <span className="inline-flex items-center gap-1.5">
        <span
          className="h-1.5 w-1.5 rounded-full bg-muted-foreground/60 pulse-soft"
          style={{ animationDelay: "0ms" }}
        />
        <span
          className="h-1.5 w-1.5 rounded-full bg-muted-foreground/60 pulse-soft"
          style={{ animationDelay: "180ms" }}
        />
        <span
          className="h-1.5 w-1.5 rounded-full bg-muted-foreground/60 pulse-soft"
          style={{ animationDelay: "360ms" }}
        />
      </span>
    </article>
  );
}

function LoadingRow() {
  return (
    <div className="flex items-center gap-2 text-[12.5px] text-muted-foreground/65">
      <span className="h-1.5 w-1.5 rounded-full bg-muted-foreground/55 pulse-soft" />
      Loading…
    </div>
  );
}

/** Tiny pill that appears next to the company name in the chat header
 *  when the backend is still researching that company. */
function OverviewStatusBadge({ overview }: { overview: CompanyOverview | null }) {
  if (!overview) return null;
  if (overview.status === "queued" || overview.status === "running") {
    const label =
      overview.status === "queued"
        ? "Queued"
        : overview.discovered_competitors.length > 0
          ? `Researching · ${overview.discovered_competitors.length} found`
          : "Researching company";
    return (
      <span className="ml-1 inline-flex items-center gap-1.5 rounded-full bg-amber-500/[0.08] px-2 py-0.5 text-[10.5px] font-medium tracking-tight text-amber-400/90 ring-1 ring-amber-500/25">
        <span className="h-1 w-1 animate-pulse rounded-full bg-amber-400" />
        {label}
      </span>
    );
  }
  if (overview.status === "completed") {
    return (
      <span className="ml-1 inline-flex items-center gap-1 rounded-full bg-emerald-500/[0.08] px-2 py-0.5 text-[10.5px] font-medium tracking-tight text-emerald-400/85 ring-1 ring-emerald-500/25">
        <span className="h-1 w-1 rounded-full bg-emerald-400" />
        Overview ready
      </span>
    );
  }
  if (overview.status === "failed") {
    return (
      <span className="ml-1 rounded-full bg-red-500/[0.08] px-2 py-0.5 text-[10.5px] font-medium tracking-tight text-red-400 ring-1 ring-red-500/25">
        Overview failed
      </span>
    );
  }
  return null;
}

/* ─── ResearchingCard ────────────────────────────────────────────────────── */

/** Locks the chat surface while we build the overview. Renders a spinner,
 *  the company being researched, a count of competitors found so far, and a
 *  live activity log of source_started / source_completed / competitor_found
 *  events from the overview SSE feed. */
function ResearchingCard({
  company,
  overview,
  activity,
}: {
  company: ChatGroup;
  overview: CompanyOverview | null;
  activity: OverviewActivityEvent[];
}) {
  const found = overview?.discovered_competitors.length ?? 0;
  const isQueued = overview?.status === "queued";

  // Reverse so newest is on top — feels live.
  const reversed = useMemo(() => [...activity].reverse(), [activity]);

  return (
    <div className="diagonal-line-card rounded-2xl border border-border p-3 shadow-sm">
      <div className="rounded-xl border border-border bg-card p-6">
        {/* header */}
        <div className="flex items-start gap-4">
          <Spinner />
          <div className="min-w-0 flex-1">
            <p className="text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
              {isQueued ? "Overview queued" : "Researching company"}
            </p>
            <h2 className="mt-1 font-chillax text-[22px] font-semibold leading-[1.15] tracking-tight text-foreground">
              {company.name}
            </h2>
            <p className="mt-1.5 text-[12.5px] text-muted-foreground/65">
              {isQueued
                ? "Warming up the research pipeline…"
                : "We're searching SERP, public reviews, and Reddit threads to build a profile. Sit tight."}
            </p>
          </div>
          <div className="shrink-0 rounded-full bg-foreground/[0.06] px-2.5 py-1 text-[10.5px] font-semibold tabular-nums text-foreground/70 ring-1 ring-border/60">
            {found} competitors
          </div>
        </div>

        {/* live activity log */}
        <div className="mt-6 border-t border-border/50 pt-4">
          <p className="mb-3 text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
            Live activity
          </p>
          {reversed.length === 0 ? (
            <p className="text-[12.5px] text-muted-foreground/45">
              Waiting for the first SERP fetch to start…
            </p>
          ) : (
            <ol className="space-y-1.5">
              {reversed.slice(0, 24).map((event, idx) => (
                <ActivityRow key={`${event.at}-${idx}`} event={event} />
              ))}
            </ol>
          )}
        </div>

        {/* competitors found */}
        {overview && overview.discovered_competitors.length > 0 && (
          <div className="mt-6 border-t border-border/50 pt-4">
            <p className="mb-3 text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
              Competitors found
            </p>
            <div className="flex flex-wrap gap-1.5">
              {overview.discovered_competitors.map((comp, i) => (
                <span
                  key={`${comp.name}-${i}`}
                  className="inline-flex items-center gap-1.5 rounded-full bg-foreground/[0.04] px-2.5 py-1 text-[12px] font-medium tracking-tight text-foreground/85 ring-1 ring-border/60"
                >
                  <span className="h-1 w-1 rounded-full bg-emerald-400/70" />
                  {comp.name}
                </span>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function ActivityRow({ event }: { event: OverviewActivityEvent }) {
  if (event.kind === "source_started") {
    return (
      <li className="flex items-start gap-2.5 text-[12.5px] leading-[1.55]">
        <span className="mt-1.5 h-1.5 w-1.5 shrink-0 animate-pulse rounded-full bg-amber-400/80" />
        <span className="min-w-0 flex-1">
          <span className="font-mono text-[10.5px] font-bold uppercase tracking-[0.12em] text-muted-foreground/55">
            {event.source}
          </span>
          <span className="ml-2 text-foreground/85">{event.detail}</span>
        </span>
      </li>
    );
  }
  if (event.kind === "source_completed") {
    return (
      <li className="flex items-start gap-2.5 text-[12.5px] leading-[1.55]">
        <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-emerald-400/70" />
        <span className="min-w-0 flex-1">
          <span className="font-mono text-[10.5px] font-bold uppercase tracking-[0.12em] text-muted-foreground/55">
            {event.source}
          </span>
          <span className="ml-2 text-muted-foreground/65">{event.detail}</span>
          {typeof event.found === "number" && (
            <span className="ml-2 tabular-nums text-muted-foreground/45">
              · {event.found} found
            </span>
          )}
        </span>
      </li>
    );
  }
  // competitor_found
  return (
    <li className="flex items-start gap-2.5 text-[12.5px] leading-[1.55]">
      <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-violet-400/70" />
      <span className="min-w-0 flex-1">
        <span className="font-mono text-[10.5px] font-bold uppercase tracking-[0.12em] text-muted-foreground/55">
          found
        </span>
        <span className="ml-2 font-medium text-foreground/90">{event.name}</span>
        {event.domain && (
          <span className="ml-2 text-muted-foreground/50">{event.domain}</span>
        )}
      </span>
    </li>
  );
}

function Spinner() {
  return (
    <span
      aria-hidden
      className="relative grid h-10 w-10 shrink-0 place-items-center rounded-full bg-foreground/[0.04] ring-1 ring-border/60"
    >
      <svg
        viewBox="0 0 24 24"
        className="h-5 w-5 animate-spin text-amber-400/80"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      >
        <path d="M12 3a9 9 0 1 0 9 9" />
      </svg>
    </span>
  );
}

/* ─── OverviewDrawer ─────────────────────────────────────────────────────── */

/** Trigger pill that toggles the overview drawer open. Lives in the chat
 *  company header strip; only renders when the overview is completed. */
function OverviewDrawerTrigger({ overview }: { overview: CompanyOverview }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        className={cn(
          "ml-auto inline-flex h-6 items-center gap-1.5 rounded-full bg-foreground/[0.05] px-2.5",
          "text-[10.5px] font-semibold tracking-tight text-foreground/75",
          "ring-1 ring-border/60 transition-colors hover:bg-foreground/[0.09] hover:text-foreground",
        )}
      >
        Overview
        <IconChevronDown size={10} className="-rotate-90" />
      </button>
      {open && <OverviewDrawer overview={overview} onClose={() => setOpen(false)} />}
    </>
  );
}

function OverviewDrawer({
  overview,
  onClose,
}: {
  overview: CompanyOverview;
  onClose: () => void;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 flex justify-end">
      <button
        type="button"
        aria-label="Close overview"
        onClick={onClose}
        className="absolute inset-0 bg-black/40 backdrop-blur-[2px]"
      />
      <aside
        className={cn(
          "relative z-10 flex h-full w-full max-w-[640px] flex-col overflow-hidden",
          "border-l border-border bg-shell shadow-[0_24px_56px_-10px_rgba(0,0,0,0.5)]",
          "drawer-enter",
        )}
      >
        <header className="flex h-14 shrink-0 items-center gap-3 border-b border-border/60 bg-background px-5">
          <IconFolder size={14} className="text-muted-foreground/55" />
          <div className="min-w-0 flex-1">
            <p className="text-[10px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
              Company overview
            </p>
            <p className="truncate text-[13.5px] font-medium tracking-tight text-foreground">
              {overview.company_name}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close"
            className="grid h-8 w-8 place-items-center rounded-lg text-muted-foreground/65 hover:bg-card/60 hover:text-foreground"
          >
            <IconClose size={15} />
          </button>
        </header>

        <div className="flex-1 overflow-y-auto px-5 py-5">
          {overview.summary && <OverviewSummary overview={overview} />}
          {overview.discovered_competitors.length > 0 && (
            <div className="mb-6">
              <p className="mb-2 text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
                Competitors
              </p>
              <div className="flex flex-wrap gap-1.5">
                {overview.discovered_competitors.map((comp, i) => (
                  <span
                    key={`${comp.name}-${i}`}
                    className="inline-flex items-center gap-1.5 rounded-full bg-foreground/[0.04] px-2.5 py-1 text-[12px] font-medium tracking-tight text-foreground/85 ring-1 ring-border/60"
                  >
                    {comp.name}
                    {comp.domain && (
                      <span className="text-muted-foreground/45">· {comp.domain}</span>
                    )}
                  </span>
                ))}
              </div>
            </div>
          )}
          {overview.markdown_brief && (
            <div className="diagonal-line-card rounded-2xl border border-border p-3">
              <div className="rounded-xl border border-border bg-card px-5 py-4">
                <p className="mb-3 text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
                  Full brief
                </p>
                <div className="assistant-markdown text-[13px] leading-relaxed">
                  <ReactMarkdown remarkPlugins={[remarkGfm]}>
                    {overview.markdown_brief}
                  </ReactMarkdown>
                </div>
              </div>
            </div>
          )}
        </div>
      </aside>
    </div>
  );
}

function OverviewSummary({ overview }: { overview: CompanyOverview }) {
  const s = overview.summary;
  if (!s) return null;
  const rows: Array<{ label: string; value: string }> = [
    { label: "Actual competitors",   value: s.actual_competitors },
    { label: "Customer trust",       value: s.customer_trust_and_desire_to_use },
    { label: "Faults",               value: s.faults },
    { label: "Rating",               value: s.rating },
    { label: "Where to do better",   value: s.where_to_do_better },
    { label: "Durability",           value: s.how_long_this_will_last },
    { label: "Market saturation",    value: s.market_saturation_and_overlap },
    { label: "Confidence notes",     value: s.confidence_notes },
  ].filter((r) => r.value && r.value.trim().length > 0);

  if (rows.length === 0) return null;

  return (
    <div className="mb-6 space-y-3">
      {rows.map((r) => (
        <div key={r.label} className="rounded-xl border border-border/60 bg-card px-4 py-3">
          <p className="text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
            {r.label}
          </p>
          <p className="mt-1 text-[13px] leading-relaxed text-foreground/90">{r.value}</p>
        </div>
      ))}
    </div>
  );
}
