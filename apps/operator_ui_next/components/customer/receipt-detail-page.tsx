"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useCallback, useEffect, useMemo, useState } from "react";
import { canWriteRequests, readSession } from "@/lib/app-state";
import { EmptyState, Button, Badge, Card, CardHeader } from "@/components/ui";
import { apiGet, formatMs, shortId } from "@/lib/client-api";
import type {
  CallbackHistoryResponse,
  HistoryResponse,
  ReceiptLookupResponse,
  ReceiptResponse,
  RequestStatusResponse,
  UiConfigResponse,
  UnifiedRequestStatusResponse,
} from "@/lib/types";
import { UnifiedRequestInsights } from "@/components/customer/unified-request-insights";

type DetailTab = "summary" | "timeline" | "attempts" | "callbacks" | "raw" | "advanced";

const DETAIL_TABS: Array<{ id: DetailTab; label: string }> = [
  { id: "summary", label: "Summary" },
  { id: "timeline", label: "Timeline" },
  { id: "attempts", label: "Attempts" },
  { id: "callbacks", label: "Callbacks" },
  { id: "raw", label: "Raw" },
  { id: "advanced", label: "Advanced" },
];

function appendTenant(path: string, tenantId: string): string {
  if (!tenantId) return path;
  const separator = path.includes("?") ? "&" : "?";
  return `${path}${separator}tenant_id=${encodeURIComponent(tenantId)}`;
}

export function ReceiptDetailPage({ receiptId }: { receiptId: string }) {
  const searchParams = useSearchParams();
  const tenantOverride = (searchParams.get("tenant_id") ?? "").trim();
  const tenantQuery = tenantOverride ? `?tenant_id=${encodeURIComponent(tenantOverride)}` : "";

  const [lookup, setLookup] = useState<ReceiptLookupResponse | null>(null);
  const [request, setRequest] = useState<RequestStatusResponse | null>(null);
  const [receipt, setReceipt] = useState<ReceiptResponse | null>(null);
  const [history, setHistory] = useState<HistoryResponse | null>(null);
  const [callbacks, setCallbacks] = useState<CallbackHistoryResponse | null>(null);
  const [unified, setUnified] = useState<UnifiedRequestStatusResponse | null>(null);
  const [config, setConfig] = useState<UiConfigResponse | null>(null);

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<DetailTab>("summary");
  const [canSeeAdvanced, setCanSeeAdvanced] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const encodedReceiptId = encodeURIComponent(receiptId);

      const receiptLookup = await apiGet<ReceiptLookupResponse>(
        appendTenant(`status/receipts/${encodedReceiptId}`, tenantOverride)
      );

      const encodedIntentId = encodeURIComponent(receiptLookup.intent_id);

      const unifiedData = await apiGet<UnifiedRequestStatusResponse>(
        appendTenant(`status/requests/${encodedIntentId}/unified`, tenantOverride)
      );

      setLookup(receiptLookup);
      setUnified(unifiedData);
      setRequest(unifiedData.request);
      setReceipt(unifiedData.receipt);
      setHistory(unifiedData.history);
      setCallbacks(unifiedData.callbacks);
    } catch (loadError: unknown) {
      setLookup(null);
      setUnified(null);
      setRequest(null);
      setReceipt(null);
      setHistory(null);
      setCallbacks(null);
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }, [receiptId, tenantOverride]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    apiGet<UiConfigResponse>("config")
      .then((nextConfig) => setConfig(nextConfig))
      .catch(() => setConfig(null));
  }, []);

  useEffect(() => {
    void readSession().then((session) => {
      setCanSeeAdvanced(Boolean(session && canWriteRequests(session.role)));
    });
  }, []);

  const orderedEntries = useMemo(() => {
    return [...(receipt?.entries ?? [])].sort(
      (left, right) => left.occurred_at_ms - right.occurred_at_ms
    );
  }, [receipt?.entries]);

  const selectedEntry = useMemo(() => {
    if (!lookup) return null;

    return (
      orderedEntries.find((entry) => entry.receipt_id === lookup.receipt_id) ??
      lookup.entry ??
      null
    );
  }, [lookup, orderedEntries]);

  const executionFunding = useMemo(() => {
    const details = selectedEntry?.details ?? {};

    return {
      signingMode: details.signing_mode ?? "unknown",
      payerSource: details.payer_source ?? "unknown",
      feePayer: details.fee_payer ?? "unknown",
      txRef: details.tx_hash ?? details.signature ?? "pending",
    };
  }, [selectedEntry?.details]);

  const attemptEvents = useMemo(() => {
    if (!orderedEntries.length) return [];

    const grouped = new Map<number, typeof orderedEntries>();

    for (const entry of orderedEntries) {
      const current = grouped.get(entry.attempt_no) ?? [];
      current.push(entry);
      grouped.set(entry.attempt_no, current);
    }

    return [...grouped.entries()].sort((a, b) => a[0] - b[0]);
  }, [orderedEntries]);

  const lifecycleHistory = useMemo(() => {
    return orderedEntries.map((entry, index) => {
      const next = orderedEntries[index + 1];

      return {
        receipt_id: entry.receipt_id,
        state: entry.state,
        entered_at_ms: entry.occurred_at_ms,
        exited_at_ms: next?.occurred_at_ms ?? null,
        duration_ms: next ? Math.max(0, next.occurred_at_ms - entry.occurred_at_ms) : null,
      };
    });
  }, [orderedEntries]);

  const adapterEvidence = useMemo(() => {
    const evidence = new Map<string, string>();

    for (const entry of orderedEntries) {
      for (const [key, value] of Object.entries(entry.details ?? {})) {
        if (
          key.includes("signature") ||
          key.includes("provider") ||
          key.includes("blockhash") ||
          key.includes("simulation") ||
          key.includes("error")
        ) {
          evidence.set(key, String(value));
        }
      }
    }

    return [...evidence.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [orderedEntries]);

  return (
    <div className="flex flex-col gap-6 max-w-7xl mx-auto p-6">
      {/* Hero Section */}
      <div className="bg-gradient-to-br from-card to-card/80 border border-border rounded-2xl p-8">
        <div className="flex-1">
          <p className="text-xs font-semibold uppercase tracking-wider text-primary mb-2">Receipt</p>
          <h2 className="text-2xl font-bold text-foreground mb-2">{shortId(receiptId)}</h2>
          <p className="text-sm text-muted-foreground">
            Inspect the receipt, request context, timeline, attempts, callbacks, and raw payloads in one place.
          </p>
        </div>
        <div className="mt-4">
          <Button variant="ghost" onClick={() => void load()} disabled={loading}>
            {loading ? "Refreshing..." : "Refresh"}
          </Button>
        </div>
      </div>

      {error ? (
        <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-4 text-red-400">
          {error}
        </div>
      ) : null}

      {lookup ? (
        <>
          {/* Summary Grid */}
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
            <Summary label="Receipt ID" value={shortId(lookup.receipt_id)} />
            <Summary label="Intent ID" value={shortId(lookup.intent_id)} />
            <Summary label="State" value={selectedEntry?.state ?? "-"} />
            <Summary label="Class" value={selectedEntry?.classification ?? "-"} />
            <Summary label="Attempt" value={String(selectedEntry?.attempt_no ?? 0)} />
            <Summary label="Signing mode" value={executionFunding.signingMode} />
            <Summary label="Payer source" value={executionFunding.payerSource} />
            <Summary label="Fee payer" value={executionFunding.feePayer} />
            <Summary label="Tx ref" value={executionFunding.txRef} />
            <Summary label="Occurred" value={formatMs(selectedEntry?.occurred_at_ms)} />
          </div>

          {/* Tabs */}
          <Card>
            <CardHeader
              title="Receipt view"
              subtitle="Move between summary, event flow, attempts, callbacks, and raw data."
              action={
                <Link href={`/app/requests/${encodeURIComponent(lookup.intent_id)}${tenantQuery}`}>
                  <Button variant="ghost" size="small">Open request</Button>
                </Link>
              }
            />
            
            <div className="flex gap-1 border-b border-border mb-4">
              {DETAIL_TABS.map((item) => (
                <Tab
                  key={item.id}
                  id={item.id}
                  label={item.label}
                  current={tab}
                  onSelect={setTab}
                />
              ))}
            </div>

            {/* Tab Content */}
            {tab === "summary" && (
              <div className="space-y-4">
                {unified && config?.reconciliation_customer_visible ? (
                  <UnifiedRequestInsights unified={unified} tenantQuery={tenantQuery} />
                ) : config ? (
                  <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
                    <span className="text-sm text-muted-foreground">Confidence rollout</span>
                    <strong className="block mt-1 text-foreground">
                      Reconciliation-backed confidence is still operator-only for this workspace.
                    </strong>
                    <small className="text-xs text-muted-foreground">
                      Receipt, timeline, attempts, and callback evidence remain available while the confidence layer stays behind operator rollout.
                    </small>
                  </div>
                ) : null}

                <div className="bg-muted/30 rounded-xl p-5 border border-border/50">
                  <p className="text-sm font-medium text-foreground mb-3">
                    {selectedEntry?.summary ?? "No summary available for this receipt."}
                  </p>
                  <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                    <span>state:{selectedEntry?.state ?? "-"}</span>
                    <span>class:{selectedEntry?.classification ?? "-"}</span>
                    <span>attempt:{selectedEntry?.attempt_no ?? 0}</span>
                    <span>time:{formatMs(selectedEntry?.occurred_at_ms)}</span>
                  </div>
                </div>

                <div className="bg-muted/30 rounded-xl p-5 border border-border/50">
                  <p className="text-sm font-medium text-foreground mb-3">Request context</p>
                  <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                    <span>adapter:{request?.adapter_id ?? "-"}</span>
                    <span>attempts:{request ? `${request.attempt}/${request.max_attempts}` : "-"}</span>
                    <span>replay:{request?.replay_count ?? 0}</span>
                    <span>updated:{formatMs(request?.updated_at_ms)}</span>
                  </div>
                </div>

                <div className="bg-muted/30 rounded-xl p-5 border border-border/50">
                  <p className="text-sm font-medium text-foreground mb-3">Execution funding</p>
                  <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                    <span>signing:{executionFunding.signingMode}</span>
                    <span>payer:{executionFunding.payerSource}</span>
                    <span>fee_payer:{executionFunding.feePayer}</span>
                    <span>tx_ref:{executionFunding.txRef}</span>
                  </div>
                </div>
              </div>
            )}

            {tab === "timeline" && (
              <div className="space-y-3">
                {orderedEntries.length > 0 ? (
                  orderedEntries.map((entry) => (
                    <div
                      key={entry.receipt_id}
                      className={`rounded-xl p-4 border ${
                        entry.receipt_id === lookup.receipt_id 
                          ? "bg-primary/10 border-primary/30" 
                          : "bg-muted/30 border-border/50"
                      }`}
                    >
                      <div className="flex items-center gap-2 mb-2 flex-wrap">
                        <Badge variant="default">{entry.state}</Badge>
                        <Badge variant="default">{entry.classification}</Badge>
                        {entry.receipt_id === lookup.receipt_id && (
                          <Badge variant="success">selected</Badge>
                        )}
                      </div>
                      <p className="text-sm text-foreground mb-2">{entry.summary}</p>
                      <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                        <span>receipt:{shortId(entry.receipt_id)}</span>
                        <span>attempt:{entry.attempt_no}</span>
                        <span>time:{formatMs(entry.occurred_at_ms)}</span>
                      </div>
                    </div>
                  ))
                ) : (
                  <EmptyState
                    title="No receipt timeline yet"
                    description="Receipt entries appear after execution events are recorded."
                  />
                )}
              </div>
            )}

            {tab === "attempts" && (
              <div className="space-y-4">
                {attemptEvents.length > 0 ? (
                  attemptEvents.map(([attemptNo, entries]) => (
                    <div key={`attempt-${attemptNo}`} className="space-y-3">
                      <div className="flex items-center gap-3">
                        <Badge variant="default">Attempt {attemptNo}</Badge>
                        <span className="text-xs text-muted-foreground">{entries.length} events</span>
                      </div>
                      {entries.map((entry) => (
                        <div key={entry.receipt_id} className="bg-muted/30 rounded-lg p-4 border border-border/50 ml-4">
                          <p className="text-sm text-foreground mb-2">{entry.summary}</p>
                          <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                            <span>{entry.state}</span>
                            <span>{entry.classification}</span>
                            <span>{formatMs(entry.occurred_at_ms)}</span>
                          </div>
                        </div>
                      ))}
                    </div>
                  ))
                ) : (
                  <EmptyState
                    title="No attempts yet"
                    description="Attempt groups appear when receipt events are available."
                  />
                )}
              </div>
            )}

            {tab === "callbacks" && (
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {(callbacks?.callbacks ?? []).length > 0 ? (
                  (callbacks?.callbacks ?? []).map((callback) => (
                    <div key={callback.callback_id} className="bg-muted/30 rounded-xl p-4 border border-border/50">
                      <Link
                        href={`/app/callbacks/${encodeURIComponent(callback.callback_id)}${tenantQuery}`}
                        className="text-sm font-medium text-primary hover:underline"
                      >
                        {callback.callback_id}
                      </Link>
                      <div className="flex items-center gap-2 mt-2 flex-wrap">
                        <Badge variant="default">{callback.state}</Badge>
                        <Badge variant="default">attempts {callback.attempts}</Badge>
                        <Badge variant="default">http {callback.last_http_status ?? "-"}</Badge>
                      </div>
                    </div>
                  ))
                ) : (
                  <EmptyState
                    title="No callback deliveries"
                    description="Configure a callback destination to track delivery attempts."
                    actionHref={`/app/callbacks${tenantQuery}`}
                    actionLabel="Open callbacks"
                  />
                )}
              </div>
            )}

            {tab === "raw" && (
              <div className="space-y-4">
                <div className="bg-muted/30 rounded-xl p-4 border border-border/50">
                  <h3 className="text-sm font-semibold text-foreground mb-2">Lookup</h3>
                  <pre className="text-xs font-mono text-foreground overflow-auto">
                    {JSON.stringify(lookup, null, 2)}
                  </pre>
                </div>
                <div className="bg-muted/30 rounded-xl p-4 border border-border/50">
                  <h3 className="text-sm font-semibold text-foreground mb-2">Request + history</h3>
                  <pre className="text-xs font-mono text-foreground overflow-auto">
                    {JSON.stringify({ request, history }, null, 2)}
                  </pre>
                </div>
                <div className="bg-muted/30 rounded-xl p-4 border border-border/50">
                  <h3 className="text-sm font-semibold text-foreground mb-2">Receipt + callbacks</h3>
                  <pre className="text-xs font-mono text-foreground overflow-auto">
                    {JSON.stringify({ receipt, callbacks }, null, 2)}
                  </pre>
                </div>
              </div>
            )}

            {tab === "advanced" && (
              canSeeAdvanced ? (
                <div className="space-y-4">
                  <div className="bg-muted/30 rounded-xl p-4 border border-border/50">
                    <p className="text-sm font-medium text-foreground mb-2">State durations</p>
                    <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                      {lifecycleHistory.map((row) => (
                        <span key={row.receipt_id}>
                          {row.state}:{row.duration_ms == null ? "running" : `${row.duration_ms}ms`}
                        </span>
                      ))}
                    </div>
                  </div>

                  <div className="bg-muted/30 rounded-xl p-4 border border-border/50">
                    <p className="text-sm font-medium text-foreground mb-2">Adapter evidence</p>
                    <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                      {adapterEvidence.length > 0 ? (
                        adapterEvidence.map(([key, value]) => (
                          <span key={`${key}-${value}`}>
                            {key}:{value}
                          </span>
                        ))
                      ) : (
                        <span>No evidence keys found</span>
                      )}
                    </div>
                  </div>

                  <div className="bg-muted/30 rounded-xl p-4 border border-border/50">
                    <p className="text-sm font-medium text-foreground mb-2">Lineage</p>
                    <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                      <span>intent:{lookup.intent_id}</span>
                      <span>receipt:{lookup.receipt_id}</span>
                      <span>transitions:{history?.transitions?.length ?? 0}</span>
                    </div>
                  </div>
                </div>
              ) : (
                <EmptyState
                  title="Advanced view is restricted"
                  description="Developer, admin, and owner roles can inspect deeper diagnostics."
                />
              )
            )}
          </Card>
        </>
      ) : (
        <EmptyState
          title="Receipt not loaded"
          description="Use a valid receipt id from the receipts index."
          actionHref="/app/receipts"
          actionLabel="Back to receipts"
        />
      )}
    </div>
  );
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted/30 rounded-xl p-4 border border-border/50">
      <p className="text-xs uppercase tracking-wider text-muted-foreground mb-1">{label}</p>
      <p className="text-sm font-semibold text-foreground font-mono truncate" title={value}>{value}</p>
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
      className={`px-4 py-2 text-sm font-medium capitalize transition-colors relative ${
        current === id 
          ? "text-primary" 
          : "text-muted-foreground hover:text-foreground"
      }`}
      onClick={() => onSelect(id)}
    >
      {label}
      {current === id && (
        <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-primary" />
      )}
    </button>
  );
}
