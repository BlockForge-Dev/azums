"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { apiGet, formatMs, shortId } from "@/lib/client-api";
import type { AdminOverviewResponse } from "@/lib/types";

export type InternalAdminView =
  | "overview"
  | "tenants"
  | "workspaces"
  | "dead_letters"
  | "incidents"
  | "adapter_health";

export function InternalAdminPage({ view }: { view: InternalAdminView }) {
  const [data, setData] = useState<AdminOverviewResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void load();
  }, []);

  async function load() {
    setLoading(true);
    setError(null);
    try {
      const next = await apiGet<AdminOverviewResponse>("admin/overview");
      setData(next);
    } catch (loadError: unknown) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
      setData(null);
    } finally {
      setLoading(false);
    }
  }

  const totals = useMemo(() => {
    return {
      tenants: data?.tenants.length ?? 0,
      workspaces: data?.workspaces.length ?? 0,
      deadLetters: data?.dead_letters.length ?? 0,
      incidents: data?.incidents.length ?? 0,
      adapters: data?.adapter_health.length ?? 0,
    };
  }, [data]);

  const titleMap: Record<InternalAdminView, string> = {
    overview: "Internal Admin Overview",
    tenants: "Tenant Inventory",
    workspaces: "Workspace Inventory",
    dead_letters: "Dead Letter Queue",
    incidents: "Incidents",
    adapter_health: "Adapter Health",
  };

  return (
    <div className="stack">
      <section className="surface hero-surface">
        <p className="eyebrow">Internal Admin</p>
        <h2>{titleMap[view]}</h2>
        <p>Global platform controls and reliability visibility for internal operations.</p>
      </section>

      <section className="surface">
        <div className="controls inline">
          <button className="btn ghost" type="button" onClick={() => void load()}>
            {loading ? "Refreshing..." : "Refresh"}
          </button>
          <Link href="/ops">Back to operator overview</Link>
        </div>
        {error ? <p className="inline-error">{error}</p> : null}
      </section>

      <section className="surface summary-grid">
        <Summary label="Tenants" value={String(totals.tenants)} />
        <Summary label="Workspaces" value={String(totals.workspaces)} />
        <Summary label="Dead letters" value={String(totals.deadLetters)} />
        <Summary label="Incidents" value={String(totals.incidents)} />
        <Summary label="Adapter health rows" value={String(totals.adapters)} />
        <Summary label="Generated" value={formatMs(data?.generated_at_ms)} />
      </section>

      {view === "overview" || view === "tenants" ? (
        <section className="surface">
          <h3>Tenants</h3>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Tenant</th>
                  <th>Workspaces</th>
                  <th>Principals</th>
                  <th>API keys</th>
                  <th>Active webhook keys</th>
                </tr>
              </thead>
              <tbody>
                {(data?.tenants ?? []).map((tenant) => (
                  <tr key={tenant.tenant_id}>
                    <td>{tenant.tenant_id}</td>
                    <td>{tenant.workspace_count}</td>
                    <td>{tenant.principal_count}</td>
                    <td>{tenant.api_key_count}</td>
                    <td>{tenant.active_webhook_keys}</td>
                  </tr>
                ))}
                {(data?.tenants ?? []).length === 0 ? (
                  <tr>
                    <td colSpan={5}>No tenant rows yet.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}

      {view === "overview" || view === "workspaces" ? (
        <section className="surface">
          <h3>Workspaces</h3>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Workspace</th>
                  <th>Tenant</th>
                  <th>Principals</th>
                  <th>Plan</th>
                  <th>Mode</th>
                </tr>
              </thead>
              <tbody>
                {(data?.workspaces ?? []).map((workspace) => (
                  <tr key={workspace.workspace_id}>
                    <td title={workspace.workspace_id}>
                      {workspace.workspace_name} ({shortId(workspace.workspace_id)})
                    </td>
                    <td>{workspace.tenant_id}</td>
                    <td>{workspace.principals}</td>
                    <td>{workspace.plan}</td>
                    <td>{workspace.access_mode}</td>
                  </tr>
                ))}
                {(data?.workspaces ?? []).length === 0 ? (
                  <tr>
                    <td colSpan={5}>No workspace rows yet.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}

      {view === "overview" || view === "dead_letters" ? (
        <section className="surface">
          <h3>Dead letters</h3>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Tenant</th>
                  <th>Intent</th>
                  <th>State</th>
                  <th>Classification</th>
                  <th>Adapter</th>
                  <th>Updated</th>
                </tr>
              </thead>
              <tbody>
                {(data?.dead_letters ?? []).map((job) => (
                  <tr key={job.intent_id}>
                    <td>{job.tenant_id}</td>
                    <td title={job.intent_id}>
                      <Link
                        href={`/ops/requests/${encodeURIComponent(job.intent_id)}?tenant_id=${encodeURIComponent(
                          job.tenant_id
                        )}`}
                      >
                        {shortId(job.intent_id)}
                      </Link>
                    </td>
                    <td>{job.state}</td>
                    <td>{job.classification}</td>
                    <td>{job.adapter_id ?? "-"}</td>
                    <td>{formatMs(job.updated_at_ms)}</td>
                  </tr>
                ))}
                {(data?.dead_letters ?? []).length === 0 ? (
                  <tr>
                    <td colSpan={6}>No dead-lettered jobs found.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}

      {view === "overview" || view === "incidents" ? (
        <section className="surface">
          <h3>Incidents</h3>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>ID</th>
                  <th>Kind</th>
                  <th>Severity</th>
                  <th>Tenant</th>
                  <th>Intent</th>
                  <th>Classification</th>
                  <th>State</th>
                  <th>Updated</th>
                </tr>
              </thead>
              <tbody>
                {(data?.incidents ?? []).map((incident) => (
                  <tr key={incident.incident_id}>
                    <td>{incident.incident_id}</td>
                    <td>{incident.kind}</td>
                    <td>{incident.severity}</td>
                    <td>{incident.tenant_id}</td>
                    <td title={incident.intent_id}>
                      <Link
                        href={`/ops/requests/${encodeURIComponent(
                          incident.intent_id
                        )}?tenant_id=${encodeURIComponent(incident.tenant_id)}`}
                      >
                        {shortId(incident.intent_id)}
                      </Link>
                    </td>
                    <td>{incident.classification}</td>
                    <td>{incident.state}</td>
                    <td>{formatMs(incident.updated_at_ms)}</td>
                  </tr>
                ))}
                {(data?.incidents ?? []).length === 0 ? (
                  <tr>
                    <td colSpan={8}>No incidents found.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}

      {view === "overview" || view === "adapter_health" ? (
        <section className="surface">
          <h3>Adapter health</h3>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Adapter</th>
                  <th>Total</th>
                  <th>Success</th>
                  <th>Failure</th>
                  <th>Retrying</th>
                </tr>
              </thead>
              <tbody>
                {(data?.adapter_health ?? []).map((row) => (
                  <tr key={row.adapter_id}>
                    <td>{row.adapter_id}</td>
                    <td>{row.total_jobs}</td>
                    <td>{row.success_jobs}</td>
                    <td>{row.failure_jobs}</td>
                    <td>{row.retrying_jobs}</td>
                  </tr>
                ))}
                {(data?.adapter_health ?? []).length === 0 ? (
                  <tr>
                    <td colSpan={5}>No adapter health rows found.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}
    </div>
  );
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="summary-card">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
