/**
 * Canvas drawer + lethargic preview.
 *
 * In-thread we render a quiet one-line preview ("5 agents · 11 countries · 17
 * calls — Open canvas ↗"). Clicking it slides in a right-side drawer that
 * shows: the world heatmap, large and interactive at the top; a per-agent
 * breakdown below with the queries each one fired, elapsed time, and result
 * counts. The map honours <spec6-pins> markdown emitted by the LLM, so
 * hovering a country pulls up the analyst's narrative for that location.
 */

import { useEffect, useMemo, useRef } from "react";
import { DiagonalAccent, HatchedChip } from "../components/diagonal";
import {
  IconArrowRight,
  IconBolt,
  IconClose,
  IconCompass,
  IconDot,
} from "../components/icons";
import { cn } from "../lib/cn";
import {
  AGENT_ROSTER,
  type AgentDescriptor,
  type AgentPersona,
  PERSONA_META,
} from "./agent-roster";
import type { ToolCallItem } from "./types";
import {
  formatElapsed,
  isCogneeTool,
  type ParsedMapPin,
  sourceTypeLabel,
  sourceTypeToAgentId,
} from "./utils";
import { WorldHeatmap, pinSummary } from "./world-heatmap";

/* ─── inline map card ───────────────────────────────────────────────────── */

/**
 * Compact world map mounted directly into the chat thread when the model
 * emits pins. Same interactive heatmap as the drawer, sized for in-thread
 * reading. Click anywhere on it to open the full drawer.
 */
export function InlineCanvasMap({
  pins,
  onOpen,
}: {
  pins: ParsedMapPin[];
  onOpen: () => void;
}) {
  if (pins.length === 0) return null;
  const { markerCount, countryCount } = pinSummary(pins);

  return (
    <div className="diagonal-line-card mb-4 rounded-xl border border-border p-2 shadow-sm">
      <div className="flex flex-col gap-2 rounded-lg border border-border bg-card p-3 text-foreground">
        <div className="flex items-center gap-2">
          <span className="text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/65">
            <IconBolt size={11} className="mr-1 inline-block align-text-bottom text-muted-foreground/50" />
            Geographic signal
          </span>
          <span className="text-[10px] tabular-nums text-muted-foreground/55">
            {pins.length} pin{pins.length === 1 ? "" : "s"} · {countryCount}{" "}
            countr{countryCount === 1 ? "y" : "ies"}
            {markerCount > 0 ? ` · ${markerCount} point${markerCount === 1 ? "" : "s"}` : ""}
          </span>
          <button
            type="button"
            onClick={onOpen}
            className="ml-auto inline-flex items-center gap-1 rounded-md bg-foreground/[0.05] px-2 py-1 text-[10.5px] font-semibold text-foreground/80 ring-1 ring-border/60 transition hover:bg-foreground/[0.08] hover:text-foreground"
          >
            Open full canvas
            <IconArrowRight size={11} />
          </button>
        </div>
        <div className="overflow-hidden rounded-md">
          <WorldHeatmap aiPins={pins} height={260} />
        </div>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-0.5 text-[9.5px] text-muted-foreground/55">
          <LegendDot tone="critical" label="Critical" />
          <LegendDot tone="high" label="High" />
          <LegendDot tone="medium" label="Medium" />
          <LegendDot tone="low" label="Low" />
          <span className="ml-auto text-[9.5px] text-muted-foreground/45">
            Hover a flagged country for the analyst note.
          </span>
        </div>
      </div>
    </div>
  );
}

/* ─── canvas preview ────────────────────────────────────────────────────── */

export function CanvasPreview({
  calls,
  pins,
  onOpen,
}: {
  calls: ToolCallItem[];
  pins: ParsedMapPin[];
  onOpen: () => void;
}) {
  if (calls.length === 0) return null;

  const triggered = uniqueAgentIds(calls);
  const running = calls.filter((c) => c.status === "running").length;
  const done = calls.filter((c) => c.status === "done").length;
  const { markerCount, countryCount } = useMemo(() => pinSummary(pins), [pins]);
  const hasPins = pins.length > 0;
  const hasCounterfit = pins.some((p) => p.tone === "critical");
  const accentTone = hasCounterfit
    ? "rose"
    : pins.some((p) => p.tone === "high")
      ? "orange"
      : pins.some((p) => p.tone === "medium")
        ? "amber"
        : "emerald";

  return (
    <button
      type="button"
      onClick={onOpen}
      className={cn(
        "group relative mb-4 block w-full overflow-hidden rounded-xl border p-3 text-left shadow-[0_1px_2px_rgba(0,0,0,0.05)] transition",
        hasPins
          ? "border-border bg-card ring-1 ring-border hover:bg-card hover:ring-border/80 hover:shadow-[0_8px_28px_rgba(0,0,0,0.12)]"
          : "border-border/60 bg-card/60 ring-1 ring-border/30 hover:bg-card hover:ring-border hover:shadow-[0_4px_16px_rgba(0,0,0,0.07)]",
      )}
    >
      <DiagonalAccent
        className={cn(
          "text-foreground rounded-xl transition-opacity duration-200",
          hasPins ? "opacity-[0.04]" : "opacity-0 group-hover:opacity-[0.035]",
        )}
      />

      <div className="relative flex items-center gap-3">
        <div className="relative">
          <HatchedChip size={hasPins ? 32 : 26}>
            <IconCompass size={hasPins ? 16 : 14} />
          </HatchedChip>
          {hasPins && (
            <span
              className={cn(
                "absolute -right-0.5 -top-0.5 inline-flex h-2.5 w-2.5",
              )}
            >
              <span
                className={cn(
                  "absolute inset-0 animate-ping rounded-full opacity-70",
                  accentTone === "rose" && "bg-rose-400",
                  accentTone === "orange" && "bg-orange-400",
                  accentTone === "amber" && "bg-amber-400",
                  accentTone === "emerald" && "bg-emerald-400",
                )}
              />
              <span
                className={cn(
                  "relative h-2.5 w-2.5 rounded-full ring-2 ring-card",
                  accentTone === "rose" && "bg-rose-400",
                  accentTone === "orange" && "bg-orange-400",
                  accentTone === "amber" && "bg-amber-400",
                  accentTone === "emerald" && "bg-emerald-400",
                )}
              />
            </span>
          )}
        </div>

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
              Spec6 · agent canvas
            </span>
            {hasPins && (
              <span
                className={cn(
                  "rounded-full px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-[0.12em] ring-1",
                  accentTone === "rose" && "bg-rose-500/10 text-rose-400 ring-rose-500/25",
                  accentTone === "orange" && "bg-orange-500/10 text-orange-400 ring-orange-500/25",
                  accentTone === "amber" && "bg-amber-500/10 text-amber-400 ring-amber-500/25",
                  accentTone === "emerald" && "bg-emerald-500/10 text-emerald-400 ring-emerald-500/25",
                )}
              >
                Map updated
              </span>
            )}
          </div>

          <div
            className={cn(
              "mt-0.5 leading-tight",
              hasPins ? "text-[13px] font-semibold text-foreground" : "text-[12px] text-foreground/80",
            )}
          >
            {hasPins ? (
              <span>
                Spec6 pinned <span className="tabular-nums">{pins.length}</span> location
                {pins.length === 1 ? "" : "s"} across{" "}
                <span className="tabular-nums">{countryCount}</span> countr
                {countryCount === 1 ? "y" : "ies"}
                {markerCount > 0 && (
                  <>
                    {" "}with <span className="tabular-nums">{markerCount}</span> point
                    {markerCount === 1 ? "" : "s"}
                  </>
                )}
                .
              </span>
            ) : (
              <span className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-foreground/80">
                <span className="font-semibold tabular-nums text-foreground">
                  {triggered.length} agent{triggered.length === 1 ? "" : "s"}
                </span>
                <span className="tabular-nums text-muted-foreground/70">
                  {done}/{calls.length} call{calls.length === 1 ? "" : "s"} complete
                </span>
              </span>
            )}
          </div>

          <div className="mt-1.5 flex items-center gap-1.5">
            {triggered.slice(0, 6).map((id) => {
              const agent = AGENT_ROSTER.find((a) => a.id === id);
              if (!agent) return null;
              return (
                <span
                  key={id}
                  className="rounded-full bg-foreground/[0.06] px-2 py-0.5 text-[10px] font-medium text-foreground/75 ring-1 ring-border/50"
                  title={agent.label}
                >
                  {agent.label}
                </span>
              );
            })}
            {triggered.length > 6 && (
              <span className="text-[10px] tabular-nums text-muted-foreground/60">
                +{triggered.length - 6}
              </span>
            )}
          </div>
        </div>

        {/* Mini map glyph — only when pins are present, for instant signal. */}
        {hasPins && (
          <MiniMapGlyph pins={pins} />
        )}

        {running > 0 && (
          <span className="shrink-0 inline-flex items-center gap-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-emerald-500">
            <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-400" />
            Live
          </span>
        )}
        <span
          className={cn(
            "shrink-0 inline-flex items-center gap-1 transition",
            hasPins
              ? "text-[12px] font-semibold text-foreground"
              : "text-[11px] font-semibold text-muted-foreground/70 group-hover:text-foreground",
          )}
        >
          Open canvas
          <IconArrowRight size={12} />
        </span>
      </div>
    </button>
  );
}

/* ─── compact in-thread agent work card ─────────────────────────────────── */

/**
 * Small card rendered at the BOTTOM of an assistant message in the normal chat
 * thread. Shows, per agent, what it's doing and how the multi-agent fan-out is
 * progressing — live while tools run, summarised when done. Clicking opens the
 * full canvas drawer.
 */
export function AgentWorkCard({
  calls,
  onOpen,
}: {
  calls: ToolCallItem[];
  onOpen: () => void;
}) {
  const groups = useMemo(() => {
    const map = new Map<string, ToolCallItem[]>();
    for (const c of calls) {
      const key =
        sourceTypeToAgentId(c.sourceType) ??
        (isCogneeTool(c.toolName) ? "cognee" : "web");
      const list = map.get(key) ?? [];
      list.push(c);
      map.set(key, list);
    }
    return Array.from(map.entries());
  }, [calls]);

  if (calls.length === 0) return null;

  const running = calls.filter((c) => c.status === "running").length;
  const done = calls.filter((c) => c.status === "done").length;

  const labelFor = (key: string, sample: ToolCallItem): string => {
    const agent = AGENT_ROSTER.find((a) => a.id === key);
    if (agent) return agent.label;
    if (key === "cognee") return "Knowledge Graph";
    return sourceTypeLabel(sample.toolName);
  };

  return (
    <div className="mt-4 overflow-hidden rounded-xl border border-border/70 bg-card/60 shadow-[0_1px_2px_rgba(0,0,0,0.05)]">
      <button
        type="button"
        onClick={onOpen}
        className="group relative flex w-full items-center gap-2.5 border-b border-border/40 px-3 py-2 text-left transition hover:bg-card"
      >
        <DiagonalAccent className="text-foreground rounded-t-xl opacity-[0.03]" />
        <HatchedChip size={22}>
          <IconCompass size={12} />
        </HatchedChip>
        <span className="text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/60">
          Agent activity
        </span>
        <span className="text-[10.5px] tabular-nums text-muted-foreground/55">
          {groups.length} agent{groups.length === 1 ? "" : "s"} · {done}/{calls.length}{" "}
          call{calls.length === 1 ? "" : "s"}
        </span>
        {running > 0 && (
          <span className="inline-flex items-center gap-1 text-[10px] font-semibold uppercase tracking-[0.12em] text-emerald-500">
            <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-400" />
            Live
          </span>
        )}
        <span className="ml-auto inline-flex items-center gap-1 text-[10.5px] font-semibold text-muted-foreground/70 transition group-hover:text-foreground">
          Open canvas
          <IconArrowRight size={11} />
        </span>
      </button>
      <ul className="divide-y divide-border/30">
        {groups.map(([key, list]) => {
          const sample = list[list.length - 1];
          const isRun = list.some((c) => c.status === "running");
          const cognee = key === "cognee";
          const hits = list.reduce((a, c) => a + (c.resultCount ?? 0), 0);
          const elapsed = list.reduce(
            (a, c) =>
              a +
              (c.endedAt && c.endedAt > c.startedAt ? c.endedAt - c.startedAt : 0),
            0,
          );
          return (
            <li key={key} className="flex items-center gap-2 px-3 py-1.5">
              <span
                className={cn(
                  "h-1.5 w-1.5 shrink-0 rounded-full",
                  isRun
                    ? cognee
                      ? "animate-pulse bg-violet-400"
                      : "animate-pulse bg-emerald-400"
                    : cognee
                      ? "bg-violet-400/70"
                      : "bg-foreground/70",
                )}
              />
              <span
                className={cn(
                  "shrink-0 text-[11px] font-semibold",
                  cognee ? "text-violet-400/85" : "text-foreground/80",
                )}
              >
                {labelFor(key, sample)}
              </span>
              <span className="min-w-0 flex-1 truncate font-mono text-[10.5px] text-muted-foreground/60">
                {isRun
                  ? sample.query
                  : `${list.length} quer${list.length === 1 ? "y" : "ies"} · ${sample.query}`}
              </span>
              {hits > 0 && (
                <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground/45">
                  {hits} hit{hits === 1 ? "" : "s"}
                </span>
              )}
              <span className="shrink-0 font-mono text-[10px] tabular-nums text-muted-foreground/40">
                {isRun ? "…" : formatElapsed(elapsed)}
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/**
 * Tiny inline "world" abstraction: a 64x32 grid showing where the pinned
 * countries sit at a glance. Pure visual signal — full map lives in the
 * drawer. We project each pin's lat/lng (or country centroid for ISO-only
 * pins) onto the grid and place a coloured dot.
 */
function MiniMapGlyph({ pins }: { pins: ParsedMapPin[] }) {
  // Approximate centroids for the ISO codes we ship in the prompt.
  const isoCentroids: Record<string, [number, number]> = {
    "036": [-25, 134], "040": [47, 13], "032": [-34, -64], "050": [23, 90],
    "056": [50, 4], "076": [-10, -55], "104": [21, 96], "116": [12, 105],
    "124": [56, -106], "144": [7, 80], "152": [-30, -71], "156": [35, 104],
    "158": [23, 121], "170": [4, -72], "203": [49, 15], "208": [56, 9],
    "231": [9, 38], "246": [64, 26], "250": [46, 2], "276": [51, 9],
    "300": [39, 22], "344": [22, 114], "356": [22, 79], "360": [-2, 118],
    "364": [32, 53], "368": [33, 44], "372": [53, -8], "376": [31, 35],
    "380": [42, 12], "392": [36, 138], "400": [31, 36], "404": [-1, 38],
    "410": [37, 127], "422": [33, 35], "458": [4, 102], "484": [23, -102],
    "504": [31, -7], "524": [28, 84], "528": [52, 5], "554": [-42, 174],
    "566": [9, 8], "578": [60, 8], "586": [30, 69], "604": [-9, -75],
    "608": [12, 122], "616": [51, 19], "620": [39, -8], "634": [25, 51],
    "643": [60, 100], "682": [25, 45], "702": [1, 103], "704": [16, 107],
    "710": [-30, 24], "724": [40, -3], "752": [60, 18], "756": [46, 8],
    "764": [15, 100], "784": [24, 54], "792": [38, 35], "804": [49, 31],
    "818": [26, 30], "826": [54, -2], "840": [38, -97],
  };

  const W = 56;
  const H = 28;
  const project = (lat: number, lng: number) => {
    const x = ((lng + 180) / 360) * W;
    const y = ((90 - lat) / 180) * H;
    return { x, y };
  };

  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      width="64"
      height="32"
      className="shrink-0 rounded-sm bg-background/40 ring-1 ring-border/40"
      aria-hidden
    >
      {/* faint reference rectangle */}
      <rect x="0" y="0" width={W} height={H} fill="currentColor" opacity="0.04" />
      {pins.map((p, i) => {
        const lat = p.lat ?? (p.iso ? isoCentroids[p.iso]?.[0] : undefined);
        const lng = p.lng ?? (p.iso ? isoCentroids[p.iso]?.[1] : undefined);
        if (lat === undefined || lng === undefined) return null;
        const { x, y } = project(lat, lng);
        const color =
          p.tone === "critical"
            ? "rgb(244,63,94)"
            : p.tone === "high"
              ? "rgb(251,146,60)"
              : p.tone === "medium"
                ? "rgb(251,191,36)"
                : "rgb(56,189,248)";
        return (
          <g key={i}>
            <circle cx={x} cy={y} r={2} fill={color} opacity="0.35" />
            <circle cx={x} cy={y} r={0.9} fill={color} />
          </g>
        );
      })}
    </svg>
  );
}

/* ─── drawer ────────────────────────────────────────────────────────────── */

export function CanvasDrawer({
  open,
  calls,
  pins,
  onClose,
}: {
  open: boolean;
  calls: ToolCallItem[];
  pins: ParsedMapPin[];
  onClose: () => void;
}) {
  // Lock body scroll while drawer is open.
  useEffect(() => {
    if (!open) return;
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = prev;
    };
  }, [open]);

  // ESC to close.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  const triggered = useMemo(() => uniqueAgentIds(calls), [calls]);
  const summary = useMemo(() => pinSummary(pins), [pins]);
  const cogneeCalls = calls.filter((c) => isCogneeTool(c.toolName));
  const cogneeUsed = cogneeCalls.length > 0;

  return (
    <>
      {/* backdrop */}
      <div
        aria-hidden
        onClick={onClose}
        className={cn(
          "fixed inset-0 z-40 bg-background/70 backdrop-blur-sm transition-opacity duration-200",
          open ? "opacity-100" : "pointer-events-none opacity-0",
        )}
      />
      {/* panel */}
      <aside
        role="dialog"
        aria-label="Spec6 agent canvas"
        className={cn(
          "fixed inset-y-0 right-0 z-50 flex w-full max-w-[920px] flex-col overflow-hidden border-l border-border bg-shell shadow-[0_24px_56px_-10px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,255,255,0.04)] transition-transform duration-[260ms]",
          open ? "translate-x-0" : "translate-x-full",
        )}
        style={{ transitionTimingFunction: "cubic-bezier(0.16,1,0.3,1)" }}
      >
        <DrawerHeader
          calls={calls}
          triggered={triggered}
          cogneeUsed={cogneeUsed}
          onClose={onClose}
        />

        <div className="flex-1 overflow-y-auto">
          <section className="border-b border-border/40 bg-background/40 p-4 sm:p-5">
            <SectionLabel
              label="World view"
              caption={
                pins.length === 0
                  ? "Awaiting analyst pins"
                  : `${summary.markerCount} marker${summary.markerCount === 1 ? "" : "s"} · ${summary.countryCount} countr${summary.countryCount === 1 ? "y" : "ies"} · ${pins.length} pin${pins.length === 1 ? "" : "s"}`
              }
            />
            <div className="mt-2 overflow-hidden rounded-xl border border-border/60 bg-card/40 p-2 text-foreground">
              <WorldHeatmap aiPins={pins} height={420} />
              <div className="flex flex-wrap items-center gap-x-4 gap-y-1 px-2 pb-1 pt-2 text-[10px] text-muted-foreground/60">
                <LegendDot tone="critical" label="Critical" />
                <LegendDot tone="high" label="High" />
                <LegendDot tone="medium" label="Medium" />
                <LegendDot tone="low" label="Low" />
                {pins.length > 0 ? (
                  <span className="ml-auto text-[10px] text-muted-foreground/55">
                    Hover a flagged country or pin for the analyst note.
                  </span>
                ) : (
                  <span className="ml-auto text-[10px] text-muted-foreground/45">
                    Geography populates once the analyst emits pins for this brand.
                  </span>
                )}
              </div>
            </div>
          </section>

          <section className="p-4 sm:p-5">
            <SectionLabel
              label="Agent breakdown"
              caption={`${triggered.length} agent${triggered.length === 1 ? "" : "s"} dispatched · ${calls.length} call${calls.length === 1 ? "" : "s"}`}
            />
            <div className="mt-2 grid gap-3 md:grid-cols-3">
              <PersonaColumn
                persona="counterfit"
                agents={AGENT_ROSTER.filter((a) => a.persona === "counterfit")}
                calls={calls}
              />
              <PersonaColumn
                persona="consulting"
                agents={AGENT_ROSTER.filter((a) => a.persona === "consulting")}
                calls={calls}
              />
              <PersonaColumn
                persona="sales"
                agents={AGENT_ROSTER.filter((a) => a.persona === "sales")}
                calls={calls}
              />
            </div>

            {cogneeUsed && (
              <div className="mt-3 rounded-lg border border-violet-500/25 bg-violet-500/[0.05] p-3 text-[12.5px] text-violet-300">
                <div className="text-[9.5px] font-bold uppercase tracking-[0.14em] text-violet-400/85">
                  Knowledge graph · Cognee
                </div>
                <div className="mt-0.5 text-[12.5px] text-foreground/85">
                  {cogneeCalls.length} graph quer{cogneeCalls.length === 1 ? "y" : "ies"} fired — context drawn from prior conversations + ingested company data.
                </div>
              </div>
            )}

            <div className="mt-4">
              <SectionLabel
                label="Agent log"
                caption={`${calls.length} call${calls.length === 1 ? "" : "s"}`}
              />
              <LogPanel calls={calls} />
            </div>
          </section>
        </div>
      </aside>
    </>
  );
}

/* ─── drawer header ─────────────────────────────────────────────────────── */

function DrawerHeader({
  calls,
  triggered,
  cogneeUsed,
  onClose,
}: {
  calls: ToolCallItem[];
  triggered: string[];
  cogneeUsed: boolean;
  onClose: () => void;
}) {
  const running = calls.filter((c) => c.status === "running").length;
  const done = calls.filter((c) => c.status === "done").length;

  return (
    <header className="relative overflow-hidden border-b border-border bg-card">
      <div className="absolute inset-0 bg-gradient-to-br from-zinc-700 via-zinc-900 to-zinc-950" />
      <div className="diagonal-line-corner absolute inset-0" />
      <div className="absolute inset-0 bg-gradient-to-r from-background/30 via-background/5 to-transparent" />

      <div className="relative z-10 flex items-center gap-3 p-4">
        <HatchedChip size={36}>
          <IconCompass size={18} />
        </HatchedChip>
        <div className="min-w-0 flex-1">
          <div className="text-[10px] font-bold uppercase tracking-[0.14em] text-white/55">
            Spec6 · agent canvas
          </div>
          <div className="font-chillax text-[18px] font-semibold tracking-tight text-white">
            {triggered.length} agent{triggered.length === 1 ? "" : "s"} dispatched
            {running > 0
              ? ` · ${running} live`
              : ` · ${done}/${calls.length} complete`}
          </div>
        </div>
        {cogneeUsed && (
          <span className="hidden items-center gap-1.5 rounded-full bg-violet-500/[0.18] px-2 py-0.5 text-[10px] font-semibold text-violet-200 ring-1 ring-violet-300/30 sm:inline-flex">
            <span className="h-1.5 w-1.5 rounded-full bg-violet-300" />
            Knowledge graph
          </span>
        )}
        <button
          type="button"
          onClick={onClose}
          className="rounded-lg bg-background/15 p-2 text-white/85 ring-1 ring-white/15 transition hover:bg-background/25"
          aria-label="Close drawer"
        >
          <IconClose size={15} />
        </button>
      </div>
    </header>
  );
}

/* ─── persona column ────────────────────────────────────────────────────── */

function PersonaColumn({
  persona,
  agents,
  calls,
}: {
  persona: AgentPersona;
  agents: AgentDescriptor[];
  calls: ToolCallItem[];
}) {
  const meta = PERSONA_META[persona];
  const agentCalls = useMemo(() => {
    const map = new Map<string, ToolCallItem[]>();
    for (const c of calls) {
      const id = sourceTypeToAgentId(c.sourceType);
      if (!id) continue;
      const list = map.get(id) ?? [];
      list.push(c);
      map.set(id, list);
    }
    return map;
  }, [calls]);

  const running = agents.filter(
    (a) => (agentCalls.get(a.id) ?? []).some((c) => c.status === "running"),
  ).length;
  const done = agents.filter(
    (a) => (agentCalls.get(a.id) ?? []).length > 0 && !(agentCalls.get(a.id) ?? []).some((c) => c.status === "running"),
  ).length;

  return (
    <div className="diagonal-line-card rounded-xl border border-border p-2 shadow-sm">
      <div className="flex h-full flex-col gap-2 rounded-lg border border-border bg-card p-2.5">
        <div className="flex items-center justify-between">
          <span className="text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/65">
            {meta.label}
          </span>
          <span className="inline-flex items-center gap-1 text-[10px] tabular-nums text-muted-foreground/55">
            {done}/{agents.length}
            {running > 0 && (
              <span className="ml-0.5 h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-400" />
            )}
          </span>
        </div>
        <p className="text-[11px] leading-[1.4] text-muted-foreground/70">
          {meta.tagline}
        </p>
        <div className="flex flex-col gap-1.5">
          {agents.map((agent) => (
            <AgentDetailCard
              key={agent.id}
              agent={agent}
              calls={agentCalls.get(agent.id) ?? []}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

function AgentDetailCard({
  agent,
  calls,
}: {
  agent: AgentDescriptor;
  calls: ToolCallItem[];
}) {
  const running = calls.some((c) => c.status === "running");
  const done = calls.length > 0 && !running;
  const errored = calls.some((c) => c.errored);
  const totalElapsed = calls.reduce(
    (acc, c) =>
      acc + ((c.endedAt && c.endedAt > c.startedAt ? c.endedAt - c.startedAt : 0) || 0),
    0,
  );
  const totalResults = calls.reduce((acc, c) => acc + (c.resultCount ?? 0), 0);

  return (
    <div
      className={cn(
        "relative overflow-hidden rounded-md ring-1 transition",
        calls.length === 0 && "bg-card/30 ring-border/30 opacity-70",
        running && "bg-card ring-border shadow-[0_1px_2px_rgba(0,0,0,0.05)]",
        done && "bg-card ring-border/80 shadow-[0_1px_2px_rgba(0,0,0,0.05)]",
        errored && "bg-rose-500/[0.06] ring-rose-500/30",
      )}
    >
      {running && (
        <DiagonalAccent className="text-foreground opacity-[0.05] rounded-md" />
      )}
      <div className="relative flex items-center gap-2 p-2">
        <HatchedChip size={24}>
          <span className="font-chillax text-[9px] font-bold tracking-tight">
            {abbreviation(agent.label)}
          </span>
        </HatchedChip>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[11.5px] font-semibold text-foreground">
            {agent.label}
          </div>
          <div className="truncate text-[10px] text-muted-foreground/65">
            {calls.length === 0 ? "scheduled" : agent.tool}
          </div>
        </div>
        <div className="shrink-0 text-right">
          <div className="font-mono text-[10px] tabular-nums text-muted-foreground/65">
            {calls.length === 0 ? "—" : formatElapsed(totalElapsed)}
          </div>
          {totalResults > 0 && (
            <div className="text-[9.5px] tabular-nums text-muted-foreground/55">
              {totalResults} hit{totalResults === 1 ? "" : "s"}
            </div>
          )}
        </div>
      </div>
      {calls.length > 0 && (
        <ul className="relative space-y-0.5 border-t border-border/40 px-2 py-1.5 text-[10.5px] text-muted-foreground/75">
          {calls.map((c) => (
            <li key={c.callId} className="flex items-start gap-1.5 leading-[1.45]">
              <span
                className={cn(
                  "mt-1 h-1 w-1 shrink-0 rounded-full",
                  c.status === "running"
                    ? "animate-pulse bg-emerald-400"
                    : c.errored
                      ? "bg-rose-400"
                      : "bg-foreground/70",
                )}
              />
              <span className="min-w-0 flex-1 truncate font-mono">{c.query}</span>
              <span className="shrink-0 font-mono tabular-nums text-muted-foreground/45">
                {c.endedAt && c.endedAt > c.startedAt
                  ? formatElapsed(c.endedAt - c.startedAt)
                  : "…"}
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/* ─── log panel ─────────────────────────────────────────────────────────── */

function LogPanel({ calls }: { calls: ToolCallItem[] }) {
  const scrollerRef = useRef<HTMLDivElement>(null);
  const sorted = useMemo(
    () => [...calls].sort((a, b) => a.startedAt - b.startedAt),
    [calls],
  );

  useEffect(() => {
    const el = scrollerRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [sorted.length]);

  return (
    <div
      ref={scrollerRef}
      className="mt-2 max-h-[260px] overflow-y-auto rounded-lg bg-background/40 p-2 ring-1 ring-border/40"
    >
      {sorted.length === 0 ? (
        <div className="font-mono text-[11px] text-muted-foreground/40">
          No calls yet.
        </div>
      ) : (
        sorted.map((c) => <LogLine key={c.callId} call={c} />)
      )}
    </div>
  );
}

function LogLine({ call }: { call: ToolCallItem }) {
  const isCognee = isCogneeTool(call.toolName);
  const elapsed =
    call.endedAt && call.endedAt > call.startedAt ? call.endedAt - call.startedAt : 0;
  return (
    <div className="flex items-center gap-2 font-mono text-[10.5px] leading-[1.6] text-muted-foreground/80">
      <span className="shrink-0 tabular-nums text-muted-foreground/35">
        {formatTime(call.startedAt)}
      </span>
      <span
        className={cn(
          "h-1.5 w-1.5 shrink-0 rounded-full",
          call.status === "running"
            ? isCognee
              ? "animate-pulse bg-violet-400"
              : "animate-pulse bg-emerald-400"
            : call.errored
              ? "bg-rose-400"
              : isCognee
                ? "bg-violet-400/70"
                : "bg-foreground/75",
        )}
      />
      <span
        className={cn(
          "shrink-0 text-[10.5px] font-semibold",
          isCognee ? "text-violet-400/85" : "text-foreground/80",
        )}
      >
        {sourceTypeLabel(call.toolName)}
      </span>
      <span className="min-w-0 flex-1 truncate text-muted-foreground/70">
        {call.query}
      </span>
      <span className="shrink-0 tabular-nums text-muted-foreground/45">
        {call.status === "running" ? "…" : formatElapsed(elapsed)}
      </span>
    </div>
  );
}

/* ─── helpers ───────────────────────────────────────────────────────────── */

function SectionLabel({ label, caption }: { label: string; caption?: string }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-[10px] font-bold uppercase tracking-[0.14em] text-muted-foreground/65">
        <IconBolt size={11} className="mr-1 inline-block align-text-bottom text-muted-foreground/50" />
        {label}
      </span>
      {caption && (
        <span className="text-[10px] tabular-nums text-muted-foreground/55">
          {caption}
        </span>
      )}
    </div>
  );
}

function LegendDot({
  tone,
  label,
}: {
  tone: "critical" | "high" | "medium" | "low";
  label: string;
}) {
  const color =
    tone === "critical"
      ? "bg-rose-500"
      : tone === "high"
        ? "bg-orange-400"
        : tone === "medium"
          ? "bg-amber-400"
          : "bg-sky-400";
  return (
    <span className="inline-flex items-center gap-1">
      <span className={cn("h-1.5 w-1.5 rounded-full", color)} />
      {label}
    </span>
  );
}

function uniqueAgentIds(calls: ToolCallItem[]): string[] {
  const set = new Set<string>();
  for (const c of calls) {
    const id = sourceTypeToAgentId(c.sourceType);
    if (id) set.add(id);
  }
  return Array.from(set);
}

function abbreviation(label: string): string {
  const cleaned = label.replace(/[^a-zA-Z ]+/g, " ").trim();
  const parts = cleaned.split(/\s+/);
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase();
  return (parts[0][0] + parts[1][0]).toUpperCase();
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toTimeString().slice(0, 8);
}

// Avoid an unused IconDot import.
void IconDot;
