"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { apiGet, formatMs } from "@/lib/client-api";
import { readSession, switchWorkspace } from "@/lib/app-state";
import { EmptyState } from "@/components/ui/empty-state";
import type { CallbackDestinationResponse, WorkspaceDetailResponse } from "@/lib/types";

type WorkspaceDetailTab =
  | "overview"
  | "settings"
  | "api_keys"
  | "members"
  | "usage"
  | "billing"
  | "callback";

function middleEllipsis(value: string, start = 16, end = 10) {
  if (!value || value.length <= start + end + 3) return value;
  return `${value.slice(0, start)}...${value.slice(-end)}`;
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted/30 rounded-lg p-4">
      <span className="text-xs text-muted-foreground">{label}</span>
      <strong className="block text-sm text-foreground mt-1" title={value}>{value}</strong>
    </div>
  );
}

function Tab({
  id,
  label,
  current,
  onSelect,
}: {
  id: WorkspaceDetailTab;
  label: string;
  current: WorkspaceDetailTab;
  onSelect: (next: WorkspaceDetailTab) => void;
}) {
  return (
    <button
      type="button"
      className={`px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
        current === id
          ? "bg-primary text-primary-foreground"
          : "text-muted-foreground hover:text-foreground hover:bg-muted"
      }`}
      onClick={() => onSelect(id)}
    >
      {label}
    </button>
  );
}

function JsonPreview({
  title,
  value,
}: {
  title: string;
  value: unknown;
}) {
  return (
    <details className="bg-muted/20 rounded-lg border border-border/50 overflow-hidden">
      <summary className="px-4 py-3 cursor-pointer font-medium text-foreground hover:bg-muted/50 transition-colors">{title}</summary>
      <pre className="px-4 pb-4 text-xs font-mono text-muted-foreground overflow-x-auto">{JSON.stringify(value, null, 2)}</pre>
    </details>
  );
}

export function WorkspaceDetailPage({ workspaceId }: { workspaceId: string }) {
  const [detail, setDetail] = useState<WorkspaceDetailResponse | null>(null);
  const [callbackConfig, setCallbackConfig] = useState<CallbackDestinationResponse | null>(null);
  const [tab, setTab] = useState<WorkspaceDetailTab>("overview");
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [switching, setSwitching] = useState(false);

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaceId]);

  async function load() {
    setLoading(true);
    setError(null);
    setMessage(null);

    try {
      const encoded = encodeURIComponent(workspaceId);
      const payload = await apiGet<WorkspaceDetailResponse>(`account/workspaces/${encoded}/detail`);
      setDetail(payload);

      const session = await readSession();

      if (session?.workspace_id === payload.workspace.workspace_id) {
        const callback = await apiGet<CallbackDestinationResponse>(
          "status/tenant/callback-destination"
        ).catch(() => null);
        setCallbackConfig(callback);
      } else {
        setCallbackConfig(null);
      }
    } catch (loadError: unknown) {
      setDetail(null);
      setCallbackConfig(null);
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  async function onSwitchWorkspace() {
    if (!detail || detail.workspace.is_current) return;

    setSwitching(true);
    setError(null);
    setMessage(null);

    try {
      await switchWorkspace({ workspace_id: detail.workspace.workspace_id });
      setMessage("Workspace switched. Refreshing details...");
      await load();
    } catch (switchError: unknown) {
      setError(switchError instanceof Error ? switchError.message : String(switchError));
    } finally {
      setSwitching(false);
    }
  }

  const activeApiKeys = useMemo(
    () => detail?.api_keys.filter((row) => row.revoked_at_ms == null).length ?? 0,
    [detail?.api_keys]
  );

  const invoiceCount = detail?.invoices.length ?? 0;
  const memberCount = detail?.members.length ?? 0;
  const isCurrent = detail?.workspace.is_current ?? false;

  return (
    <div className="space-y-6">
      <section className="bg-gradient-to-br from-primary/20 via-card to-card rounded-2xl p-8 border border-primary/20">
        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-6">
          <div>
            <p className="text-sm font-medium text-primary mb-2">Workspace detail</p>
            <h2 className="text-2xl font-bold text-foreground mb-2">{detail?.workspace.workspace_name ?? middleEllipsis(workspaceId)}</h2>
            <p className="text-muted-foreground max-w-lg">
              Review this workspace, switch into it when needed, and inspect keys,
              members, usage, billing, and callback setup.
            </p>
          </div>

          <div className="flex flex-wrap gap-2">
            <span className="badge badge-neutral">
              {detail?.workspace.environment?.toUpperCase() ?? "WORKSPACE"}
            </span>
            <span className={`badge ${isCurrent ? "badge-green" : "badge-neutral"}`}>
              {isCurrent ? "Current" : "Not current"}
            </span>
            <span className="badge badge-neutral">{detail?.workspace.role ?? "-"}</span>
          </div>
        </div>
      </section>

      {loading ? <section className="bg-card rounded-xl border border-border/50 p-6">Loading workspace details...</section> : null}
      {error ? <section className="bg-destructive/10 border border-destructive/30 rounded-xl p-4 text-destructive">{error}</section> : null}
      {message ? <section className="bg-primary/10 border border-primary/30 rounded-xl p-4 text-primary">{message}</section> : null}

      {detail ? (
        <>
          <section className="bg-card rounded-xl border border-border/50 p-6">
            <div className="mb-6">
              <h3 className="text-lg font-semibold text-foreground">Workspace summary</h3>
              <p className="text-sm text-muted-foreground mt-1">A quick view of the important details.</p>
            </div>

            <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-5 gap-4">
              <Summary label="Name" value={detail.workspace.workspace_name} />
              <Summary label="Environment" value={detail.workspace.environment.toUpperCase()} />
              <Summary label="Workspace ID" value={middleEllipsis(detail.workspace.workspace_id)} />
              <Summary label="Tenant" value={middleEllipsis(detail.workspace.tenant_id)} />
              <Summary label="Role" value={detail.workspace.role} />
              <Summary label="Plan" value={detail.billing.plan} />
              <Summary label="Access mode" value={detail.billing.access_mode === "paid" ? "Paid" : "Free"} />
              <Summary label="Active keys" value={String(activeApiKeys)} />
              <Summary label="Members" value={String(memberCount)} />
              <Summary label="Invoices" value={String(invoiceCount)} />
            </div>
          </section>

          <section className="bg-card rounded-xl border border-border/50 p-6">
            <div className="flex flex-wrap gap-3">
              <button
                className="btn btn-primary"
                type="button"
                disabled={switching || isCurrent}
                onClick={() => void onSwitchWorkspace()}
              >
                {isCurrent
                  ? "Current workspace"
                  : switching
                    ? "Switching..."
                    : "Switch to this workspace"}
              </button>

              <Link href="/app/workspaces" className="btn btn-ghost">
                Back to workspaces
              </Link>
            </div>
          </section>

          <section className="bg-card rounded-xl border border-border/50 overflow-hidden">
            <div className="flex flex-wrap gap-1 p-2 border-b border-border/50">
              <Tab id="overview" current={tab} label="Overview" onSelect={setTab} />
              <Tab id="settings" current={tab} label="Settings" onSelect={setTab} />
              <Tab id="api_keys" current={tab} label="API keys" onSelect={setTab} />
              <Tab id="members" current={tab} label="Members" onSelect={setTab} />
              <Tab id="usage" current={tab} label="Usage" onSelect={setTab} />
              <Tab id="billing" current={tab} label="Billing" onSelect={setTab} />
              <Tab id="callback" current={tab} label="Callbacks" onSelect={setTab} />
            </div>

            <div className="p-6">
              {tab === "overview" ? (
                <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Workspace ID</span>
                    <strong className="block text-sm text-foreground font-mono mt-1" title={detail.workspace.workspace_id}>
                      {middleEllipsis(detail.workspace.workspace_id)}
                    </strong>
                    <small className="text-xs text-muted-foreground">Unique workspace identifier</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Tenant ID</span>
                    <strong className="block text-sm text-foreground font-mono mt-1" title={detail.workspace.tenant_id}>
                      {middleEllipsis(detail.workspace.tenant_id)}
                    </strong>
                    <small className="text-xs text-muted-foreground">Tenant linked to this workspace</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Requests used</span>
                    <strong className="block text-sm text-foreground mt-1">{String(detail.usage.used_requests)}</strong>
                    <small className="text-xs text-muted-foreground">Requests consumed so far</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Remaining</span>
                    <strong className="block text-sm text-foreground mt-1">{String(detail.usage.remaining_requests ?? "Unlimited")}</strong>
                    <small className="text-xs text-muted-foreground">Available requests remaining</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Invoices</span>
                    <strong className="block text-sm text-foreground mt-1">{String(invoiceCount)}</strong>
                    <small className="text-xs text-muted-foreground">Billing records for this workspace</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Members</span>
                    <strong className="block text-sm text-foreground mt-1">{String(memberCount)}</strong>
                    <small className="text-xs text-muted-foreground">People with access to this workspace</small>
                  </article>
                </div>
              ) : null}

              {tab === "settings" ? (
                <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Callbacks by default</span>
                    <strong className="block text-sm text-foreground mt-1">{detail.settings.callback_default_enabled ? "Enabled" : "Disabled"}</strong>
                    <small className="text-xs text-muted-foreground">Default callback behavior</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Retention</span>
                    <strong className="block text-sm text-foreground mt-1">{String(detail.settings.request_retention_days)} days</strong>
                    <small className="text-xs text-muted-foreground">Request retention period</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Customer replay</span>
                    <strong className="block text-sm text-foreground mt-1">
                      {detail.settings.allow_replay_from_customer_app ? "Enabled" : "Disabled"}
                    </strong>
                    <small className="text-xs text-muted-foreground">Replay availability in customer-facing flows</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Execution policy</span>
                    <strong className="block text-sm text-foreground mt-1">{detail.settings.execution_policy}</strong>
                    <small className="text-xs text-muted-foreground">Execution behavior for this workspace</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Sponsored cap</span>
                    <strong className="block text-sm text-foreground mt-1">{String(detail.settings.sponsored_monthly_cap_requests)}</strong>
                    <small className="text-xs text-muted-foreground">Monthly sponsored request limit</small>
                  </article>

                  <article className="bg-muted/30 rounded-lg p-4">
                    <span className="text-xs text-muted-foreground">Updated</span>
                    <strong className="block text-sm text-foreground mt-1">{formatMs(detail.settings.updated_at_ms)}</strong>
                    <small className="text-xs text-muted-foreground">Last settings update</small>
                  </article>
                </div>
              ) : null}

              {tab === "api_keys" ? (
                detail.api_keys.length === 0 ? (
                  <EmptyState
                    compact
                    title="No API keys"
                    description="This workspace does not have any API keys yet."
                    actionHref="/app/api-keys"
                    actionLabel="Open API keys"
                  />
                ) : (
                  <div className="overflow-x-auto">
                    <table className="w-full text-sm">
                      <thead className="bg-muted/50">
                        <tr>
                          <th className="text-left px-4 py-3 font-medium text-muted-foreground">Name</th>
                          <th className="text-left px-4 py-3 font-medium text-muted-foreground">Prefix</th>
                          <th className="text-left px-4 py-3 font-medium text-muted-foreground">Status</th>
                          <th className="text-left px-4 py-3 font-medium text-muted-foreground">Created</th>
                          <th className="text-left px-4 py-3 font-medium text-muted-foreground">Last used</th>
                        </tr>
                      </thead>
                      <tbody className="divide-y divide-border/50">
                        {detail.api_keys.map((row) => (
                          <tr key={row.id} className="hover:bg-muted/30 transition-colors">
                            <td className="px-4 py-3 text-foreground">{row.name}</td>
                            <td className="px-4 py-3 text-foreground font-mono text-xs">
                              {row.prefix}...{row.last4}
                            </td>
                            <td className="px-4 py-3">
                              <span className={row.revoked_at_ms ? "badge badge-yellow" : "badge badge-green"}>
                                {row.revoked_at_ms ? "Revoked" : "Active"}
                              </span>
                            </td>
                            <td className="px-4 py-3 text-foreground">{formatMs(row.created_at_ms)}</td>
                            <td className="px-4 py-3 text-foreground">{formatMs(row.last_used_at_ms)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )
              ) : null}

              {tab === "members" ? (
                <div className="space-y-4">
                  <div className="flex gap-3">
                    <Link href="/app/team" className="btn btn-ghost btn-sm">
                      Open team
                    </Link>
                  </div>

                  {detail.members.length === 0 ? (
                    <EmptyState
                      compact
                      title="No members"
                      description="No members are currently listed for this workspace."
                    />
                  ) : (
                    <div className="overflow-x-auto">
                      <table className="w-full text-sm">
                        <thead className="bg-muted/50">
                          <tr>
                            <th className="text-left px-4 py-3 font-medium text-muted-foreground">Email</th>
                            <th className="text-left px-4 py-3 font-medium text-muted-foreground">Role</th>
                            <th className="text-left px-4 py-3 font-medium text-muted-foreground">Status</th>
                            <th className="text-left px-4 py-3 font-medium text-muted-foreground">Added</th>
                          </tr>
                        </thead>
                        <tbody className="divide-y divide-border/50">
                          {detail.members.map((member) => (
                            <tr key={member.id} className="hover:bg-muted/30 transition-colors">
                              <td className="px-4 py-3 text-foreground">{member.email}</td>
                              <td className="px-4 py-3 text-foreground">{member.role}</td>
                              <td className="px-4 py-3 text-foreground">{member.status}</td>
                              <td className="px-4 py-3 text-foreground">{formatMs(member.added_at_ms)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              ) : null}

              {tab === "usage" ? (
                <div className="space-y-6">
                  <div className="grid grid-cols-2 gap-4">
                    <article className="bg-muted/30 rounded-lg p-4">
                      <span className="text-xs text-muted-foreground">Used requests</span>
                      <strong className="block text-2xl text-foreground mt-1">{String(detail.usage.used_requests)}</strong>
                      <small className="text-xs text-muted-foreground">Total requests used</small>
                    </article>

                    <article className="bg-muted/30 rounded-lg p-4">
                      <span className="text-xs text-muted-foreground">Remaining requests</span>
                      <strong className="block text-2xl text-foreground mt-1">{String(detail.usage.remaining_requests ?? "Unlimited")}</strong>
                      <small className="text-xs text-muted-foreground">Requests still available</small>
                    </article>
                  </div>

                  <JsonPreview title="View raw usage data" value={detail.usage} />
                </div>
              ) : null}

              {tab === "billing" ? (
                <div className="space-y-6">
                  <div className="grid grid-cols-2 gap-4">
                    <article className="bg-muted/30 rounded-lg p-4">
                      <span className="text-xs text-muted-foreground">Plan</span>
                      <strong className="block text-lg text-foreground mt-1">{detail.billing.plan}</strong>
                      <small className="text-xs text-muted-foreground">Current billing plan</small>
                    </article>

                    <article className="bg-muted/30 rounded-lg p-4">
                      <span className="text-xs text-muted-foreground">Access mode</span>
                      <strong className="block text-lg text-foreground mt-1">{detail.billing.access_mode === "paid" ? "Paid" : "Free"}</strong>
                      <small className="text-xs text-muted-foreground">Workspace access mode</small>
                    </article>
                  </div>

                  <JsonPreview title="View raw billing data" value={detail.billing} />

                  {detail.invoices.length === 0 ? (
                    <EmptyState
                      compact
                      title="No invoices"
                      description="This workspace does not have any invoices yet."
                    />
                  ) : (
                    <div className="overflow-x-auto">
                      <table className="w-full text-sm">
                        <thead className="bg-muted/50">
                          <tr>
                            <th className="text-left px-4 py-3 font-medium text-muted-foreground">Invoice</th>
                            <th className="text-left px-4 py-3 font-medium text-muted-foreground">Period</th>
                            <th className="text-left px-4 py-3 font-medium text-muted-foreground">Amount</th>
                            <th className="text-left px-4 py-3 font-medium text-muted-foreground">Status</th>
                            <th className="text-left px-4 py-3 font-medium text-muted-foreground">Issued</th>
                          </tr>
                        </thead>
                        <tbody className="divide-y divide-border/50">
                          {detail.invoices.map((invoice) => (
                            <tr key={invoice.id} className="hover:bg-muted/30 transition-colors">
                              <td className="px-4 py-3 text-foreground font-mono text-xs" title={invoice.id}>{middleEllipsis(invoice.id, 10, 8)}</td>
                              <td className="px-4 py-3 text-foreground">{invoice.period}</td>
                              <td className="px-4 py-3 text-foreground">${invoice.amount_usd.toFixed(2)}</td>
                              <td className="px-4 py-3 text-foreground">{invoice.status}</td>
                              <td className="px-4 py-3 text-foreground">{formatMs(invoice.issued_at_ms)}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  )}
                </div>
              ) : null}

              {tab === "callback" ? (
                isCurrent ? (
                  callbackConfig ? (
                    <JsonPreview title="View callback configuration" value={callbackConfig} />
                  ) : (
                    <EmptyState
                      compact
                      title="No callback configuration"
                      description="No callback destination is configured for the current workspace."
                      actionHref="/app/callbacks"
                      actionLabel="Open callbacks"
                    />
                  )
                ) : (
                  <EmptyState
                    compact
                    title="Switch to view callbacks"
                    description="Callback details are available after switching this workspace into the current session."
                  />
                )
              ) : null}
            </div>
          </section>
        </>
      ) : null}
    </div>
  );
}