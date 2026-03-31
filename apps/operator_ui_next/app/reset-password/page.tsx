"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { FormEvent, useEffect, useState } from "react";
import { confirmPasswordReset } from "@/lib/app-state";

export default function ResetPasswordPage() {
  const router = useRouter();
  const [token, setToken] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [done, setDone] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const query = new URLSearchParams(window.location.search);
    const value = query.get("token");
    if (value) {
      setToken(value);
    }
  }, []);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    if (!token.trim()) {
      setError("Reset token is missing.");
      return;
    }
    if (password !== confirmPassword) {
      setError("Passwords do not match.");
      return;
    }

    setLoading(true);
    try {
      await confirmPasswordReset(token, password);
      setDone(true);
      setTimeout(() => {
        router.replace("/login");
      }, 1200);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="auth-page">
      <section className="auth-card">
        <p className="eyebrow">Azums</p>
        <h1>Choose new password</h1>
        <p>Set a new password for your account.</p>

        <form className="controls" onSubmit={(event) => void submit(event)}>
          <label>
            Reset token
            <input
              value={token}
              onChange={(event) => setToken(event.target.value)}
              placeholder="tok_xxx"
              required
            />
          </label>
          <label>
            New password
            <input
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              required
            />
          </label>
          <label>
            Confirm password
            <input
              type="password"
              value={confirmPassword}
              onChange={(event) => setConfirmPassword(event.target.value)}
              required
            />
          </label>
          <button className="btn primary" type="submit" disabled={loading || done}>
            {done ? "Updated" : loading ? "Updating..." : "Update password"}
          </button>
        </form>

        {done ? <p className="inline-message">Password updated. Redirecting to sign in…</p> : null}
        {error ? <p className="inline-error">{error}</p> : null}

        <div className="auth-links">
          <Link href="/login">Back to sign in</Link>
        </div>
      </section>
    </div>
  );
}
