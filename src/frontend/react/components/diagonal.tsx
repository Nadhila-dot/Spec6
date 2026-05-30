import { cn } from "../lib/cn";

/**
 * Absolutely-positioned 135° pinstripe overlay.
 * Inherits `currentColor` so a parent's text colour drives the hatch tone.
 * Used as the "active state" inside nav items, tabs, hero corners, etc.
 */
export function DiagonalAccent({
  className,
  spacing = 6,
  opacity = 0.085,
  style,
}: {
  className?: string;
  spacing?: number;
  opacity?: number;
  style?: React.CSSProperties;
}) {
  return (
    <span
      aria-hidden
      className={cn("pointer-events-none absolute inset-0", className)}
      style={{
        opacity,
        backgroundImage: `repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent ${spacing}px)`,
        ...style,
      }}
    />
  );
}

/**
 * The canonical "branded icon chip" — zinc gradient + white 0.16 hatch + ring.
 * Always dark regardless of theme; this is the design's only fixed tone.
 */
export function HatchedChip({
  children,
  size = 36,
  className,
}: {
  children: React.ReactNode;
  size?: number;
  className?: string;
}) {
  return (
    <div
      className={cn(
        "relative shrink-0 overflow-hidden rounded-lg ring-1 ring-white/10",
        className,
      )}
      style={{ width: size, height: size }}
    >
      <div className="absolute inset-0 bg-gradient-to-br from-zinc-700 to-zinc-950" />
      <div
        className="absolute inset-0"
        style={{
          backgroundImage:
            "repeating-linear-gradient(135deg,rgba(255,255,255,0.16) 0,rgba(255,255,255,0.16) 1px,transparent 1px,transparent 6px)",
        }}
      />
      <span className="absolute inset-0 flex items-center justify-center text-white/85">
        {children}
      </span>
    </div>
  );
}
