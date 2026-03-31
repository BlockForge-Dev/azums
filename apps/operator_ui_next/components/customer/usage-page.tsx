"use client";

import { useEffect, useMemo, useState } from "react";
import { getUsageSummary, type UsageSummary } from "@/lib/app-state";
import { apiGet } from "@/lib/client-api";
import type {
  IntakeAuditsResponse,
  JobListResponse,
  RequestStatusResponse,
} from "@/lib/types";
import { EmptyState, Card, CardHeader, Button, Table, SummaryCard, StatGrid, Badge } from "@/components/ui";
import { PLAN_SPECS } from "@/lib/plans";

type CountRow = [label: string, count: number];

export function UsagePage() {
  const [summary, setSummary] = useState<UsageSummary | null>(null);
  const [jobsCount, setJobsCount] = useState(0);
  const [adapterCounts, setAdapterCounts] = useState<Record<string, number>>({});
  const [sourceCounts, setSourceCounts] = useState<Record<string, number>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      setLoading(true);
      setError(null);

      try {
        const [jobsData, usage] = await Promise.all([
          apiGet<JobListResponse>("status/jobs?limit=140&offset=0"),
          getUsageSummary(),
        ]);

        if (cancelled) return;

        const jobs = jobsData.jobs ?? [];
        setJobsCount(jobs.length);
        setSummary(usage);

        const requestRows = await Promise.all(
          jobs.slice(0, 80).map(async (job) => {
            try {
              const encoded = encodeURIComponent(job.intent_id);
              return await apiGet<RequestStatusResponse>(`status/requests/${encoded}`);
            } catch {
              return null;
            }
          })
        );

        if (cancelled) return;

        const nextAdapterCounts: Record<string, number> = {};
        for (const row of requestRows) {
          const adapter = row?.adapter_id?.trim() || "Unknown";
          nextAdapterCounts[adapter] = (nextAdapterCounts[adapter] ?? 0) + 1;
        }
        setAdapterCounts(nextAdapterCounts);

        const audits = await apiGet<IntakeAuditsResponse>(
          "status/tenant/intake-audits?validation_result=accepted&limit=400&offset=0"
        ).catch(() => null);

        if (cancelled) return;

        const nextSourceCounts: Record<string, number> = {};
        for (const audit of audits?.audits ?? []) {
          const source = audit.principal_id?.trim() || "Unknown";
          nextSourceCounts[source] = (nextSourceCounts[source] ?? 0) + 1;
        }
        setSourceCounts(nextSourceCounts);
      } catch (loadError: unknown) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : String(loadError));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    void load();

    return () => {
      cancelled = true;
    };
  }, []);

  const plan = summary?.plan ?? "Developer";
  const used = summary?.used_requests ?? 0;
  const quota = summary?.free_play_limit ?? 0;
  const isPaid = summary?.access_mode === "paid";
  const remaining = isPaid ? null : Math.max(0, quota - used);
  const usagePercent =
    !isPaid && quota > 0 ? Math.min(100, Math.round((used / quota) * 100)) : 0;

  const planSpec = PLAN_SPECS[plan as keyof typeof PLAN_SPECS];
  const monthlyPrice = planSpec?.monthly_price_usd ?? 0;

  const adapterRows = useMemo<CountRow[]>(
    () => Object.entries(adapterCounts).sort((a, b) => b[1] - a[1]),
    [adapterCounts]
  );

  const sourceRows = useMemo<CountRow[]>(
    () => Object.entries(sourceCounts).sort((a, b) => b[1] - a[1]),
    [sourceCounts]
  );

  const topAdapter = adapterRows[0]?.[0] ?? "-";
  const topSource = sourceRows[0]?.[0] ?? "-";

  function exportUsageCsv() {
    const rows: string[] = [];
    rows.push("section,key,count");

    for (const [adapter, count] of adapterRows) {
      rows.push(`integration,${adapter},${count}`);
    }

    for (const [source, count] of sourceRows) {
      rows.push(`source,${source},${count}`);
    }

    const csv = rows.join("\n");
    const blob = new Blob([csv], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "usage-breakdown.csv";
    anchor.click();
    URL.revokeObjectURL(url);
  }

  return (
    <div className="flex flex-col gap-6 p-6 max-w-7xl mx-auto">
      <div className="bg-gradient-to-br from-card to-card/80 rounded-xl border border-border/50 p-6">
        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-4 mb-4">
          <div>
            <p className="text-sm font-medium text-muted-foreground mb-1">Usage</p>
            <h2 className="text-2xl font-semibold text-foreground">Usage overview</h2>
            <p className="text-muted-foreground mt-1">
              Review plan usage, request volume, integration mix, and where traffic is coming from.
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
              {plan}
            </span>
            <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${isPaid ? "bg-green-500/20 text-green-400" : "bg-yellow-500/20 text-yellow-400"}`}>
              {isPaid ? "Paid" : "Free play"}
            </span>
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
              {loading ? "Loading..." : `${used.toLocaleString()} used`}
            </span>
          </div>
        </div>
      </div>

      {error ? <div className="bg-red-500/10 border border-red-500/30 text-red-400 rounded-lg p-4">{error}</div> : null}

      {summary?.metering_warning ? (
        <div className="bg-yellow-500/10 border border-yellow-500/30 text-yellow-400 rounded-lg p-4">{summary.metering_warning}</div>
      ) : null}

      <div className="bg-card rounded-xl border border-border/50 p-6">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
          <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Plan</span>
            <strong className="text-foreground block text-lg mt-1">{plan}</strong>
            <small className="text-xs text-muted-foreground">${monthlyPrice}/month</small>
          </article>

          <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Requests used</span>
            <strong className="text-foreground block text-lg mt-1">{loading ? "..." : used.toLocaleString()}</strong>
            <small className="text-xs text-muted-foreground">Current billing window</small>
          </article>

          <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">{isPaid ? "Access" : "Remaining"}</span>
            <strong className="text-foreground block text-lg mt-1">
              {loading ? "..." : isPaid ? "Unlimited" : remaining?.toLocaleString() ?? "0"}
            </strong>
            <small className="text-xs text-muted-foreground">{isPaid ? "No fixed request cap" : "Requests left in this window"}</small>
          </article>

          <article className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Jobs sampled</span>
            <strong className="text-foreground block text-lg mt-1">{loading ? "..." : jobsCount.toLocaleString()}</strong>
            <small className="text-xs text-muted-foreground">Recent jobs used for this view</small>
          </article>
        </div>

        {!isPaid ? (
          <div className="space-y-2">
            <div className="h-2 bg-muted rounded-full overflow-hidden">
              <div className="h-full bg-primary transition-all" style={{ width: `${usagePercent}%` }} />
            </div>
            <p className="text-sm text-muted-foreground">
              {used.toLocaleString()} of {quota.toLocaleString()} requests used ({usagePercent}%).
            </p>
          </div>
        ) : (
          <p className="text-sm text-muted-foreground">
            {used.toLocaleString()} requests recorded in the current billing window.
          </p>
        )}
      </div>

      <div className="bg-card rounded-xl border border-border/50 p-6">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Top integration</span>
            <strong className="text-foreground block text-lg mt-1">{topAdapter}</strong>
          </div>
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Top request source</span>
            <strong className="text-foreground block text-lg mt-1">{topSource}</strong>
          </div>
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Tracked integrations</span>
            <strong className="text-foreground block text-lg mt-1">{adapterRows.length}</strong>
          </div>
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Tracked sources</span>
            <strong className="text-foreground block text-lg mt-1">{sourceRows.length}</strong>
          </div>
        </div>
      </div>

      <div className="bg-card rounded-xl border border-border/50 overflow-hidden">
        <div className="p-6 pb-4">
          <h3 className="text-lg font-semibold text-foreground">By integration</h3>
          <p className="text-sm text-muted-foreground mt-1">Where requests are being executed.</p>
        </div>

        {adapterRows.length === 0 && !loading ? (
          <div className="px-6 pb-6">
            <EmptyState
              compact
              title="No integration usage yet"
              description="Submit requests to start seeing usage by integration."
            />
          </div>
        ) : (
          <Table
            columns={[
              { key: "integration", header: "Integration" },
              { key: "requests", header: "Requests", render: (row: CountRow) => row[1] },
            ]}
            data={adapterRows}
            keyExtractor={(row: CountRow) => row[0]}
            isLoading={loading}
            emptyMessage="No integration usage yet"
          />
        )}
      </div>

      <div className="bg-card rounded-xl border border-border/50 overflow-hidden">
        <div className="p-6 pb-4">
          <h3 className="text-lg font-semibold text-foreground">By request source</h3>
          <p className="text-sm text-muted-foreground mt-1">Accepted traffic grouped by source identity.</p>
        </div>

        {sourceRows.length === 0 && !loading ? (
          <div className="px-6 pb-6">
            <EmptyState
              compact
              title="No request source data yet"
              description="Accepted intake activity will appear here."
            />
          </div>
        ) : (
          <Table
            columns={[
              { key: "source", header: "Source" },
              { key: "requests", header: "Requests", render: (row: CountRow) => row[1] },
            ]}
            data={sourceRows}
            keyExtractor={(row: CountRow) => row[0]}
            isLoading={loading}
            emptyMessage="No request source data yet"
          />
        )}
      </div>

      <div className="bg-card rounded-xl border border-border/50 p-6">
        <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-4">
          <div>
            <h3 className="text-lg font-semibold text-foreground">Export usage</h3>
            <p className="text-sm text-muted-foreground mt-1">
              Download the current integration and request-source breakdown as CSV.
            </p>
          </div>

          <div className="flex flex-col gap-2">
            <Button variant="ghost" onClick={exportUsageCsv}>
              Export CSV
            </Button>
            <span className="text-xs text-muted-foreground">Includes the data shown on this page.</span>
          </div>
        </div>
      </div>
    </div>
  );
}



