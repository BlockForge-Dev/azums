"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { useEffect, useMemo, useState } from "react";
import { canManageWorkspace, readSession } from "@/lib/app-state";
import { Badge, Button, Card, CardHeader, EmptyState, Input } from "@/components/ui";
import { apiGet, apiRequest, formatMs, shortId } from "@/lib/client-api";
import type {
  ExceptionActionResponse,
  ExceptionDetailResponse,
  ExceptionIndexResponse,
  ReconciliationRolloutSummaryResponse,
  ReconActionResponse,
  ReplayReviewResponse,
  ExceptionStateTransitionResponse,
  UiConfigResponse,
  UnifiedRequestStatusResponse,
} from "@/lib/types";
import {
  dashboardBadgeVariant,
  formatDashboardStatus,
  severityBadgeVariant,
} from "@/lib/unified";

type FilterState = {
  state: string;
  severity: string;
  category: string;
  adapter: string;
  search: string;
  includeTerminal: boolean;
  limit: string;
};

const DEFAULT_FILTERS: FilterState = {
  state: "",
  severity: "",
  category: "",
  adapter: "",
  search: "",
  includeTerminal: false,
  limit: "80",
};

const TRANSITION_STATES = [
  "open",
  "acknowledged",
  "investigating",
  "resolved",
  "dismissed",
  "false_positive",
];

function isPaystackAdapter(adapterId: string | null | undefined): boolean {
  return (adapterId ?? "").trim().toLowerCase() === "adapter_paystack";
}

function payloadValue(
  payload: Record<string, unknown> | null | undefined,
  key: string
): string | null {
  const value = payload?.[key];
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed ? trimmed : null;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return null;
}

function nestedPayloadValue(
  payload: Record<string, unknown> | null | undefined,
  path: string[]
): string | null {
  let current: unknown = payload;

  for (const key of path) {
    if (!current || typeof current !== "object" || Array.isArray(current)) {
      return null;
    }
    current = (current as Record<string, unknown>)[key];
  }

  if (typeof current === "string") {
    const trimmed = current.trim();
    return trimmed ? trimmed : null;
  }
  if (typeof current === "number" || typeof current === "boolean") {
    return String(current);
  }
  return null;
}

function payloadPreview(payload: Record<string, unknown> | null | undefined): string {
  if (!payload) return "{}";
  const raw = JSON.stringify(payload, null, 2);
  return raw.length > 1200 ? `${raw.slice(0, 1200)}\n...` : raw;
}

function formatMachineReason(reason: string | null | undefined): string {
  if (!reason) return "pending";
  return reason.replaceAll("_", " ");
}

function withTenant(path: string, tenantId: string): string {
  if (!tenantId) return path;
  const separator = path.includes("?") ? "&" : "?";
  return `${path}${separator}tenant_id=${encodeURIComponent(tenantId)}`;
}

export function OperatorExceptionsPage() {
  const searchParams = useSearchParams();
  const tenantOverride = (searchParams.get("tenant_id") ?? "").trim();
  const requestedCaseId = (searchParams.get("case_id") ?? "").trim();

  const [filters, setFilters] = useState<FilterState>(DEFAULT_FILTERS);
  const [config, setConfig] = useState<UiConfigResponse | null>(null);
  const [rolloutSummary, setRolloutSummary] =
    useState<ReconciliationRolloutSummaryResponse | null>(null);
  const [index, setIndex] = useState<ExceptionIndexResponse | null>(null);
  const [selectedCaseId, setSelectedCaseId] = useState("");
  const [detail, setDetail] = useState<ExceptionDetailResponse | null>(null);
  const [unified, setUnified] = useState<UnifiedRequestStatusResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [summaryLoading, setSummaryLoading] = useState(false);
  const [detailLoading, setDetailLoading] = useState(false);
  const [transitionState, setTransitionState] = useState("investigating");
  const [transitionReason, setTransitionReason] = useState(
    "Operator reviewed exception case."
  );
  const [submitting, setSubmitting] = useState(false);
  const [canTransition, setCanTransition] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([readSession(), apiGet<UiConfigResponse>("config").catch(() => null)]).then(
      ([session, nextConfig]) => {
        setCanTransition(Boolean(session && canManageWorkspace(session.role)));
        setConfig(nextConfig);
      }
    );
  }, []);

  useEffect(() => {
    if (!config?.reconciliation_operator_visible) {
      setIndex(null);
      setDetail(null);
      setUnified(null);
      setRolloutSummary(null);
      return;
    }
    void Promise.all([loadIndex(), loadRolloutSummary()]);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config]);

  useEffect(() => {
    if (!index) return;
    const nextSelected =
      (requestedCaseId &&
      index.cases.some((item) => item.case_id === requestedCaseId)
        ? requestedCaseId
        : index.cases[0]?.case_id) ?? "";
    setSelectedCaseId(nextSelected);
  }, [index, requestedCaseId]);

  useEffect(() => {
    if (!selectedCaseId) {
      setDetail(null);
      setUnified(null);
      return;
    }
    void loadDetail(selectedCaseId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedCaseId]);

  const cases = index?.cases ?? [];

  const stats = useMemo(() => {
    const unresolved = cases.filter(
      (item) =>
        item.state !== "resolved" &&
        item.state !== "dismissed" &&
        item.state !== "false_positive"
    );
    return {
      total: cases.length,
      unresolved: unresolved.length,
      manualReview: unresolved.filter((item) => item.category === "manual_review_required")
        .length,
      highSeverity: unresolved.filter(
        (item) => item.severity === "high" || item.severity === "critical"
      ).length,
      paystack: unresolved.filter((item) => isPaystackAdapter(item.adapter_id)).length,
    };
  }, [cases]);

  const paystackCase = useMemo(() => {
    if (!detail) return null;
    const latestExecutionReceipt = unified?.receipt.entries?.length
      ? unified.receipt.entries[unified.receipt.entries.length - 1]
      : null;
    const paystackEvidence = detail.case.evidence.filter(
      (entry) => entry.source_table?.startsWith("paystack.") ?? false
    );
    const executionRows = paystackEvidence.filter(
      (entry) => entry.source_table === "paystack.executions"
    );
    const webhookRows = paystackEvidence.filter(
      (entry) => entry.source_table === "paystack.webhook_events"
    );
    const reference =
      latestExecutionReceipt?.connector_outcome?.reference ??
      latestExecutionReceipt?.recon_linkage?.connector_reference ??
      latestExecutionReceipt?.adapter_execution_reference ??
      executionRows
        .map((entry) =>
          payloadValue(entry.payload, "reference") ??
          payloadValue(entry.payload, "provider_reference") ??
          payloadValue(entry.payload, "remote_id") ??
          entry.source_id ??
          null
        )
        .find(Boolean) ??
      webhookRows
        .map((entry) =>
          payloadValue(entry.payload, "reference") ??
          payloadValue(entry.payload, "provider_reference") ??
          payloadValue(entry.payload, "remote_id") ??
          entry.source_id ??
          null
        )
        .find(Boolean) ??
      null;
    const providerStatus =
      latestExecutionReceipt?.details?.provider_status ??
      executionRows
        .map((entry) => payloadValue(entry.payload, "status"))
        .find(Boolean) ??
      webhookRows
        .map((entry) => payloadValue(entry.payload, "status"))
        .find(Boolean) ??
      null;
    const destination =
      latestExecutionReceipt?.details?.destination_reference ??
      executionRows
        .map((entry) => payloadValue(entry.payload, "destination_reference"))
        .find(Boolean) ??
      null;
    const amount =
      latestExecutionReceipt?.details?.amount_minor ??
      executionRows
        .map((entry) => payloadValue(entry.payload, "amount"))
        .find(Boolean) ??
      null;
    const currency =
      latestExecutionReceipt?.details?.currency ??
      executionRows
        .map((entry) => payloadValue(entry.payload, "currency"))
        .find(Boolean) ??
      null;

    return {
      enabled:
        isPaystackAdapter(detail.case.adapter_id) || paystackEvidence.length > 0,
      reference,
      providerStatus,
      destination,
      amount,
      currency,
      executionRows,
      webhookRows,
      mismatchFocus: formatMachineReason(detail.case.machine_reason),
    };
  }, [detail, unified]);

  async function loadRolloutSummary() {
    setSummaryLoading(true);
    try {
      const summary = await apiGet<ReconciliationRolloutSummaryResponse>(
        withTenant("status/reconciliation/rollout-summary?lookback_hours=168", tenantOverride)
      );
      setRolloutSummary(summary);
    } catch {
      setRolloutSummary(null);
    } finally {
      setSummaryLoading(false);
    }
  }

  async function loadIndex(nextFilters: FilterState = filters) {
    setLoading(true);
    setError(null);
    setMessage(null);
    try {
      const query = new URLSearchParams();
      if (nextFilters.state.trim()) query.set("state", nextFilters.state.trim());
      if (nextFilters.severity.trim()) query.set("severity", nextFilters.severity.trim());
      if (nextFilters.category.trim()) query.set("category", nextFilters.category.trim());
      if (nextFilters.adapter.trim()) query.set("adapter_id", nextFilters.adapter.trim());
      if (nextFilters.search.trim()) query.set("search", nextFilters.search.trim());
      if (nextFilters.includeTerminal) query.set("include_terminal", "true");
      query.set("limit", String(Math.max(1, Math.min(200, Number(nextFilters.limit || "80")))));
      query.set("offset", "0");

      const nextIndex = await apiGet<ExceptionIndexResponse>(
        withTenant(`status/exceptions?${query.toString()}`, tenantOverride)
      );
      setIndex(nextIndex);
    } catch (loadError: unknown) {
      setIndex(null);
      setDetail(null);
      setUnified(null);
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  async function loadDetail(caseId: string) {
    setDetailLoading(true);
    setError(null);
    try {
      const nextDetail = await apiGet<ExceptionDetailResponse>(
        withTenant(`status/exceptions/${encodeURIComponent(caseId)}`, tenantOverride)
      );
      setDetail(nextDetail);
      setTransitionState(nextDetail.case.state === "open" ? "investigating" : nextDetail.case.state);

      const nextUnified = await apiGet<UnifiedRequestStatusResponse>(
        withTenant(
          `status/requests/${encodeURIComponent(nextDetail.case.intent_id)}/unified`,
          tenantOverride
        )
      ).catch(() => null);
      setUnified(nextUnified);
    } catch (loadError: unknown) {
      setDetail(null);
      setUnified(null);
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setDetailLoading(false);
    }
  }

  async function submitTransition() {
    if (!detail) return;
    if (!transitionReason.trim() || transitionReason.trim().length < 8) {
      setError("Transition reason must be at least 8 characters.");
      return;
    }

    setSubmitting(true);
    setError(null);
    setMessage(null);
    try {
      const response = await apiRequest<ExceptionStateTransitionResponse>(
        withTenant(
          `status/exceptions/${encodeURIComponent(detail.case.case_id)}/state`,
          tenantOverride
        ),
        {
          method: "POST",
          body: JSON.stringify({
            state: transitionState,
            reason: transitionReason.trim(),
          }),
        }
      );
      setMessage(`Exception case moved to ${response.case.state}.`);
      await Promise.all([loadIndex(), loadDetail(detail.case.case_id), loadRolloutSummary()]);
    } catch (submitError: unknown) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    } finally {
      setSubmitting(false);
    }
  }

  async function submitExceptionAction(pathSuffix: string, successLabel: string) {
    if (!detail) return;
    if (!transitionReason.trim() || transitionReason.trim().length < 8) {
      setError("Operator note must be at least 8 characters.");
      return;
    }

    setSubmitting(true);
    setError(null);
    setMessage(null);
    try {
      const response = await apiRequest<ExceptionActionResponse>(
        withTenant(
          `status/exceptions/${encodeURIComponent(detail.case.case_id)}/${pathSuffix}`,
          tenantOverride
        ),
        {
          method: "POST",
          body: JSON.stringify({
            reason: transitionReason.trim(),
            payload: {
              source: "operator_exceptions_page",
              case_id: detail.case.case_id,
            },
          }),
        }
      );
      setMessage(`${successLabel}: ${response.case.state}.`);
      await Promise.all([loadIndex(), loadDetail(detail.case.case_id), loadRolloutSummary()]);
    } catch (submitError: unknown) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    } finally {
      setSubmitting(false);
    }
  }

  async function submitReconAction(pathSuffix: string, successLabel: string) {
    if (!detail) return;
    if (!transitionReason.trim() || transitionReason.trim().length < 8) {
      setError("Operator note must be at least 8 characters.");
      return;
    }

    setSubmitting(true);
    setError(null);
    setMessage(null);
    try {
      const response = await apiRequest<ReconActionResponse>(
        withTenant(
          `status/requests/${encodeURIComponent(detail.case.intent_id)}/reconciliation/${pathSuffix}`,
          tenantOverride
        ),
        {
          method: "POST",
          body: JSON.stringify({
            reason: transitionReason.trim(),
            payload: {
              source: "operator_exceptions_page",
              case_id: detail.case.case_id,
              subject_id: detail.case.subject_id,
            },
          }),
        }
      );
      setMessage(
        `${successLabel}: ${shortId(response.subject.subject_id)} queued for reconciliation.`
      );
      await Promise.all([loadIndex(), loadDetail(detail.case.case_id), loadRolloutSummary()]);
    } catch (submitError: unknown) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    } finally {
      setSubmitting(false);
    }
  }

  async function submitReplayReview() {
    if (!detail) return;
    if (!transitionReason.trim() || transitionReason.trim().length < 8) {
      setError("Operator note must be at least 8 characters.");
      return;
    }

    setSubmitting(true);
    setError(null);
    setMessage(null);
    try {
      const response = await apiRequest<ReplayReviewResponse>(
        withTenant(
          `status/requests/${encodeURIComponent(detail.case.intent_id)}/replay-review`,
          tenantOverride
        ),
        {
          method: "POST",
          body: JSON.stringify({
            reason: transitionReason.trim(),
            payload: {
              source: "operator_exceptions_page",
              case_id: detail.case.case_id,
              subject_id: detail.case.subject_id,
            },
          }),
        }
      );
      setMessage(
        `Replay review handed off to ${response.handoff}: new job ${shortId(
          response.replay.replay_job_id
        )}.`
      );
      await Promise.all([loadIndex(), loadDetail(detail.case.case_id), loadRolloutSummary()]);
    } catch (submitError: unknown) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="flex flex-col gap-6 max-w-7xl mx-auto p-6">
      <section className="bg-gradient-to-br from-card to-card/80 rounded-xl border border-border/50 p-6">
        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
          <div>
            <p className="text-sm font-medium text-muted-foreground mb-1">Exceptions</p>
            <h2 className="text-2xl font-semibold text-foreground">
              Reconciliation-backed exception index.
            </h2>
            <p className="text-muted-foreground mt-1">
              Inspect unresolved divergence, see the evidence trail, and move cases through an
              explicit operator workflow without altering Azums execution truth.
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Badge variant="default">{stats.total} cases</Badge>
            <Badge variant="warn">{stats.unresolved} unresolved</Badge>
            <Badge variant="error">{stats.manualReview} manual review</Badge>
            <Badge variant="default">{stats.paystack} fiat rail</Badge>
          </div>
        </div>
      </section>

      {error ? <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-4 text-red-400">{error}</div> : null}
      {message ? <div className="bg-green-500/10 border border-green-500/30 rounded-xl p-4 text-green-400">{message}</div> : null}

      {config && !config.reconciliation_operator_visible ? (
        <Card>
          <CardHeader
            title="Rollout hidden"
            subtitle="Reconciliation and exception dashboards are still in internal hidden mode for this workspace."
          />
        </Card>
      ) : null}

      {config?.reconciliation_operator_visible ? (
        <Card>
          <CardHeader
            title="Rollout summary"
            subtitle="Seven-day operational snapshot for reconciliation latency, exception rate, false positives, and sampled dashboard query time."
          />
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
            <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
              <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Dirty subjects</span>
              <strong className="text-2xl font-semibold text-foreground">
                {summaryLoading ? "..." : String(rolloutSummary?.intake.dirty_subjects ?? 0)}
              </strong>
              <small className="text-xs text-muted-foreground">
                Pending intake or re-run work in the current window
              </small>
            </div>
            <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
              <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Exception rate</span>
              <strong className="text-2xl font-semibold text-foreground">
                {summaryLoading
                  ? "..."
                  : `${(((rolloutSummary?.exceptions.exception_rate ?? 0) * 100)).toFixed(1)}%`}
              </strong>
              <small className="text-xs text-muted-foreground">
                Exception cases against recent reconciled subjects
              </small>
            </div>
            <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
              <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">False positives</span>
              <strong className="text-2xl font-semibold text-foreground">
                {summaryLoading
                  ? "..."
                  : `${(((rolloutSummary?.exceptions.false_positive_rate ?? 0) * 100)).toFixed(1)}%`}
              </strong>
              <small className="text-xs text-muted-foreground">
                Share of cases operators closed as false positive
              </small>
            </div>
            <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
              <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Unified query sample</span>
              <strong className="text-2xl font-semibold text-foreground">
                {summaryLoading
                  ? "..."
                  : `${rolloutSummary?.queries.unified_request_query_ms ?? 0} ms`}
              </strong>
              <small className="text-xs text-muted-foreground">
                Sampled dashboard read path for one recent request
              </small>
            </div>
          </div>
        </Card>
      ) : null}

      {config?.reconciliation_operator_visible ? (
      <>
      <Card>
        <CardHeader
          title="Filters"
          subtitle="Search by adapter, state, severity, or machine reason."
        />
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-6 gap-4">
          <Input label="State" value={filters.state} onChange={(e) => setFilters((prev) => ({ ...prev, state: e.target.value }))} placeholder="open" />
          <Input label="Severity" value={filters.severity} onChange={(e) => setFilters((prev) => ({ ...prev, severity: e.target.value }))} placeholder="high" />
          <Input label="Category" value={filters.category} onChange={(e) => setFilters((prev) => ({ ...prev, category: e.target.value }))} placeholder="manual_review_required" />
          <Input label="Adapter" value={filters.adapter} onChange={(e) => setFilters((prev) => ({ ...prev, adapter: e.target.value }))} placeholder="solana" />
          <Input label="Search" value={filters.search} onChange={(e) => setFilters((prev) => ({ ...prev, search: e.target.value }))} placeholder="case id, summary, machine reason" className="xl:col-span-2" />
          <Input label="Limit" type="number" min={1} max={200} value={filters.limit} onChange={(e) => setFilters((prev) => ({ ...prev, limit: e.target.value }))} />
          <label className="flex items-end gap-2 text-sm text-muted-foreground">
            <input
              type="checkbox"
              checked={filters.includeTerminal}
              onChange={(e) =>
                setFilters((prev) => ({ ...prev, includeTerminal: e.target.checked }))
              }
            />
            Include terminal cases
          </label>
        </div>
        <div className="mt-4 flex items-center gap-3">
          <Button
            type="button"
            variant="primary"
            onClick={() => void Promise.all([loadIndex(), loadRolloutSummary()])}
            disabled={loading}
          >
            {loading ? "Loading..." : "Load exceptions"}
          </Button>
          <Button
            type="button"
            variant="ghost"
            onClick={() => {
              setFilters(DEFAULT_FILTERS);
              void Promise.all([loadIndex(DEFAULT_FILTERS), loadRolloutSummary()]);
            }}
          >
            Reset
          </Button>
        </div>
      </Card>

      <div className="grid grid-cols-1 xl:grid-cols-[1.15fr_0.85fr] gap-6">
        <Card>
          <CardHeader
            title="Exception index"
            subtitle="Global operator view across unresolved and recently resolved cases."
          />

          {cases.length === 0 ? (
            <EmptyState
              compact
              title="No exception cases"
              description="No cases matched the current filters."
            />
          ) : (
            <div className="space-y-3">
              {cases.map((item) => (
                <button
                  key={item.case_id}
                  type="button"
                  onClick={() => setSelectedCaseId(item.case_id)}
                  className={`w-full text-left rounded-xl border p-4 transition-colors ${
                    selectedCaseId === item.case_id
                      ? "border-primary/40 bg-primary/10"
                      : "border-border/50 bg-muted/20 hover:bg-muted/30"
                  }`}
                >
                  <div className="flex flex-wrap items-center gap-2 mb-2">
                    <Badge variant={severityBadgeVariant(item.severity)}>{item.severity}</Badge>
                    <Badge variant="default">{item.category}</Badge>
                    <Badge variant="default">{item.state}</Badge>
                    {isPaystackAdapter(item.adapter_id) ? (
                      <Badge variant="default">fiat rail</Badge>
                    ) : null}
                    <span className="text-xs text-muted-foreground font-mono">
                      {shortId(item.case_id)}
                    </span>
                  </div>
                  <p className="text-sm text-foreground">{item.summary}</p>
                  <div className="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                    <span>adapter:{item.adapter_id}</span>
                    <span>focus:{formatMachineReason(item.machine_reason)}</span>
                    <span>intent:{shortId(item.intent_id)}</span>
                    <span>updated:{formatMs(item.updated_at_ms)}</span>
                  </div>
                </button>
              ))}
            </div>
          )}
        </Card>

        <Card>
          <CardHeader
            title="Drill-down"
            subtitle="Evidence, lifecycle, and operator actions for the selected case."
          />

          {detailLoading ? <p className="text-sm text-muted-foreground">Loading case detail...</p> : null}

          {!detail && !detailLoading ? (
            <EmptyState
              compact
              title="Select a case"
              description="Choose a case from the exception index to inspect it."
            />
          ) : null}

          {detail ? (
            <div className="space-y-4">
              <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
                <div className="flex flex-wrap items-center gap-2 mb-2">
                  <Badge variant={severityBadgeVariant(detail.case.severity)}>
                    {detail.case.severity}
                  </Badge>
                  <Badge variant="default">{detail.case.category}</Badge>
                  <Badge variant="default">{detail.case.state}</Badge>
                  {isPaystackAdapter(detail.case.adapter_id) ? (
                    <Badge variant="default">fiat rail</Badge>
                  ) : null}
                  {unified ? (
                    <Badge variant={dashboardBadgeVariant(unified.dashboard_status)}>
                      {formatDashboardStatus(unified.dashboard_status)}
                    </Badge>
                  ) : null}
                </div>
                <p className="text-sm text-foreground">{detail.case.summary}</p>
                <div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                  <span>{detail.case.machine_reason}</span>
                  <span>focus:{formatMachineReason(detail.case.machine_reason)}</span>
                  <span>first_seen:{formatMs(detail.case.first_seen_at_ms)}</span>
                  <span>last_seen:{formatMs(detail.case.last_seen_at_ms)}</span>
                </div>
                <div className="mt-3 flex flex-wrap gap-3 text-xs">
                  <Link
                    href={`/ops/requests/${encodeURIComponent(detail.case.intent_id)}${
                      tenantOverride ? `?tenant_id=${encodeURIComponent(tenantOverride)}` : ""
                    }`}
                    className="text-primary hover:underline"
                  >
                    Open request
                  </Link>
                  {detail.case.latest_execution_receipt_id ? (
                    <Link
                      href={`/app/receipts/${encodeURIComponent(detail.case.latest_execution_receipt_id)}${
                        tenantOverride ? `?tenant_id=${encodeURIComponent(tenantOverride)}` : ""
                      }`}
                      className="text-primary hover:underline"
                    >
                      Open receipt
                    </Link>
                  ) : null}
                </div>
              </div>

              {canTransition ? (
                <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
                  <h3 className="text-sm font-semibold text-foreground mb-3">Operator workflow</h3>
                  <p className="text-sm text-muted-foreground mb-4">
                    Every action is auditable. Reconciliation actions requeue downstream work only;
                    replay review hands off to execution core rather than mutating execution truth
                    directly.
                  </p>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                    <Input
                      label="Operator note"
                      value={transitionReason}
                      onChange={(e) => setTransitionReason(e.target.value)}
                      placeholder="Why is this action being taken?"
                      className="md:col-span-2"
                    />
                  </div>
                  <div className="mt-4 grid grid-cols-1 md:grid-cols-2 gap-3">
                    <Button
                      type="button"
                      variant="primary"
                      onClick={() => void submitExceptionAction("acknowledge", "Exception acknowledged")}
                      disabled={submitting}
                    >
                      {submitting ? "Working..." : "Acknowledge exception"}
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => void submitReconAction("rerun", "Reconciliation re-run requested")}
                      disabled={submitting}
                    >
                      Re-run reconciliation
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => void submitReconAction("refresh-observation", "Observation refresh requested")}
                      disabled={submitting}
                    >
                      Refresh observation
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => void submitReplayReview()}
                      disabled={submitting}
                    >
                      Request replay review
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => void submitExceptionAction("resolve", "Exception resolved")}
                      disabled={submitting}
                    >
                      Resolve with note
                    </Button>
                    <Button
                      type="button"
                      variant="ghost"
                      onClick={() => void submitExceptionAction("false-positive", "Exception marked false positive")}
                      disabled={submitting}
                    >
                      Mark false positive
                    </Button>
                  </div>

                  <div className="mt-5 border-t border-border/50 pt-4">
                    <h4 className="text-xs font-semibold uppercase tracking-wide text-muted-foreground mb-3">
                      Manual state transition
                    </h4>
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                      <label className="text-sm text-muted-foreground flex flex-col gap-1">
                        Next state
                        <select
                          className="w-full rounded-lg border border-border bg-input px-3 py-2 text-foreground"
                          value={transitionState}
                          onChange={(e) => setTransitionState(e.target.value)}
                        >
                          {TRANSITION_STATES.map((item) => (
                            <option key={item} value={item}>
                              {item}
                            </option>
                          ))}
                        </select>
                      </label>
                    </div>
                    <div className="mt-3">
                      <Button
                        type="button"
                        variant="ghost"
                        onClick={() => void submitTransition()}
                        disabled={submitting}
                      >
                        {submitting ? "Updating..." : "Update case state"}
                      </Button>
                    </div>
                  </div>
                </div>
              ) : null}

              {paystackCase?.enabled ? (
                <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
                  <h3 className="text-sm font-semibold text-foreground mb-3">
                    Fiat rail investigation
                  </h3>
                  <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-3">
                    <div className="rounded-lg border border-border/50 bg-background/40 p-3">
                      <span className="text-xs text-muted-foreground">Verification reference</span>
                      <p
                        className="mt-1 text-sm text-foreground font-mono break-all"
                        title={paystackCase.reference ?? "-"}
                      >
                        {paystackCase.reference ?? "-"}
                      </p>
                    </div>
                    <div className="rounded-lg border border-border/50 bg-background/40 p-3">
                      <span className="text-xs text-muted-foreground">Provider status</span>
                      <p className="mt-1 text-sm text-foreground font-mono">
                        {paystackCase.providerStatus ?? "pending"}
                      </p>
                    </div>
                    <div className="rounded-lg border border-border/50 bg-background/40 p-3">
                      <span className="text-xs text-muted-foreground">Mismatch focus</span>
                      <p className="mt-1 text-sm text-foreground font-mono">
                        {paystackCase.mismatchFocus}
                      </p>
                    </div>
                    <div className="rounded-lg border border-border/50 bg-background/40 p-3">
                      <span className="text-xs text-muted-foreground">Evidence rows</span>
                      <p className="mt-1 text-sm text-foreground font-mono">
                        {paystackCase.executionRows.length} execution /{" "}
                        {paystackCase.webhookRows.length} webhook
                      </p>
                    </div>
                  </div>
                  <div className="mt-3 flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                    <span>amount:{paystackCase.amount ?? "-"}</span>
                    <span>currency:{paystackCase.currency ?? "-"}</span>
                    <span>destination:{paystackCase.destination ?? "-"}</span>
                    <span>
                      latest_recon_receipt:{detail.case.latest_recon_receipt_id ?? "not_written"}
                    </span>
                  </div>
                  <p className="mt-3 text-xs text-muted-foreground">
                    Paystack webhook evidence is downstream verification evidence. Operator actions
                    here can re-run reconciliation or request replay review, but they do not
                    rewrite Azums execution truth.
                  </p>
                </div>
              ) : null}

              <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
                <h3 className="text-sm font-semibold text-foreground mb-3">Evidence</h3>
                <div className="space-y-3">
                  {detail.case.evidence.length > 0 ? (
                    detail.case.evidence.slice(0, 10).map((entry) => (
                      <div key={entry.evidence_id} className="rounded-lg border border-border/50 bg-background/40 p-3">
                        <div className="flex flex-wrap items-center gap-2 mb-1">
                          <Badge variant="default">{entry.evidence_type}</Badge>
                          {entry.source_table ? (
                            <span className="text-xs text-muted-foreground font-mono">
                              {entry.source_table}
                            </span>
                          ) : null}
                          {entry.source_table?.startsWith("paystack.") ? (
                            <Badge variant="default">fiat evidence</Badge>
                          ) : null}
                        </div>
                        <p className="text-xs text-muted-foreground font-mono break-all">
                          {entry.source_id ?? shortId(entry.evidence_id)}
                        </p>
                        {entry.source_table?.startsWith("paystack.") ? (
                          <div className="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                            {payloadValue(entry.payload, "reference") ? (
                              <span>ref:{payloadValue(entry.payload, "reference")}</span>
                            ) : null}
                            {payloadValue(entry.payload, "provider_reference") ? (
                              <span>provider_ref:{payloadValue(entry.payload, "provider_reference")}</span>
                            ) : null}
                            {payloadValue(entry.payload, "remote_id") ? (
                              <span>remote_id:{payloadValue(entry.payload, "remote_id")}</span>
                            ) : null}
                            {payloadValue(entry.payload, "status") ? (
                              <span>status:{payloadValue(entry.payload, "status")}</span>
                            ) : null}
                          </div>
                        ) : null}
                        {canTransition &&
                        entry.source_table === "paystack.webhook_events" ? (
                          <details className="mt-2 rounded-md border border-border/40 bg-muted/20 overflow-hidden">
                            <summary className="cursor-pointer px-3 py-2 text-xs font-medium text-foreground hover:bg-muted/30 transition-colors">
                              Webhook payload excerpt
                            </summary>
                            <div className="px-3 pb-3 pt-1">
                              <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono mb-2">
                                {payloadValue(entry.payload, "event") ? (
                                  <span>event:{payloadValue(entry.payload, "event")}</span>
                                ) : null}
                                {nestedPayloadValue(entry.payload, ["data", "reference"]) ? (
                                  <span>
                                    ref:
                                    {nestedPayloadValue(entry.payload, ["data", "reference"])}
                                  </span>
                                ) : null}
                                {nestedPayloadValue(entry.payload, ["data", "status"]) ? (
                                  <span>
                                    provider_status:
                                    {nestedPayloadValue(entry.payload, ["data", "status"])}
                                  </span>
                                ) : null}
                                {nestedPayloadValue(entry.payload, ["data", "gateway_response"]) ? (
                                  <span>
                                    gateway:
                                    {nestedPayloadValue(entry.payload, ["data", "gateway_response"])}
                                  </span>
                                ) : null}
                              </div>
                              <pre className="text-xs text-muted-foreground font-mono overflow-x-auto whitespace-pre-wrap break-words">
                                {payloadPreview(entry.payload)}
                              </pre>
                            </div>
                          </details>
                        ) : null}
                        <small className="text-xs text-muted-foreground">
                          {formatMs(entry.observed_at_ms ?? entry.created_at_ms)}
                        </small>
                      </div>
                    ))
                  ) : (
                    <p className="text-sm text-muted-foreground">No evidence attached to this case.</p>
                  )}
                </div>
              </div>

              <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
                <h3 className="text-sm font-semibold text-foreground mb-3">Events</h3>
                <div className="space-y-2">
                  {detail.events.length > 0 ? (
                    detail.events.slice().reverse().map((entry) => (
                      <div key={entry.event_id} className="rounded-lg border border-border/50 bg-background/40 p-3">
                        <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                          <span>{entry.event_type}</span>
                          <span>{entry.from_state ?? "-"}</span>
                          <span>{entry.to_state ?? "-"}</span>
                          <span>{entry.actor}</span>
                          <span>{formatMs(entry.created_at_ms)}</span>
                        </div>
                        <p className="text-sm text-foreground mt-1">{entry.reason}</p>
                      </div>
                    ))
                  ) : (
                    <p className="text-sm text-muted-foreground">No case events recorded yet.</p>
                  )}
                </div>
              </div>
            </div>
          ) : null}
        </Card>
      </div>
      </>
      ) : null}
    </div>
  );
}
