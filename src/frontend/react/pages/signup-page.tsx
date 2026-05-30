import { useState } from "react";
import { Button } from "../components/button";
import { IconArrowRight } from "../components/icons";
import { Input } from "../components/input";
import { AuthShell, InlineError } from "./login-page";

export function SignupPage() {
  return (
    <AuthShell
      heading="Create an account"
      sub="Get access to Sentinel."
      form={<SignupForm />}
      switchTo={{ href: "/login", label: "Already have an account?", cta: "Sign in" }}
    />
  );
}

function SignupForm() {
  const [username, setUsername] = useState("");
  const [display, setDisplay] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      const res = await fetch("/api/auth/signup", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({
          username,
          display_name: display || username,
          password,
        }),
      });
      if (!res.ok) {
        const data = await res.json().catch(() => ({}));
        throw new Error(data.error ?? `Request failed (${res.status})`);
      }
      window.location.href = "/chat";
    } catch (err) {
      setError(err instanceof Error ? err.message : "Sign-up failed");
      setBusy(false);
    }
  };

  return (
    <form className="flex flex-col gap-3" onSubmit={submit}>
      <Input
        autoFocus
        autoComplete="username"
        required
        minLength={3}
        maxLength={24}
        value={username}
        onChange={(e) => setUsername(e.target.value)}
        placeholder="Username"
        className="h-12 px-4 text-[14px]"
      />
      <Input
        autoComplete="nickname"
        value={display}
        maxLength={48}
        onChange={(e) => setDisplay(e.target.value)}
        placeholder="Display name (optional)"
        className="h-12 px-4 text-[14px]"
      />
      <Input
        type="password"
        autoComplete="new-password"
        required
        minLength={8}
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        placeholder="Password (8+ characters)"
        className="h-12 px-4 text-[14px]"
      />

      {error && <InlineError message={error} />}

      <Button type="submit" disabled={busy} className="mt-1 h-12 w-full text-[14px]">
        {busy ? "Creating account…" : "Continue"}
        {!busy && <IconArrowRight size={15} />}
      </Button>
    </form>
  );
}
