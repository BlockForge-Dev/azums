"use client";

import Link from "next/link";
import { FormEvent, useState } from "react";
import { requestPasswordReset } from "@/lib/app-state";

export default function ForgotPasswordPage() {
  const passwordResetEnabled = process.env.NEXT_PUBLIC_PASSWORD_RESET_ENABLED !== "false";
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!passwordResetEnabled) {
      setError("Password reset is disabled for this deployment.");
      return;
    }
    setError(null);
    setLoading(true);
    try {
      await requestPasswordReset(email);
      setSent(true);
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
        <h1>Reset password</h1>
        <p>
          {passwordResetEnabled
            ? "Enter your account email and we will send a password reset link if the account exists."
            : "Password reset is disabled for this deployment. Contact workspace support for account recovery."}
        </p>

        <form className="controls" onSubmit={(event) => void submit(event)}>
          <label>
            Email
            <input
              type="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              required
            />
          </label>
          <button
            className="btn primary"
            type="submit"
            disabled={loading || !passwordResetEnabled}
          >
            {loading ? "Sending..." : "Send reset link"}
          </button>
        </form>

        {sent ? <p className="inline-message">If account exists, reset instructions were sent.</p> : null}
        {error ? <p className="inline-error">{error}</p> : null}

        <div className="auth-links">
          <Link href="/login">Back to sign in</Link>
        </div>
      </section>
    </div>
  );
}
