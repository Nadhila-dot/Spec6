/**
 * Autonomous monitoring status — the "Watchtower".
 *
 * Sentinel doesn't only answer when asked. A background patrol re-scans every
 * tracked company on a schedule, refreshing its threat dossier without a human
 * in the loop. This banner surfaces that activity and lets the operator kick a
 * patrol off on demand (great for live demos).
 */

import { useCallback, useEffect, useState } from "react";
import { DiagonalAccent, HatchedChip } from "../components/diagonal";
import { IconBolt } from "../components/icons";

interface WatchtowerStatus {
  enabled: boolean;
  interval_secs: number;
  last_patrol_unix_ms: number;
  total_patrols: number;
  total_scans_triggered: number;
}

export function AutonomyBanner() {
  const [status, setStatus] = useState<WatchtowerStatus | null>(null);
  const [running, setRunning] = useState(false);
  const [flash, setFlash] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const r = await fetch("/api/watchtower/status", { credentials: "include" });
      if (!r.ok) return;
      setStatus((await r.json()) as WatchtowerStatus);
    } catch {
      /* non-fatal */
    }
  }, []);

  useEffect(() => {
    load();
    const id = setInterval(load, 15000);
    return () => clearInterval(id);
  }, [load]);

  const runNow = useCallback(async () => {
    setRunning(true);
    setFlash(null);
    try {
      const r = await fetch("/api/watchtower/run", {
        method: "POST",
        credentials: "include",
      });
      if (!r.ok) throw new Error(`run ${r.status}`);
      const data = (await r.json()) as { scans_triggered: number };
      setFlash(
        data.scans_triggered > 0
          ? `Dispatched ${data.scans_triggered} autonomous scan${data.scans_triggered === 1 ? "" : "s"}.`
          : "All companies are already up to date.",
      );
      load();
    } catch {
      setFlash("Couldn't start a patrol.");
    } finally {
      setRunning(false);
    }
  }, [load]);

  if (!status) return null;

  return (
    <div className="relative w-full max-w-xl overflow-hidden rounded-xl border border-border bg-card p-3 text-left shadow-[0_1px_2px_rgba(0,0,0,0.05)]">
      <DiagonalAccent className="text-foreground rounded-xl opacity-[0.035]" />
      <div className="relative flex items-center gap-3">
        <HatchedChip size={32}>
          <IconBolt size={15} />
        </HatchedChip>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[9.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/55">
              Autonomous monitoring
            </span>
            <span
              className={
                "inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[9px] font-bold uppercase tracking-[0.12em] ring-1 " +
                (status.enabled
                  ? "bg-emerald-500/10 text-emerald-400 ring-emerald-500/25"
                  : "bg-foreground/5 text-muted-foreground/60 ring-border/60")
              }
            >
              {status.enabled && (
                <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-400" />
              )}
              {status.enabled ? "Armed" : "Idle"}
            </span>
          </div>
          <div className="mt-0.5 text-[12px] leading-tight text-foreground/80">
            {flash ?? (
              <>
                Sentinel re-scans every tracked company every{" "}
                <span className="tabular-nums">
                  {formatInterval(status.interval_secs)}
                </span>
                .{" "}
                <span className="text-muted-foreground/65">
                  {status.total_scans_triggered > 0
                    ? `${status.total_scans_triggered} autonomous scan${status.total_scans_triggered === 1 ? "" : "s"} so far · last patrol ${formatAgo(status.last_patrol_unix_ms)}.`
                    : "No patrol has run yet."}
                </span>
              </>
            )}
          </div>
        </div>
        <button
          type="button"
          onClick={runNow}
          disabled={running}
          className="shrink-0 rounded-lg bg-foreground/[0.06] px-2.5 py-1.5 text-[11px] font-semibold text-foreground/85 ring-1 ring-border/60 transition hover:bg-foreground/[0.1] hover:text-foreground disabled:opacity-50"
        >
          {running ? "Patrolling…" : "Run patrol now"}
        </button>
      </div>
    </div>
  );
}

function formatInterval(secs: number): string {
  if (secs >= 3600) return `${Math.round(secs / 3600)}h`;
  if (secs >= 60) return `${Math.round(secs / 60)}m`;
  return `${secs}s`;
}

function formatAgo(unixMs: number): string {
  if (!unixMs) return "never";
  const delta = Date.now() - unixMs;
  if (delta < 60_000) return "just now";
  if (delta < 3_600_000) return `${Math.round(delta / 60_000)}m ago`;
  if (delta < 86_400_000) return `${Math.round(delta / 3_600_000)}h ago`;
  return `${Math.round(delta / 86_400_000)}d ago`;
}
