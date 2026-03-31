"use client";

import { useEffect, useMemo, useState } from "react";
import { apiGet, formatMs } from "@/lib/client-api";
import type { IntakeAudit, IntakeAuditsResponse, UiConfigResponse } from "@/lib/types";
import {
  canManageWorkspace,
  createWebhookKey,
  listWebhookKeys,
  readSession,
  revokeWebhookKey,
  type WebhookKeyRecord,
} from "@/lib/app-state";

type MappingRow = {
  event_type: string;
  intent_kind: string;
  notes: string;
};

const DEFAULT_SOURCE = "default";
const DEFAULT_ISSUE_GRACE = "900";
const DEFAULT_REVOKE_GRACE = "0";
const DEFAULT_AUDIT_LIMIT = "50";

const DEFAULT_MAPPINGS: MappingRow[] = [
  {
    event_type: "github.push",
    intent_kind: "solana.transfer.v1",
    notes: "Example mapping for sandbox validation.",
  },
  {
    event_type: "github.release",
    intent_kind: "solana.broadcast.v1",
    notes: "Example mapping for signed payload workflows.",
  },
];

export function WebhooksPage() {
  const [config, setConfig] = useState<UiConfigResponse | null>(null);
  const [keys, setKeys] = useState<WebhookKeyRecord[]>([]);
  const [audits, setAudits] = useState<IntakeAudit[]>([]);
  const [mappings, setMappings] = useState<MappingRow[]>(DEFAULT_MAPPINGS);

  const [keySource, setKeySource] = useState(DEFAULT_SOURCE);
  const [issueGrace, setIssueGrace] = useState(DEFAULT_ISSUE_GRACE);
  const [revokeGrace, setRevokeGrace] = useState(DEFAULT_REVOKE_GRACE);
  const [auditResult, setAuditResult] = useState("");
  const [auditLimit, setAuditLimit] = useState(DEFAULT_AUDIT_LIMIT);

  const [latestSecret, setLatestSecret] = useState<{
    key_id: string;
    source: string;
    secret: string;
  } | null>(null);

  const [canManage, setCanManage] = useState(false);
  const [pageLoading, setPageLoading] = useState(false);
  const [keyLoading, setKeyLoading] = useState(false);
  const [auditsLoading, setAuditsLoading] = useState(false);

  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const currentSource = useMemo(
    () => keySource.trim() || DEFAULT_SOURCE,
    [keySource]
  );

  useEffect(() => {
    void refreshAll();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const receiverUrl = useMemo(() => {
    const base = config?.ingress_base_url?.trim() || "";
    const normalized = base.replace(/\/$/, "");
    const encodedSource = encodeURIComponent(currentSource);
    return normalized ? `${normalized}/webhooks/${encodedSource}` : `/webhooks/${encodedSource}`;
  }, [config?.ingress_base_url, currentSource]);

  const activeKeyCount = useMemo(
    () => keys.filter((row) => row.active).length,
    [keys]
  );

  const auditStats = useMemo(() => {
    return {
      loaded: audits.length,
      accepted: audits.filter((row) => row.validation_result === "accepted").length,
      rejected: audits.filter((row) => row.validation_result === "rejected").length,
    };
  }, [audits]);

  const eventTypesSeen = useMemo(() => {
    const values = new Set<string>();

    for (const audit of audits) {
      const eventType = audit.details_json?.event_type;
      if (typeof eventType === "string" && eventType.trim()) {
        values.add(eventType.trim());
      }
    }

    return [...values].sort();
  }, [audits]);

  async function fetchAudits(validationResult: string): Promise<IntakeAudit[]> {
    const params = new URLSearchParams();
    params.set("channel", "webhook");
    params.set("limit", String(Number(auditLimit || DEFAULT_AUDIT_LIMIT)));
    params.set("offset", "0");

    if (validationResult.trim()) {
      params.set("validation_result", validationResult.trim());
    }

    const data = await apiGet<IntakeAuditsResponse>(
      `status/tenant/intake-audits?${params.toString()}`
    );

    return data.audits ?? [];
  }

  async function refreshAll() {
    setPageLoading(true);
    setError(null);

    try {
      const [cfg, session, webhookKeys, auditRows] = await Promise.all([
        apiGet<UiConfigResponse>("config").catch(() => null),
        readSession(),
        listWebhookKeys(currentSource, true).catch(() => []),
        fetchAudits(auditResult).catch(() => []),
      ]);

      setConfig(cfg);
      setCanManage(Boolean(session && canManageWorkspace(session.role)));
      setKeys(webhookKeys);
      setAudits(auditRows);
    } catch (loadError: unknown) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setPageLoading(false);
    }
  }

  async function loadWebhookKeysForSource() {
    setKeyLoading(true);
    setError(null);

    try {
      const rows = await listWebhookKeys(currentSource, true);
      setKeys(rows);
    } catch (loadError: unknown) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setKeyLoading(false);
    }
  }

  async function loadAuditsForFilter() {
    setAuditsLoading(true);
    setError(null);

    try {
      const rows = await fetchAudits(auditResult);
      setAudits(rows);
    } catch (loadError: unknown) {
      setAudits([]);
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setAuditsLoading(false);
    }
  }

  async function issueWebhookKey() {
    if (!canManage) {
      setError("Your role cannot issue webhook keys.");
      return;
    }

    setError(null);
    setMessage(null);
    setLatestSecret(null);
    setKeyLoading(true);

    try {
      const result = await createWebhookKey({
        source: currentSource,
        grace_seconds: Number(issueGrace || DEFAULT_ISSUE_GRACE),
      });

      setLatestSecret({
        key_id: result.webhook_key.key_id,
        source: result.webhook_key.source,
        secret: result.webhook_key.secret,
      });

      setMessage(`Webhook key issued: ${result.webhook_key.key_id}`);
      setKeys(await listWebhookKeys(currentSource, true));
    } catch (createError: unknown) {
      setError(createError instanceof Error ? createError.message : String(createError));
    } finally {
      setKeyLoading(false);
    }
  }

  async function revokeKey(keyId: string) {
    if (!canManage) {
      setError("Your role cannot revoke webhook keys.");
      return;
    }

    if (!window.confirm("Revoke this webhook key?")) {
      return;
    }

    setError(null);
    setMessage(null);
    setKeyLoading(true);

    try {
      await revokeWebhookKey(keyId, Number(revokeGrace || DEFAULT_REVOKE_GRACE));
      setMessage(`Webhook key revoked: ${keyId}`);
      setKeys(await listWebhookKeys(currentSource, true));
    } catch (revokeError: unknown) {
      setError(revokeError instanceof Error ? revokeError.message : String(revokeError));
    } finally {
      setKeyLoading(false);
    }
  }

  function updateMapping(index: number, patch: Partial<MappingRow>) {
    setMappings((previous) =>
      previous.map((row, rowIndex) =>
        rowIndex === index ? { ...row, ...patch } : row
      )
    );
  }

  function addMappingRow() {
    setMappings((previous) => [
      ...previous,
      { event_type: "", intent_kind: "", notes: "" },
    ]);
  }

  return (
    <div className="flex flex-col gap-6 p-6 max-w-7xl mx-auto">
      <section className="bg-gradient-to-br from-card to-card/80 rounded-xl border border-border/50 p-6">
        <div className="flex flex-col md:flex-row md:items-start md:justify-between gap-4">
          <div>
            <p className="text-sm font-medium text-muted-foreground mb-1">Inbound Webhooks</p>
            <h2 className="text-2xl font-semibold text-foreground">Verify webhook signatures and inspect inbound events.</h2>
            <p className="text-muted-foreground mt-1">
              Manage signing keys, inspect accepted and rejected webhook intake,
              and maintain event-to-intent mappings.
            </p>
          </div>

          <button className="px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors self-start" type="button" onClick={() => void refreshAll()}>
            {pageLoading ? "Refreshing..." : "Refresh"}
          </button>
        </div>
      </section>

      {error ? <section className="bg-red-500/10 border border-red-500/30 text-red-400 rounded-lg p-4">{error}</section> : null}
      {message ? <section className="bg-green-500/10 border border-green-500/30 text-green-400 rounded-lg p-4">{message}</section> : null}

      {!canManage ? (
        <section className="bg-yellow-500/10 border border-yellow-500/30 text-yellow-400 rounded-lg p-4">
          Read-only webhook view. Workspace owner or admin can issue and revoke signing keys.
        </section>
      ) : null}

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">Receiver</h3>
        <p className="text-sm text-muted-foreground mb-6">
          Use this endpoint and these headers when sending inbound webhooks.
        </p>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Tenant</span>
            <code className="text-foreground block mt-1 text-sm break-all">{config?.tenant_id ?? "-"}</code>
          </div>
          <div className="bg-muted/30 rounded-lg border border-border/50 p-4">
            <span className="text-xs text-muted-foreground uppercase tracking-wide">Active signing keys</span>
            <strong className="text-foreground block mt-1 text-lg">{activeKeyCount}</strong>
          </div>
        </div>

        <div className="bg-muted/30 rounded-lg border border-border/50 p-4 mb-4">
          <span className="text-xs text-muted-foreground uppercase tracking-wide block mb-2">Webhook URL</span>
          <code className="text-primary text-sm break-all">{receiverUrl}</code>
        </div>

        <div className="bg-muted/20 rounded-lg border border-border/30 p-4">
          <span className="text-xs text-muted-foreground uppercase tracking-wide block mb-2">Required headers</span>
          <code className="text-muted-foreground text-sm whitespace-pre-wrap">
            x-tenant-id, x-principal-id, x-submitter-kind: signed_webhook_sender,
            {"\n"}x-webhook-key-id, x-webhook-signature
          </code>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground mb-1">Signing keys</h3>
          <p className="text-sm text-muted-foreground">
            Issue, rotate, and revoke webhook signing keys by source.
          </p>
        </div>

        <div className="flex flex-wrap items-end gap-4 mb-6">
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Source</span>
            <input
              data-testid="webhooks-source"
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 w-48"
              value={keySource}
              onChange={(event) => setKeySource(event.target.value)}
              placeholder="default / github / stripe"
            />
          </label>

          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Rotation grace seconds</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 w-32"
              value={issueGrace}
              onChange={(event) => setIssueGrace(event.target.value)}
              type="number"
              min={0}
            />
          </label>

          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Revoke grace seconds</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 w-32"
              value={revokeGrace}
              onChange={(event) => setRevokeGrace(event.target.value)}
              type="number"
              min={0}
            />
          </label>

          <button
            data-testid="webhooks-issue-key"
            className="px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
            type="button"
            onClick={() => void issueWebhookKey()}
            disabled={!canManage || keyLoading}
          >
            {keyLoading ? "Working..." : "Issue / rotate key"}
          </button>

          <button
            className="px-3 py-2 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded-lg transition-colors"
            type="button"
            onClick={() => void loadWebhookKeysForSource()}
            disabled={keyLoading}
          >
            Load source
          </button>
        </div>

        {latestSecret ? (
          <div className="mb-6 bg-green-500/10 border border-green-500/30 rounded-lg p-4">
            <strong className="text-green-400 block mb-2">Copy this secret now. It is shown once.</strong>
            <pre className="text-sm text-green-400 overflow-x-auto">{`key_id=${latestSecret.key_id}\nsource=${latestSecret.source}\nsecret=${latestSecret.secret}`}</pre>
          </div>
        ) : null}

        <div className="overflow-x-auto rounded-lg border border-border/50">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Key</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Source</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Status</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Last4</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Created</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Last used</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Action</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {keys.length === 0 ? (
                <tr>
                  <td colSpan={7} className="px-4 py-6 text-center text-muted-foreground">No signing keys found for this source.</td>
                </tr>
              ) : (
                keys.map((row) => (
                  <tr key={row.key_id} className="hover:bg-muted/30 transition-colors">
                    <td className="px-4 py-3 text-foreground">{row.key_id}</td>
                    <td className="px-4 py-3 text-foreground">{row.source}</td>
                    <td className="px-4 py-3 text-foreground">{row.active ? "active" : "inactive"}</td>
                    <td className="px-4 py-3 text-foreground">{row.secret_last4}</td>
                    <td className="px-4 py-3 text-foreground">{formatMs(row.created_at_ms)}</td>
                    <td className="px-4 py-3 text-foreground">{formatMs(row.last_used_at_ms)}</td>
                    <td className="px-4 py-3">
                      {row.active ? (
                        <button
                          className="px-2 py-1 text-xs font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                          type="button"
                          onClick={() => void revokeKey(row.key_id)}
                          disabled={!canManage || keyLoading}
                        >
                          Revoke
                        </button>
                      ) : (
                        "-"
                      )}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground mb-1">Inbound audit stream</h3>
          <p className="text-sm text-muted-foreground">
            Inspect accepted and rejected webhook intake events.
          </p>
        </div>

        <div className="flex flex-wrap items-end gap-4 mb-6">
          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Validation result</span>
            <select 
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 w-32"
              value={auditResult} 
              onChange={(event) => setAuditResult(event.target.value)}
            >
              <option value="">All</option>
              <option value="accepted">Accepted</option>
              <option value="rejected">Rejected</option>
            </select>
          </label>

          <label className="flex flex-col gap-2">
            <span className="text-sm font-medium text-foreground">Limit</span>
            <input
              className="px-3 py-2 bg-background border border-border rounded-lg text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/50 w-24"
              type="number"
              min={1}
              max={200}
              value={auditLimit}
              onChange={(event) => setAuditLimit(event.target.value)}
            />
          </label>

          <button className="px-4 py-2 bg-primary text-primary-foreground hover:bg-primary/90 font-medium rounded-lg transition-colors" type="button" onClick={() => void loadAuditsForFilter()}>
            {auditsLoading ? "Loading..." : "Load audits"}
          </button>
        </div>

        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Loaded</span>
            <strong className="text-foreground block text-lg mt-1">{auditStats.loaded}</strong>
          </div>
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Accepted</span>
            <strong className="text-foreground block text-lg mt-1">{auditStats.accepted}</strong>
          </div>
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Rejected</span>
            <strong className="text-foreground block text-lg mt-1">{auditStats.rejected}</strong>
          </div>
          <div className="bg-muted/30 rounded-xl border border-border/50 p-4">
            <span className="text-sm text-muted-foreground">Event types seen</span>
            <strong className="text-foreground block text-lg mt-1">{eventTypesSeen.length}</strong>
          </div>
        </div>

        <div className="overflow-x-auto rounded-lg border border-border/50">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Request</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Result</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Event type</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Intent kind</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Reason</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Principal</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Created</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {audits.length === 0 ? (
                <tr>
                  <td colSpan={7} className="px-4 py-6 text-center text-muted-foreground">No inbound webhook audits for this filter.</td>
                </tr>
              ) : (
                audits.map((audit) => {
                  const eventType =
                    typeof audit.details_json?.event_type === "string"
                      ? audit.details_json.event_type
                      : "-";

                  return (
                    <tr key={`${audit.request_id}-${audit.created_at_ms}`} className="hover:bg-muted/30 transition-colors">
                      <td className="px-4 py-3 text-foreground">{audit.request_id}</td>
                      <td className="px-4 py-3 text-foreground">{audit.validation_result}</td>
                      <td className="px-4 py-3 text-foreground">{eventType}</td>
                      <td className="px-4 py-3 text-foreground">{audit.intent_kind ?? "-"}</td>
                      <td className="px-4 py-3 text-foreground">{audit.rejection_reason ?? audit.error_message ?? "-"}</td>
                      <td className="px-4 py-3 text-foreground">{audit.principal_id ?? "-"}</td>
                      <td className="px-4 py-3 text-foreground">{formatMs(audit.created_at_ms)}</td>
                    </tr>
                  );
                })
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h3 className="text-lg font-semibold text-foreground mb-1">Event mappings</h3>
            <p className="text-sm text-muted-foreground">
              Map inbound event types to supported intent handlers.
            </p>
          </div>

          <button className="px-3 py-1.5 text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-muted rounded-md transition-colors" type="button" onClick={addMappingRow}>
            Add row
          </button>
        </div>

        {eventTypesSeen.length > 0 ? (
          <p className="text-sm text-muted-foreground mb-4">Observed event types: {eventTypesSeen.join(", ")}</p>
        ) : null}

        <div className="overflow-x-auto rounded-lg border border-border/50">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Event type</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Intent kind</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Notes</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {mappings.map((mapping, index) => (
                <tr key={`${mapping.event_type}-${index}`} className="hover:bg-muted/30 transition-colors">
                  <td className="px-4 py-3">
                    <input
                      className="w-full px-2 py-1 bg-background border border-border rounded text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/50"
                      value={mapping.event_type}
                      onChange={(event) =>
                        updateMapping(index, { event_type: event.target.value })
                      }
                    />
                  </td>
                  <td className="px-4 py-3">
                    <input
                      className="w-full px-2 py-1 bg-background border border-border rounded text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/50"
                      value={mapping.intent_kind}
                      onChange={(event) =>
                        updateMapping(index, { intent_kind: event.target.value })
                      }
                    />
                  </td>
                  <td className="px-4 py-3">
                    <input
                      className="w-full px-2 py-1 bg-background border border-border rounded text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-primary/50"
                      value={mapping.notes}
                      onChange={(event) =>
                        updateMapping(index, { notes: event.target.value })
                      }
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
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
