/**
 * Growth / projection charts, powered by Recharts.
 *
 * The analyst emits a <spec6-trend>…</spec6-trend> JSON block (sibling to
 * <spec6-pins>). We parse it here and render a monochrome, DESIGN.md-styled
 * area / line / bar chart inline in the thread. Projected (forecast) series are
 * drawn dashed so the eye separates "measured" from "modelled".
 */

import { useMemo } from "react";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { IconBolt } from "../components/icons";

/* ─── types ─────────────────────────────────────────────────────────────── */

type TrendTone = "emerald" | "rose" | "amber" | "violet" | "sky" | "foreground";

export interface TrendSeries {
  name: string;
  tone?: TrendTone;
  /** Forecast / projected series — rendered dashed. */
  dashed?: boolean;
}

export interface ParsedTrend {
  title?: string;
  kind: "area" | "line" | "bar";
  unit?: string;
  series: TrendSeries[];
  points: Array<Record<string, string | number>>;
  note?: string;
}

const TONE_HEX: Record<TrendTone, string> = {
  emerald: "#34d399",
  rose: "#fb7185",
  amber: "#fbbf24",
  violet: "#a78bfa",
  sky: "#38bdf8",
  foreground: "#a1a1aa",
};

/* ─── parsing ───────────────────────────────────────────────────────────── */

export function parseSpec6Trend(raw: string): ParsedTrend | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  const tryParse = (s: string): unknown => {
    try {
      return JSON.parse(s);
    } catch {
      try {
        return JSON.parse(
          s
            .replace(/,\s*]/g, "]")
            .replace(/,\s*}/g, "}")
            .replace(/[“”]/g, '"'),
        );
      } catch {
        return null;
      }
    }
  };

  const parsed = tryParse(trimmed);
  if (!parsed || typeof parsed !== "object") return null;
  const obj = parsed as Record<string, unknown>;

  const points = Array.isArray(obj.points)
    ? (obj.points.filter(
        (p) => p && typeof p === "object",
      ) as Array<Record<string, string | number>>)
    : [];
  if (points.length === 0) return null;

  let series: TrendSeries[] = Array.isArray(obj.series)
    ? (obj.series as unknown[])
        .map((s) => {
          if (!s || typeof s !== "object") return null;
          const so = s as Record<string, unknown>;
          if (typeof so.name !== "string") return null;
          const tone =
            typeof so.tone === "string" && so.tone in TONE_HEX
              ? (so.tone as TrendTone)
              : undefined;
          return { name: so.name, tone, dashed: so.dashed === true };
        })
        .filter((s): s is TrendSeries => s !== null)
    : [];

  // Infer series from the point keys if none were declared.
  if (series.length === 0) {
    const keys = new Set<string>();
    for (const p of points) {
      for (const k of Object.keys(p)) {
        if (k !== "x" && typeof p[k] === "number") keys.add(k);
      }
    }
    series = Array.from(keys).map((name) => ({ name }));
  }
  if (series.length === 0) return null;

  const kindRaw = typeof obj.kind === "string" ? obj.kind.toLowerCase() : "area";
  const kind = (["area", "line", "bar"].includes(kindRaw) ? kindRaw : "area") as
    | "area"
    | "line"
    | "bar";

  return {
    title: typeof obj.title === "string" ? obj.title : undefined,
    kind,
    unit: typeof obj.unit === "string" ? obj.unit : undefined,
    series,
    points,
    note: typeof obj.note === "string" ? obj.note : undefined,
  };
}

/* ─── component ─────────────────────────────────────────────────────────── */

const TONE_CYCLE: TrendTone[] = ["emerald", "violet", "amber", "sky", "rose"];

export function TrendChart({ trend }: { trend: ParsedTrend }) {
  const series = useMemo(
    () =>
      trend.series.map((s, i) => ({
        ...s,
        color: TONE_HEX[s.tone ?? TONE_CYCLE[i % TONE_CYCLE.length]],
      })),
    [trend.series],
  );

  const fmt = (v: number) =>
    trend.unit ? `${v}${trend.unit}` : `${v}`;

  return (
    <div className="diagonal-line-card mb-4 rounded-xl border border-border p-2 shadow-sm">
      <div className="flex flex-col gap-2 rounded-lg border border-border bg-card p-3 text-foreground">
        <div className="flex items-center gap-2">
          <span className="text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/65">
            <IconBolt
              size={11}
              className="mr-1 inline-block align-text-bottom text-muted-foreground/50"
            />
            {trend.title ?? "Growth projection"}
          </span>
          <span className="ml-auto flex items-center gap-2.5 text-[9.5px] text-muted-foreground/55">
            {series.map((s) => (
              <span key={s.name} className="inline-flex items-center gap-1">
                <span
                  className="h-1.5 w-1.5 rounded-full"
                  style={{ background: s.color }}
                />
                {s.name}
                {s.dashed ? " · proj." : ""}
              </span>
            ))}
          </span>
        </div>

        <div className="h-[200px] w-full">
          <ResponsiveContainer width="100%" height="100%">
            {renderChart(trend, series, fmt)}
          </ResponsiveContainer>
        </div>

        {trend.note && (
          <p className="text-[11px] leading-[1.45] text-muted-foreground/65">
            {trend.note}
          </p>
        )}
      </div>
    </div>
  );
}

type ResolvedSeries = TrendSeries & { color: string };

function renderChart(
  trend: ParsedTrend,
  series: ResolvedSeries[],
  fmt: (v: number) => string,
) {
  const axis = {
    stroke: "currentColor",
    tick: { fill: "currentColor", fontSize: 10, opacity: 0.5 },
    tickLine: false,
    axisLine: false as const,
  };
  const grid = (
    <CartesianGrid
      stroke="currentColor"
      strokeOpacity={0.08}
      vertical={false}
    />
  );
  const tooltip = (
    <Tooltip
      cursor={{ stroke: "currentColor", strokeOpacity: 0.15 }}
      contentStyle={{
        background: "hsl(var(--popover))",
        border: "1px solid hsl(var(--border))",
        borderRadius: 12,
        fontSize: 11.5,
        boxShadow:
          "0 24px 56px -10px rgba(0,0,0,0.5), 0 0 0 1px rgba(255,255,255,0.04)",
      }}
      labelStyle={{ color: "hsl(var(--foreground))", fontWeight: 600 }}
      formatter={(v: number | string, name: string) => [
        typeof v === "number" ? fmt(v) : v,
        name,
      ]}
    />
  );

  if (trend.kind === "bar") {
    return (
      <BarChart data={trend.points} margin={{ top: 6, right: 6, bottom: 0, left: -18 }}>
        {grid}
        <XAxis dataKey="x" {...axis} />
        <YAxis {...axis} width={44} tickFormatter={fmt} />
        {tooltip}
        {series.map((s) => (
          <Bar key={s.name} dataKey={s.name} fill={s.color} radius={[4, 4, 0, 0]} fillOpacity={s.dashed ? 0.45 : 0.9} />
        ))}
      </BarChart>
    );
  }

  if (trend.kind === "line") {
    return (
      <LineChart data={trend.points} margin={{ top: 6, right: 6, bottom: 0, left: -18 }}>
        {grid}
        <XAxis dataKey="x" {...axis} />
        <YAxis {...axis} width={44} tickFormatter={fmt} />
        {tooltip}
        {series.map((s) => (
          <Line
            key={s.name}
            type="monotone"
            dataKey={s.name}
            stroke={s.color}
            strokeWidth={2}
            strokeDasharray={s.dashed ? "4 4" : undefined}
            dot={false}
            activeDot={{ r: 4 }}
          />
        ))}
      </LineChart>
    );
  }

  return (
    <AreaChart data={trend.points} margin={{ top: 6, right: 6, bottom: 0, left: -18 }}>
      <defs>
        {series.map((s) => (
          <linearGradient key={s.name} id={`grad-${slug(s.name)}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={s.color} stopOpacity={0.35} />
            <stop offset="100%" stopColor={s.color} stopOpacity={0.02} />
          </linearGradient>
        ))}
      </defs>
      {grid}
      <XAxis dataKey="x" {...axis} />
      <YAxis {...axis} width={44} tickFormatter={fmt} />
      {tooltip}
      {series.map((s) => (
        <Area
          key={s.name}
          type="monotone"
          dataKey={s.name}
          stroke={s.color}
          strokeWidth={2}
          strokeDasharray={s.dashed ? "4 4" : undefined}
          fill={`url(#grad-${slug(s.name)})`}
        />
      ))}
    </AreaChart>
  );
}

function slug(name: string): string {
  return name.replace(/[^a-zA-Z0-9]+/g, "-").toLowerCase();
}
