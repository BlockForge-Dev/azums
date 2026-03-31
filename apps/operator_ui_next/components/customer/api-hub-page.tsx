"use client";

import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  createApiKey,
  createWebhookKey,
  listApiKeys,
  listWebhookKeys,
  readSession,
  revokeApiKey,
  revokeWebhookKey,
  type ApiKeyRecord,
  type WebhookKeyRecord,
} from "@/lib/app-state";
import { apiGet, formatMs } from "@/lib/client-api";
import type { UiConfigResponse } from "@/lib/types";

export function ApiHubPage() {
  const [config, setConfig] = useState<UiConfigResponse | null>(null);
  const [apiKeys, setApiKeys] = useState<ApiKeyRecord[]>([]);
  const [webhookKeys, setWebhookKeys] = useState<WebhookKeyRecord[]>([]);
  const [newApiKeyName, setNewApiKeyName] = useState("backend-service");
  const [webhookSource, setWebhookSource] = useState("default");
  const [webhookGrace, setWebhookGrace] = useState("900");
  const [latestApiToken, setLatestApiToken] = useState<string | null>(null);
  const [latestWebhookSecret, setLatestWebhookSecret] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void Promise.all([
      apiGet<UiConfigResponse>("config"),
      listApiKeys(),
      listWebhookKeys("default", true),
      readSession(),
    ])
      .then(([cfg, keys, webhookRows]) => {
        if (cancelled) return;
        setConfig(cfg);
        setApiKeys(keys);
        setWebhookKeys(webhookRows);
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

  const ingressRequestUrl = useMemo(() => {
    const base = config?.ingress_base_url?.replace(/\/$/, "") ?? "http://127.0.0.2:8000";
    return `${base}/api/requests`;
  }, [config?.ingress_base_url]);

  const webhookReceiverUrl = useMemo(() => {
    const base = config?.ingress_base_url?.replace(/\/$/, "") ?? "http://127.0.0.2:8000";
    return `${base}/webhooks/${encodeURIComponent(webhookSource.trim() || "default")}`;
  }, [config?.ingress_base_url, webhookSource]);

  const sampleRequestPayload = useMemo(
    () =>
      JSON.stringify(
        {
          intent_kind: "solana.transfer.v1",
          payload: {
            intent_id: "intent_demo_from_api",
            type: "transfer",
            to_addr: "GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC",
            amount: 1,
          },
        },
        null,
        2
      ),
    []
  );

  async function refresh() {
    const [keys, webhookRows] = await Promise.all([
      listApiKeys(),
      listWebhookKeys(webhookSource || "default", true),
    ]);
    setApiKeys(keys);
    setWebhookKeys(webhookRows);
  }

  async function onCreateApiKey(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setMessage(null);
    setLatestApiToken(null);
    try {
      const created = await createApiKey(newApiKeyName.trim() || "default");
      setLatestApiToken(created.token);
      setMessage(`API key created: ${created.key.id}`);
      setNewApiKeyName("");
      await refresh();
    } catch (createError: unknown) {
      setError(createError instanceof Error ? createError.message : String(createError));
    }
  }

  async function onRevokeApiKey(keyId: string) {
    if (!window.confirm("Revoke this API key?")) return;
    setError(null);
    setMessage(null);
    try {
      await revokeApiKey(keyId);
      setMessage(`API key revoked: ${keyId}`);
      await refresh();
    } catch (revokeError: unknown) {
      setError(revokeError instanceof Error ? revokeError.message : String(revokeError));
    }
  }

  async function onIssueWebhookKey() {
    setError(null);
    setMessage(null);
    setLatestWebhookSecret(null);
    try {
      const created = await createWebhookKey({
        source: webhookSource.trim() || "default",
        grace_seconds: Number(webhookGrace || "900"),
      });
      setLatestWebhookSecret(created.webhook_key.secret);
      setMessage(`Webhook key issued: ${created.webhook_key.key_id}`);
      await refresh();
    } catch (issueError: unknown) {
      setError(issueError instanceof Error ? issueError.message : String(issueError));
    }
  }

  async function onRevokeWebhookKey(keyId: string) {
    if (!window.confirm("Revoke this webhook key?")) return;
    setError(null);
    setMessage(null);
    try {
      await revokeWebhookKey(keyId, 0);
      setMessage(`Webhook key revoked: ${keyId}`);
      await refresh();
    } catch (revokeError: unknown) {
      setError(revokeError instanceof Error ? revokeError.message : String(revokeError));
    }
  }

  const curlSnippet = useMemo(() => {
    const tenantId = config?.tenant_id ?? "<TENANT_ID>";
    return `curl -X POST "${ingressRequestUrl}" \\
  -H "content-type: application/json" \\
  -H "x-tenant-id: ${tenantId}" \\
  -H "x-principal-id: backend-service" \\
  -H "x-submitter-kind: api_key_holder" \\
  -H "x-api-key: <API_KEY>" \\
  -d '${sampleRequestPayload}'`;
  }, [config?.tenant_id, ingressRequestUrl, sampleRequestPayload]);

  return (
    <div className="stack">
      <section className="surface hero-surface">
        <p className="eyebrow">API & Integrations</p>
        <h2>Unified API Hub</h2>
        <p>Manage API keys, webhook signing keys, and backend integration snippets from one page.</p>
      </section>

      {error ? <section className="surface error-surface">{error}</section> : null}
      {message ? <section className="surface success-surface">{message}</section> : null}
      {loading ? <section className="surface">Loading integration hub...</section> : null}

      <section className="surface">
        <h3>Environment URLs</h3>
        <div className="meta-grid">
          <div>
            <span>Ingress base</span>
            <strong>{config?.ingress_base_url ?? "-"}</strong>
          </div>
          <div>
            <span>Status base</span>
            <strong>{config?.status_base_url ?? "-"}</strong>
          </div>
          <div>
            <span>Request endpoint</span>
            <strong>{ingressRequestUrl}</strong>
          </div>
          <div>
            <span>Webhook receiver</span>
            <strong>{webhookReceiverUrl}</strong>
          </div>
        </div>
      </section>

      <section className="surface">
        <h3>Backend quickstart</h3>
        <p className="hint-line">Use Playground for customer-side activation. Use these examples for backend and webhook integrations.</p>
        <pre>{curlSnippet}</pre>
        <pre>{sampleRequestPayload}</pre>
      </section>

      <section className="surface">
        <h3>API keys</h3>
        <form className="controls inline" onSubmit={(event) => void onCreateApiKey(event)}>
          <label className="wide">
            Key name
            <input
              value={newApiKeyName}
              onChange={(event) => setNewApiKeyName(event.target.value)}
              placeholder="backend-service"
            />
          </label>
          <button className="btn primary" type="submit">
            Create key
          </button>
        </form>
        {latestApiToken ? (
          <div className="surface subtle-surface">
            <strong>Copy this API key now:</strong>
            <pre>{latestApiToken}</pre>
          </div>
        ) : null}
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Name</th>
                <th>Prefix</th>
                <th>Status</th>
                <th>Created</th>
                <th>Last used</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {apiKeys.length === 0 ? (
                <tr>
                  <td colSpan={6}>No API keys yet.</td>
                </tr>
              ) : (
                apiKeys.map((row) => (
                  <tr key={row.id}>
                    <td>{row.name}</td>
                    <td>
                      {row.prefix}...{row.last4}
                    </td>
                    <td>{row.revoked_at_ms ? "revoked" : "active"}</td>
                    <td>{formatMs(row.created_at_ms)}</td>
                    <td>{formatMs(row.last_used_at_ms)}</td>
                    <td>
                      {row.revoked_at_ms ? (
                        "-"
                      ) : (
                        <button
                          className="btn ghost btn-tight"
                          type="button"
                          onClick={() => void onRevokeApiKey(row.id)}
                        >
                          Revoke
                        </button>
                      )}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </section>

      <section className="surface">
        <h3>Webhook signing keys</h3>
        <div className="controls inline">
          <label>
            Source
            <input
              value={webhookSource}
              onChange={(event) => setWebhookSource(event.target.value)}
              placeholder="default"
            />
          </label>
          <label>
            Rotation grace seconds
            <input
              value={webhookGrace}
              onChange={(event) => setWebhookGrace(event.target.value)}
              type="number"
              min={0}
              max={86400}
            />
          </label>
          <button className="btn primary" type="button" onClick={() => void onIssueWebhookKey()}>
            Issue/Rotate key
          </button>
        </div>
        {latestWebhookSecret ? (
          <div className="surface subtle-surface">
            <strong>Copy this webhook secret now:</strong>
            <pre>{latestWebhookSecret}</pre>
          </div>
        ) : null}
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Key</th>
                <th>Source</th>
                <th>Status</th>
                <th>Created</th>
                <th>Last used</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {webhookKeys.length === 0 ? (
                <tr>
                  <td colSpan={6}>No webhook keys found for this source.</td>
                </tr>
              ) : (
                webhookKeys.map((row) => (
                  <tr key={row.key_id}>
                    <td>{row.key_id}</td>
                    <td>{row.source}</td>
                    <td>{row.active ? "active" : "inactive"}</td>
                    <td>{formatMs(row.created_at_ms)}</td>
                    <td>{formatMs(row.last_used_at_ms)}</td>
                    <td>
                      {row.active ? (
                        <button
                          className="btn ghost btn-tight"
                          type="button"
                          onClick={() => void onRevokeWebhookKey(row.key_id)}
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
    </div>
  );
}
