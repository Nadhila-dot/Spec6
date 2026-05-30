/**
 * Geographic intelligence panel — real country borders driven by AI-emitted
 * pins ONLY. Markers and country shading come from the model's
 * `<spec6-pins>` block; we never invent geography on the brand's behalf.
 *
 * Uses react-simple-maps over the 110m world TopoJSON shipped from
 * /world-110m.json.
 */

import { useMemo, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import {
  ComposableMap,
  Geographies,
  Geography,
  Graticule,
  Marker,
  Sphere,
} from "react-simple-maps";
import type { ParsedMapPin } from "./utils";

export type HeatTone = "critical" | "high" | "medium" | "low" | "neutral";

const GEO_URL = "/world-110m.json";

const TONE_COLOR: Record<HeatTone, string> = {
  critical: "rgb(244, 63, 94)", // rose-500
  high: "rgb(251, 146, 60)", // orange-400
  medium: "rgb(251, 191, 36)", // amber-400
  low: "rgb(56, 189, 248)", // sky-400
  neutral: "rgb(228, 228, 231)", // zinc-200
};

const TONE_FILL_LIGHT: Record<HeatTone, string> = {
  critical: "rgba(244, 63, 94, 0.42)",
  high: "rgba(251, 146, 60, 0.36)",
  medium: "rgba(251, 191, 36, 0.32)",
  low: "rgba(56, 189, 248, 0.26)",
  neutral: "transparent",
};

interface WorldHeatmapProps {
  /** AI-pushed markdown tooltips. Keyed by ISO numeric code OR by lat/lng. */
  aiPins?: ParsedMapPin[];
  className?: string;
  height?: number;
}

interface Tooltip {
  x: number;
  y: number;
  title: string;
  body?: string;
  isMarkdown?: boolean;
}

export function WorldHeatmap({
  aiPins = [],
  className,
  height = 360,
}: WorldHeatmapProps) {
  const [tooltip, setTooltip] = useState<Tooltip | null>(null);

  const aiPinByIso = useMemo(() => {
    const map = new Map<string, ParsedMapPin>();
    for (const p of aiPins) if (p.iso) map.set(p.iso, p);
    return map;
  }, [aiPins]);

  const aiPinPoints = useMemo(
    () =>
      aiPins.filter(
        (p): p is ParsedMapPin & { lat: number; lng: number } =>
          typeof p.lat === "number" && typeof p.lng === "number",
      ),
    [aiPins],
  );

  return (
    <div className={className}>
      <div className="relative">
        <ComposableMap
          projection="geoEqualEarth"
          projectionConfig={{ scale: Math.max(140, height * 0.5) }}
          width={780}
          height={height}
          style={{ width: "100%", height: "auto" }}
        >
          <Sphere
            id="ww-sphere"
            stroke="currentColor"
            strokeWidth={0.5}
            strokeOpacity={0.12}
            fill="transparent"
          />
          <Graticule
            stroke="currentColor"
            strokeOpacity={0.05}
            strokeWidth={0.4}
          />

          <Geographies geography={GEO_URL}>
            {({ geographies }: { geographies: Array<{ rsmKey: string; id: string; properties: { name?: string } }> }) =>
              geographies.map((geo) => {
                const iso = String(geo.id);
                const aiPin = aiPinByIso.get(iso);
                const heatTone = aiPin?.tone;
                const baseFill = heatTone
                  ? TONE_FILL_LIGHT[heatTone]
                  : "rgba(120, 120, 130, 0.10)";
                const hoverFill = heatTone
                  ? TONE_FILL_LIGHT[heatTone].replace(/0\.\d+\)/, "0.65)")
                  : "rgba(160, 160, 170, 0.18)";
                const name = geo.properties.name ?? "—";
                const tooltipTitle = aiPin
                  ? aiPin.label ?? name
                  : name;
                return (
                  <Geography
                    key={geo.rsmKey}
                    geography={geo}
                    onMouseEnter={(e) =>
                      setTooltip({
                        x: e.clientX,
                        y: e.clientY,
                        title: tooltipTitle,
                        body: aiPin?.md,
                        isMarkdown: !!aiPin?.md,
                      })
                    }
                    onMouseMove={(e) =>
                      setTooltip((prev) =>
                        prev ? { ...prev, x: e.clientX, y: e.clientY } : prev,
                      )
                    }
                    onMouseLeave={() => setTooltip(null)}
                    style={{
                      default: {
                        fill: baseFill,
                        stroke: "currentColor",
                        strokeOpacity: 0.18,
                        strokeWidth: 0.4,
                        outline: "none",
                      },
                      hover: {
                        fill: hoverFill,
                        stroke: "currentColor",
                        strokeOpacity: 0.4,
                        strokeWidth: 0.5,
                        outline: "none",
                        cursor: aiPin ? "pointer" : "default",
                      },
                      pressed: { outline: "none", fill: hoverFill },
                    }}
                  />
                );
              })
            }
          </Geographies>

          {/* AI-pushed lat/lng pins. */}
          {aiPinPoints.map((p, i) => {
            const tone = TONE_COLOR[p.tone];
            return (
              <Marker
                key={`ai-${i}-${p.lat}-${p.lng}`}
                coordinates={[p.lng!, p.lat!]}
                onMouseEnter={(e: { clientX: number; clientY: number }) =>
                  setTooltip({
                    x: e.clientX,
                    y: e.clientY,
                    title: p.label ?? "Spec6 pin",
                    body: p.md,
                    isMarkdown: true,
                  })
                }
                onMouseLeave={() => setTooltip(null)}
              >
                <circle r={9} fill={tone} opacity={0.22}>
                  <animate
                    attributeName="r"
                    values="6;13;6"
                    dur="2.4s"
                    repeatCount="indefinite"
                  />
                </circle>
                <circle r={4.5} fill={tone} stroke="white" strokeWidth={1.1} />
              </Marker>
            );
          })}
        </ComposableMap>

        {tooltip && (tooltip.body || aiPinByIso.size > 0 || tooltip.title) && (
          <div
            className="pointer-events-none fixed z-50 max-w-[320px] rounded-lg bg-popover px-3 py-2 text-[11.5px] leading-[1.45] text-foreground shadow-[0_24px_56px_-10px_rgba(0,0,0,0.5),0_0_0_1px_rgba(255,255,255,0.04)] ring-1 ring-border/60"
            style={{
              left: tooltip.x + 14,
              top: tooltip.y + 14,
            }}
          >
            <div className="mb-1 text-[10px] font-bold uppercase tracking-[0.14em] text-muted-foreground/65">
              {tooltip.title}
            </div>
            {tooltip.body && tooltip.isMarkdown && (
              <div className="prose prose-xs prose-zinc max-w-none dark:prose-invert prose-p:my-0.5 prose-ul:my-1 prose-li:my-0 prose-strong:text-foreground prose-a:text-foreground/80">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>
                  {tooltip.body}
                </ReactMarkdown>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/** Summary stats for the panel header. */
export function pinSummary(pins: ParsedMapPin[]): {
  markerCount: number;
  countryCount: number;
} {
  const countries = new Set<string>();
  let markers = 0;
  for (const p of pins) {
    if (p.iso) countries.add(p.iso);
    if (typeof p.lat === "number" && typeof p.lng === "number") markers += 1;
  }
  return { markerCount: markers, countryCount: countries.size };
}
