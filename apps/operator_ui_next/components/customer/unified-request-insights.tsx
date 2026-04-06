"use client";

import Link from "next/link";
import { Badge } from "@/components/ui";
import { formatMs, shortId } from "@/lib/client-api";
import type { UnifiedRequestStatusResponse } from "@/lib/types";
import {
  dashboardBadgeVariant,
  formatDashboardStatus,
  latestReconRun,
  latestReceiptEntry,
  severityBadgeVariant,
  unresolvedExceptions,
} from "@/lib/unified";

function InsightCard({
  label,
  value,
  detail,
}: {
  label: string;
  value: string;
  detail: string;
}) {
  return (
    <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
      <span className="text-sm text-muted-foreground">{label}</span>
      <strong className="text-foreground block mt-1">{value}</strong>
      <small className="text-xs text-muted-foreground">{detail}</small>
    </article>
  );
}

export function UnifiedRequestInsights({
  unified,
  tenantQuery = "",
  operatorCaseHrefPrefix,
}: {
  unified: UnifiedRequestStatusResponse;
  tenantQuery?: string;
  operatorCaseHrefPrefix?: string;
}) {
  const latestRun = latestReconRun(unified);
  const latestReceipt = latestReceiptEntry(unified);
  const openCases = unresolvedExceptions(unified);
  const paystackEvidenceReferences = unified.evidence_references.filter(
    (reference) => reference.source_table?.startsWith("paystack.") ?? false
  );
  const paystackExecutionEvidence = paystackEvidenceReferences.filter(
    (reference) => reference.source_table === "paystack.executions"
  );
  const paystackWebhookEvidence = paystackEvidenceReferences.filter(
    (reference) => reference.source_table === "paystack.webhook_events"
  );
  const paystackReference =
    latestReceipt?.connector_outcome?.reference ??
    latestReceipt?.recon_linkage?.connector_reference ??
    latestReceipt?.adapter_execution_reference ??
    latestReceipt?.details?.reference ??
    latestReceipt?.details?.provider_reference ??
    latestReceipt?.details?.remote_id ??
    null;
  const showPaystackEvidence =
    unified.request.adapter_id === "adapter_paystack" || paystackEvidenceReferences.length > 0;

  return (
    <div className="flex flex-col gap-6">
      <section className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4">
        <InsightCard
          label="Confidence"
          value={formatDashboardStatus(unified.dashboard_status)}
          detail={
            unified.reconciliation_eligible
              ? "Unified Azums confidence from execution, recon, and exceptions."
              : "Execution path is not marked as reconciliation-eligible."
          }
        />
        <InsightCard
          label="Execution"
          value={latestReceipt?.state ?? unified.request.state}
          detail={latestReceipt?.summary ?? "Latest execution receipt summary."}
        />
        <InsightCard
          label="Reconciliation"
          value={
            unified.recon_status
              ? formatDashboardStatus(unified.recon_status)
              : "Not started"
          }
          detail={
            unified.reconciliation.latest_receipt?.summary ??
            "No reconciliation receipt has been written yet."
          }
        />
        <InsightCard
          label="Exceptions"
          value={
            openCases.length > 0
              ? `${openCases.length} open`
              : `${unified.exception_summary.total_cases} total`
          }
          detail={
            unified.exception_summary.highest_severity
              ? `Highest severity: ${unified.exception_summary.highest_severity}`
              : "No unresolved exception cases."
          }
        />
      </section>

      <section className="bg-muted/20 rounded-xl border border-border/50 p-5">
        <div className="flex flex-wrap items-center gap-2 mb-3">
          <Badge variant={dashboardBadgeVariant(unified.dashboard_status)}>
            {formatDashboardStatus(unified.dashboard_status)}
          </Badge>
          <Badge variant="default">
            execution {latestReceipt?.state ?? unified.request.state}
          </Badge>
          <Badge
            variant={
              unified.recon_status === "matched"
                ? "success"
                : unified.recon_status
                    ? dashboardBadgeVariant(unified.dashboard_status)
                    : "default"
            }
          >
            recon {formatDashboardStatus(unified.recon_status ?? "pending_verification")}
          </Badge>
          {unified.exception_summary.unresolved_cases > 0 ? (
            <Badge variant={severityBadgeVariant(unified.exception_summary.highest_severity)}>
              {unified.exception_summary.unresolved_cases} unresolved exception
              {unified.exception_summary.unresolved_cases === 1 ? "" : "s"}
            </Badge>
          ) : null}
        </div>

        <p className="text-sm text-muted-foreground">
          Azums execution status is shown separately from downstream reconciliation status.
          A request can execute successfully and still require later verification or operator review.
        </p>
      </section>

      <section className="grid grid-cols-1 xl:grid-cols-2 gap-4">
        <article className="bg-muted/30 rounded-xl border border-border/50 p-5">
          <h3 className="text-sm font-semibold text-foreground mb-3">
            Latest reconciliation receipt
          </h3>
          {unified.reconciliation.latest_receipt ? (
            <div className="space-y-3">
              <div className="flex flex-wrap items-center gap-2">
                <Badge
                  variant={
                    unified.reconciliation.latest_receipt.normalized_result === "matched"
                      ? "success"
                      : dashboardBadgeVariant(unified.dashboard_status)
                  }
                >
                  {formatDashboardStatus(
                    unified.reconciliation.latest_receipt.normalized_result ??
                      unified.reconciliation.latest_receipt.outcome
                  )}
                </Badge>
                <span className="text-xs text-muted-foreground font-mono">
                  {shortId(unified.reconciliation.latest_receipt.recon_receipt_id)}
                </span>
                <span className="text-xs text-muted-foreground">
                  {formatMs(unified.reconciliation.latest_receipt.created_at_ms)}
                </span>
              </div>
              <p className="text-sm text-foreground">
                {unified.reconciliation.latest_receipt.summary}
              </p>
              {latestRun ? (
                <div className="flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                  <span>run:{shortId(latestRun.run_id)}</span>
                  <span>state:{latestRun.lifecycle_state}</span>
                  <span>facts:{latestRun.matched_fact_count}/{latestRun.expected_fact_count}</span>
                </div>
              ) : null}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              No reconciliation receipt exists yet. This request is still waiting for downstream
              verification or was not eligible.
            </p>
          )}
        </article>

        <article className="bg-muted/30 rounded-xl border border-border/50 p-5">
          <h3 className="text-sm font-semibold text-foreground mb-3">
            Exception summary
          </h3>
          {openCases.length > 0 ? (
            <div className="space-y-3">
              {openCases.slice(0, 3).map((item) => (
                <div
                  key={item.case_id}
                  className="rounded-lg border border-border/50 bg-background/40 p-3"
                >
                  <div className="flex flex-wrap items-center gap-2 mb-2">
                    <Badge variant={severityBadgeVariant(item.severity)}>{item.severity}</Badge>
                    <Badge variant="default">{item.category}</Badge>
                    <Badge variant="default">{item.state}</Badge>
                    {operatorCaseHrefPrefix ? (
                      <Link
                        href={`${operatorCaseHrefPrefix}${encodeURIComponent(item.case_id)}${tenantQuery}`}
                        className="text-xs text-primary hover:underline"
                      >
                        Open case
                      </Link>
                    ) : null}
                  </div>
                  <p className="text-sm text-foreground">{item.summary}</p>
                  <div className="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground font-mono">
                    <span>{shortId(item.case_id)}</span>
                    <span>updated:{formatMs(item.updated_at_ms)}</span>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              No unresolved exceptions. If reconciliation later detects divergence, cases will
              appear here without changing the original execution history.
            </p>
          )}
        </article>
      </section>

      {showPaystackEvidence ? (
        <section className="bg-muted/30 rounded-xl border border-border/50 p-5">
          <div className="flex flex-wrap items-center gap-2 mb-3">
            <Badge variant="default">Fiat rail evidence</Badge>
            <Badge variant="default">
              execution rows {paystackExecutionEvidence.length}
            </Badge>
            <Badge variant="default">
              webhook rows {paystackWebhookEvidence.length}
            </Badge>
            {latestRun?.machine_reason ? (
              <Badge variant={dashboardBadgeVariant(unified.dashboard_status)}>
                {latestRun.machine_reason}
              </Badge>
            ) : null}
          </div>
          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-3">
            <div className="rounded-lg border border-border/50 bg-background/40 p-3">
              <span className="text-xs text-muted-foreground">Verification reference</span>
              <p
                className="mt-1 text-sm text-foreground font-mono break-all"
                title={paystackReference ?? "-"}
              >
                {paystackReference ?? "-"}
              </p>
            </div>
            <div className="rounded-lg border border-border/50 bg-background/40 p-3">
              <span className="text-xs text-muted-foreground">Provider status</span>
              <p className="mt-1 text-sm text-foreground font-mono">
                {latestReceipt?.details?.provider_status ??
                  latestReceipt?.details?.status ??
                  "pending"}
              </p>
            </div>
            <div className="rounded-lg border border-border/50 bg-background/40 p-3">
              <span className="text-xs text-muted-foreground">Amount / currency</span>
              <p className="mt-1 text-sm text-foreground font-mono">
                {latestReceipt?.details?.amount_minor ?? "-"}{" "}
                {latestReceipt?.details?.currency ?? ""}
              </p>
            </div>
            <div className="rounded-lg border border-border/50 bg-background/40 p-3">
              <span className="text-xs text-muted-foreground">Webhook lineage</span>
              <p className="mt-1 text-sm text-foreground font-mono">
                {paystackWebhookEvidence.length > 0
                  ? paystackWebhookEvidence
                      .slice(0, 2)
                      .map((reference) => reference.source_id ?? reference.label)
                      .join(", ")
                  : "No correlated webhook evidence yet"}
              </p>
            </div>
          </div>
          <p className="mt-3 text-xs text-muted-foreground">
            Paystack webhook evidence is downstream verification evidence. It does not rewrite the
            original Azums execution receipt.
          </p>
        </section>
      ) : null}

      <section className="bg-muted/30 rounded-xl border border-border/50 p-5">
        <h3 className="text-sm font-semibold text-foreground mb-3">Evidence references</h3>
        {unified.evidence_references.length > 0 ? (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {unified.evidence_references.slice(0, 8).map((reference, index) => (
              <div
                key={`${reference.kind}-${reference.source_id ?? index}`}
                className="rounded-lg border border-border/50 bg-background/40 p-3"
              >
                <div className="flex flex-wrap items-center gap-2 mb-2">
                  <Badge variant="default">{reference.kind}</Badge>
                  {reference.source_table ? (
                    <span className="text-xs text-muted-foreground font-mono">
                      {reference.source_table}
                    </span>
                  ) : null}
                </div>
                <p
                  className="text-sm text-foreground font-mono break-all"
                  title={reference.source_id ?? reference.label}
                >
                  {reference.source_id ?? reference.label}
                </p>
                <small className="text-xs text-muted-foreground">
                  {reference.observed_at_ms
                    ? formatMs(reference.observed_at_ms)
                    : "durable reference"}
                </small>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            No downstream evidence references have been recorded for this request yet.
          </p>
        )}
      </section>
    </div>
  );
}
