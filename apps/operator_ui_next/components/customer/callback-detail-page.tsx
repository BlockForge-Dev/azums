"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import { canWriteRequests, readSession } from "@/lib/app-state";
import { apiGet, formatMs, shortId } from "@/lib/client-api";
import type { CallbackDetailResponse } from "@/lib/types";
import { EmptyState } from "@/components/ui/empty-state";

type CallbackTab = "summary" | "attempts" | "payload" | "headers" | "raw" | "advanced";

export function CallbackDetailPage({ callbackId }: { callbackId: string }) {
  const searchParams = useSearchParams();
  const [detail, setDetail] = useState<CallbackDetailResponse | null>(null);
  const [tab, setTab] = useState<CallbackTab>("summary");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [advancedEnabled, setAdvancedEnabled] = useState(false);
  const tenantOverride = (searchParams.get("tenant_id") ?? "").trim();
  const tenantQuery = tenantOverride
    ? `?tenant_id=${encodeURIComponent(tenantOverride)}`
    : "";

  function withTenant(path: string): string {
    if (!tenantOverride) return path;
    const separator = path.includes("?") ? "&" : "?";
    return `${path}${separator}tenant_id=${encodeURIComponent(tenantOverride)}`;
  }

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [callbackId]);

  useEffect(() => {
    void readSession().then((session) => {
      setAdvancedEnabled(Boolean(session && canWriteRequests(session.role)));
    });
  }, []);

  const callback = detail?.callback;
  const latestReceipt = useMemo(() => {
    const entries = detail?.receipt?.entries ?? [];
    if (!entries.length) return null;
    return entries[entries.length - 1];
  }, [detail?.receipt?.entries]);

  async function load() {
    setLoading(true);
    setError(null);
    try {
      const encoded = encodeURIComponent(callbackId);
      const response = await apiGet<CallbackDetailResponse>(withTenant(`status/callbacks/${encoded}`));
      setDetail(response);
    } catch (loadError: unknown) {
      setDetail(null);
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex flex-col gap-6 p-6 max-w-7xl mx-auto">
      <section className="bg-gradient-to-br from-card to-card/80 rounded-xl border border-border/50 p-6">
        <p className="text-sm font-medium text-muted-foreground mb-1">Callback Detail</p>
        <h2 className="text-2xl font-semibold text-foreground">{callbackId}</h2>
        <p className="text-muted-foreground mt-1">Delivery attempts, execution linkage, and raw callback evidence for this callback id.</p>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-4">
        <button className="px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors" type="button" onClick={() => void load()}>
          {loading ? "Refreshing..." : "Refresh"}
        </button>
        {error ? <p className="mt-3 text-sm text-red-400 bg-red-500/10 px-3 py-2 rounded-md">{error}</p> : null}
      </section>

      {detail && callback ? (
        <>
          <section className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
            <Summary label="Callback ID" value={shortId(detail.callback_id)} />
            <Summary label="Intent ID" value={shortId(detail.intent_id)} />
            <Summary label="State" value={callback.state} />
            <Summary label="Attempts" value={String(callback.attempts)} />
            <Summary label="Last HTTP" value={String(callback.last_http_status ?? "-")} />
            <Summary label="Updated" value={formatMs(callback.updated_at_ms)} />
          </section>

          <section className="bg-card rounded-xl border border-border/50 overflow-hidden">
            <div className="p-4 border-b border-border/50">
              <Link href={`/app/requests/${encodeURIComponent(detail.intent_id)}${tenantQuery}`} className="text-primary hover:underline font-medium">
                Open request detail
              </Link>
            </div>

            <div className="flex border-b border-border">
              <Tab id="summary" current={tab} onSelect={setTab} label="Summary" />
              <Tab id="attempts" current={tab} onSelect={setTab} label="Attempts" />
              <Tab id="payload" current={tab} onSelect={setTab} label="Payload" />
              <Tab id="headers" current={tab} onSelect={setTab} label="Headers" />
              <Tab id="raw" current={tab} onSelect={setTab} label="Raw" />
              <Tab id="advanced" current={tab} onSelect={setTab} label="Advanced" />
            </div>

            <div className="p-6">
              {tab === "summary" ? (
                <div className="space-y-3">
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <p className="text-foreground font-medium">
                      Callback delivery is tracked separately from execution truth.
                    </p>
                    <div className="flex flex-wrap gap-2 mt-2 text-sm text-muted-foreground">
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">delivery_state:{callback.state}</span>
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">attempts:{callback.attempts}</span>
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">next_retry:{formatMs(callback.next_attempt_at_ms ?? null)}</span>
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">delivered:{formatMs(callback.delivered_at_ms ?? null)}</span>
                    </div>
                  </article>
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <p className="text-foreground font-medium">Linked execution state</p>
                    <div className="flex flex-wrap gap-2 mt-2 text-sm text-muted-foreground">
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">request_state:{detail.request.state}</span>
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">classification:{detail.request.classification}</span>
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">updated:{formatMs(detail.request.updated_at_ms)}</span>
                    </div>
                  </article>
                </div>
              ) : null}

              {tab === "attempts" ? (
                <div className="space-y-3">
                  {(callback.attempt_history ?? []).length > 0 ? (
                    (callback.attempt_history ?? []).map((attempt) => (
                      <article
                        key={`${detail.callback_id}-${attempt.attempt_no}-${attempt.occurred_at_ms}`}
                        className="bg-muted/20 rounded-lg border border-border/30 p-4"
                      >
                        <p className="text-foreground font-medium">
                          Attempt #{attempt.attempt_no} - {attempt.outcome}
                        </p>
                        <div className="flex flex-wrap gap-2 mt-2 text-sm text-muted-foreground">
                          <span className="px-2 py-0.5 bg-muted rounded text-xs">http:{attempt.http_status ?? "-"}</span>
                          <span className="px-2 py-0.5 bg-muted rounded text-xs">failure_class:{attempt.failure_class ?? "-"}</span>
                          <span className="px-2 py-0.5 bg-muted rounded text-xs">error:{attempt.error_message ?? "-"}</span>
                          <span className="px-2 py-0.5 bg-muted rounded text-xs">time:{formatMs(attempt.occurred_at_ms)}</span>
                        </div>
                      </article>
                    ))
                  ) : (
                    <EmptyState
                      title="No callback attempts"
                      description="Attempt history appears once callback delivery starts."
                    />
                  )}
                </div>
              ) : null}

              {tab === "payload" ? (
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <h3 className="text-sm font-medium text-foreground mb-2">Latest receipt summary</h3>
                    <pre className="text-xs text-muted-foreground overflow-x-auto">{JSON.stringify(latestReceipt, null, 2)}</pre>
                  </article>
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <h3 className="text-sm font-medium text-foreground mb-2">Callback delivery object</h3>
                    <pre className="text-xs text-muted-foreground overflow-x-auto">{JSON.stringify(callback, null, 2)}</pre>
                  </article>
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <h3 className="text-sm font-medium text-foreground mb-2">Execution status</h3>
                    <pre className="text-xs text-muted-foreground overflow-x-auto">{JSON.stringify(detail.request, null, 2)}</pre>
                  </article>
                </div>
              ) : null}

              {tab === "headers" ? (
                <div className="space-y-3">
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <p className="text-foreground font-medium">Callback headers (expected)</p>
                    <div className="flex flex-wrap gap-2 mt-2 text-sm text-muted-foreground">
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">x-callback-id:{detail.callback_id}</span>
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">x-intent-id:{detail.intent_id}</span>
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">content-type:application/json</span>
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">signature:configured per destination policy</span>
                    </div>
                  </article>
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <p className="text-foreground font-medium">Delivery note</p>
                    <div className="flex flex-wrap gap-2 mt-2 text-sm text-muted-foreground">
                      <span className="px-2 py-0.5 bg-muted rounded text-xs">
                        callback failure does not change underlying execution truth
                      </span>
                    </div>
                  </article>
                </div>
              ) : null}

              {tab === "raw" ? (
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <h3 className="text-sm font-medium text-foreground mb-2">Callback detail</h3>
                    <pre className="text-xs text-muted-foreground overflow-x-auto">{JSON.stringify(detail, null, 2)}</pre>
                  </article>
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <h3 className="text-sm font-medium text-foreground mb-2">Receipt</h3>
                    <pre className="text-xs text-muted-foreground overflow-x-auto">{JSON.stringify(detail.receipt, null, 2)}</pre>
                  </article>
                  <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                    <h3 className="text-sm font-medium text-foreground mb-2">History</h3>
                    <pre className="text-xs text-muted-foreground overflow-x-auto">{JSON.stringify(detail.history, null, 2)}</pre>
                  </article>
                </div>
              ) : null}

              {tab === "advanced" ? (
                advancedEnabled ? (
                  <div className="space-y-3">
                    <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                      <p className="text-foreground font-medium">Transition evidence</p>
                      <div className="flex flex-wrap gap-2 mt-2 text-sm text-muted-foreground">
                        {(detail.history.transitions ?? []).map((transition) => (
                          <span className="px-2 py-0.5 bg-muted rounded text-xs" key={transition.transition_id}>
                            {transition.to_state}:{transition.reason_code}
                          </span>
                        ))}
                        {(detail.history.transitions ?? []).length === 0 ? (
                          <span className="px-2 py-0.5 bg-muted rounded text-xs">No transition evidence.</span>
                        ) : null}
                      </div>
                    </article>
                    <article className="bg-muted/20 rounded-lg border border-border/30 p-4">
                      <p className="text-foreground font-medium">Adapter metadata hints</p>
                      <div className="flex flex-wrap gap-2 mt-2 text-sm text-muted-foreground">
                        {Object.entries(latestReceipt?.details ?? {}).map(([key, value]) => (
                          <span className="px-2 py-0.5 bg-muted rounded text-xs" key={`${key}-${value}`}>
                            {key}:{value}
                          </span>
                        ))}
                        {Object.keys(latestReceipt?.details ?? {}).length === 0 ? (
                          <span className="px-2 py-0.5 bg-muted rounded text-xs">No adapter metadata on latest receipt.</span>
                        ) : null}
                      </div>
                    </article>
                  </div>
                ) : (
                  <EmptyState
                    title="Advanced view is role-gated"
                    description="Developer/admin/owner roles can inspect deeper callback evidence."
                  />
                )
              ) : null}
            </div>
          </section>
        </>
      ) : (
        <section className="bg-card rounded-xl border border-border/50 p-6">
          <EmptyState
            title="Callback not loaded"
            description="Use a valid callback id from requests/receipts/search to inspect detail."
            actionHref={`/app/requests${tenantQuery}`}
            actionLabel="Open requests"
          />
        </section>
      )}
    </div>
  );
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
      <span className="text-sm text-muted-foreground">{label}</span>
      <strong className="text-foreground block text-lg mt-1">{value}</strong>
    </div>
  );
}

function Tab({
  id,
  label,
  current,
  onSelect,
}: {
  id: CallbackTab;
  label: string;
  current: CallbackTab;
  onSelect: (next: CallbackTab) => void;
}) {
  return (
    <button type="button" className={`px-4 py-2 text-sm font-medium transition-colors border-b-2 ${current === id ? "text-primary border-primary" : "text-muted-foreground border-transparent hover:text-foreground hover:border-border"}`} onClick={() => onSelect(id)}>
      {label}
    </button>
  );
}
