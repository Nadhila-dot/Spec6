import { forwardRef } from "react";
import { cn } from "../lib/cn";

export interface InputProps
  extends React.InputHTMLAttributes<HTMLInputElement> {
  invalid?: boolean;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { className, invalid, ...rest },
  ref,
) {
  return (
    <input
      ref={ref}
      className={cn(
        "h-10 w-full rounded-lg bg-card px-3 text-[13px] text-foreground placeholder:text-muted-foreground/55",
        "ring-1 ring-border/60 outline-none transition-shadow",
        "focus:ring-foreground/35 focus:bg-card",
        "disabled:cursor-not-allowed disabled:opacity-50",
        invalid && "ring-red-500/50 focus:ring-red-400/70",
        className,
      )}
      {...rest}
    />
  );
});

export interface LabelProps
  extends React.LabelHTMLAttributes<HTMLLabelElement> {}

export function Label({ className, ...rest }: LabelProps) {
  return (
    <label
      className={cn(
        "text-[10.5px] font-bold uppercase tracking-[0.14em] text-muted-foreground/85",
        className,
      )}
      {...rest}
    />
  );
}
