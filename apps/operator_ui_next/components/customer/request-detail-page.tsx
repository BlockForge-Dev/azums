"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import {
  canManageWorkspace,
  markOnboardingStep,
  readSession,
  type WorkspaceRole,
} from "@/lib/app-state";
import { apiGet, apiRequest, formatMs } from "@/lib/client-api";
import type {
  CallbackHistoryResponse,
  HistoryResponse,
  ReceiptResponse,
  ReplayResponse,
  RequestStatusResponse,
  UiConfigResponse,
  UnifiedRequestStatusResponse,
} from "@/lib/types";
import { EmptyState } from "@/components/ui/empty-state";
import { UnifiedRequestInsights } from "@/components/customer/unified-request-insights";

type DetailTab = "receipt" | "attempts" | "callbacks" | "payload" | "replay";

function middleEllipsis(value: string, start = 18, end = 12) {
  if (!value || value.length <= start + end + 3) return value;
  return `${value.slice(0, start)}...${value.slice(-end)}`;
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
      <span className="text-sm text-muted-foreground">{label}</span>
      <strong className="text-foreground block mt-1 truncate" title={value}>{value}</strong>
    </div>
  );
}

function Tab({
  id,
  label,
  current,
  onSelect,
}: {
  id: DetailTab;
  label: string;
  current: DetailTab;
  onSelect: (next: DetailTab) => void;
}) {
  return (
    <button
      type="button"
      className={`px-4 py-2 text-sm font-medium transition-colors border-b-2 ${
        current === id
          ? "text-primary border-primary"
          : "text-muted-foreground border-transparent hover:text-foreground hover:border-border"
      }`}
      onClick={() => onSelect(id)}
    >
      {label}
    </button>
  );
}

function JsonPreview({ title, value }: { title: string; value: unknown }) {
  return (
    <details className="bg-muted/20 rounded-lg border border-border/50 overflow-hidden">
      <summary className="px-4 py-3 cursor-pointer text-sm font-medium text-foreground hover:bg-muted/30 transition-colors">{title}</summary>
      <pre className="px-4 pb-4 text-xs text-muted-foreground overflow-x-auto">{JSON.stringify(value, null, 2)}</pre>
    </details>
  );
}

export function RequestDetailPage({ intentId }: { intentId: string }) {
  const searchParams = useSearchParams();

  const [request, setRequest] = useState<RequestStatusResponse | null>(null);
  const [receipt, setReceipt] = useState<ReceiptResponse | null>(null);
  const [history, setHistory] = useState<HistoryResponse | null>(null);
  const [callbacks, setCallbacks] = useState<CallbackHistoryResponse | null>(null);
  const [unified, setUnified] = useState<UnifiedRequestStatusResponse | null>(null);
  const [config, setConfig] = useState<UiConfigResponse | null>(null);

  const [reason, setReason] = useState("");
  const [confirmReplay, setConfirmReplay] = useState(false);
  const [tab, setTab] = useState<DetailTab>("receipt");

  const [loading, setLoading] = useState(false);
  const [replaying, setReplaying] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [role, setRole] = useState<WorkspaceRole>("viewer");
  const [canReplay, setCanReplay] = useState(false);

  const tenantOverride = (searchParams.get("tenant_id") ?? "").trim();
  const tenantQuery = tenantOverride ? `?tenant_id=${encodeURIComponent(tenantOverride)}` : "";

  function withTenant(path: string): string {
    if (!tenantOverride) return path;
    const separator = path.includes("?") ? "&" : "?";
    return `${path}${separator}tenant_id=${encodeURIComponent(tenantOverride)}`;
  }

  useEffect(() => {
    const requestedTab = (searchParams.get("tab") ?? "").trim().toLowerCase();

    if (
      requestedTab === "receipt" ||
      requestedTab === "attempts" ||
      requestedTab === "callbacks" ||
      requestedTab === "payload" ||
      requestedTab === "replay"
    ) {
      setTab(requestedTab);
    }
  }, [searchParams]);

  useEffect(() => {
    void loadAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [intentId, tenantOverride]);

  useEffect(() => {
    apiGet<UiConfigResponse>("config")
      .then((nextConfig) => setConfig(nextConfig))
      .catch(() => setConfig(null));
  }, []);

  useEffect(() => {
    void readSession().then((session) => {
      if (!session) return;
      setRole(session.role);
      setCanReplay(canManageWorkspace(session.role));
    });
  }, []);

  const latestReceiptEntry = useMemo(
    () => (receipt?.entries?.length ? receipt.entries[receipt.entries.length - 1] : null),
    [receipt?.entries]
  );

  const attemptGroups = useMemo(() => {
    const grouped = new Map<number, ReceiptResponse["entries"]>();

    for (const entry of receipt?.entries ?? []) {
      const current = grouped.get(entry.attempt_no) ?? [];
      current.push(entry);
      grouped.set(entry.attempt_no, current);
    }

    return [...grouped.entries()].sort((a, b) => a[0] - b[0]);
  }, [receipt]);

  const timelineStates = useMemo(() => {
    return (history?.transitions ?? []).map((transition) => ({
      id: transition.transition_id,
      state: transition.to_state,
      classification: transition.classification,
      occurred_at_ms: transition.occurred_at_ms,
    }));
  }, [history?.transitions]);

  const solanaEvidence = useMemo(() => {
    const details = latestReceiptEntry?.details ?? {};
    return {
      signature: details.signature ?? details.tx_signature ?? null,
      tx_hash: details.tx_hash ?? null,
      fee_payer: details.fee_payer ?? null,
      provider_reference: details.provider_reference ?? details.provider_tx_id ?? null,
    };
  }, [latestReceiptEntry?.details]);

  const replayEligible = useMemo(() => {
    return Boolean(canReplay && request);
  }, [canReplay, request]);

  async function loadAll() {
    setLoading(true);
    setError(null);
    setMessage(null);

    try {
      const encoded = encodeURIComponent(intentId);

      const unifiedData = await apiGet<UnifiedRequestStatusResponse>(
        withTenant(`status/requests/${encoded}/unified`)
      );

      setUnified(unifiedData);
      setRequest(unifiedData.request);
      setReceipt(unifiedData.receipt);
      setHistory(unifiedData.history);
      setCallbacks(unifiedData.callbacks);

      if ((unifiedData.receipt.entries?.length ?? 0) > 0) {
        void markOnboardingStep("viewed_receipt");
      }
    } catch (loadError: unknown) {
      setUnified(null);
      setError(loadError instanceof Error ? loadError.message : String(loadError));
      setRequest(null);
      setReceipt(null);
      setHistory(null);
      setCallbacks(null);
    } finally {
      setLoading(false);
    }
  }

  async function triggerReplay() {
    if (!replayEligible) {
      setError("Replay requires owner or admin access.");
      return;
    }

    if (!confirmReplay) {
      setError("Please confirm replay before continuing.");
      return;
    }

    if (reason.trim().length < 8) {
      setError("Replay reason must be at least 8 characters.");
      return;
    }

    if (!window.confirm("This will create a new execution path. Continue?")) return;

    setReplaying(true);
    setMessage(null);
    setError(null);

    try {
      const encoded = encodeURIComponent(intentId);

      const out = await apiRequest<ReplayResponse>(withTenant(`status/requests/${encoded}/replay`), {
        method: "POST",
        body: JSON.stringify({ reason: reason.trim() }),
      });

      setMessage(`Replay triggered: source=${out.source_job_id} new=${out.replay_job_id}`);
      setConfirmReplay(false);
      await loadAll();
    } catch (replayError: unknown) {
      setError(replayError instanceof Error ? replayError.message : String(replayError));
    } finally {
      setReplaying(false);
    }
  }

  return (
    <div className="flex flex-col gap-6 p-6 max-w-7xl mx-auto">
      <section className="bg-gradient-to-br from-card to-card/80 rounded-xl border border-border/50 p-6">
        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
          <div>
            <p className="text-sm font-medium text-muted-foreground mb-1">Request detail</p>
            <h2 className="text-2xl font-semibold text-foreground truncate" title={intentId}>{middleEllipsis(intentId)}</h2>
            <p className="text-muted-foreground mt-1">
              Review the request status, attempts, callbacks, payload context, and replay options.
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
              {request?.state ?? "Loading..."}
            </span>
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
              {request?.adapter_id ?? "adapter"}
            </span>
            <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${replayEligible ? "bg-green-500/20 text-green-400" : "bg-yellow-500/20 text-yellow-400"}`}>
              Replay {replayEligible ? "available" : "restricted"}
            </span>
          </div>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-4">
        <div className="flex items-center gap-3">
          <button className="px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors" type="button" onClick={() => void loadAll()}>
            {loading ? "Refreshing..." : "Refresh"}
          </button>

          <Link className="px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors" href={`/app/requests${tenantQuery}`}>
            Back to requests
          </Link>
        </div>

        {error ? <p className="mt-3 text-sm text-red-400 bg-red-500/10 px-3 py-2 rounded-md">{error}</p> : null}
        {message ? <p className="mt-3 text-sm text-green-400 bg-green-500/10 px-3 py-2 rounded-md">{message}</p> : null}
      </section>

      {request ? (
        <section className="bg-card rounded-xl border border-border/50 p-4">
          <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-4">
            <Summary label="State" value={request.state} />
            <Summary label="Classification" value={request.classification} />
            <Summary label="Attempts" value={`${request.attempt}/${request.max_attempts}`} />
            <Summary label="Adapter" value={request.adapter_id ?? "-"} />
            <Summary label="Correlation ID" value={request.correlation_id ?? "-"} />
            <Summary label="Updated" value={formatMs(request.updated_at_ms)} />
          </div>
        </section>
      ) : null}

      <section className="bg-card rounded-xl border border-border/50 overflow-hidden">
        <div className="flex border-b border-border">
          <Tab id="receipt" current={tab} onSelect={setTab} label="Overview" />
          <Tab id="attempts" current={tab} onSelect={setTab} label="Attempts" />
          <Tab id="callbacks" current={tab} onSelect={setTab} label="Callbacks" />
          <Tab id="payload" current={tab} onSelect={setTab} label="Payload" />
          <Tab id="replay" current={tab} onSelect={setTab} label="Replay" />
        </div>

        <div className="p-6">
          {tab === "receipt" ? (
            <div className="flex flex-col gap-6">
              {unified && config?.reconciliation_customer_visible ? (
                <UnifiedRequestInsights unified={unified} tenantQuery={tenantQuery} />
              ) : config ? (
                <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
                  <span className="text-sm text-muted-foreground">Confidence rollout</span>
                  <strong className="text-foreground block mt-1">
                    Reconciliation-backed confidence is still operator-only for this workspace.
                  </strong>
                  <small className="text-xs text-muted-foreground">
                    Execution truth remains available below. Confidence status becomes customer-visible after rollout promotion.
                  </small>
                </article>
              ) : null}

              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
                  <span className="text-sm text-muted-foreground">Latest summary</span>
                  <strong className="text-foreground block mt-1">{latestReceiptEntry?.summary ?? "No receipt summary yet"}</strong>
                  <small className="text-xs text-muted-foreground">Most recent receipt entry</small>
                </article>

                <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
                  <span className="text-sm text-muted-foreground">Last state</span>
                  <strong className="text-foreground block mt-1">{latestReceiptEntry?.state ?? request?.state ?? "-"}</strong>
                  <small className="text-xs text-muted-foreground">Latest known request state</small>
                </article>

                <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
                  <span className="text-sm text-muted-foreground">Classification</span>
                  <strong className="text-foreground block mt-1">{latestReceiptEntry?.classification ?? request?.classification ?? "-"}</strong>
                  <small className="text-xs text-muted-foreground">Latest classification recorded</small>
                </article>

                <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
                  <span className="text-sm text-muted-foreground">Signature</span>
                  <strong className="text-foreground block mt-1 truncate" title={solanaEvidence.signature ?? "-"}>
                    {solanaEvidence.signature ? middleEllipsis(solanaEvidence.signature) : "-"}
                  </strong>
                  <small className="text-xs text-muted-foreground">Network signature if available</small>
                </article>

                <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
                  <span className="text-sm text-muted-foreground">Transaction hash</span>
                  <strong className="text-foreground block mt-1 truncate" title={solanaEvidence.tx_hash ?? "-"}>
                    {solanaEvidence.tx_hash ? middleEllipsis(solanaEvidence.tx_hash) : "-"}
                  </strong>
                  <small className="text-xs text-muted-foreground">Transaction reference</small>
                </article>

                <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
                  <span className="text-sm text-muted-foreground">Fee payer</span>
                  <strong className="text-foreground block mt-1 truncate" title={solanaEvidence.fee_payer ?? "-"}>
                    {solanaEvidence.fee_payer ? middleEllipsis(solanaEvidence.fee_payer) : "-"}
                  </strong>
                  <small className="text-xs text-muted-foreground">Fee payer recorded on the request</small>
                </article>
              </div>

              <section>
                <h3 className="text-lg font-semibold text-foreground mb-4">Timeline</h3>

                {timelineStates.length > 0 ? (
                  <div className="space-y-3">
                    {timelineStates.map((row) => (
                      <article key={row.id} className="bg-muted/20 rounded-lg border border-border/30 p-3">
                        <p className="text-foreground font-medium">{row.state}</p>
                        <div className="flex items-center gap-4 mt-2 text-sm text-muted-foreground">
                          <span className="px-2 py-0.5 bg-muted rounded text-xs">{row.classification}</span>
                          <span className="text-xs">{formatMs(row.occurred_at_ms)}</span>
                        </div>
                      </article>
                    ))}
                  </div>
                ) : (
                  <EmptyState
                    compact
                    title="No timeline yet"
                    description="State transitions will appear here when they are recorded."
                  />
                )}
              </section>
            </div>
          ) : null}

          {tab === "attempts" ? (
            <div className="space-y-4">
              {attemptGroups.length === 0 ? (
                <EmptyState
                  title="No attempt events yet"
                  description="Attempt details appear after receipt entries are emitted."
                />
              ) : (
                attemptGroups.map(([attemptNo, entries]) => (
                  <section key={`attempt-${attemptNo}`} className="border border-border/50 rounded-xl overflow-hidden">
                    <div className="flex items-center justify-between px-4 py-3 bg-muted/20 border-b border-border/50">
                      <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
                        Attempt {attemptNo}
                      </span>
                      <span className="text-sm text-muted-foreground">{entries.length} events</span>
                    </div>

                    <div className="divide-y divide-border/30">
                      {entries.map((entry) => (
                        <article key={entry.receipt_id} className="p-4">
                          <p className="text-foreground font-medium">{entry.summary}</p>
                          <div className="flex items-center gap-3 mt-2 text-sm text-muted-foreground">
                            <span className="px-2 py-0.5 bg-muted rounded text-xs">{entry.state}</span>
                            <span className="px-2 py-0.5 bg-muted rounded text-xs">{entry.classification}</span>
                            <span className="text-xs">{formatMs(entry.occurred_at_ms)}</span>
                          </div>
                        </article>
                      ))}
                    </div>
                  </section>
                ))
              )}
            </div>
          ) : null}

          {tab === "callbacks" ? (
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {(callbacks?.callbacks ?? []).length > 0 ? (
                (callbacks?.callbacks ?? []).map((callback) => (
                  <article key={callback.callback_id} className="bg-muted/20 rounded-xl border border-border/50 p-4">
                    <h3 className="text-lg font-semibold text-foreground mb-3">
                      <Link
                        href={`/app/callbacks/${encodeURIComponent(callback.callback_id)}${tenantQuery}`}
                        className="text-primary hover:underline"
                      >
                        {middleEllipsis(callback.callback_id, 14, 10)}
                      </Link>
                    </h3>

                    <div className="flex flex-wrap gap-2 mb-3">
                      <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
                        {callback.state}
                      </span>
                      <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
                        attempts {callback.attempts}
                      </span>
                      <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
                        http {callback.last_http_status ?? "-"}
                      </span>
                    </div>

                    {(callback.attempt_history ?? []).slice(0, 5).map((attempt) => (
                      <p
                        key={`${callback.callback_id}-${attempt.attempt_no}-${attempt.occurred_at_ms}`}
                        className="text-sm text-muted-foreground mt-2"
                      >
                        #{attempt.attempt_no} {attempt.outcome} {attempt.http_status ?? "-"} at{" "}
                        {formatMs(attempt.occurred_at_ms)}
                      </p>
                    ))}
                  </article>
                ))
              ) : (
                <EmptyState
                  title="No callback records"
                  description="Configure an outbound callback destination to track delivery history."
                  actionHref={`/app/callbacks${tenantQuery}`}
                  actionLabel="Open callbacks"
                />
              )}
            </div>
          ) : null}

          {tab === "payload" ? (
            <div className="flex flex-col gap-4">
              <JsonPreview title="Request payload context" value={request} />
              <JsonPreview title="Latest receipt details" value={latestReceiptEntry?.details ?? {}} />
              <JsonPreview title="History context" value={history ?? {}} />
            </div>
          ) : null}

          {tab === "replay" ? (
            <div className="flex flex-col gap-6">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
                  <span className="text-sm text-muted-foreground">Your role</span>
                  <strong className="text-foreground block mt-1">{role}</strong>
                  <small className="text-xs text-muted-foreground">Current workspace role</small>
                </article>

                <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
                  <span className="text-sm text-muted-foreground">Replay allowed</span>
                  <strong className="text-foreground block mt-1">{replayEligible ? "Yes" : "No"}</strong>
                  <small className="text-xs text-muted-foreground">Replay requires owner or admin access</small>
                </article>
              </div>

              <div className="flex flex-col gap-4">
                <label className="flex flex-col gap-2">
                  <span className="text-sm font-medium text-foreground">Replay reason</span>
                  <input
                    className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
                    value={reason}
                    onChange={(event) => setReason(event.target.value)}
                    placeholder="Explain why replay is required"
                    disabled={!replayEligible}
                  />
                </label>

                <button
                  className="self-start px-4 py-2 bg-red-600 hover:bg-red-700 text-white font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  type="button"
                  disabled={!replayEligible || replaying}
                  onClick={() => void triggerReplay()}
                >
                  {replaying ? "Triggering..." : "Trigger replay"}
                </button>
              </div>

              <label className="flex items-center gap-3 text-sm text-foreground cursor-pointer">
                <input
                  type="checkbox"
                  checked={confirmReplay}
                  onChange={(event) => setConfirmReplay(event.target.checked)}
                  disabled={!replayEligible}
                  className="w-4 h-4 rounded border-border bg-background text-primary focus:ring-primary/50"
                />
                I understand this creates a new execution attempt.
              </label>
            </div>
          ) : null}
        </div>
      </section>
    </div>
  );
}
