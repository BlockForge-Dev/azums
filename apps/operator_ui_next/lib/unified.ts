import type {
  ExceptionCaseRecord,
  UnifiedDashboardStatus,
  UnifiedExceptionSummary,
  UnifiedRequestStatusResponse,
} from "@/lib/types";

export function formatDashboardStatus(value: string | null | undefined): string {
  if (!value) return "Pending verification";
  return value
    .split("_")
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
}

export function dashboardBadgeVariant(
  value: string | null | undefined
): "default" | "success" | "warn" | "error" {
  switch (value) {
    case "matched":
      return "success";
    case "manual_review_required":
      return "error";
    case "mismatch_detected":
      return "warn";
    default:
      return "default";
  }
}

export function severityBadgeVariant(
  value: string | null | undefined
): "default" | "success" | "warn" | "error" {
  switch ((value ?? "").toLowerCase()) {
    case "critical":
    case "high":
      return "error";
    case "warning":
      return "warn";
    case "info":
      return "success";
    default:
      return "default";
  }
}

export function latestReconRun(
  unified: UnifiedRequestStatusResponse | null | undefined
) {
  return unified?.reconciliation.runs?.length
    ? unified.reconciliation.runs[unified.reconciliation.runs.length - 1]
    : null;
}

export function latestReceiptEntry(
  unified: UnifiedRequestStatusResponse | null | undefined
) {
  return unified?.receipt.entries?.length
    ? unified.receipt.entries[unified.receipt.entries.length - 1]
    : null;
}

export function unresolvedExceptions(
  unified: UnifiedRequestStatusResponse | null | undefined
): ExceptionCaseRecord[] {
  return (unified?.exceptions.cases ?? []).filter(
    (item) =>
      item.state !== "resolved" &&
      item.state !== "dismissed" &&
      item.state !== "false_positive"
  );
}

export function summarizeDashboardStates(
  rows: Array<Pick<UnifiedRequestStatusResponse, "dashboard_status"> | null>
): Record<UnifiedDashboardStatus, number> {
  const out: Record<UnifiedDashboardStatus, number> = {
    matched: 0,
    pending_verification: 0,
    mismatch_detected: 0,
    manual_review_required: 0,
  };

  for (const row of rows) {
    const key = (row?.dashboard_status ?? "pending_verification") as UnifiedDashboardStatus;
    if (key in out) {
      out[key] += 1;
    }
  }

  return out;
}

export function topExceptionSummary(
  summary: UnifiedExceptionSummary | null | undefined
): string {
  if (!summary || summary.unresolved_cases === 0) return "No open exceptions";
  if (summary.highest_severity) {
    return `${summary.unresolved_cases} open · ${formatDashboardStatus(
      summary.highest_severity
    )} severity`;
  }
  return `${summary.unresolved_cases} open`;
}
