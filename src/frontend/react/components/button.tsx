import { forwardRef } from "react";
import { cn } from "../lib/cn";

type Variant = "default" | "secondary" | "outline" | "ghost" | "destructive";
type Size = "default" | "sm" | "lg" | "icon";

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  asChild?: boolean;
}

const sizeClasses: Record<Size, string> = {
  default: "h-9 px-4 text-[13px]",
  sm: "h-8 px-3 text-[12px]",
  lg: "h-10 px-8 text-[13.5px]",
  icon: "h-9 w-9 text-[13px]",
};

const variantClasses: Record<Variant, string> = {
  default:
    "bg-foreground text-background ring-1 ring-foreground/40 hover:bg-foreground/95",
  secondary:
    "bg-card text-foreground ring-1 ring-border hover:bg-card/80",
  outline:
    "bg-transparent text-foreground ring-1 ring-border hover:bg-card/60",
  ghost:
    "bg-transparent text-foreground hover:bg-card/60 ring-1 ring-transparent",
  destructive:
    "bg-red-500/10 text-red-400 ring-1 ring-red-500/30 hover:bg-red-500/15",
};

/**
 * Cntrl-Panel button: every variant carries a 0.18-opacity 135° hatched ::before
 * via an inline overlay span. The hatch is what makes buttons feel "stamped".
 */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, variant = "default", size = "default", children, type, ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={cn(
        "relative isolate inline-flex select-none items-center justify-center gap-2 rounded-lg font-semibold tracking-tight outline-none",
        "shadow-[0_1px_2px_rgba(0,0,0,0.05)] transition-colors",
        "disabled:cursor-not-allowed disabled:opacity-50",
        "focus-visible:ring-2 focus-visible:ring-foreground/30 focus-visible:ring-offset-0",
        sizeClasses[size],
        variantClasses[variant],
        className,
      )}
      {...rest}
    >
      <span
        aria-hidden
        className="pointer-events-none absolute inset-0 z-0 rounded-[inherit] opacity-[0.18] transition-opacity duration-200 group-hover:opacity-[0.26] hover:opacity-[0.26]"
        style={{
          backgroundImage:
            "repeating-linear-gradient(135deg,currentColor 0,currentColor 1px,transparent 1px,transparent 6px)",
        }}
      />
      <span className="relative z-10 inline-flex items-center gap-2">
        {children}
      </span>
    </button>
  );
});
