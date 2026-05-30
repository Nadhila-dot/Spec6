import { cn } from "../lib/cn";

/**
 * The signature double-wrapper card.
 * Outer hatched mat + inner solid card. Never collapse into one layer.
 */
export function Card({
  className,
  innerClassName,
  children,
}: {
  className?: string;
  innerClassName?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "diagonal-line-card rounded-xl border border-border p-3 shadow-sm",
        className,
      )}
    >
      <div
        className={cn(
          "flex h-full flex-col gap-6 rounded-lg border border-border bg-card p-6 text-card-foreground",
          innerClassName,
        )}
      >
        {children}
      </div>
    </div>
  );
}
