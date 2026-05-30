import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { IconAlert, IconClose } from "../components/icons";
import { cn } from "./cn";

type ErrorEntry = {
  id: string;
  message: string;
  createdAt: number;
};

type ErrorStoreValue = {
  error: ErrorEntry | null;
  pushError: (message: string) => string;
  dismissError: () => void;
};

const AUTO_DISMISS_MS = 7000;

const ErrorStoreContext = createContext<ErrorStoreValue | null>(null);

export function ErrorStoreProvider({ children }: { children: ReactNode }) {
  const [error, setError] = useState<ErrorEntry | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimer = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  const dismissError = useCallback(() => {
    clearTimer();
    setError(null);
  }, []);

  const pushError = useCallback((message: string) => {
    const trimmed = message.trim();
    if (!trimmed) return "";
    const id = `err-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    clearTimer();
    setError({ id, message: trimmed, createdAt: Date.now() });
    timerRef.current = setTimeout(() => {
      setError(null);
      timerRef.current = null;
    }, AUTO_DISMISS_MS);
    return id;
  }, []);

  // Cleanup any pending timer on unmount.
  useEffect(() => {
    return () => clearTimer();
  }, []);

  return (
    <ErrorStoreContext.Provider
      value={{ error, pushError, dismissError }}
    >
      {/* Top-level shell: banner pushes app content down (no overlay). */}
      <div className="flex h-screen w-screen flex-col overflow-hidden">
        {error && (
          <ErrorBanner
            key={error.id}
            message={error.message}
            onDismiss={dismissError}
          />
        )}
        <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
      </div>
    </ErrorStoreContext.Provider>
  );
}

export function useErrorStore(): ErrorStoreValue {
  const ctx = useContext(ErrorStoreContext);
  if (!ctx) {
    throw new Error("useErrorStore must be used inside <ErrorStoreProvider>");
  }
  return ctx;
}

/**
 * Edge-to-edge top notice bar. Solid red tint, 135° hatch signature,
 * centered single-line message with an absolute dismiss on the right.
 * No gradients, no glow — just the design language at the top of the page.
 */
function ErrorBanner({
  message,
  onDismiss,
}: {
  message: string;
  onDismiss: () => void;
}) {
  return (
    <div
      role="alert"
      className={cn(
        "pointer-events-auto relative isolate w-full overflow-hidden",
        "bg-red-500/[0.14] border-b border-red-500/35",
        "err-banner-enter",
      )}
    >
      {/* 135° hatch — design signature, tinted red */}
      <span
        aria-hidden
        className="pointer-events-none absolute inset-0 z-0 opacity-[0.20]"
        style={{
          backgroundImage:
            "repeating-linear-gradient(135deg,#ef4444 0,#ef4444 1px,transparent 1px,transparent 6px)",
        }}
      />

      <div className="relative z-10 flex h-10 items-center justify-center px-12">
        <span className="inline-flex min-w-0 max-w-full items-center gap-2">
          <IconAlert
            size={12}
            className="shrink-0 text-red-200/85"
          />
          <span className="shrink-0 text-[10px] font-bold uppercase tracking-[0.16em] text-red-200/75">
            Error
          </span>
          <span aria-hidden className="shrink-0 text-red-300/35">
            ·
          </span>
          <span
            className="min-w-0 truncate text-[12.5px] tracking-tight text-red-50/95"
            title={message}
          >
            {message}
          </span>
        </span>

        <button
          type="button"
          onClick={onDismiss}
          aria-label="Dismiss error"
          className={cn(
            "absolute right-3 top-1/2 grid h-6 w-6 -translate-y-1/2 place-items-center rounded-md text-red-200/70",
            "transition-colors hover:bg-red-500/15 hover:text-red-100",
          )}
        >
          <IconClose size={12} />
        </button>
      </div>
    </div>
  );
}
