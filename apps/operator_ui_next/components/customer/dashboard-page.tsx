"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { onboardingProgress, readSession } from "@/lib/app-state";
import { apiGet, formatMs, shortId } from "@/lib/client-api";
import { EmptyState, Card, CardHeader, SummaryCard, StatGrid, Badge, Button, Table } from "@/components/ui";
import type {
  CallbackHistoryResponse,
  JobListResponse,
  JobRow,
  UiConfigResponse,
  UiHealthResponse,
  UnifiedRequestStatusResponse,
} from "@/lib/types";
import { dashboardBadgeVariant, formatDashboardStatus, summarizeDashboardStates } from "@/lib/unified";

type CallbackSummary = {
  intent_id: string;
  state: string;
};

type ConfidenceSummary = {
  intent_id: string;
  dashboard_status: string;
  recon_status: string | null;
  unresolved_exceptions: number;
};

function middleEllipsis(value: string, start = 16, end = 10) {
  if (!value || value.length <= start + end + 3) return value;
  return `${value.slice(0, start)}...${value.slice(-end)}`;
}

export function DashboardPage() {
  const [config, setConfig] = useState<UiConfigResponse | null>(null);
  const [health, setHealth] = useState<UiHealthResponse | null>(null);
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [callbackSummaries, setCallbackSummaries] = useState<CallbackSummary[]>([]);
  const [confidenceSummaries, setConfidenceSummaries] = useState<ConfidenceSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [session, setSession] = useState<Awaited<ReturnType<typeof readSession>>>(null);
  const customerConfidenceVisible = config?.reconciliation_customer_visible ?? false;

  useEffect(() => {
    let cancelled = false;

    setLoading(true);

    void readSession().then((nextSession) => {
      if (!cancelled) setSession(nextSession);
    });

    void Promise.all([
      apiGet<UiConfigResponse>("config").catch(() => null),
      apiGet<UiHealthResponse>("health").catch(() => null),
      apiGet<JobListResponse>("status/jobs?limit=80&offset=0").catch(() => ({ jobs: [] })),
    ])
      .then(async ([cfg, hlth, jobData]) => {
        if (cancelled) return;

        const rows = jobData.jobs ?? [];
        setConfig(cfg);
        setHealth(hlth);
        setJobs(rows);

        const recent = rows.slice(0, 24);

        const recentInsights = await Promise.all(
          recent.map(async (job) => {
            const encoded = encodeURIComponent(job.intent_id);
            const [callbackResponse, unifiedResponse] = await Promise.all([
              apiGet<CallbackHistoryResponse>(
                `status/requests/${encoded}/callbacks?include_attempts=false&attempt_limit=1`
              ).catch(() => null),
              cfg?.reconciliation_customer_visible
                ? apiGet<UnifiedRequestStatusResponse>(`status/requests/${encoded}/unified`).catch(
                    () => null
                  )
                : Promise.resolve(null),
            ]);

            return {
              callback: callbackResponse?.callbacks?.[0]
                ? {
                    intent_id: job.intent_id,
                    state: callbackResponse.callbacks[0].state,
                  }
                : null,
              confidence: unifiedResponse
                ? {
                    intent_id: job.intent_id,
                    dashboard_status: unifiedResponse.dashboard_status,
                    recon_status: unifiedResponse.recon_status ?? null,
                    unresolved_exceptions:
                      unifiedResponse.exception_summary.unresolved_cases,
                  }
                : null,
            };
          })
        );

        if (cancelled) return;

        setCallbackSummaries(
          recentInsights
            .map((row) => row.callback)
            .filter((row): row is CallbackSummary => row != null)
        );
        setConfidenceSummaries(
          recentInsights
            .map((row) => row.confidence)
            .filter((row): row is ConfidenceSummary => row != null)
        );
        setError(null);
      })
      .catch((loadError: unknown) => {
        if (cancelled) return;
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const progress = useMemo(
    () => (session ? onboardingProgress(session) : { completed: 0, total: 5, percent: 0 }),
    [session]
  );

  const stats = useMemo(() => {
    const now = Date.now();
    const last24h = now - 24 * 60 * 60 * 1000;
    const last7d = now - 7 * 24 * 60 * 60 * 1000;

    const dayRows = jobs.filter((job) => job.updated_at_ms >= last24h);
    const weekRows = jobs.filter((job) => job.updated_at_ms >= last7d);

    const isRetrying = (job: JobRow) =>
      job.state.includes("retry") || job.classification === "RetryableFailure";

    const isFailed = (job: JobRow) =>
      job.state === "failed_terminal" || job.state === "dead_lettered";

    const isSucceeded = (job: JobRow) => job.state === "succeeded";

    return {
      total24h: dayRows.length,
      total7d: weekRows.length,
      success24h: dayRows.filter(isSucceeded).length,
      retry24h: dayRows.filter(isRetrying).length,
      failed24h: dayRows.filter(isFailed).length,
      success7d: weekRows.filter(isSucceeded).length,
    };
  }, [jobs]);

  const callbackStats = useMemo(() => {
    const total = callbackSummaries.length;
    const delivered = callbackSummaries.filter((row) =>
      row.state.toLowerCase().includes("deliver")
    ).length;
    const failed = callbackSummaries.filter((row) =>
      row.state.toLowerCase().includes("fail")
    ).length;
    const retrying = callbackSummaries.filter((row) =>
      row.state.toLowerCase().includes("retry")
    ).length;

    return { total, delivered, failed, retrying };
  }, [callbackSummaries]);

  const confidenceStats = useMemo(
    () =>
      summarizeDashboardStates(
        confidenceSummaries.map((row) => ({ dashboard_status: row.dashboard_status }))
      ),
    [confidenceSummaries]
  );

  const confidenceByIntent = useMemo(
    () => new Map(confidenceSummaries.map((row) => [row.intent_id, row])),
    [confidenceSummaries]
  );

  const latestRequests = useMemo(() => jobs.slice(0, 8), [jobs]);

  const apiStatusLabel = health?.status_api_reachable
    ? `Connected${health.status_api_status_code ? ` (${health.status_api_status_code})` : ""}`
    : "Unavailable";

  return (
    <div className="flex flex-col gap-6 max-w-7xl mx-auto p-6">
      <Card className="bg-gradient-to-br from-card to-card/80 border border-border rounded-2xl p-8 flex flex-col md:flex-row items-start md:items-center justify-between gap-6">
        <div className="flex-1">
          <p className="text-xs font-semibold uppercase tracking-wider text-primary mb-2">Dashboard</p>
          <h2 className="text-2xl font-bold text-foreground mb-2">Your workspace at a glance.</h2>
          <p className="text-sm text-muted-foreground">
            See recent activity, delivery results, setup progress, and the fastest next actions.
          </p>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <Badge variant={loading ? "default" : "default"}>
            {loading ? "Loading..." : `${jobs.length} requests`}
          </Badge>
          <Badge variant="default">
            Setup {progress.completed}/{progress.total}
          </Badge>
          <Badge variant={health?.status_api_reachable ? "success" : "warn"}>
            API {apiStatusLabel}
          </Badge>
        </div>
      </Card>

      {error ? <Card className="bg-destructive/10 border-destructive/30 text-destructive rounded-xl p-4 text-sm">{error}</Card> : null}

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        <div className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5">
          <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Completed today</span>
          <strong className="text-3xl font-bold text-foreground block mb-1">{loading ? "..." : stats.success24h}</strong>
          <small className="text-xs text-muted-foreground">Successful requests in the last 24 hours</small>
        </div>

        <div className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5">
          <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Retrying today</span>
          <strong className="text-3xl font-bold text-foreground block mb-1">{loading ? "..." : stats.retry24h}</strong>
          <small className="text-xs text-muted-foreground">Requests retrying or recently retried</small>
        </div>

        <div className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5">
          <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Failed today</span>
          <strong className="text-3xl font-bold text-foreground block mb-1">{loading ? "..." : stats.failed24h}</strong>
          <small className="text-xs text-muted-foreground">Requests that ended in failure today</small>
        </div>

        <div className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5">
          <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Total this week</span>
          <strong className="text-3xl font-bold text-foreground block mb-1">{loading ? "..." : stats.total7d}</strong>
          <small className="text-xs text-muted-foreground">Requests updated in the last 7 days</small>
        </div>
      </div>

      {config && customerConfidenceVisible ? (
        <Card>
          <CardHeader
            title="Confidence"
            subtitle="Execution and reconciliation remain distinct. These badges show downstream confidence after Azums executed."
          />

          <StatGrid>
            <SummaryCard label="Matched" value={loading ? "..." : String(confidenceStats.matched)} />
            <SummaryCard
              label="Pending verification"
              value={loading ? "..." : String(confidenceStats.pending_verification)}
            />
            <SummaryCard
              label="Mismatch detected"
              value={loading ? "..." : String(confidenceStats.mismatch_detected)}
            />
            <SummaryCard
              label="Manual review"
              value={loading ? "..." : String(confidenceStats.manual_review_required)}
            />
          </StatGrid>
        </Card>
      ) : config ? (
        <Card>
          <CardHeader
            title="Confidence"
            subtitle="Reconciliation-backed confidence is still in operator rollout mode for this workspace."
          />
        </Card>
      ) : null}

      <Card>
        <CardHeader
          title="Delivery"
          subtitle="Recent callback delivery results from your latest requests."
        />

        <StatGrid>
          <SummaryCard label="Tracked" value={loading ? "..." : String(callbackStats.total)} />
          <SummaryCard label="Delivered" value={loading ? "..." : String(callbackStats.delivered)} />
          <SummaryCard label="Failed" value={loading ? "..." : String(callbackStats.failed)} />
          <SummaryCard label="Retrying" value={loading ? "..." : String(callbackStats.retrying)} />
        </StatGrid>
      </Card>

      <Card>
        <CardHeader
          title="Recent requests"
          subtitle="Open any request to see its full result and details."
        />

        {latestRequests.length === 0 && !loading ? (
          <EmptyState
            compact
            title="No requests yet"
            description="Start with a test request in Playground."
            actionHref="/app/playground"
            actionLabel="Open Playground"
          />
        ) : (
          <Table
            columns={[
              { key: "id", header: "ID", render: (job) => <span title={job.intent_id}>{shortId(job.intent_id)}</span> },
              { key: "status", header: "Status", render: (job) => job.state },
              {
                key: "confidence",
                header: "Confidence",
                render: (job) => {
                  const insight = confidenceByIntent.get(job.intent_id);
                  return insight ? (
                    <Badge variant={dashboardBadgeVariant(insight.dashboard_status)}>
                      {formatDashboardStatus(insight.dashboard_status)}
                    </Badge>
                  ) : (
                    config && !customerConfidenceVisible ? "Operator only" : "-"
                  );
                },
              },
              { key: "attempts", header: "Attempts", render: (job) => `${job.attempt}/${job.max_attempts}` },
              { key: "updated", header: "Updated", render: (job) => formatMs(job.updated_at_ms) },
              { 
                key: "actions", 
                header: "", 
                render: (job) => (
                  <Link href={`/app/requests/${encodeURIComponent(job.intent_id)}`} className="text-[var(--accent)]">
                    Open
                  </Link>
                ) 
              },
            ]}
            data={latestRequests}
            keyExtractor={(job) => `${job.intent_id}-${job.updated_at_ms}`}
            isLoading={loading}
            emptyMessage="No requests yet"
          />
        )}
      </Card>

      <Card>
        <CardHeader
          title="Quick actions"
          subtitle="Jump to the pages used most often."
        />

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <Link className="bg-gradient-to-br from-primary/10 to-primary/5 border border-primary/30 rounded-xl p-5 transition-all duration-200 hover:border-primary/50 hover:shadow-lg hover:shadow-primary/5 group" href="/app/playground">
            <h4 className="text-sm font-semibold text-primary mb-1">Run a test request</h4>
            <p className="text-xs text-muted-foreground">Use Playground to submit and inspect a safe request.</p>
          </Link>

          <Link className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5 group" href="/app/requests">
            <h4 className="text-sm font-semibold text-foreground mb-1 group-hover:text-primary transition-colors">View requests</h4>
            <p className="text-xs text-muted-foreground">Browse recent requests and open full details.</p>
          </Link>

          <Link className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5 group" href="/app/callbacks">
            <h4 className="text-sm font-semibold text-foreground mb-1 group-hover:text-primary transition-colors">Manage callbacks</h4>
            <p className="text-xs text-muted-foreground">Choose where delivery updates should be sent.</p>
          </Link>

          <Link className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5 group" href="/app/api-keys">
            <h4 className="text-sm font-semibold text-foreground mb-1 group-hover:text-primary transition-colors">Manage API keys</h4>
            <p className="text-xs text-muted-foreground">Create and review access keys for your apps.</p>
          </Link>
        </div>
      </Card>

      <Card className="bg-card border border-border rounded-xl p-6">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h3 className="text-lg font-semibold text-foreground">Workspace</h3>
            <p className="text-sm text-muted-foreground mt-0.5">Core details for this workspace.</p>
          </div>

          <Link href="/app/workspaces" className="px-3 py-1.5 text-xs bg-transparent border-none text-muted-foreground hover:bg-muted hover:text-foreground rounded-lg transition-colors">
            Open workspace
          </Link>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <div className="bg-muted/30 rounded-lg p-4 border border-border/50">
            <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Tenant</span>
            <strong className="text-sm font-semibold text-foreground block mb-1 truncate" title={config?.tenant_id ?? "-"}>
              {config?.tenant_id ? middleEllipsis(config.tenant_id) : "-"}
            </strong>
            <small className="text-xs text-muted-foreground">Workspace identifier</small>
          </div>

          <div className="bg-muted/30 rounded-lg p-4 border border-border/50">
            <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Your role</span>
            <strong className="text-sm font-semibold text-foreground block mb-1">{session?.role ?? "-"}</strong>
            <small className="text-xs text-muted-foreground">Access level in this workspace</small>
          </div>

          <div className="bg-muted/30 rounded-lg p-4 border border-border/50">
            <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">API</span>
            <strong className="text-sm font-semibold text-foreground block mb-1">{apiStatusLabel}</strong>
            <small className="text-xs text-muted-foreground">
              {health?.status_api_reachable ? "Service is reachable" : "Service needs attention"}
            </small>
          </div>

          <div className="bg-muted/30 rounded-lg p-4 border border-border/50 relative">
            <span className="text-xs font-medium uppercase tracking-wider text-muted-foreground block mb-1">Setup progress</span>
            <strong className="text-sm font-semibold text-foreground block mb-1">
              {progress.completed}/{progress.total}
            </strong>
            <div className="h-1.5 bg-muted rounded-full mt-2 overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-primary to-emerald-400 rounded-full transition-all duration-500"
                style={{ width: `${progress.percent}%` }}
              />
            </div>
            <small className="text-xs text-muted-foreground">{progress.percent}% complete</small>
          </div>
        </div>
      </Card>
    </div>
  );
}
