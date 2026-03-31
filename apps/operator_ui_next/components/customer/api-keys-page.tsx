"use client";

import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  canWriteRequests,
  createApiKey,
  listApiKeys,
  readSession,
  revokeApiKey,
  type ApiKeyRecord,
} from "@/lib/app-state";
import { apiGet, formatMs } from "@/lib/client-api";
import { EmptyState } from "@/components/ui/empty-state";
import type { IntakeAuditsResponse } from "@/lib/types";

const DEFAULT_KEY_NAME = "backend-service";

type RequestCountMap = Record<string, number>;

export function ApiKeysPage() {
  const [rows, setRows] = useState<ApiKeyRecord[]>([]);
  const [name, setName] = useState(DEFAULT_KEY_NAME);
  const [latestToken, setLatestToken] = useState<string | null>(null);

  const [sampleApiRequests, setSampleApiRequests] = useState(0);
  const [keyRequestCounts, setKeyRequestCounts] = useState<RequestCountMap>({});

  const [pageLoading, setPageLoading] = useState(false);
  const [working, setWorking] = useState(false);
  const [canManage, setCanManage] = useState(false);

  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    void loadAll();
  }, []);

  const active = useMemo(() => rows.filter((row) => row.revoked_at_ms == null).length, [rows]);

  const keysWithTraffic = useMemo(() => {
    return rows.filter((row) => (keyRequestCounts[row.id] ?? 0) > 0).length;
  }, [rows, keyRequestCounts]);

  async function loadAll() {
    setPageLoading(true);
    setError(null);

    try {
      const [session, keys, auditCounts] = await Promise.all([
        readSession(),
        listApiKeys(),
        loadRequestCounts(),
      ]);

      setCanManage(Boolean(session && canWriteRequests(session.role)));
      setRows(keys);
      setSampleApiRequests(auditCounts.sampleApiRequests);
      setKeyRequestCounts(auditCounts.keyRequestCounts);
    } catch (loadError: unknown) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setPageLoading(false);
    }
  }

  async function loadRequestCounts(): Promise<{
    sampleApiRequests: number;
    keyRequestCounts: RequestCountMap;
  }> {
    try {
      const out = await apiGet<IntakeAuditsResponse>(
        "status/tenant/intake-audits?channel=api&validation_result=accepted&limit=400&offset=0"
      );

      const counts: RequestCountMap = {};

      for (const audit of out.audits ?? []) {
        const details = audit.details_json ?? {};
        const keyId = typeof details.api_key_id === "string" ? details.api_key_id.trim() : "";

        if (keyId) {
          counts[keyId] = (counts[keyId] ?? 0) + 1;
        }
      }

      return {
        sampleApiRequests: (out.audits ?? []).length,
        keyRequestCounts: counts,
      };
    } catch {
      return {
        sampleApiRequests: 0,
        keyRequestCounts: {},
      };
    }
  }

  async function onCreateKey(event: FormEvent) {
    event.preventDefault();

    if (!canManage) {
      setError("Your role cannot create API keys.");
      return;
    }

    setWorking(true);
    setError(null);
    setMessage(null);
    setLatestToken(null);

    try {
      const created = await createApiKey(name.trim() || DEFAULT_KEY_NAME);
      setLatestToken(created.token);
      setMessage(`API key created: ${created.key.id}`);
      setName(DEFAULT_KEY_NAME);
      await loadAll();
    } catch (createError: unknown) {
      setError(createError instanceof Error ? createError.message : String(createError));
    } finally {
      setWorking(false);
    }
  }

  async function onRevokeKey(id: string) {
    if (!canManage) {
      setError("Your role cannot revoke API keys.");
      return;
    }

    if (!window.confirm("Revoke this API key? This action cannot be undone.")) {
      return;
    }

    setWorking(true);
    setError(null);
    setMessage(null);

    try {
      await revokeApiKey(id);
      setMessage(`API key revoked: ${id}`);
      await loadAll();
    } catch (revokeError: unknown) {
      setError(revokeError instanceof Error ? revokeError.message : String(revokeError));
    } finally {
      setWorking(false);
    }
  }

  return (
    <div className="space-y-6">
      <section className="bg-gradient-to-br from-primary/20 via-card to-card rounded-2xl p-8 border border-primary/20">
        <div className="flex flex-col md:flex-row md:items-center md:justify-between gap-6">
          <div>
            <p className="text-sm font-medium text-primary mb-2">API Keys</p>
            <h2 className="text-2xl font-bold text-foreground mb-2">Manage developer keys clearly.</h2>
            <p className="text-muted-foreground max-w-md">
              Create, copy once, inspect usage, and revoke workspace API keys from one page.
            </p>
          </div>

          <button 
            className="btn btn-ghost" 
            type="button" 
            onClick={() => void loadAll()}
          >
            {pageLoading ? "Refreshing..." : "Refresh"}
          </button>
        </div>
      </section>

      {error ? <section className="bg-destructive/10 border border-destructive/30 rounded-xl p-4 text-destructive">{error}</section> : null}
      {message ? <section className="bg-primary/10 border border-primary/30 rounded-xl p-4 text-primary">{message}</section> : null}

      {!canManage ? (
        <section className="bg-yellow-500/10 border border-yellow-500/30 rounded-xl p-4 text-yellow-500">
          Read-only key inventory for your role. Owner, admin, and developer roles can manage API keys.
        </section>
      ) : null}

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <Summary label="Active keys" value={String(active)} />
          <Summary label="Total keys" value={String(rows.length)} />
          <Summary label="Accepted requests" value={String(sampleApiRequests)} />
          <Summary label="Keys with traffic" value={String(keysWithTraffic)} />
        </div>
      </section>

      <section className="bg-card rounded-xl border border-border/50 p-6">
        <div className="mb-6">
          <h3 className="text-lg font-semibold text-foreground">Create key</h3>
          <p className="text-sm text-muted-foreground mt-1">Generate a key for backend or server-to-server access.</p>
        </div>

        <form className="flex flex-col sm:flex-row gap-4 items-start sm:items-end" onSubmit={(event) => void onCreateKey(event)}>
          <label className="flex-1 w-full">
            Key name
            <input
              data-testid="api-keys-name"
              className="input mt-1.5"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="service-name"
              required
              disabled={!canManage}
            />
          </label>

          <button data-testid="api-keys-create" className="btn btn-primary" type="submit" disabled={working || !canManage}>
            {working ? "Working..." : "Create key"}
          </button>
        </form>

        {latestToken ? (
          <div className="mt-4 bg-primary/10 border border-primary/30 rounded-xl p-4">
            <strong className="text-primary">Copy this key now. It is shown once.</strong>
            <pre className="mt-2 p-3 bg-card rounded-lg border border-border/50 overflow-x-auto text-sm font-mono">{latestToken}</pre>
          </div>
        ) : null}
      </section>

      <section className="bg-card rounded-xl border border-border/50 overflow-hidden">
        <div className="p-6 pb-4">
          <h3 className="text-lg font-semibold text-foreground">Key inventory</h3>
          <p className="text-sm text-muted-foreground mt-1">
            Review status, usage, and recent activity for each API key.
          </p>
        </div>

        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead className="bg-muted/50">
              <tr>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Name</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Prefix</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Scope</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Status</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Created</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Last used</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Requests</th>
                <th className="text-left px-4 py-3 font-medium text-muted-foreground">Action</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/50">
              {rows.length === 0 ? (
                <tr>
                  <td colSpan={8} className="px-4 py-6">
                    <EmptyState
                      compact
                      title="No API keys yet"
                      description="Generate your first key to start sending requests."
                    />
                  </td>
                </tr>
              ) : (
                rows.map((row) => (
                  <tr key={row.id} className="hover:bg-muted/30 transition-colors">
                    <td className="px-4 py-3 text-foreground">{row.name}</td>
                    <td className="px-4 py-3 text-foreground font-mono text-xs">
                      {row.prefix}...{row.last4}
                    </td>
                    <td className="px-4 py-3 text-foreground">workspace</td>
                    <td className="px-4 py-3">
                      <span className={row.revoked_at_ms ? "badge badge-yellow" : "badge badge-green"}>
                        {row.revoked_at_ms ? "revoked" : "active"}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-foreground">{formatMs(row.created_at_ms)}</td>
                    <td className="px-4 py-3 text-foreground">{formatMs(row.last_used_at_ms)}</td>
                    <td className="px-4 py-3 text-foreground">{keyRequestCounts[row.id] ?? 0}</td>
                    <td className="px-4 py-3">
                      {row.revoked_at_ms ? (
                        <span className="text-muted-foreground">-</span>
                      ) : (
                        <button
                          className="btn btn-ghost btn-sm"
                          type="button"
                          disabled={!canManage || working}
                          onClick={() => void onRevokeKey(row.id)}
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
    </div>
  );
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <div className="bg-muted/30 rounded-lg p-4">
      <span className="text-sm text-muted-foreground">{label}</span>
      <strong className="block text-2xl font-bold text-foreground mt-1">{value}</strong>
    </div>
  );
}
