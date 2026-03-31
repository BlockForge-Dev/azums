"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { FormEvent, useEffect, useState } from "react";
import { readSession, signup } from "@/lib/app-state";
import { PLAN_ORDER, type PlanTier, formatFreePlayLimit, formatMonthlyPrice } from "@/lib/plans";

// Icons
const BoltIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
    <path d="M13 2L3 14h9l-1 8 10-12h-9l1-8z" />
  </svg>
);

const UserIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
    <circle cx="12" cy="7" r="4" />
  </svg>
);

const MailIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect width="20" height="16" x="2" y="4" rx="2" />
    <path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7" />
  </svg>
);

const LockIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect width="18" height="11" x="3" y="11" rx="2" ry="2" />
    <path d="M7 11V7a5 5 0 0 1 10 0v4" />
  </svg>
);

const BuildingIcon = ({ className }: { className?: string }) => (
  <svg className={className} width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect width="16" height="20" x="4" y="2" rx="2" ry="2" />
    <path d="M9 22v-4h6v4" />
    <path d="M8 6h.01" />
    <path d="M16 6h.01" />
    <path d="M12 6h.01" />
    <path d="M12 10h.01" />
    <path d="M12 14h.01" />
    <path d="M16 10h.01" />
    <path d="M16 14h.01" />
    <path d="M8 10h.01" />
    <path d="M8 14h.01" />
  </svg>
);

export default function SignupPage() {
  const router = useRouter();
  const [fullName, setFullName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [workspace, setWorkspace] = useState("");
  const [plan, setPlan] = useState<PlanTier>("Developer");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (typeof window !== "undefined") {
      const query = new URLSearchParams(window.location.search);
      const inviteToken = query.get("token");
      if (inviteToken) {
        router.replace(`/accept-invite?token=${encodeURIComponent(inviteToken)}`);
        return;
      }
    }
    void readSession().then((session) => {
      if (cancelled) return;
      if (session) {
        router.replace("/app/dashboard");
      }
    });
    return () => {
      cancelled = true;
    };
  }, [router]);

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const result = await signup({
        full_name: fullName,
        email,
        password,
        workspace_name: workspace,
        plan,
      });
      if (result.requires_email_verification) {
        const query = new URLSearchParams({ email });
        router.replace(`/verify-email?${query.toString()}`);
      } else {
        router.replace("/app/onboarding");
      }
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-8 relative">
      {/* Background */}
      <div className="fixed inset-0 -z-10 overflow-hidden">
        <div className="absolute -top-[30%] left-[-20%] w-[60%] h-[60%] rounded-full bg-primary/10 blur-[60px]" />
        <div className="absolute bottom-[-20%] right-[-15%] w-[50%] h-[50%] rounded-full bg-emerald-500/6 blur-[60px]" />
        <div className="absolute inset-0 bg-[linear-gradient(rgba(255,255,255,0.02)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,0.02)_1px,transparent_1px)] bg-[size:40px_40px] [mask-image:radial-gradient(ellipse_80%_80%_at_50%_50%,black_30%,transparent_70%)]" />
      </div>

      <div className="w-full max-w-md animate-fade-in-up">
        <div className="bg-card/80 backdrop-blur-xl border border-border rounded-[20px] p-10 shadow-[0_20px_60px_-20px_rgba(0,0,0,0.4)]">
          {/* Logo */}
          <div className="text-center mb-6">
            <Link href="/" className="inline-flex items-center gap-2 text-foreground no-underline font-bold text-2xl">
              <BoltIcon className="text-primary" />
              <span>Azums</span>
            </Link>
          </div>

          <div className="text-center mb-6">
            <h1 className="text-2xl font-bold mb-2">Create your account</h1>
            <p className="text-muted-foreground text-sm">Start onboarding: workspace, API key, first request, first receipt.</p>
          </div>

          <form className="flex flex-col gap-4" onSubmit={(event) => void onSubmit(event)}>
            <div className="flex flex-col gap-2">
              <label className="text-sm font-semibold text-foreground">Full name</label>
              <div className="relative flex items-center">
                <UserIcon className="absolute left-4 text-muted-foreground pointer-events-none" />
                <input
                  value={fullName}
                  onChange={(event) => setFullName(event.target.value)}
                  placeholder="Jane Operator"
                  required
                  className="w-full px-4 py-3.5 pl-11 border border-border rounded-xl bg-input text-foreground text-sm transition-all duration-200 focus:outline-none focus:border-primary focus:shadow-[0_0_0_3px_rgba(120,200,180,0.15)] placeholder:text-muted-foreground"
                />
              </div>
            </div>
            
            <div className="flex flex-col gap-2">
              <label className="text-sm font-semibold text-foreground">Work email</label>
              <div className="relative flex items-center">
                <MailIcon className="absolute left-4 text-muted-foreground pointer-events-none" />
                <input
                  type="email"
                  value={email}
                  onChange={(event) => setEmail(event.target.value)}
                  placeholder="jane@company.com"
                  required
                  className="w-full px-4 py-3.5 pl-11 border border-border rounded-xl bg-input text-foreground text-sm transition-all duration-200 focus:outline-none focus:border-primary focus:shadow-[0_0_0_3px_rgba(120,200,180,0.15)] placeholder:text-muted-foreground"
                />
              </div>
            </div>
            
            <div className="flex flex-col gap-2">
              <label className="text-sm font-semibold text-foreground">Password</label>
              <div className="relative flex items-center">
                <LockIcon className="absolute left-4 text-muted-foreground pointer-events-none" />
                <input
                  type="password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  placeholder="••••••••"
                  required
                  className="w-full px-4 py-3.5 pl-11 border border-border rounded-xl bg-input text-foreground text-sm transition-all duration-200 focus:outline-none focus:border-primary focus:shadow-[0_0_0_3px_rgba(120,200,180,0.15)] placeholder:text-muted-foreground"
                />
              </div>
            </div>
            
            <div className="flex flex-col gap-2">
              <label className="text-sm font-semibold text-foreground">Workspace name</label>
              <div className="relative flex items-center">
                <BuildingIcon className="absolute left-4 text-muted-foreground pointer-events-none" />
                <input
                  value={workspace}
                  onChange={(event) => setWorkspace(event.target.value)}
                  placeholder="Acme Platform Team"
                  required
                  className="w-full px-4 py-3.5 pl-11 border border-border rounded-xl bg-input text-foreground text-sm transition-all duration-200 focus:outline-none focus:border-primary focus:shadow-[0_0_0_3px_rgba(120,200,180,0.15)] placeholder:text-muted-foreground"
                />
              </div>
            </div>
            
            <div className="flex flex-col gap-2">
              <label className="text-sm font-semibold text-foreground">Plan</label>
              <select 
                value={plan} 
                onChange={(event) => setPlan(event.target.value as PlanTier)}
                className="w-full px-4 py-3.5 border border-border rounded-xl bg-input text-foreground text-sm transition-all duration-200 focus:outline-none focus:border-primary focus:shadow-[0_0_0_3px_rgba(120,200,180,0.15)]"
              >
                {PLAN_ORDER.map((tier) => (
                  <option key={tier} value={tier}>
                    {tier} - {formatMonthlyPrice(tier)}/mo ({formatFreePlayLimit(tier)} free)
                  </option>
                ))}
              </select>
            </div>
            
            <button 
              className="w-full py-4 rounded-xl border-none bg-gradient-to-r from-primary to-emerald-400 text-primary-foreground text-base font-bold cursor-pointer transition-all duration-200 hover:brightness-110 hover:-translate-y-0.5 hover:shadow-[0_8px_20px_-4px_rgba(120,200,180,0.4)] disabled:opacity-70 disabled:cursor-not-allowed disabled:hover:translate-y-0 mt-1" 
              type="submit" 
              disabled={loading}
            >
              {loading ? (
                <span className="flex items-center justify-center gap-2">
                  <svg className="animate-spin h-4 w-4" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                  </svg>
                  Creating account...
                </span>
              ) : (
                "Create workspace"
              )}
            </button>
          </form>

          {error && (
            <div className="mt-4 p-4 rounded-xl bg-destructive/10 border border-destructive/20 text-destructive text-sm text-center">
              {error}
            </div>
          )}

          {/* Links */}
          <div className="mt-6 pt-6 border-t border-border/50 text-center">
            <Link href="/login" className="text-primary hover:underline text-sm no-underline">
              Already have an account? Sign in
            </Link>
            <div className="mt-4">
              <Link href="/" className="text-muted-foreground hover:text-foreground text-sm no-underline">
                ← Back to landing
              </Link>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
