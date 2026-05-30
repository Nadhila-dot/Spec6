import { cn } from "../lib/cn";

const DIAGONAL_STYLE: React.CSSProperties = {
  backgroundImage:
    "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
};

/**
 * The "studied" footer: horizontal meta strip pill.
 * Hatched left segment with version + commit + date, then copyright, then a
 * free-form message on the right. Opacity descends from left (most important)
 * to right (least). The footer is furniture, not content.
 */
export function MetaFooter({
  version = "v0.1.0",
  commit = "fad1abc",
  date,
  message,
  className,
}: {
  version?: string;
  commit?: string;
  date?: Date;
  message?: React.ReactNode;
  className?: string;
}) {
  const d = date ?? new Date();
  const dateStr = d.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
  const year = d.getFullYear();

  return (
    <footer className={cn("mt-3 px-1 pb-1.5", className)}>
      <div className="relative flex items-stretch overflow-hidden rounded-xl ring-1 ring-border/35">
        <div className="relative flex shrink-0 items-center gap-2.5 px-4 py-2.5">
          <div
            aria-hidden
            className="absolute inset-0 bg-foreground/[0.025] text-foreground/[0.08]"
            style={DIAGONAL_STYLE}
          />
          <span className="relative inline-flex items-center rounded-full bg-foreground/[0.08] px-2.5 py-0.5 text-[10px] font-bold tabular-nums tracking-tight text-muted-foreground/70 ring-1 ring-white/[0.07]">
            {version}
          </span>
          <span className="relative inline-flex items-center gap-1.5 font-mono text-[10px] text-muted-foreground/45">
            <CommitIcon />
            {commit.slice(0, 7)}
          </span>
          <span className="relative text-[10px] tabular-nums text-muted-foreground/35">
            {dateStr}
          </span>
        </div>

        <Divider />

        <div className="flex shrink-0 items-center px-3 text-[10px] tabular-nums text-muted-foreground/50">
          © {year}
        </div>

        <Divider />

        <div className="flex flex-1 items-center justify-end gap-3 px-4 text-[10.5px] text-muted-foreground/40">
          {message ?? <span>Made by nadhi.dev</span>}
        </div>
      </div>
    </footer>
  );
}

function Divider() {
  return <div aria-hidden className="w-px self-stretch bg-border/35" />;
}

function CommitIcon() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <circle cx="12" cy="12" r="3.2" />
      <path d="M2 12h6.5M15.5 12H22" />
    </svg>
  );
}
