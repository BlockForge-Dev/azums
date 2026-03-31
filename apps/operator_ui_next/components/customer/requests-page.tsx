"use client";

import Link from "next/link";
import { FormEvent, useEffect, useMemo, useState } from "react";
import { apiGet, formatMs, shortId } from "@/lib/client-api";
import type {
  CallbackHistoryResponse,
  JobListResponse,
  JobRow,
  RequestStatusResponse,
  UiConfigResponse,
  UnifiedRequestStatusResponse,
} from "@/lib/types";
import { EmptyState, Card, CardHeader, Button, Input, Table, SummaryCard, StatGrid, Badge } from "@/components/ui";
import { dashboardBadgeVariant, formatDashboardStatus } from "@/lib/unified";

type RequestIndexRow = {
  job: JobRow;
  request: RequestStatusResponse | null;
  callback_state: string | null;
  confidence_status: string | null;
  unresolved_exceptions: number;
};

export function RequestsPage() {
  const [config, setConfig] = useState<UiConfigResponse | null>(null);
  const [rows, setRows] = useState<RequestIndexRow[]>([]);
  const [stateFilter, setStateFilter] = useState("");
  const [adapterFilter, setAdapterFilter] = useState("");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [search, setSearch] = useState("");
  const [limit, setLimit] = useState("50");
  const [offset, setOffset] = useState("0");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const customerConfidenceVisible = config?.reconciliation_customer_visible ?? false;

  useEffect(() => {
    void loadRows();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filteredRows = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const fromMs = dateFrom ? new Date(`${dateFrom}T00:00:00`).getTime() : null;
    const toMs = dateTo ? new Date(`${dateTo}T23:59:59`).getTime() : null;

    return rows.filter((row) => {
      const request = row.request;
      const adapter = request?.adapter_id ?? "";
      const updatedAt = request?.updated_at_ms ?? row.job.updated_at_ms;

      if (stateFilter.trim() && row.job.state !== stateFilter.trim()) return false;
      if (adapterFilter.trim() && adapter !== adapterFilter.trim()) return false;
      if (fromMs != null && updatedAt < fromMs) return false;
      if (toMs != null && updatedAt > toMs) return false;

      if (!needle) return true;

      return (
        row.job.intent_id.toLowerCase().includes(needle) ||
        (request?.request_id ?? "").toLowerCase().includes(needle) ||
        (request?.correlation_id ?? "").toLowerCase().includes(needle) ||
        (row.callback_state ?? "").toLowerCase().includes(needle) ||
        adapter.toLowerCase().includes(needle)
      );
    });
  }, [adapterFilter, dateFrom, dateTo, rows, search, stateFilter]);

  async function loadRows(event?: FormEvent) {
    event?.preventDefault();

    setLoading(true);
    setError(null);

    try {
      const params = new URLSearchParams();

      if (stateFilter.trim()) {
        params.set("state", stateFilter.trim());
      }

      params.set("limit", String(Math.max(1, Math.min(200, Number(limit || "50")))));
      params.set("offset", String(Math.max(0, Number(offset || "0"))));

      const [cfg, jobs] = await Promise.all([
        apiGet<UiConfigResponse>("config").catch(() => null),
        apiGet<JobListResponse>(`status/jobs?${params.toString()}`),
      ]);
      setConfig(cfg);

      const nextRows = await Promise.all(
        (jobs.jobs ?? []).map(async (job) => {
          const encoded = encodeURIComponent(job.intent_id);

          const [request, callbackHistory, unified] = await Promise.all([
            apiGet<RequestStatusResponse>(`status/requests/${encoded}`).catch(() => null),
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
            job,
            request,
            callback_state: callbackHistory?.callbacks?.[0]?.state ?? null,
            confidence_status: unified?.dashboard_status ?? null,
            unresolved_exceptions: unified?.exception_summary.unresolved_cases ?? 0,
          } satisfies RequestIndexRow;
        })
      );

      setRows(nextRows);
    } catch (loadError: unknown) {
      setRows([]);
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  function resetFilters() {
    setStateFilter("");
    setAdapterFilter("");
    setDateFrom("");
    setDateTo("");
    setSearch("");
    setLimit("50");
    setOffset("0");
  }

  const withCallbacks = filteredRows.filter((row) => row.callback_state != null).length;
  const terminalCount = filteredRows.filter((row) =>
    ["failed_terminal", "dead_lettered"].includes(row.job.state)
  ).length;
  const attentionCount = filteredRows.filter(
    (row) =>
      customerConfidenceVisible &&
      (row.confidence_status === "mismatch_detected" ||
        row.confidence_status === "manual_review_required")
  ).length;

  return (
    <div className="flex flex-col gap-6 p-6 max-w-7xl mx-auto">
      <div className="bg-gradient-to-br from-card to-card/80 rounded-xl border border-border/50 p-6">
        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-4 mb-4">
          <div>
            <p className="text-sm font-medium text-muted-foreground mb-1">Requests</p>
            <h2 className="text-2xl font-semibold text-foreground">Browse and inspect recent requests.</h2>
            <p className="text-muted-foreground mt-1">
              Filter by state, adapter, date, or ID, then open any request for more detail.
            </p>
          </div>

          <div className="flex items-center gap-2">
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
              {loading ? "Loading..." : `${rows.length} loaded`}
            </span>
            <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-muted text-muted-foreground">
              {filteredRows.length} visible
            </span>
          </div>
        </div>
      </div>

      <Card>
        <CardHeader
          title="Filters"
          subtitle="Narrow the list and reload from the server when needed."
        />

        <form className="flex flex-col gap-4" onSubmit={(event) => void loadRows(event)}>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
            <Input
              label="State"
              value={stateFilter}
              onChange={(e) => setStateFilter(e.target.value)}
              placeholder="succeeded"
            />

            <Input
              label="Adapter"
              value={adapterFilter}
              onChange={(e) => setAdapterFilter(e.target.value)}
              placeholder="solana"
            />

            <Input
              label="Date from"
              type="date"
              value={dateFrom}
              onChange={(e) => setDateFrom(e.target.value)}
            />

            <Input
              label="Date to"
              type="date"
              value={dateTo}
              onChange={(e) => setDateTo(e.target.value)}
            />

            <Input
              label="Search"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="intent id, request id, correlation id"
              className="md:col-span-2"
            />

            <Input
              label="Limit"
              type="number"
              min={1}
              max={200}
              value={limit}
              onChange={(e) => setLimit(e.target.value)}
            />

            <Input
              label="Offset"
              type="number"
              min={0}
              value={offset}
              onChange={(e) => setOffset(e.target.value)}
            />
          </div>

          <div className="flex items-center gap-3 mt-4">
            <Button type="submit" variant="primary" disabled={loading}>
              {loading ? "Loading..." : "Load requests"}
            </Button>

            <Button type="button" variant="ghost" onClick={resetFilters}>
              Clear filters
            </Button>
          </div>
        </form>
      </Card>

      {error ? <div className="bg-red-500/10 border border-red-500/30 text-red-400 rounded-lg p-4">{error}</div> : null}

      <Card>
        <StatGrid>
          <SummaryCard label="Loaded" value={String(rows.length)} />
          <SummaryCard label="Visible" value={String(filteredRows.length)} />
          <SummaryCard label="With callbacks" value={String(withCallbacks)} />
          <SummaryCard label="Terminal" value={String(terminalCount)} />
          <SummaryCard label="Needs review" value={String(attentionCount)} />
        </StatGrid>
      </Card>

      {config && !customerConfidenceVisible ? (
        <Card>
          <CardHeader
            title="Confidence rollout"
            subtitle="Reconciliation-backed confidence is still operator-only for this workspace."
          />
        </Card>
      ) : null}

      <Card>
        <CardHeader
          title="Request list"
          subtitle="Open any request to inspect its full details."
        />

        {filteredRows.length === 0 ? (
          <EmptyState
            compact
            title="No requests found"
            description="Adjust the filters or submit a request from Playground."
            actionHref="/app/playground"
            actionLabel="Open Playground"
          />
        ) : (
          <Table
            columns={[
              { key: "id", header: "ID", render: (row) => <span title={row.job.intent_id}>{shortId(row.job.intent_id)}</span> },
              { key: "status", header: "Status", render: (row) => row.job.state },
              {
                key: "confidence",
                header: "Confidence",
                render: (row) =>
                  row.confidence_status ? (
                    <Badge variant={dashboardBadgeVariant(row.confidence_status)}>
                      {formatDashboardStatus(row.confidence_status)}
                    </Badge>
                  ) : (
                    config && !customerConfidenceVisible ? "Operator only" : "-"
                  ),
              },
              { key: "adapter", header: "Adapter", render: (row) => row.request?.adapter_id ?? "-" },
              { key: "attempts", header: "Attempts", render: (row) => `${row.request?.attempt ?? row.job.attempt}/${row.request?.max_attempts ?? row.job.max_attempts}` },
              { key: "updated", header: "Updated", render: (row) => formatMs(row.request?.updated_at_ms ?? row.job.updated_at_ms) },
              { key: "callback", header: "Callback", render: (row) => row.callback_state ?? "-" },
              { 
                key: "actions", 
                header: "", 
                render: (row) => (
                  <Link href={`/app/requests/${encodeURIComponent(row.job.intent_id)}`} className="text-primary hover:underline font-medium">
                    Open
                  </Link>
                ) 
              },
            ]}
            data={filteredRows}
            keyExtractor={(row) => `${row.job.intent_id}-${row.request?.updated_at_ms ?? row.job.updated_at_ms}`}
            isLoading={loading}
            emptyMessage="No requests found"
          />
        )}
      </Card>
    </div>
  );
}
