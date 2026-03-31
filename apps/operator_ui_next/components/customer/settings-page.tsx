"use client";

import { FormEvent, useEffect, useState } from "react";
import {
  canManageWorkspace,
  getWorkspaceSettings,
  readSession,
  type WorkspaceExecutionPolicy,
  updateWorkspaceSettings,
} from "@/lib/app-state";
import { apiGet, formatMs } from "@/lib/client-api";
import type { UiConfigResponse } from "@/lib/types";
import { Card, CardHeader, Button, Input, Select } from "@/components/ui";
import { PolicyStagingSection } from "@/components/customer/policy-staging-section";

export function SettingsPage() {
  const [config, setConfig] = useState<UiConfigResponse | null>(null);
  const [retentionDays, setRetentionDays] = useState("30");
  const [callbackDefaultEnabled, setCallbackDefaultEnabled] = useState(true);
  const [allowReplayFromCustomerApp, setAllowReplayFromCustomerApp] = useState(true);
  const [executionPolicy, setExecutionPolicy] =
    useState<WorkspaceExecutionPolicy>("customer_signed");
  const [sponsoredCap, setSponsoredCap] = useState("10000");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [canManage, setCanManage] = useState(false);
  const [session, setSession] = useState<Awaited<ReturnType<typeof readSession>>>(null);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([apiGet<UiConfigResponse>("config"), readSession(), getWorkspaceSettings()])
      .then(([cfg, currentSession, settings]) => {
        if (cancelled) return;
        setConfig(cfg);
        setSession(currentSession);
        setCanManage(Boolean(currentSession && canManageWorkspace(currentSession.role)));
        if (settings) {
          setRetentionDays(String(settings.request_retention_days));
          setCallbackDefaultEnabled(settings.callback_default_enabled);
          setAllowReplayFromCustomerApp(settings.allow_replay_from_customer_app);
          setExecutionPolicy(settings.execution_policy);
          setSponsoredCap(String(settings.sponsored_monthly_cap_requests));
        }
      })
      .catch((loadError: unknown) => {
        if (cancelled) return;
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function saveWorkspace(event: FormEvent) {
    event.preventDefault();
    if (!canManage) {
      setError("Only owner/admin can change workspace settings.");
      return;
    }
    try {
      setError(null);
      const retentionValue = Number(retentionDays);
      if (!Number.isFinite(retentionValue) || retentionValue < 7 || retentionValue > 3650) {
        throw new Error("Request retention days must be between 7 and 3650.");
      }
      const sponsoredCapValue = Number(sponsoredCap);
      if (!Number.isFinite(sponsoredCapValue) || sponsoredCapValue < 1) {
        throw new Error("Sponsored monthly cap must be a positive number.");
      }
      const out = await updateWorkspaceSettings({
        callback_default_enabled: callbackDefaultEnabled,
        request_retention_days: retentionValue,
        allow_replay_from_customer_app: allowReplayFromCustomerApp,
        execution_policy: executionPolicy,
        sponsored_monthly_cap_requests: sponsoredCapValue,
      });
      setRetentionDays(String(out.request_retention_days));
      setCallbackDefaultEnabled(out.callback_default_enabled);
      setAllowReplayFromCustomerApp(out.allow_replay_from_customer_app);
      setExecutionPolicy(out.execution_policy);
      setSponsoredCap(String(out.sponsored_monthly_cap_requests));
      setMessage("Workspace settings saved.");
    } catch (saveError: unknown) {
      setError(saveError instanceof Error ? saveError.message : String(saveError));
    }
  }

  return (
    <div className="space-y-6">
      <section className="bg-gradient-to-br from-primary/20 via-card to-card rounded-2xl p-8 border border-primary/20">
        <p className="text-sm font-medium text-primary mb-2">Settings</p>
        <h2 className="text-2xl font-bold text-foreground mb-2">Workspace and security controls</h2>
        <p className="text-muted-foreground">Manage workspace defaults, data retention posture, and safety boundaries.</p>
      </section>

      {error ? <div className="bg-destructive/10 border border-destructive/30 rounded-xl p-4 text-destructive">{error}</div> : null}
      {message ? <div className="bg-primary/10 border border-primary/30 rounded-xl p-4 text-primary">{message}</div> : null}

      <Card className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">Workspace profile</h3>
        <form className="flex flex-wrap gap-4 items-start" onSubmit={(event) => void saveWorkspace(event)}>
          <Select
            label="Default callback mode"
            options={[
              { value: "enabled", label: "enabled" },
              { value: "disabled", label: "disabled" },
            ]}
            value={callbackDefaultEnabled ? "enabled" : "disabled"}
            onChange={(e) => setCallbackDefaultEnabled(e.target.value === "enabled")}
            disabled={!canManage}
          />
          <Input
            label="Request retention days"
            type="number"
            min={7}
            max={3650}
            value={retentionDays}
            onChange={(e) => setRetentionDays(e.target.value)}
            disabled={!canManage}
          />
          <Select
            label="Execution policy"
            options={[
              { value: "customer_signed", label: "customer_signed" },
              { value: "customer_managed_signer", label: "customer_managed_signer" },
              { value: "sponsored", label: "sponsored" },
            ]}
            value={executionPolicy}
            onChange={(e) => setExecutionPolicy(e.target.value as WorkspaceExecutionPolicy)}
            disabled={!canManage}
          />
          <Input
            label="Sponsored monthly cap (requests)"
            type="number"
            min={1}
            value={sponsoredCap}
            onChange={(e) => setSponsoredCap(e.target.value)}
            disabled={!canManage}
          />
          <label className="flex items-center gap-2 w-full col-span-2 cursor-pointer">
            <input
              type="checkbox"
              checked={allowReplayFromCustomerApp}
              onChange={(event) => setAllowReplayFromCustomerApp(event.target.checked)}
              disabled={!canManage}
              className="w-4 h-4 rounded border-border"
            />
            <span className="text-sm text-foreground">Allow replay actions from customer app (owner/admin only)</span>
          </label>
          <Button type="submit" variant="primary" disabled={!canManage} className="w-full sm:w-auto">
            Save settings
          </Button>
        </form>
      </Card>

      <Card className="bg-card rounded-xl border border-border/50 p-6">
        <h3 className="text-lg font-semibold text-foreground mb-4">Environment snapshot</h3>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <div>
            <span className="text-sm text-muted-foreground">Workspace ID</span>
            <strong className="block text-foreground font-mono text-sm">{session?.workspace_id ?? "-"}</strong>
          </div>
          <div>
            <span className="text-sm text-muted-foreground">Tenant ID</span>
            <strong className="block text-foreground font-mono text-sm">{config?.tenant_id ?? session?.tenant_id ?? "-"}</strong>
          </div>
          <div>
            <span className="text-sm text-muted-foreground">Current role</span>
            <strong className="block text-foreground">{session?.role ?? "-"}</strong>
          </div>
          <div>
            <span className="text-sm text-muted-foreground">Member since</span>
            <strong className="block text-foreground">{formatMs(session?.created_at_ms ?? null)}</strong>
          </div>
        </div>
      </Card>

      <PolicyStagingSection canManage={canManage} session={session} />

      <div className="bg-yellow-500/10 border border-yellow-500/30 rounded-xl p-6">
        <h3 className="text-lg font-semibold text-yellow-500 mb-4">Security posture</h3>
        <ul className="space-y-2 text-sm text-muted-foreground">
          <li>• Frontend reads backend state and must not invent lifecycle truth.</li>
          <li>• Replay remains restricted to owner/admin and operator surfaces.</li>
          <li>• Secrets should not be rendered in customer-facing pages.</li>
          <li>• Tenant boundary checks stay server-side, never only in UI filters.</li>
        </ul>
      </div>
    </div>
  );
}
