"use client";

import { FormEvent, useEffect, useMemo, useState } from "react";
import type {
  BillingAuditEvent,
  BillingProfile,
  BillingProviderConfig,
  UsageSummary,
} from "@/lib/app-state";
import {
  canViewBilling,
  getBillingProviderConfig,
  getBillingProfile,
  getUsageSummary,
  listBillingAuditEvents,
  listInvoices,
  readSession,
  updateBillingProfile,
} from "@/lib/app-state";
import { formatMs } from "@/lib/client-api";
import { EmptyState } from "@/components/ui/empty-state";
import {
  PLAN_ORDER,
  PLAN_SPECS,
  type PlanTier,
  formatFreePlayLimit,
  formatMonthlyPrice,
} from "@/lib/plans";

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
      <span className="text-sm text-muted-foreground">{label}</span>
      <strong className="text-foreground block text-lg mt-1">{value}</strong>
    </div>
  );
}

export function BillingPage() {
  const [profile, setProfile] = useState<BillingProfile | null>(null);
  const [usage, setUsage] = useState<UsageSummary | null>(null);
  const [invoices, setInvoices] = useState<Awaited<ReturnType<typeof listInvoices>>>([]);
  const [billingAudit, setBillingAudit] = useState<BillingAuditEvent[]>([]);
  const [providerConfig, setProviderConfig] = useState<BillingProviderConfig | null>(null);

  const [billingEmail, setBillingEmail] = useState("");
  const [cardBrand, setCardBrand] = useState("");
  const [cardLast4, setCardLast4] = useState("");
  const [paymentReference, setPaymentReference] = useState("");

  const [canManage, setCanManage] = useState(false);
  const [initialLoading, setInitialLoading] = useState(true);
  const [savingPlan, setSavingPlan] = useState<PlanTier | null>(null);
  const [savingMode, setSavingMode] = useState<"free_play" | "paid" | null>(null);
  const [savingPayment, setSavingPayment] = useState(false);

  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      setInitialLoading(true);

      try {
        const [session, usageSummary] = await Promise.all([
          readSession(),
          getUsageSummary(),
        ]);

        if (cancelled) return;

        const allowed = Boolean(session && canViewBilling(session.role));
        setCanManage(allowed);
        setUsage(usageSummary);

        if (!allowed) {
          setProfile(null);
          setInvoices([]);
          setBillingAudit([]);
          setProviderConfig(null);
          return;
        }

        const [nextProfile, nextInvoices, nextAudit, nextProviderConfig] =
          await Promise.all([
            getBillingProfile(),
            listInvoices(),
            listBillingAuditEvents(),
            getBillingProviderConfig(),
          ]);

        if (cancelled) return;

        setProfile(nextProfile);
        setInvoices(nextInvoices);
        setBillingAudit(nextAudit);
        setProviderConfig(nextProviderConfig);

        setBillingEmail(nextProfile?.billing_email ?? "");
        setCardBrand(nextProfile?.card_brand ?? "");
        setCardLast4(nextProfile?.card_last4 ?? "");
      } catch (loadError: unknown) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : String(loadError));
        }
      } finally {
        if (!cancelled) setInitialLoading(false);
      }
    }

    void load();

    return () => {
      cancelled = true;
    };
  }, []);

  const used = usage?.used_requests ?? 0;
  const limit = usage?.free_play_limit ?? 0;
  const isPaid = profile?.access_mode === "paid";
  const usagePct =
    !isPaid && limit > 0 ? Math.min(100, Math.round((used / limit) * 100)) : 0;

  const activePlan = profile?.plan ?? "Developer";
  const currentPlanSpec = PLAN_SPECS[activePlan];
  const paymentReady = Boolean(providerConfig?.ready);

  const paymentStatus = useMemo(() => {
    if (profile?.payment_verified_at_ms) return "Verified";
    if (profile?.payment_reference) return "Pending review";
    return "Not set";
  }, [profile?.payment_reference, profile?.payment_verified_at_ms]);

  async function choosePlan(plan: PlanTier) {
    if (!canManage) {
      setError("Only workspace owner or admin can manage billing.");
      return;
    }

    setSavingPlan(plan);
    setError(null);
    setMessage(null);

    try {
      const next = await updateBillingProfile({ plan });
      setProfile(next);
      setMessage(`Plan updated to ${plan}.`);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSavingPlan(null);
    }
  }

  async function switchBillingMode(mode: "free_play" | "paid") {
    if (!canManage) {
      setError("Only workspace owner or admin can manage billing.");
      return;
    }

    setSavingMode(mode);
    setError(null);
    setMessage(null);

    try {
      const next = await updateBillingProfile({ access_mode: mode });
      const nextUsage = await getUsageSummary();
      setProfile(next);
      setUsage(nextUsage);
      setMessage(
        mode === "paid"
          ? "Billing mode updated to Paid."
          : "Billing mode updated to Free Play."
      );
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSavingMode(null);
    }
  }

  async function savePayment(event: FormEvent) {
    event.preventDefault();

    if (!canManage) {
      setError("Only workspace owner or admin can manage billing.");
      return;
    }

    setSavingPayment(true);
    setMessage(null);
    setError(null);

    const trimmedEmail = billingEmail.trim();
    const trimmedReference = paymentReference.trim();
    const trimmedLast4 = cardLast4.replace(/\D/g, "");

    if (!trimmedEmail) {
      setError("Billing email is required.");
      setSavingPayment(false);
      return;
    }

    if (trimmedReference && !paymentReady) {
      setError("Payment verification is not available yet for this workspace.");
      setSavingPayment(false);
      return;
    }

    if (!trimmedReference) {
      if (!cardBrand.trim()) {
        setError("Select a card brand.");
        setSavingPayment(false);
        return;
      }

      if (trimmedLast4.length !== 4) {
        setError("Enter exactly 4 digits for card last4.");
        setSavingPayment(false);
        return;
      }
    }

    try {
      const next = await updateBillingProfile({
        billing_email: trimmedEmail,
        card_brand: trimmedReference ? undefined : cardBrand,
        card_last4: trimmedReference ? undefined : trimmedLast4,
        flutterwave_transaction_id: trimmedReference || undefined,
      });

      setProfile(next);
      setBillingEmail(next.billing_email ?? trimmedEmail);
      setCardBrand(next.card_brand ?? cardBrand);
      setCardLast4(next.card_last4 ?? "");
      setPaymentReference("");

      setMessage(
        trimmedReference
          ? "Payment reference submitted and billing method updated."
          : "Billing method updated."
      );
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSavingPayment(false);
    }
  }

  return (
    <div className="flex flex-col gap-6 p-6 max-w-7xl mx-auto">
      <section className="bg-gradient-to-br from-card to-card/80 rounded-xl border border-border/50 p-6">
        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
          <div>
            <p className="text-sm font-medium text-muted-foreground mb-1">Billing</p>
            <h2 className="text-2xl font-semibold text-foreground">Plans, usage, and payment</h2>
            <p className="text-muted-foreground mt-1">
              Manage your plan, monitor request usage, and keep your billing details up to date.
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
              {activePlan}
            </span>
            <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${isPaid ? "bg-green-500/20 text-green-400" : "bg-yellow-500/20 text-yellow-400"}`}>
              {isPaid ? "Paid" : "Free Play"}
            </span>
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
              {paymentStatus}
            </span>
          </div>
        </div>
      </section>

      {error ? <section className="bg-red-500/10 border border-red-500/30 text-red-400 rounded-lg p-4">{error}</section> : null}
      {message ? <section className="bg-green-500/10 border border-green-500/30 text-green-400 rounded-lg p-4">{message}</section> : null}

      {!canManage ? (
        <section className="bg-yellow-500/10 border border-yellow-500/30 text-yellow-400 rounded-lg p-4">
          You can view billing information, but only workspace owners or admins can make changes.
        </section>
      ) : null}

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
          <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Current plan</span>
            <strong className="text-foreground block text-lg mt-1">{activePlan}</strong>
            <small className="text-xs text-muted-foreground">{currentPlanSpec.headline}</small>
          </article>

          <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Billing mode</span>
            <strong className="text-foreground block text-lg mt-1">{isPaid ? "Paid" : "Free Play"}</strong>
            <small className="text-xs text-muted-foreground">
              {isPaid ? "No request cap" : formatFreePlayLimit(activePlan)}
            </small>
          </article>

          <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Requests used</span>
            <strong className="text-foreground block text-lg mt-1">{initialLoading ? "..." : used.toLocaleString()}</strong>
            <small className="text-xs text-muted-foreground">Current billing window</small>
          </article>

          <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Payment status</span>
            <strong className="text-foreground block text-lg mt-1">{paymentStatus}</strong>
            <small className="text-xs text-muted-foreground">
              {profile?.payment_verified_at_ms
                ? `Verified ${formatMs(profile.payment_verified_at_ms)}`
                : "No verified payment yet"}
            </small>
          </article>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground">Usage this month</h3>
          <p className="text-sm text-muted-foreground mt-1">
            See how your current request volume compares with your plan.
          </p>
        </div>

        {!isPaid ? (
          <>
            <div className="h-2 bg-muted rounded-full overflow-hidden mb-3">
              <div className="h-full bg-primary transition-all" style={{ width: `${usagePct}%` }} />
            </div>
            <p className="text-foreground mb-2">
              {used.toLocaleString()} / {limit.toLocaleString()} requests used ({usagePct}%)
            </p>
          </>
        ) : (
          <p className="text-foreground mb-3">{used.toLocaleString()} requests recorded in the current billing window.</p>
        )}

        <p className="text-sm text-muted-foreground">
          {isPaid
            ? "Paid workspaces are not limited by monthly request caps."
            : "Free Play usage is limited by your current plan."}
        </p>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground">Choose a plan</h3>
          <p className="text-sm text-muted-foreground mt-1">
            Pick the plan that matches your workspace size and usage needs.
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 mb-6">
          {PLAN_ORDER.map((plan) => {
            const active = profile?.plan === plan;
            const choosing = savingPlan === plan;

            return (
              <article
                key={plan}
                className={`rounded-xl border p-4 ${active ? "border-primary bg-primary/10" : "border-border/50 bg-muted/20"}`}
              >
                <h4 className="text-lg font-semibold text-foreground">{plan}</h4>
                <p className="text-foreground mt-1">{formatMonthlyPrice(plan)}</p>
                <p className="text-sm text-muted-foreground">Free Play: {formatFreePlayLimit(plan)}</p>
                <p className="text-sm text-muted-foreground mb-4">{PLAN_SPECS[plan].headline}</p>

                <button
                  className={`w-full px-3 py-2 rounded-lg font-medium transition-colors ${active ? "bg-muted text-muted-foreground hover:bg-muted/80" : "bg-primary text-primary-foreground hover:bg-primary/90"}`}
                  type="button"
                  onClick={() => void choosePlan(plan)}
                  disabled={!canManage || choosing}
                >
                  {active ? "Current plan" : choosing ? "Updating..." : "Choose plan"}
                </button>
              </article>
            );
          })}
        </div>

        <div className="flex flex-wrap gap-3">
          <button
            className={`px-4 py-2 rounded-lg font-medium transition-colors ${profile?.access_mode === "free_play" ? "bg-muted text-muted-foreground" : "bg-primary text-primary-foreground hover:bg-primary/90"}`}
            type="button"
            disabled={!canManage || savingMode !== null}
            onClick={() => void switchBillingMode("free_play")}
          >
            {savingMode === "free_play" ? "Updating..." : "Use Free Play"}
          </button>

          <button
            className={`px-4 py-2 rounded-lg font-medium transition-colors ${profile?.access_mode === "paid" ? "bg-muted text-muted-foreground" : "bg-primary text-primary-foreground hover:bg-primary/90"}`}
            type="button"
            disabled={!canManage || savingMode !== null}
            onClick={() => void switchBillingMode("paid")}
          >
            {savingMode === "paid" ? "Updating..." : "Use Paid mode"}
          </button>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground">Payment details</h3>
          <p className="text-sm text-muted-foreground mt-1">
            Save your billing contact and attach a verified payment reference or card summary.
          </p>
        </div>

        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Provider</span>
            <strong className="text-foreground block mt-1">{profile?.payment_provider ?? "Not set"}</strong>
          </div>
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Reference</span>
            <strong className="text-foreground block mt-1">{profile?.payment_reference ?? "Not set"}</strong>
          </div>
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Verified at</span>
            <strong className="text-foreground block mt-1">
              {profile?.payment_verified_at_ms
                ? formatMs(profile.payment_verified_at_ms)
                : "Not set"}
            </strong>
          </div>
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Method</span>
            <strong className="text-foreground block mt-1">
              {profile?.card_last4
                ? `${profile.card_brand ?? "Card"} •••• ${profile.card_last4}`
                : "Not set"}
            </strong>
          </div>
        </div>

        <form className="grid grid-cols-1 md:grid-cols-2 gap-4" onSubmit={(event) => void savePayment(event)}>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Billing email</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              type="email"
              value={billingEmail}
              onChange={(event) => setBillingEmail(event.target.value)}
              placeholder="billing@company.com"
              disabled={!canManage}
              required
            />
          </label>

          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Card brand</span>
            <select
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              value={cardBrand}
              onChange={(event) => setCardBrand(event.target.value)}
              disabled={!canManage}
            >
              <option value="">Select brand</option>
              <option value="Visa">Visa</option>
              <option value="Mastercard">Mastercard</option>
              <option value="Amex">Amex</option>
            </select>
          </label>

          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Card last4</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              value={cardLast4}
              onChange={(event) => setCardLast4(event.target.value)}
              placeholder="4242"
              maxLength={4}
              disabled={!canManage}
            />
          </label>

          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Payment reference</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              value={paymentReference}
              onChange={(event) => setPaymentReference(event.target.value)}
              placeholder="Verified payment reference"
              disabled={!canManage}
            />
          </label>

          <div className="md:col-span-2 flex flex-col gap-3">
            <button
              className="self-start px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              type="submit"
              disabled={!canManage || savingPayment}
            >
              {savingPayment ? "Saving..." : "Save payment details"}
            </button>

            <span className="text-xs text-muted-foreground">
              {paymentReady
                ? "Verified payment references are supported."
                : "Payment verification is not available yet."}
            </span>
          </div>
        </form>
      </section>

      <section className="bg-card rounded-xl border border-border/50 overflow-hidden">
        <div className="p-6 pb-4">
          <h3 className="text-lg font-semibold text-foreground">Invoices</h3>
          <p className="text-sm text-muted-foreground mt-1">Closed billing periods and invoice history for this workspace.</p>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Invoice</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Period</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Amount</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Status</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Issued</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {invoices.length === 0 ? (
                <tr>
                  <td colSpan={5} className="px-4 py-6"><EmptyState compact title="No invoices yet" description="Invoices will appear after a billing cycle closes." /></td>
                </tr>
              ) : (
                invoices.map((invoice) => (
                  <tr key={invoice.id} className="hover:bg-muted/30 transition-colors"><td className="px-4 py-3 text-foreground">{invoice.id}</td><td className="px-4 py-3 text-foreground">{invoice.period}</td><td className="px-4 py-3 text-foreground">${invoice.amount_usd.toFixed(2)}</td><td className="px-4 py-3 text-foreground">{invoice.status}</td><td className="px-4 py-3 text-foreground">{formatMs(invoice.issued_at_ms)}</td></tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 overflow-hidden">
        <div className="p-6 pb-4">
          <h3 className="text-lg font-semibold text-foreground">Billing history</h3>
          <p className="text-sm text-muted-foreground mt-1">Track plan, access mode, and payment updates over time.</p>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Change</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Actor</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Payment updated</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">When</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {billingAudit.length === 0 ? (
                <tr>
                  <td colSpan={4} className="px-4 py-6"><EmptyState compact title="No billing changes yet" description="Billing updates will appear here." /></td>
                </tr>
              ) : (
                billingAudit.map((event) => (
                  <tr key={event.event_id} className="hover:bg-muted/30 transition-colors">
                    <td className="px-4 py-3 text-foreground">{event.plan_before} / {event.access_mode_before} → {event.plan_after} / {event.access_mode_after}</td>
                    <td className="px-4 py-3 text-foreground">{event.actor_email} ({event.actor_role})</td>
                    <td className="px-4 py-3 text-foreground">{event.payment_method_updated ? "Yes" : "No"}</td>
                    <td className="px-4 py-3 text-foreground">{formatMs(event.changed_at_ms)}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}