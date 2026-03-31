"use client";

import Link from "next/link";
import { useRouter } from "next/navigation";
import { FormEvent, useEffect, useMemo, useState } from "react";
import { canManageWorkspace, readSession } from "@/lib/app-state";
import { apiGet, apiRequest, formatMs } from "@/lib/client-api";
import type {
  CallbackDestinationResponse,
  CallbackDestinationUpsertRequest,
  CallbackHistoryResponse,
  JobListResponse,
  RequestStatusResponse,
} from "@/lib/types";
import { EmptyState } from "@/components/ui/empty-state";

type CallbackDeliveryRow = {
  callback_id: string;
  intent_id: string;
  adapter_id: string | null;
  destination: string | null;
  state: string;
  attempts: number;
  last_http_status: number | null;
  updated_at_ms: number;
  next_attempt_at_ms: number | null;
};

export function CallbacksPage() {
  const router = useRouter();
  const [destinationData, setDestinationData] = useState<CallbackDestinationResponse | null>(null);
  const [deliveryRows, setDeliveryRows] = useState<CallbackDeliveryRow[]>([]);
  const [deliveryStateFilter, setDeliveryStateFilter] = useState("");
  const [httpStatusMin, setHttpStatusMin] = useState("");
  const [httpStatusMax, setHttpStatusMax] = useState("");
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [destinationFilter, setDestinationFilter] = useState("");
  const [requestFilter, setRequestFilter] = useState("");
  const [adapterFilter, setAdapterFilter] = useState("");
  const [callbackLookupId, setCallbackLookupId] = useState("");

  const [deliveryUrl, setDeliveryUrl] = useState("");
  const [timeoutMs, setTimeoutMs] = useState("3000");
  const [allowedHosts, setAllowedHosts] = useState("");
  const [enabled, setEnabled] = useState(true);
  const [allowPrivate, setAllowPrivate] = useState(false);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [canManage, setCanManage] = useState(false);

  useEffect(() => {
    void Promise.all([readSession(), loadDestination(), loadDeliveries()]).then(([session]) => {
      setCanManage(Boolean(session && canManageWorkspace(session.role)));
    });
  }, []);

  const filteredRows = useMemo(() => {
    const minStatus = httpStatusMin.trim() ? Number(httpStatusMin) : null;
    const maxStatus = httpStatusMax.trim() ? Number(httpStatusMax) : null;
    const fromMs = dateFrom ? new Date(`${dateFrom}T00:00:00`).getTime() : null;
    const toMs = dateTo ? new Date(`${dateTo}T23:59:59`).getTime() : null;
    return deliveryRows.filter((row) => {
      if (deliveryStateFilter.trim() && row.state !== deliveryStateFilter.trim()) return false;
      if (destinationFilter.trim() && !(row.destination ?? "").includes(destinationFilter.trim())) return false;
      if (requestFilter.trim() && !row.intent_id.includes(requestFilter.trim())) return false;
      if (adapterFilter.trim() && (row.adapter_id ?? "") !== adapterFilter.trim()) return false;
      if (minStatus != null && (row.last_http_status ?? 0) < minStatus) return false;
      if (maxStatus != null && (row.last_http_status ?? 0) > maxStatus) return false;
      if (fromMs != null && row.updated_at_ms < fromMs) return false;
      if (toMs != null && row.updated_at_ms > toMs) return false;
      return true;
    });
  }, [
    adapterFilter,
    dateFrom,
    dateTo,
    deliveryRows,
    deliveryStateFilter,
    destinationFilter,
    httpStatusMax,
    httpStatusMin,
    requestFilter,
  ]);

  async function loadDestination() {
    setLoading(true);
    setError(null);
    try {
      const response = await apiGet<CallbackDestinationResponse>("status/tenant/callback-destination");
      setDestinationData(response);
      if (response.destination) {
        setDeliveryUrl(response.destination.delivery_url ?? "");
        setTimeoutMs(String(response.destination.timeout_ms ?? 3000));
        setAllowedHosts((response.destination.allowed_hosts ?? []).join(","));
        setEnabled(Boolean(response.destination.enabled));
        setAllowPrivate(Boolean(response.destination.allow_private_destinations));
      }
      return response;
    } catch (destinationError: unknown) {
      setError(destinationError instanceof Error ? destinationError.message : String(destinationError));
      return null;
    } finally {
      setLoading(false);
    }
  }

  async function loadDeliveries() {
    setLoading(true);
    setError(null);
    try {
      const destination = await apiGet<CallbackDestinationResponse>(
        "status/tenant/callback-destination"
      ).catch(() => null);
      const jobs = await apiGet<JobListResponse>("status/jobs?limit=60&offset=0");
      const rows = await Promise.all(
        (jobs.jobs ?? []).map(async (job) => {
          const encoded = encodeURIComponent(job.intent_id);
          const [request, callbackHistory] = await Promise.all([
            apiGet<RequestStatusResponse>(`status/requests/${encoded}`).catch(() => null),
            apiGet<CallbackHistoryResponse>(
              `status/requests/${encoded}/callbacks?include_attempts=false&attempt_limit=10`
            ).catch(() => null),
          ]);
          return (callbackHistory?.callbacks ?? []).map((callback) => ({
            callback_id: callback.callback_id,
            intent_id: job.intent_id,
            adapter_id: request?.adapter_id ?? null,
            destination: destination?.destination?.delivery_url ?? null,
            state: callback.state,
            attempts: callback.attempts,
            last_http_status: callback.last_http_status ?? null,
            updated_at_ms: callback.updated_at_ms,
            next_attempt_at_ms: callback.next_attempt_at_ms ?? null,
          }));
        })
      );
      setDeliveryRows(rows.flat());
    } catch (deliveriesError: unknown) {
      setDeliveryRows([]);
      setError(deliveriesError instanceof Error ? deliveriesError.message : String(deliveriesError));
    } finally {
      setLoading(false);
    }
  }

  async function saveDestination(event: FormEvent) {
    event.preventDefault();
    if (!canManage) {
      setError("Only workspace owner/admin can update callback destination.");
      return;
    }
    setSaving(true);
    setError(null);
    setMessage(null);
    try {
      const payload: CallbackDestinationUpsertRequest = {
        delivery_url: deliveryUrl.trim(),
        timeout_ms: Number(timeoutMs || "3000"),
        allow_private_destinations: allowPrivate,
        allowed_hosts: allowedHosts.split(",").map((host) => host.trim()).filter(Boolean),
        enabled,
      };
      await apiRequest("status/tenant/callback-destination", {
        method: "POST",
        body: JSON.stringify(payload),
      });
      setMessage("Callback destination saved.");
      await loadDestination();
      await loadDeliveries();
    } catch (saveError: unknown) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    } finally {
      setSaving(false);
    }
  }

  async function removeDestination() {
    if (!canManage) {
      setError("Only workspace owner/admin can delete callback destination.");
      return;
    }
    if (!window.confirm("Delete callback destination?")) return;
    setError(null);
    setMessage(null);
    try {
      await apiRequest("status/tenant/callback-destination", { method: "DELETE" });
      setMessage("Callback destination deleted.");
      await loadDestination();
      await loadDeliveries();
    } catch (removeError: unknown) {
      setError(removeError instanceof Error ? removeError.message : String(removeError));
    }
  }

  return (
    <div className="flex flex-col gap-6 p-6 max-w-7xl mx-auto">
      <section className="bg-gradient-to-br from-card to-card/80 rounded-xl border border-border/50 p-6">
        <p className="text-sm font-medium text-muted-foreground mb-1">Callbacks</p>
        <h2 className="text-2xl font-semibold text-foreground">Outbound Callback Deliveries</h2>
        <p className="text-muted-foreground mt-1">Manage outbound destinations and inspect delivery outcomes separately from execution truth.</p>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-4">
        <div className="flex items-center gap-3">
          <button className="px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors" type="button" onClick={() => void Promise.all([loadDestination(), loadDeliveries()])}>
            {loading ? "Loading..." : "Refresh"}
          </button>
        </div>
      </section>

      {error ? <section className="bg-red-500/10 border border-red-500/30 text-red-400 rounded-lg p-4">{error}</section> : null}
      {message ? <section className="bg-green-500/10 border border-green-500/30 text-green-400 rounded-lg p-4">{message}</section> : null}
      {!canManage ? (
        <section className="bg-yellow-500/10 border border-yellow-500/30 text-yellow-400 rounded-lg p-4">
          Read-only destination view for your role. Owner/admin can modify callback destination.
        </section>
      ) : null}

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">Destinations</h3>
        <div className="overflow-x-auto rounded-lg border border-border/50">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Destination</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Enabled</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Timeout</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Signing</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Bearer</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Allow private</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {destinationData?.configured && destinationData.destination ? (
                <tr className="hover:bg-muted/30 transition-colors">
                  <td className="px-4 py-3 text-foreground">{destinationData.destination.delivery_url}</td>
                  <td className="px-4 py-3 text-foreground">{destinationData.destination.enabled ? "yes" : "no"}</td>
                  <td className="px-4 py-3 text-foreground">{destinationData.destination.timeout_ms}ms</td>
                  <td className="px-4 py-3 text-foreground">{destinationData.destination.has_signature_secret ? "configured (masked)" : "not set"}</td>
                  <td className="px-4 py-3 text-foreground">{destinationData.destination.has_bearer_token ? "configured (masked)" : "not set"}</td>
                  <td className="px-4 py-3 text-foreground">{destinationData.destination.allow_private_destinations ? "yes" : "no"}</td>
                </tr>
              ) : (
                <tr>
                  <td colSpan={6} className="px-4 py-6 text-center text-muted-foreground">No outbound destination configured.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-4">
        <form
          className="flex flex-wrap items-end gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            const trimmed = callbackLookupId.trim();
            if (!trimmed) return;
            router.push(`/app/callbacks/${encodeURIComponent(trimmed)}`);
          }}
        >
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Callback ID</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 w-64"
              value={callbackLookupId}
              onChange={(event) => setCallbackLookupId(event.target.value)}
              placeholder="callback_xxx"
            />
          </label>
          <button className="px-3 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded-lg transition-colors" type="submit">
            Open callback detail
          </button>
        </form>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">Delivery filters</h3>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">State</span>
            <input className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50" value={deliveryStateFilter} onChange={(event) => setDeliveryStateFilter(event.target.value)} />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">HTTP min</span>
            <input className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50" type="number" value={httpStatusMin} onChange={(event) => setHttpStatusMin(event.target.value)} />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">HTTP max</span>
            <input className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50" type="number" value={httpStatusMax} onChange={(event) => setHttpStatusMax(event.target.value)} />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Date from</span>
            <input className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50" type="date" value={dateFrom} onChange={(event) => setDateFrom(event.target.value)} />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Date to</span>
            <input className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50" type="date" value={dateTo} onChange={(event) => setDateTo(event.target.value)} />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Destination</span>
            <input className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50" value={destinationFilter} onChange={(event) => setDestinationFilter(event.target.value)} />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Request</span>
            <input className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50" value={requestFilter} onChange={(event) => setRequestFilter(event.target.value)} />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Adapter</span>
            <input className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50" value={adapterFilter} onChange={(event) => setAdapterFilter(event.target.value)} />
          </label>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Loaded deliveries</span>
            <strong className="text-foreground block text-lg mt-1">{deliveryRows.length}</strong>
          </div>
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Filtered deliveries</span>
            <strong className="text-foreground block text-lg mt-1">{filteredRows.length}</strong>
          </div>
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Delivered</span>
            <strong className="text-foreground block text-lg mt-1">{filteredRows.filter((row) => row.state.toLowerCase().includes("deliver")).length}</strong>
          </div>
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Retrying</span>
            <strong className="text-foreground block text-lg mt-1">{filteredRows.filter((row) => row.state.toLowerCase().includes("retry")).length}</strong>
          </div>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Callback ID</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Request</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Adapter</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">State</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">HTTP</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Attempts</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Next retry</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Updated</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {filteredRows.length === 0 ? (
                <tr>
                  <td colSpan={9} className="px-4 py-6">
                    <EmptyState
                      compact
                      title="No callback deliveries for current filters"
                      description="Submit intents and configure destination to populate callback delivery history."
                    />
                  </td>
                </tr>
              ) : (
                filteredRows.map((row) => (
                  <tr key={row.callback_id} className="hover:bg-muted/30 transition-colors">
                    <td className="px-4 py-3 text-foreground">{row.callback_id}</td>
                    <td className="px-4 py-3 text-foreground truncate max-w-[150px]" title={row.intent_id}>{row.intent_id}</td>
                    <td className="px-4 py-3 text-foreground">{row.adapter_id ?? "-"}</td>
                    <td className="px-4 py-3 text-foreground">{row.state}</td>
                    <td className="px-4 py-3 text-foreground">{row.last_http_status ?? "-"}</td>
                    <td className="px-4 py-3 text-foreground">{row.attempts}</td>
                    <td className="px-4 py-3 text-foreground">{formatMs(row.next_attempt_at_ms)}</td>
                    <td className="px-4 py-3 text-foreground">{formatMs(row.updated_at_ms)}</td>
                    <td className="px-4 py-3">
                      <Link href={`/app/callbacks/${encodeURIComponent(row.callback_id)}`} className="text-primary hover:underline font-medium">Open</Link>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-2">Callback destination</h3>
        <p className="text-sm text-muted-foreground mb-4">Signing/bearer values are masked and never shown in plain text.</p>
        <form className="grid grid-cols-1 md:grid-cols-2 gap-4" onSubmit={(event) => void saveDestination(event)}>
          <label className="flex flex-col gap-2 md:col-span-2">
            <span className="text-sm font-medium text-foreground">Delivery URL</span>
            <input className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50" value={deliveryUrl} onChange={(event) => setDeliveryUrl(event.target.value)} disabled={!canManage} />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Timeout ms</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              value={timeoutMs}
              onChange={(event) => setTimeoutMs(event.target.value)}
              type="number"
              min={100}
              max={120000}
              disabled={!canManage}
            />
          </label>
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Allowed hosts</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50"
              value={allowedHosts}
              onChange={(event) => setAllowedHosts(event.target.value)}
              placeholder="example.com,api.example.com"
              disabled={!canManage}
            />
          </label>
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={enabled} onChange={(event) => setEnabled(event.target.checked)} disabled={!canManage} className="w-4 h-4 rounded border-border bg-background text-primary focus:ring-primary/50" />
            <span className="text-sm text-foreground">Enabled</span>
          </label>
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={allowPrivate}
              onChange={(event) => setAllowPrivate(event.target.checked)}
              disabled={!canManage}
              className="w-4 h-4 rounded border-border bg-background text-primary focus:ring-primary/50"
            />
            <span className="text-sm text-foreground">Allow private destinations</span>
          </label>
          <div className="flex gap-3 md:col-span-2">
            <button className="px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed" type="submit" disabled={saving || !canManage}>
              {saving ? "Saving..." : "Upsert Destination"}
            </button>
            <button className="px-4 py-2 bg-red-600 hover:bg-red-700 text-white font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed" type="button" onClick={() => void removeDestination()} disabled={!canManage}>
              Delete Destination
            </button>
          </div>
        </form>
        <pre className="mt-4 p-4 bg-muted/30 rounded-lg text-xs text-muted-foreground overflow-x-auto">{JSON.stringify(destinationData, null, 2)}</pre>
      </section>
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
