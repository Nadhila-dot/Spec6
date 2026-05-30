import { useState } from "react";
import { Button } from "../components/button";
import { IconArrowRight } from "../components/icons";
import { Input } from "../components/input";
import { cn } from "../lib/cn";

export function LoginPage() {
  return (
    <AuthShell
      heading="Sign in"
      sub="Continue to Spec6."
      form={<LoginForm />}
      switchTo={{ href: "/signup", label: "No account yet?", cta: "Create one" }}
    />
  );
}

function LoginForm() {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const res = await fetch("/api/auth/login", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ username, password }),
      });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        throw new Error(data.error ?? `Request failed (${res.status})`);
      }
      window.location.href = "/chat";
    } catch (err) {
      setError(err instanceof Error ? err.message : "Sign-in failed");
      setBusy(false);
    }
  };

  return (
    <form className="flex flex-col gap-3" onSubmit={submit}>
      <Input
        autoFocus
        autoComplete="username"
        required
        value={username}
        onChange={(e) => setUsername(e.target.value)}
        placeholder="Username"
        className="h-12 px-4 text-[14px]"
      />
      <Input
        type="password"
        autoComplete="current-password"
        required
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        placeholder="Password"
        className="h-12 px-4 text-[14px]"
      />

      {error && <InlineError message={error} />}

      <Button type="submit" disabled={busy} className="mt-1 h-12 w-full text-[14px]">
        {busy ? "Signing in…" : "Continue"}
        {!busy && <IconArrowRight size={15} />}
      </Button>
    </form>
  );
}

/* ─────────────────────────────────────────────────────────────────────────
   Shared shell — minimal, centered, no double-wrapper card.
   ───────────────────────────────────────────────────────────────────────── */

export function AuthShell({
  heading,
  sub,
  form,
  switchTo,
}: {
  heading: string;
  sub: string;
  form: React.ReactNode;
  switchTo: { href: string; label: string; cta: string };
}) {
  return (
    <div className="app-container flex min-h-full flex-col bg-background text-foreground">
      <header className="flex items-center px-6 py-5 sm:px-10">
        <span className="font-chillax text-[14.5px] font-semibold tracking-tight text-foreground/95">
          Spec6
        </span>
      </header>

      <main className="flex flex-1 items-center justify-center px-6 pb-16 sm:px-10">
        <div className="flex w-full max-w-[360px] flex-col gap-7">
          <div className="flex flex-col gap-2 text-center">
            <h1 className="font-chillax text-[28px] font-semibold leading-tight tracking-tight text-foreground sm:text-[32px]">
              {heading}
            </h1>
            <p className="text-[13.5px] text-muted-foreground/75">{sub}</p>
          </div>

          {form}

          <p className="text-center text-[12.5px] text-muted-foreground/65">
            {switchTo.label}{" "}
            <a
              className="font-semibold text-foreground underline-offset-4 hover:underline"
              href={switchTo.href}
            >
              {switchTo.cta}
            </a>
          </p>
        </div>
      </main>

      <footer className="flex items-center justify-center px-6 pb-6 text-[10.5px] tabular-nums text-muted-foreground/40">
        <span>© {new Date().getFullYear()} Spec6</span>
      </footer>
    </div>
  );
}

export function InlineError({ message }: { message: string }) {
  return (
    <div
      className={cn(
        "flex items-start gap-2 rounded-lg bg-red-500/[0.08] px-3 py-2 text-[12px] text-red-400",
        "ring-1 ring-red-500/25",
      )}
    >
      <span className="mt-0.5 inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-red-400" />
      <span>{message}</span>
    </div>
  );
}
