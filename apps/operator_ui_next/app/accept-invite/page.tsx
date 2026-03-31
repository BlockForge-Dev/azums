"use client";

import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { FormEvent, Suspense, useEffect, useState } from "react";
import { acceptInvite, lookupInvite, readSession, type InviteSummary } from "@/lib/app-state";
import { formatMs } from "@/lib/client-api";
import { EmptyState } from "@/components/ui/empty-state";

export default function AcceptInvitePage() {
  return (
    <Suspense fallback={<InviteFallback />}>
      <AcceptInviteView />
    </Suspense>
  );
}

function InviteFallback() {
  return (
    <div className="auth-page">
      <section className="auth-card">
        <p className="eyebrow">Azums</p>
        <h1>Accept Team Invite</h1>
        <p>Loading invite details...</p>
      </section>
    </div>
  );
}

function AcceptInviteView() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const [invite, setInvite] = useState<InviteSummary | null>(null);
  const [fullName, setFullName] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [accepted, setAccepted] = useState(false);
  const [token, setToken] = useState("");
  const [loading, setLoading] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [hasSession, setHasSession] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const inviteToken = searchParams.get("token")?.trim() ?? "";
    setToken(inviteToken);
    if (!inviteToken) {
      setError("Invite token is missing.");
      return;
    }

    let cancelled = false;
    setLoading(true);
    void readSession().then((session) => {
      if (!cancelled) {
        setHasSession(Boolean(session));
      }
    });
    void lookupInvite(inviteToken)
      .then((data) => {
        if (cancelled) return;
        setInvite(data);
        setFullName(data.email.split("@")[0] || "");
        setError(null);
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setInvite(null);
        setError(err instanceof Error ? err.message : String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [router, searchParams]);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    if (!token) {
      setError("Invite token is missing.");
      return;
    }
    if (!invite) {
      setError("Invite is not available.");
      return;
    }
    if (!fullName.trim()) {
      setError("Full name is required.");
      return;
    }
    if (password !== confirmPassword) {
      setError("Password confirmation does not match.");
      return;
    }
    if (!isStrongPassword(password)) {
      setError("Password must be at least 10 characters and include letters and numbers.");
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      await acceptInvite({
        token,
        full_name: fullName.trim(),
        password,
      });
      setAccepted(true);
      setTimeout(() => {
        router.replace("/app/onboarding");
      }, 1200);
    } catch (err: unknown) {
      setError(normalizeInviteError(err instanceof Error ? err.message : String(err)));
    } finally {
      setSubmitting(false);
    }
  }

  const expired = Boolean(invite && invite.expires_at_ms < Date.now());

  return (
    <div className="auth-page">
      <section className="auth-card">
        <p className="eyebrow">Azums</p>
        <h1>Accept Team Invite</h1>
        <p>Use this secure invite to join the workspace with the assigned role.</p>

        {loading ? <p>Checking invite...</p> : null}
        {hasSession ? (
          <section className="surface warn-surface">
            You are already signed in. Continue only if this invite is for the same person.
          </section>
        ) : null}

        {invite ? (
          <section className="surface">
            <div className="meta-grid">
              <div>
                <span>Workspace</span>
                <strong>{invite.workspace_name}</strong>
              </div>
              <div>
                <span>Role</span>
                <strong>{invite.role}</strong>
              </div>
              <div>
                <span>Email</span>
                <strong>{invite.email}</strong>
              </div>
              <div>
                <span>Expires</span>
                <strong>{formatMs(invite.expires_at_ms)}</strong>
              </div>
              <div>
                <span>Token</span>
                <strong>{shortToken(token)}</strong>
              </div>
            </div>
          </section>
        ) : null}

        {invite && !expired && !accepted ? (
          <form className="controls" onSubmit={(event) => void onSubmit(event)}>
            <label>
              Full name
              <input
                value={fullName}
                onChange={(event) => setFullName(event.target.value)}
                placeholder="Jane Developer"
                required
              />
            </label>
            <label>
              Invited email
              <input value={invite.email} disabled />
            </label>
            <label>
              Password
              <input
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                placeholder="Create a secure password"
                required
              />
            </label>
            <label>
              Confirm password
              <input
                type="password"
                value={confirmPassword}
                onChange={(event) => setConfirmPassword(event.target.value)}
                placeholder="Repeat password"
                required
              />
            </label>
            <p className="hint-line">
              Password policy: 10+ chars with letters and numbers.
            </p>
            <button className="btn primary" type="submit" disabled={submitting}>
              {submitting ? "Joining workspace..." : "Accept invite"}
            </button>
          </form>
        ) : null}

        {invite && expired ? (
          <section className="surface warn-surface">
            This invite has expired. Ask your workspace admin to send a new invitation link.
          </section>
        ) : null}

        {accepted ? (
          <section className="surface success-surface">
            Invite accepted. Redirecting to onboarding...
          </section>
        ) : null}

        {error ? <p className="inline-error">{error}</p> : null}

        {!invite && !loading && !error ? (
          <section className="surface">
            <EmptyState
              compact
              title="Invite not found"
              description="This invite token is invalid, missing, or has already been used."
              actionHref="/login"
              actionLabel="Go to login"
            />
          </section>
        ) : null}

        <div className="auth-links">
          <Link href="/login">Sign in</Link>
          <Link href="/signup">Create workspace instead</Link>
          <Link href="/">Back to landing</Link>
        </div>
      </section>
    </div>
  );
}

function isStrongPassword(password: string) {
  return password.length >= 10 && /[a-z]/i.test(password) && /\d/.test(password);
}

function shortToken(token: string) {
  if (!token) return "-";
  if (token.length <= 14) return token;
  return `${token.slice(0, 7)}...${token.slice(-5)}`;
}

function normalizeInviteError(message: string) {
  if (message.includes("already exists")) {
    return "An account for this invite email already exists. Sign in instead.";
  }
  if (message.includes("expired")) {
    return "This invite has expired. Ask your workspace admin to send a new invite.";
  }
  if (message.includes("invalid")) {
    return "Invite is invalid or already used.";
  }
  return message;
}
