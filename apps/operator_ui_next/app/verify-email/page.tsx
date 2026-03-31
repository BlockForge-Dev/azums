"use client";

import Link from "next/link";
import { FormEvent, useEffect, useState } from "react";
import { confirmEmailVerification, requestEmailVerification } from "@/lib/app-state";

export default function VerifyEmailPage() {
  const [token, setToken] = useState("");
  const [email, setEmail] = useState("");
  const [verifying, setVerifying] = useState(false);
  const [requesting, setRequesting] = useState(false);
  const [verified, setVerified] = useState(false);
  const [requested, setRequested] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const query = new URLSearchParams(window.location.search);
    const tokenValue = query.get("token");
    const emailValue = query.get("email");
    if (tokenValue) setToken(tokenValue);
    if (emailValue) setEmail(emailValue);
  }, []);

  async function verify(event: FormEvent) {
    event.preventDefault();
    setError(null);
    if (!token.trim()) {
      setError("Verification token is required.");
      return;
    }
    setVerifying(true);
    try {
      await confirmEmailVerification(token);
      setVerified(true);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setVerifying(false);
    }
  }

  async function requestLink(event: FormEvent) {
    event.preventDefault();
    setError(null);
    if (!email.trim()) {
      setError("Email is required.");
      return;
    }
    setRequesting(true);
    try {
      await requestEmailVerification(email);
      setRequested(true);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRequesting(false);
    }
  }

  return (
    <div className="auth-page">
      <section className="auth-card">
        <p className="eyebrow">Azums</p>
        <h1>Verify email</h1>
        <p>Confirm your email before signing in to a verification-protected workspace.</p>

        <form className="controls" onSubmit={(event) => void verify(event)}>
          <label>
            Verification token
            <input
              value={token}
              onChange={(event) => setToken(event.target.value)}
              placeholder="tok_xxx"
              required
            />
          </label>
          <button className="btn primary" type="submit" disabled={verifying || verified}>
            {verified ? "Verified" : verifying ? "Verifying..." : "Verify email"}
          </button>
        </form>

        <form className="controls" onSubmit={(event) => void requestLink(event)}>
          <label>
            Resend link to email
            <input
              type="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              placeholder="you@company.com"
              required
            />
          </label>
          <button className="btn ghost" type="submit" disabled={requesting}>
            {requesting ? "Sending..." : "Send verification link"}
          </button>
        </form>

        {verified ? <p className="inline-message">Email verified. You can now sign in.</p> : null}
        {requested ? <p className="inline-message">If account exists, a verification link was sent.</p> : null}
        {error ? <p className="inline-error">{error}</p> : null}

        <div className="auth-links">
          <Link href="/login">Go to sign in</Link>
          <Link href="/">Back to landing</Link>
        </div>
      </section>
    </div>
  );
}
