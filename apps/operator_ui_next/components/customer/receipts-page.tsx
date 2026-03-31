"use client";

import Link from "next/link";
import { FormEvent, useEffect, useMemo, useState } from "react";
import { apiGet, formatMs, shortId } from "@/lib/client-api";
import type {
  CallbackHistoryResponse,
  JobListResponse,
  ReceiptResponse,
  RequestStatusResponse,
} from "@/lib/types";
import { EmptyState, Card, CardHeader, Button, Input, Badge } from "@/components/ui";

type ReceiptRow = {
  intent_id: string;
  receipt_id: string;
  adapter_id: string;
  state: string;
  classification: string;
  attempt: number;
  max_attempts: number;
  replay_count: number;
  callback_state: string | null;
  receipt_events: number;
  updated_at_ms: number;
};

type ReplayFilter = "all" | "with_replay" | "without_replay";

export function ReceiptsPage() {
  const [rows, setRows] = useState<ReceiptRow[]>([]);
  const [stateFilter, setStateFilter] = useState("");
  const [classificationFilter, setClassificationFilter] = useState("");
  const [adapterFilter, setAdapterFilter] = useState("");
  const [callbackFilter, setCallbackFilter] = useState("");
  const [replayFilter, setReplayFilter] = useState<ReplayFilter>("all");
  const [search, setSearch] = useState("");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [limit, setLimit] = useState("30");
  const [offset, setOffset] = useState("0");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void loadRows();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const filteredRows = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const fromMs = dateFrom ? new Date(`${dateFrom}T00:00:00`).getTime() : null;
    const toMs = dateTo ? new Date(`${dateTo}T23:59:59`).getTime() : null;
    return rows.filter((row) => {
      if (stateFilter.trim() && row.state !== stateFilter.trim()) return false;
      if (classificationFilter.trim() && row.classification !== classificationFilter.trim()) return false;
      if (adapterFilter.trim() && row.adapter_id !== adapterFilter.trim()) return false;
      if (callbackFilter.trim() && (row.callback_state ?? "") !== callbackFilter.trim()) return false;
      if (replayFilter === "with_replay" && row.replay_count <= 0) return false;
      if (replayFilter === "without_replay" && row.replay_count > 0) return false;
      if (fromMs != null && row.updated_at_ms < fromMs) return false;
      if (toMs != null && row.updated_at_ms > toMs) return false;
      if (!needle) return true;
      return (
        row.intent_id.toLowerCase().includes(needle) ||
        row.receipt_id.toLowerCase().includes(needle) ||
        row.classification.toLowerCase().includes(needle) ||
        (row.callback_state ?? "").toLowerCase().includes(needle)
      );
    });
  }, [
    adapterFilter,
    callbackFilter,
    classificationFilter,
    dateFrom,
    dateTo,
    replayFilter,
    rows,
    search,
    stateFilter,
  ]);

  async function loadRows(event?: FormEvent) {
    event?.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const params = new URLSearchParams();
      params.set("limit", String(Number(limit || "30")));
      params.set("offset", String(Number(offset || "0")));
      if (stateFilter.trim()) params.set("state", stateFilter.trim());

      const jobs = await apiGet<JobListResponse>(`status/jobs?${params.toString()}`);
      const items = await Promise.all(
        (jobs.jobs ?? []).map(async (job): Promise<ReceiptRow> => {
          const encoded = encodeURIComponent(job.intent_id);
          const [request, receipt, callbacks] = await Promise.all([
            apiGet<RequestStatusResponse>(`status/requests/${encoded}`).catch(() => null),
            apiGet<ReceiptResponse>(`status/requests/${encoded}/receipt`).catch(() => null),
            apiGet<CallbackHistoryResponse>(
              `status/requests/${encoded}/callbacks?include_attempts=false&attempt_limit=1`
            ).catch(() => null),
          ]);

          const entries = receipt?.entries ?? [];
          const latest = entries[entries.length - 1];
          return {
            intent_id: job.intent_id,
            receipt_id: latest?.receipt_id ?? "pending",
            adapter_id: request?.adapter_id ?? "-",
            state: latest?.state ?? request?.state ?? job.state,
            classification: latest?.classification ?? request?.classification ?? job.classification,
            attempt: request?.attempt ?? job.attempt,
            max_attempts: request?.max_attempts ?? job.max_attempts,
            replay_count: request?.replay_count ?? 0,
            callback_state: callbacks?.callbacks?.[0]?.state ?? null,
            receipt_events: entries.length,
            updated_at_ms: latest?.occurred_at_ms ?? request?.updated_at_ms ?? job.updated_at_ms,
          };
        })
      );
      setRows(items);
    } catch (loadError: unknown) {
      setRows([]);
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex flex-col gap-6 max-w-7xl mx-auto p-6">
      {/* Hero Section */}
      <div className="bg-gradient-to-br from-card to-card/80 border border-border rounded-2xl p-8">
        <p className="text-xs font-semibold uppercase tracking-wider text-primary mb-2">Receipts</p>
        <h2 className="text-2xl font-bold text-foreground mb-2">Durable Receipt Index</h2>
        <p className="text-sm text-muted-foreground">
          Filter by state/class/adapter/replay/callback/date and jump directly into receipt detail.
        </p>
      </div>

      {/* Filters */}
      <Card>
        <form onSubmit={(event) => void loadRows(event)} className="space-y-4">
          <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-5 gap-4">
            <div>
              <label className="text-xs text-muted-foreground block mb-1">State</label>
              <Input
                value={stateFilter}
                onChange={(e) => setStateFilter(e.target.value)}
                placeholder="State"
              />
            </div>
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Classification</label>
              <Input
                value={classificationFilter}
                onChange={(e) => setClassificationFilter(e.target.value)}
                placeholder="Classification"
              />
            </div>
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Adapter</label>
              <Input
                value={adapterFilter}
                onChange={(e) => setAdapterFilter(e.target.value)}
                placeholder="Adapter"
              />
            </div>
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Callback</label>
              <Input
                value={callbackFilter}
                onChange={(e) => setCallbackFilter(e.target.value)}
                placeholder="Callback"
              />
            </div>
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Replay</label>
              <select
                value={replayFilter}
                onChange={(e) => setReplayFilter(e.target.value as ReplayFilter)}
                className="w-full px-3 py-2 bg-input border border-border rounded-lg text-foreground"
              >
                <option value="all">all</option>
                <option value="with_replay">with replay</option>
                <option value="without_replay">without replay</option>
              </select>
            </div>
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Date from</label>
              <Input
                type="date"
                value={dateFrom}
                onChange={(e) => setDateFrom(e.target.value)}
              />
            </div>
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Date to</label>
              <Input
                type="date"
                value={dateTo}
                onChange={(e) => setDateTo(e.target.value)}
              />
            </div>
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Search</label>
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="intent_xxx / receipt_xxx"
              />
            </div>
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Limit</label>
              <Input
                type="number"
                min={1}
                max={100}
                value={limit}
                onChange={(e) => setLimit(e.target.value)}
              />
            </div>
            <div>
              <label className="text-xs text-muted-foreground block mb-1">Offset</label>
              <Input
                type="number"
                min={0}
                value={offset}
                onChange={(e) => setOffset(e.target.value)}
              />
            </div>
          </div>
          <div className="flex justify-end">
            <Button type="submit" disabled={loading}>
              {loading ? "Loading..." : "Load Receipts"}
            </Button>
          </div>
        </form>
      </Card>

      {error && (
        <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-4 text-red-400">
          {error}
        </div>
      )}

      {/* Summary Stats */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <Summary label="Loaded" value={String(rows.length)} />
        <Summary label="Filtered" value={String(filteredRows.length)} />
        <Summary label="With callback" value={String(filteredRows.filter((row) => row.callback_state != null).length)} />
        <Summary label="With replay" value={String(filteredRows.filter((row) => row.replay_count > 0).length)} />
      </div>

      {/* Table */}
      <Card>
        <div className="overflow-x-auto">
          <table className="w-full">
            <thead>
              <tr className="border-b border-border">
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3">Receipt</th>
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3">Request</th>
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3">Adapter</th>
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3">State</th>
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3">Class</th>
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3">Attempts</th>
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3">Replay</th>
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3">Callback</th>
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3">Updated</th>
                <th className="text-left text-xs font-medium text-muted-foreground uppercase tracking-wider px-4 py-3"></th>
              </tr>
            </thead>
            <tbody>
              {filteredRows.length === 0 ? (
                <tr>
                  <td colSpan={10} className="px-4 py-8">
                    <EmptyState
                      compact
                      title="No receipts for current filters"
                      description="Run an intent in Playground, then reload this page."
                      actionHref="/app/playground"
                      actionLabel="Open Playground"
                    />
                  </td>
                </tr>
              ) : (
                filteredRows.map((row) => (
                  <tr key={`${row.intent_id}-${row.receipt_id}`} className="border-b border-border/50 hover:bg-muted/30 transition-colors">
                    <td className="px-4 py-3 text-sm font-mono text-foreground" title={row.receipt_id}>
                      {row.receipt_id === "pending" ? "pending" : shortId(row.receipt_id)}
                    </td>
                    <td className="px-4 py-3 text-sm font-mono text-foreground" title={row.intent_id}>
                      {shortId(row.intent_id)}
                    </td>
                    <td className="px-4 py-3 text-sm font-mono text-foreground" title={row.adapter_id}>
                      {shortId(row.adapter_id, 10, 6)}
                    </td>
                    <td className="px-4 py-3 text-sm text-foreground">
                      <Badge variant="default">{row.state}</Badge>
                    </td>
                    <td className="px-4 py-3 text-sm text-foreground">
                      <Badge variant={row.classification === "error" ? "error" : "default"}>
                        {row.classification}
                      </Badge>
                    </td>
                    <td className="px-4 py-3 text-sm text-foreground">
                      {row.attempt}/{row.max_attempts}
                    </td>
                    <td className="px-4 py-3 text-sm text-foreground">
                      {row.replay_count}
                    </td>
                    <td className="px-4 py-3 text-sm text-foreground">
                      {row.callback_state ?? "-"}
                    </td>
                    <td className="px-4 py-3 text-sm text-muted-foreground">
                      {formatMs(row.updated_at_ms)}
                    </td>
                    <td className="px-4 py-3 text-sm">
                      {row.receipt_id === "pending" ? (
                        <Link href={`/app/requests/${encodeURIComponent(row.intent_id)}`} className="text-primary hover:underline">
                          Open request
                        </Link>
                      ) : (
                        <Link href={`/app/receipts/${encodeURIComponent(row.receipt_id)}`} className="text-primary hover:underline">
                          Open receipt
                        </Link>
                      )}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </Card>
    </div>
  );
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted/30 rounded-xl p-4 border border-border/50">
      <p className="text-xs uppercase tracking-wider text-muted-foreground mb-1">{label}</p>
      <p className="text-2xl font-bold text-foreground">{value}</p>
    </div>
  );
}
