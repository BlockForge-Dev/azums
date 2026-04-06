"use client";

import Link from "next/link";
import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  approveRequest,
  createAgent,
  createEnvironment,
  escalateRequest,
  listAgents,
  listApprovals,
  listConnectorBindings,
  listEnvironments,
  listPolicyBundles,
  rejectRequest,
  type ApprovalRequestRecord,
  type ConnectorBindingRecord,
  type SessionRecord,
  type TenantAgentRecord,
  type TenantEnvironmentRecord,
  type TenantPolicyBundleRecord,
} from "@/lib/app-state";
import { apiGet, formatMs } from "@/lib/client-api";
import type { ExceptionCaseRecord, ExceptionIndexResponse } from "@/lib/types";
import { Button, Card, Input, Select } from "@/components/ui";

function toneClass(value: string): string {
  const lowered = value.toLowerCase();
  if (["approved", "active", "published", "matched"].some((token) => lowered.includes(token))) {
    return "bg-primary/10 text-primary border-primary/30";
  }
  if (["rejected", "denied", "critical", "revoked"].some((token) => lowered.includes(token))) {
    return "bg-destructive/10 text-destructive border-destructive/30";
  }
  if (["pending", "warning", "draft", "escalated"].some((token) => lowered.includes(token))) {
    return "bg-yellow-500/10 text-yellow-600 border-yellow-500/30";
  }
  return "bg-muted/60 text-muted-foreground border-border/50";
}

function Pill({ value }: { value: string }) {
  return (
    <span className={`inline-flex rounded-full border px-2 py-0.5 text-xs font-medium ${toneClass(value)}`}>
      {value}
    </span>
  );
}

function shortId(value: string): string {
  return value.length <= 18 ? value : `${value.slice(0, 8)}...${value.slice(-6)}`;
}

export function AiTrustControlsSection({
  canManage,
  session,
}: {
  canManage: boolean;
  session: SessionRecord | null;
}) {
  const [open, setOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [environments, setEnvironments] = useState<TenantEnvironmentRecord[]>([]);
  const [agents, setAgents] = useState<TenantAgentRecord[]>([]);
  const [bundles, setBundles] = useState<TenantPolicyBundleRecord[]>([]);
  const [approvals, setApprovals] = useState<ApprovalRequestRecord[]>([]);
  const [exceptions, setExceptions] = useState<ExceptionCaseRecord[]>([]);
  const [connectorBindings, setConnectorBindings] = useState<ConnectorBindingRecord[]>([]);

  const [selectedEnvironmentId, setSelectedEnvironmentId] = useState("");
  const [showEnvironmentForm, setShowEnvironmentForm] = useState(false);
  const [showAgentForm, setShowAgentForm] = useState(false);
  const [environmentId, setEnvironmentId] = useState("devnet");
  const [environmentName, setEnvironmentName] = useState("Devnet");
  const [environmentKind, setEnvironmentKind] = useState("staging");
  const [agentId, setAgentId] = useState("agent_assistant");
  const [agentName, setAgentName] = useState("Customer Assistant");
  const [runtimeType, setRuntimeType] = useState("openai");
  const [runtimeIdentity, setRuntimeIdentity] = useState("assistant_primary");

  const publishedBundle = useMemo(
    () => bundles.find((bundle) => bundle.status === "published") ?? null,
    [bundles]
  );
  const pendingApprovals = approvals.filter((approval) => approval.status === "pending");
  const openExceptions = exceptions.filter(
    (exceptionCase) => !["resolved", "dismissed", "false_positive"].includes(exceptionCase.state)
  );

  async function loadData() {
    if (!canManage) return;
    setLoading(true);
    setError(null);
    try {
      const [nextEnvironments, nextAgents, nextBundles, nextApprovals, nextExceptions] =
        await Promise.all([
          listEnvironments(true, 100),
          listAgents({ include_inactive: true, limit: 100 }),
          listPolicyBundles(50),
          listApprovals({ limit: 20 }),
          apiGet<ExceptionIndexResponse>("status/exceptions?include_terminal=false&limit=20"),
        ]);
      const environmentId =
        selectedEnvironmentId ||
        nextEnvironments.find((environment) => environment.status === "active")?.environment_id ||
        nextEnvironments[0]?.environment_id ||
        "";
      setEnvironments(nextEnvironments);
      setAgents(nextAgents);
      setBundles(nextBundles);
      setApprovals(nextApprovals);
      setExceptions(nextExceptions.cases ?? []);
      setSelectedEnvironmentId(environmentId);
      if (environmentId) {
        setConnectorBindings(
          await listConnectorBindings({
            environment_id: environmentId,
            include_inactive: true,
            limit: 20,
          })
        );
      } else {
        setConnectorBindings([]);
      }
    } catch (loadError: unknown) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    if (!open || !canManage) return;
    void loadData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, canManage]);

  useEffect(() => {
    if (!open || !canManage || !selectedEnvironmentId) return;
    void listConnectorBindings({
      environment_id: selectedEnvironmentId,
      include_inactive: true,
      limit: 20,
    })
      .then((bindings) => setConnectorBindings(bindings))
      .catch(() => setConnectorBindings([]));
  }, [open, canManage, selectedEnvironmentId]);

  async function handleCreateEnvironment(event: FormEvent) {
    event.preventDefault();
    try {
      const created = await createEnvironment({
        environment_id: environmentId,
        name: environmentName,
        environment_kind: environmentKind,
        status: "active",
      });
      setEnvironments((current) => [created, ...current.filter((item) => item.environment_id !== created.environment_id)]);
      setSelectedEnvironmentId(created.environment_id);
      setShowEnvironmentForm(false);
      setMessage(`Environment ${created.environment_id} registered.`);
    } catch (submitError: unknown) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    }
  }

  async function handleCreateAgent(event: FormEvent) {
    event.preventDefault();
    try {
      const created = await createAgent({
        agent_id: agentId,
        environment_id: selectedEnvironmentId,
        name: agentName,
        runtime_type: runtimeType,
        runtime_identity: runtimeIdentity,
        status: "active",
      });
      setAgents((current) => [created, ...current.filter((item) => item.agent_id !== created.agent_id)]);
      setShowAgentForm(false);
      setMessage(`Agent ${created.agent_id} registered.`);
    } catch (submitError: unknown) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    }
  }

  async function handleApprovalAction(
    approvalRequestId: string,
    action: "approve" | "reject" | "escalate"
  ) {
    try {
      if (action === "approve") await approveRequest(approvalRequestId);
      if (action === "reject") await rejectRequest(approvalRequestId);
      if (action === "escalate") await escalateRequest(approvalRequestId);
      setApprovals(await listApprovals({ limit: 20 }));
      setMessage(`Approval ${approvalRequestId} ${action}d.`);
    } catch (submitError: unknown) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    }
  }

  return (
    <div id="ai-trust-controls">
      <Card className="bg-card rounded-xl border border-border/50 p-6">
      <div className="flex flex-col gap-4 md:flex-row md:items-start md:justify-between">
        <div>
          <p className="text-sm font-medium text-primary mb-1">Optional UI</p>
          <h3 className="text-lg font-semibold text-foreground">AI and trust controls</h3>
          <p className="text-sm text-muted-foreground mt-1 max-w-3xl">
            This is the light customer surface for agent trust. Use the API if you want full control.
            Use this if API is not your thing and you want a smaller guided path.
          </p>
        </div>
        <Button type="button" variant="ghost" onClick={() => setOpen((current) => !current)}>
          {open ? "Hide controls" : "Show controls"}
        </Button>
      </div>

        {open ? (
          <div className="space-y-6 mt-6">
          {error ? <div className="bg-destructive/10 border border-destructive/30 rounded-xl p-4 text-destructive">{error}</div> : null}
          {message ? <div className="bg-primary/10 border border-primary/30 rounded-xl p-4 text-primary">{message}</div> : null}

          <div className="grid grid-cols-1 xl:grid-cols-2 gap-6">
            <Card className="bg-muted/20 border border-border/50 p-5">
              <h4 className="text-base font-semibold text-foreground mb-2">Requests and receipts</h4>
              <p className="text-sm text-muted-foreground mb-4">
                Execution and receipt pages stay primary. This panel only adds trust-side controls around them.
              </p>
              <div className="flex flex-wrap gap-3">
                <Link href="/app/requests"><Button type="button" size="small">Open requests</Button></Link>
                <Link href="/app/receipts"><Button type="button" variant="ghost" size="small">Open receipts</Button></Link>
                <Link href="/app/team"><Button type="button" variant="ghost" size="small">Team and approvers</Button></Link>
              </div>
            </Card>

            <Card className="bg-muted/20 border border-border/50 p-5">
              <h4 className="text-base font-semibold text-foreground mb-2">Policies</h4>
              <p className="text-sm text-muted-foreground mb-4">
                Policy authoring stays in the staging section below. This keeps the published posture visible.
              </p>
              <div className="grid grid-cols-2 gap-4 mb-4">
                <div><span className="text-xs text-muted-foreground">Bundles</span><strong className="block text-xl text-foreground">{bundles.length}</strong></div>
                <div><span className="text-xs text-muted-foreground">Pending approvals</span><strong className="block text-xl text-foreground">{pendingApprovals.length}</strong></div>
              </div>
              {publishedBundle ? (
                <div className="rounded-xl border border-border/50 bg-card/60 p-3">
                  <div className="flex items-center justify-between gap-3">
                    <strong className="text-foreground">{publishedBundle.label}</strong>
                    <Pill value={publishedBundle.status} />
                  </div>
                  <p className="text-xs text-muted-foreground mt-2">
                    {publishedBundle.bundle_id} · published {formatMs(publishedBundle.published_at_ms ?? null)}
                  </p>
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">No published policy bundle yet.</p>
              )}
              <div className="mt-4">
                <a href="#policy-staging"><Button type="button" variant="ghost" size="small">Jump to policy staging</Button></a>
              </div>
            </Card>

            <Card className="bg-muted/20 border border-border/50 p-5">
              <div className="flex items-center justify-between gap-3 mb-4">
                <div>
                  <h4 className="text-base font-semibold text-foreground">Agents</h4>
                  <p className="text-sm text-muted-foreground">Register a runtime identity without leaving the workspace.</p>
                </div>
                {canManage ? (
                  <Button type="button" variant="ghost" size="small" onClick={() => setShowAgentForm((current) => !current)} disabled={!selectedEnvironmentId}>
                    {showAgentForm ? "Hide form" : "Register agent"}
                  </Button>
                ) : null}
              </div>
              <div className="flex flex-wrap gap-3 mb-4">
                <Select
                  label="Environment"
                  value={selectedEnvironmentId}
                  onChange={(event) => setSelectedEnvironmentId(event.target.value)}
                  options={environments.length ? environments.map((environment) => ({
                    value: environment.environment_id,
                    label: `${environment.name} (${environment.environment_kind})`,
                  })) : [{ value: "", label: "No environment registered" }]}
                  disabled={!canManage || environments.length === 0}
                />
                {canManage ? (
                  <div className="self-end">
                    <Button type="button" variant="ghost" size="small" onClick={() => setShowEnvironmentForm((current) => !current)}>
                      {showEnvironmentForm ? "Hide environment" : "Add environment"}
                    </Button>
                  </div>
                ) : null}
              </div>
              {showEnvironmentForm ? (
                <form className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-4" onSubmit={(event) => void handleCreateEnvironment(event)}>
                  <Input label="Environment ID" value={environmentId} onChange={(event) => setEnvironmentId(event.target.value)} />
                  <Input label="Name" value={environmentName} onChange={(event) => setEnvironmentName(event.target.value)} />
                  <Select
                    label="Kind"
                    value={environmentKind}
                    onChange={(event) => setEnvironmentKind(event.target.value)}
                    options={[{ value: "sandbox", label: "sandbox" }, { value: "staging", label: "staging" }, { value: "production", label: "production" }]}
                  />
                  <div className="md:col-span-3"><Button type="submit" size="small">Save environment</Button></div>
                </form>
              ) : null}
              {showAgentForm ? (
                <form className="grid grid-cols-1 md:grid-cols-2 gap-3 mb-4" onSubmit={(event) => void handleCreateAgent(event)}>
                  <Input label="Agent ID" value={agentId} onChange={(event) => setAgentId(event.target.value)} />
                  <Input label="Agent name" value={agentName} onChange={(event) => setAgentName(event.target.value)} />
                  <Input label="Runtime type" value={runtimeType} onChange={(event) => setRuntimeType(event.target.value)} />
                  <Input label="Runtime identity" value={runtimeIdentity} onChange={(event) => setRuntimeIdentity(event.target.value)} />
                  <div className="md:col-span-2"><Button type="submit" size="small" disabled={!selectedEnvironmentId}>Save agent</Button></div>
                </form>
              ) : null}
              <div className="space-y-3">
                {agents.slice(0, 5).map((agent) => (
                  <div key={agent.agent_id} className="rounded-xl border border-border/50 bg-card/60 p-3">
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <strong className="text-foreground">{agent.name}</strong>
                        <p className="text-xs text-muted-foreground mt-1">{agent.agent_id} · {agent.runtime_type}:{agent.runtime_identity}</p>
                      </div>
                      <Pill value={agent.status} />
                    </div>
                  </div>
                ))}
                {agents.length === 0 ? <p className="text-sm text-muted-foreground">No agents registered yet.</p> : null}
              </div>
            </Card>

            <Card className="bg-muted/20 border border-border/50 p-5">
              <h4 className="text-base font-semibold text-foreground mb-2">Approvals</h4>
              <p className="text-sm text-muted-foreground mb-4">
                Review exact approval requests here. Approver membership itself still comes from Team roles.
              </p>
              <div className="space-y-3">
                {approvals.slice(0, 5).map((approval) => (
                  <div key={approval.approval_request_id} className="rounded-xl border border-border/50 bg-card/60 p-3">
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <div>
                        <strong className="text-foreground">{approval.intent_type}</strong>
                        <p className="text-xs text-muted-foreground mt-1">{shortId(approval.approval_request_id)} · expires {formatMs(approval.expires_at_ms)}</p>
                      </div>
                      <Pill value={approval.status} />
                    </div>
                    {approval.status === "pending" ? (
                      <div className="flex flex-wrap gap-2 mt-3">
                        <Button type="button" size="small" onClick={() => void handleApprovalAction(approval.approval_request_id, "approve")}>Approve</Button>
                        <Button type="button" variant="ghost" size="small" onClick={() => void handleApprovalAction(approval.approval_request_id, "escalate")}>Escalate</Button>
                        <Button type="button" variant="danger" size="small" onClick={() => void handleApprovalAction(approval.approval_request_id, "reject")}>Reject</Button>
                      </div>
                    ) : null}
                  </div>
                ))}
                {approvals.length === 0 ? <p className="text-sm text-muted-foreground">No approval requests yet.</p> : null}
              </div>
            </Card>

            <Card className="bg-muted/20 border border-border/50 p-5">
              <h4 className="text-base font-semibold text-foreground mb-2">Exceptions</h4>
              <p className="text-sm text-muted-foreground mb-4">
                Recent mismatches and overreach signals linked back to requests and receipts.
              </p>
              <div className="space-y-3">
                {openExceptions.slice(0, 5).map((exceptionCase) => (
                  <div key={exceptionCase.case_id} className="rounded-xl border border-border/50 bg-card/60 p-3">
                    <div className="flex items-center justify-between gap-3">
                      <strong className="text-foreground">{exceptionCase.summary}</strong>
                      <Pill value={exceptionCase.severity} />
                    </div>
                    <div className="flex flex-wrap gap-3 mt-3 text-sm">
                      <Link href={`/app/requests/${encodeURIComponent(exceptionCase.intent_id)}`} className="text-primary hover:underline">Open request</Link>
                      {exceptionCase.latest_execution_receipt_id ? (
                        <Link href={`/app/receipts/${encodeURIComponent(exceptionCase.latest_execution_receipt_id)}`} className="text-primary hover:underline">Open receipt</Link>
                      ) : null}
                    </div>
                  </div>
                ))}
                {openExceptions.length === 0 ? <p className="text-sm text-muted-foreground">No open exceptions.</p> : null}
              </div>
            </Card>

            <Card className="bg-muted/20 border border-border/50 p-5 xl:col-span-2">
              <h4 className="text-base font-semibold text-foreground mb-2">Connector bindings</h4>
              <p className="text-sm text-muted-foreground mb-4">
                View connector metadata, lifecycle, versions, and secret field names. Raw secrets stay API-first and are never rendered back into the UI.
              </p>
              <div className="space-y-3">
                {connectorBindings.slice(0, 6).map((binding) => (
                  <div key={binding.binding_id} className="rounded-xl border border-border/50 bg-card/60 p-3">
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <strong className="text-foreground">{binding.name}</strong>
                        <p className="text-xs text-muted-foreground mt-1">{binding.binding_id} · {binding.connector_type} · v{binding.current_secret_version}</p>
                      </div>
                      <Pill value={binding.status} />
                    </div>
                    <div className="flex flex-wrap gap-2 mt-3 text-xs text-muted-foreground">
                      <span>environment:{binding.environment_id}</span>
                      <span>secret fields:{binding.secret_fields.join(", ") || "none"}</span>
                      <span>rotated {formatMs(binding.rotated_at_ms)}</span>
                    </div>
                  </div>
                ))}
                {connectorBindings.length === 0 ? <p className="text-sm text-muted-foreground">No connector bindings for the selected environment.</p> : null}
              </div>
            </Card>
          </div>

          {loading ? <p className="text-sm text-muted-foreground">Loading trust controls...</p> : null}
          {session ? <p className="text-xs text-muted-foreground">Current workspace: {session.workspace_name} · tenant {session.tenant_id}</p> : null}
          </div>
        ) : null}
      </Card>
    </div>
  );
}
