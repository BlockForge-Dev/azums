"use client";

import { useEffect, useMemo, useState } from "react";
import {
  Button,
  Card,
  Input,
  Select,
  Textarea,
} from "@/components/ui";
import { formatMs } from "@/lib/client-api";
import {
  createPolicyBundle,
  listAgents,
  listEnvironments,
  listPolicyBundles,
  listPolicyTemplates,
  publishPolicyBundle,
  simulatePolicyBundle,
  type PolicyRuleDefinition,
  type PolicySimulationResult,
  type PolicyTemplateDefinition,
  type SessionRecord,
  type TenantAgentRecord,
  type TenantEnvironmentRecord,
  type TenantPolicyBundleRecord,
} from "@/lib/app-state";

const DEFAULT_RULES_JSON = "[]";

const DEFAULT_SCENARIOS = {
  refund: {
    intent_type: "refund" as const,
    adapter_type: "payment_processor",
    requested_scope: "payments",
    reason: "customer refund review",
    payload: JSON.stringify(
      {
        payment_reference: "pay_ref_123",
        amount: 5000,
        currency: "USD",
        destination_reference: "cust_123",
      },
      null,
      2
    ),
  },
  transfer: {
    intent_type: "transfer" as const,
    adapter_type: "adapter_solana",
    requested_scope: "payments,playground",
    reason: "devnet transfer check",
    payload: JSON.stringify(
      {
        to_addr: "11111111111111111111111111111111",
        amount: 1,
        asset: "SOL",
      },
      null,
      2
    ),
  },
  invoice: {
    intent_type: "generate_invoice" as const,
    adapter_type: "billing_adapter",
    requested_scope: "billing",
    reason: "invoice issuance check",
    payload: JSON.stringify(
      {
        customer_reference: "cust_123",
        amount: 2500,
        currency: "USD",
        description: "March service invoice",
      },
      null,
      2
    ),
  },
};

type Props = {
  canManage: boolean;
  session: SessionRecord | null;
};

function parseRulesJson(input: string): PolicyRuleDefinition[] {
  const parsed = JSON.parse(input) as unknown;
  if (!Array.isArray(parsed)) {
    throw new Error("Rules JSON must be an array.");
  }
  return parsed as PolicyRuleDefinition[];
}

function parsePayloadJson(input: string): Record<string, unknown> {
  const parsed = JSON.parse(input) as unknown;
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("Simulation payload must be a JSON object.");
  }
  return parsed as Record<string, unknown>;
}

function normalizeScopeInput(input: string): string[] {
  return Array.from(
    new Set(
      input
        .split(/[,\n]/)
        .map((value) => value.trim())
        .filter(Boolean)
    )
  );
}

function statusTone(status: string): string {
  if (status === "published") {
    return "bg-primary/10 text-primary border border-primary/30";
  }
  if (status === "superseded" || status === "rolled_back") {
    return "bg-muted/40 text-muted-foreground border border-border/50";
  }
  return "bg-yellow-500/10 text-yellow-500 border border-yellow-500/30";
}

function decisionTone(effect: string): string {
  if (effect === "allow" || effect === "allow_with_reduced_scope") {
    return "bg-primary/10 text-primary border border-primary/30";
  }
  if (effect === "require_approval") {
    return "bg-yellow-500/10 text-yellow-500 border border-yellow-500/30";
  }
  return "bg-destructive/10 text-destructive border border-destructive/30";
}

export function PolicyStagingSection({ canManage, session }: Props) {
  const [templates, setTemplates] = useState<PolicyTemplateDefinition[]>([]);
  const [bundles, setBundles] = useState<TenantPolicyBundleRecord[]>([]);
  const [environments, setEnvironments] = useState<TenantEnvironmentRecord[]>([]);
  const [agents, setAgents] = useState<TenantAgentRecord[]>([]);

  const [bundleId, setBundleId] = useState("finance-reviewed-draft");
  const [bundleLabel, setBundleLabel] = useState("Finance Reviewed Draft");
  const [selectedTemplateIds, setSelectedTemplateIds] = useState<string[]>([]);
  const [rulesJson, setRulesJson] = useState(DEFAULT_RULES_JSON);

  const [selectedSimulationBundle, setSelectedSimulationBundle] =
    useState("__published__");
  const [selectedEnvironmentId, setSelectedEnvironmentId] = useState("");
  const [selectedAgentId, setSelectedAgentId] = useState("");
  const [intentType, setIntentType] = useState<"refund" | "transfer" | "generate_invoice">(
    "transfer"
  );
  const [adapterType, setAdapterType] = useState("adapter_solana");
  const [requestedScope, setRequestedScope] = useState("payments,playground");
  const [reason, setReason] = useState("devnet transfer check");
  const [submittedBy, setSubmittedBy] = useState(session?.email ?? "planner@workspace");
  const [payloadJson, setPayloadJson] = useState(DEFAULT_SCENARIOS.transfer.payload);

  const [result, setResult] = useState<PolicySimulationResult | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [simulating, setSimulating] = useState(false);

  const publishedBundle = useMemo(
    () => bundles.find((bundle) => bundle.status === "published") ?? null,
    [bundles]
  );

  const visibleAgents = useMemo(() => {
    if (!selectedEnvironmentId) return agents;
    return agents.filter((agent) => agent.environment_id === selectedEnvironmentId);
  }, [agents, selectedEnvironmentId]);

  useEffect(() => {
    setSubmittedBy(session?.email ?? "planner@workspace");
  }, [session?.email]);

  useEffect(() => {
    let cancelled = false;

    async function load() {
      setLoading(true);
      setError(null);
      try {
        const [nextTemplates, nextBundles, nextEnvironments, nextAgents] =
          await Promise.all([
            listPolicyTemplates(),
            listPolicyBundles(),
            listEnvironments(),
            listAgents(),
          ]);
        if (cancelled) return;
        setTemplates(nextTemplates);
        setBundles(nextBundles);
        setEnvironments(nextEnvironments);
        setAgents(nextAgents);
        if (!selectedEnvironmentId && nextEnvironments[0]) {
          setSelectedEnvironmentId(nextEnvironments[0].environment_id);
        }
      } catch (loadError: unknown) {
        if (cancelled) return;
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!visibleAgents.length) {
      setSelectedAgentId("");
      return;
    }
    if (!visibleAgents.some((agent) => agent.agent_id === selectedAgentId)) {
      setSelectedAgentId(visibleAgents[0].agent_id);
    }
  }, [visibleAgents, selectedAgentId]);

  useEffect(() => {
    if (
      selectedSimulationBundle === "__published__" &&
      !publishedBundle &&
      bundles.length > 0
    ) {
      setSelectedSimulationBundle(bundles[0].bundle_id);
    }
  }, [bundles, publishedBundle, selectedSimulationBundle]);

  function toggleTemplate(templateId: string) {
    setSelectedTemplateIds((current) =>
      current.includes(templateId)
        ? current.filter((value) => value !== templateId)
        : [...current, templateId]
    );
  }

  function loadScenario(kind: keyof typeof DEFAULT_SCENARIOS) {
    const scenario = DEFAULT_SCENARIOS[kind];
    setIntentType(scenario.intent_type);
    setAdapterType(scenario.adapter_type);
    setRequestedScope(scenario.requested_scope);
    setReason(scenario.reason);
    setPayloadJson(scenario.payload);
    setMessage(`Loaded ${kind.replace("_", " ")} simulation example.`);
    setError(null);
  }

  async function refreshPolicyData() {
    const [nextBundles, nextTemplates] = await Promise.all([
      listPolicyBundles(),
      listPolicyTemplates(),
    ]);
    setBundles(nextBundles);
    setTemplates(nextTemplates);
  }

  async function createDraftBundle() {
    if (!canManage) {
      setError("Only owner/admin can create policy bundles.");
      return;
    }
    try {
      setError(null);
      setMessage(null);
      setLoading(true);
      const created = await createPolicyBundle({
        bundle_id: bundleId.trim(),
        label: bundleLabel.trim(),
        template_ids: selectedTemplateIds,
        rules: parseRulesJson(rulesJson.trim() || DEFAULT_RULES_JSON),
      });
      await refreshPolicyData();
      setSelectedSimulationBundle(created.bundle_id);
      setMessage(`Draft bundle created: ${created.bundle_id}`);
    } catch (createError: unknown) {
      setError(createError instanceof Error ? createError.message : String(createError));
    } finally {
      setLoading(false);
    }
  }

  async function publishBundle(bundle: TenantPolicyBundleRecord) {
    if (!canManage) {
      setError("Only owner/admin can publish policy bundles.");
      return;
    }
    if (!window.confirm(`Publish policy bundle ${bundle.bundle_id}?`)) {
      return;
    }
    try {
      setError(null);
      setMessage(null);
      setLoading(true);
      const published = await publishPolicyBundle(bundle.bundle_id);
      await refreshPolicyData();
      setSelectedSimulationBundle("__published__");
      setMessage(`Published policy bundle v${published.version}: ${published.bundle_id}`);
    } catch (publishError: unknown) {
      setError(publishError instanceof Error ? publishError.message : String(publishError));
    } finally {
      setLoading(false);
    }
  }

  async function runSimulation() {
    if (!canManage) {
      setError("Only owner/admin can simulate policy bundles.");
      return;
    }
    if (!selectedEnvironmentId || !selectedAgentId) {
      setError("Pick a registered environment and agent before simulating.");
      return;
    }
    try {
      setError(null);
      setMessage(null);
      setSimulating(true);
      const simulation = await simulatePolicyBundle({
        bundle_id:
          selectedSimulationBundle === "__published__"
            ? undefined
            : selectedSimulationBundle,
        action: {
          agent_id: selectedAgentId,
          environment_id: selectedEnvironmentId,
          intent_type: intentType,
          adapter_type: adapterType.trim(),
          payload: parsePayloadJson(payloadJson),
          requested_scope: normalizeScopeInput(requestedScope),
          reason: reason.trim(),
          submitted_by: submittedBy.trim(),
        },
      });
      setResult(simulation);
      setMessage("Policy simulation completed.");
    } catch (simulationError: unknown) {
      setResult(null);
      setError(
        simulationError instanceof Error
          ? simulationError.message
          : String(simulationError)
      );
    } finally {
      setSimulating(false);
    }
  }

  return (
    <div className="space-y-6">
      <Card className="bg-card rounded-xl border border-border/50 p-6">
        <div className="flex items-start justify-between gap-4 mb-4">
          <div>
            <h3 className="text-lg font-semibold text-foreground">
              Policy staging and simulation
            </h3>
            <p className="text-sm text-muted-foreground mt-1">
              Stage draft bundles, simulate decisions with a real tenant agent, and
              publish only when the trace matches your intent.
            </p>
          </div>
          <div className="text-right">
            <span className="text-xs uppercase tracking-wide text-muted-foreground block">
              Live bundle
            </span>
            <strong className="text-sm text-foreground">
              {publishedBundle ? `${publishedBundle.bundle_id} v${publishedBundle.version}` : "none"}
            </strong>
          </div>
        </div>

        {error ? (
          <div className="bg-destructive/10 border border-destructive/30 rounded-xl p-4 text-destructive mb-4">
            {error}
          </div>
        ) : null}
        {message ? (
          <div className="bg-primary/10 border border-primary/30 rounded-xl p-4 text-primary mb-4">
            {message}
          </div>
        ) : null}

        <div className="grid grid-cols-1 xl:grid-cols-[1.1fr_0.9fr] gap-6">
          <div className="space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <Input
                label="Draft bundle ID"
                value={bundleId}
                onChange={(event) => setBundleId(event.target.value)}
                disabled={!canManage || loading}
              />
              <Input
                label="Draft label"
                value={bundleLabel}
                onChange={(event) => setBundleLabel(event.target.value)}
                disabled={!canManage || loading}
              />
            </div>

            <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
              <div className="flex items-center justify-between gap-3 mb-3">
                <div>
                  <h4 className="text-sm font-semibold text-foreground">
                    Azums templates
                  </h4>
                  <p className="text-xs text-muted-foreground">
                    Templates are reusable rulesets. Draft bundles can combine template
                    IDs with custom rules.
                  </p>
                </div>
                {loading ? (
                  <span className="text-xs text-muted-foreground">Loading…</span>
                ) : null}
              </div>
              <div className="space-y-3">
                {templates.map((template) => (
                  <label
                    key={template.template_id}
                    className="flex items-start gap-3 rounded-lg border border-border/40 p-3 bg-background/40"
                  >
                    <input
                      type="checkbox"
                      checked={selectedTemplateIds.includes(template.template_id)}
                      onChange={() => toggleTemplate(template.template_id)}
                      disabled={!canManage || loading}
                      className="mt-1 w-4 h-4 rounded border-border"
                    />
                    <div>
                      <div className="font-medium text-foreground">
                        {template.display_name}
                      </div>
                      <div className="text-xs text-muted-foreground mt-1">
                        {template.description}
                      </div>
                      <div className="text-[11px] text-muted-foreground mt-1 font-mono">
                        {template.template_id}
                      </div>
                    </div>
                  </label>
                ))}
                {!templates.length && !loading ? (
                  <p className="text-sm text-muted-foreground">
                    No policy templates available.
                  </p>
                ) : null}
              </div>
            </div>

            <Textarea
              label="Custom rules JSON"
              value={rulesJson}
              onChange={(event) => setRulesJson(event.target.value)}
              disabled={!canManage || loading}
              hint="JSON array of policy rules. Leave [] if you only want to use template IDs."
              className="min-h-[180px]"
            />

            <div className="flex gap-3">
              <Button
                variant="primary"
                onClick={() => void createDraftBundle()}
                disabled={!canManage}
                isLoading={loading}
              >
                Save draft bundle
              </Button>
            </div>
          </div>

          <div className="rounded-xl border border-border/50 bg-muted/20 p-4">
            <div className="flex items-center justify-between gap-3 mb-3">
              <div>
                <h4 className="text-sm font-semibold text-foreground">
                  Bundles on this workspace
                </h4>
                <p className="text-xs text-muted-foreground">
                  Only a published bundle affects live traffic. Drafts are simulation-only.
                </p>
              </div>
            </div>
            <div className="space-y-3">
              {bundles.map((bundle) => (
                <div
                  key={bundle.bundle_id}
                  className="rounded-lg border border-border/40 bg-background/50 p-4"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <div className="flex items-center gap-2 flex-wrap">
                        <strong className="text-sm text-foreground">
                          {bundle.label}
                        </strong>
                        <span
                          className={`px-2 py-0.5 rounded-full text-[11px] ${statusTone(
                            bundle.status
                          )}`}
                        >
                          {bundle.status}
                        </span>
                      </div>
                      <div className="text-xs text-muted-foreground mt-1 font-mono">
                        {bundle.bundle_id} • v{bundle.version}
                      </div>
                      <div className="text-xs text-muted-foreground mt-2">
                        Templates: {bundle.template_ids.length} • Custom rules:{" "}
                        {bundle.rules.length}
                      </div>
                      <div className="text-xs text-muted-foreground mt-1">
                        Created {formatMs(bundle.created_at_ms)}
                        {bundle.published_at_ms
                          ? ` • Published ${formatMs(bundle.published_at_ms)}`
                          : ""}
                      </div>
                    </div>
                    <div className="flex flex-col gap-2">
                      <Button
                        variant="ghost"
                        size="small"
                        onClick={() => setSelectedSimulationBundle(bundle.bundle_id)}
                      >
                        Simulate this
                      </Button>
                      {bundle.status !== "published" ? (
                        <Button
                          variant="primary"
                          size="small"
                          disabled={!canManage || loading}
                          onClick={() => void publishBundle(bundle)}
                        >
                          Publish
                        </Button>
                      ) : null}
                    </div>
                  </div>
                </div>
              ))}
              {!bundles.length && !loading ? (
                <p className="text-sm text-muted-foreground">
                  No policy bundles yet. Save a draft before simulating it.
                </p>
              ) : null}
            </div>
          </div>
        </div>
      </Card>

      <Card className="bg-card rounded-xl border border-border/50 p-6">
        <div className="flex items-start justify-between gap-4 mb-4">
          <div>
            <h3 className="text-lg font-semibold text-foreground">
              Pre-publish simulation
            </h3>
            <p className="text-sm text-muted-foreground mt-1">
              Run policy decisions against the published bundle or a staged draft.
              Simulations never submit live execution.
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button variant="ghost" size="small" onClick={() => loadScenario("refund")}>
              Refund example
            </Button>
            <Button variant="ghost" size="small" onClick={() => loadScenario("transfer")}>
              Transfer example
            </Button>
            <Button variant="ghost" size="small" onClick={() => loadScenario("invoice")}>
              Invoice example
            </Button>
          </div>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          <Select
            label="Bundle to simulate"
            value={selectedSimulationBundle}
            onChange={(event) => setSelectedSimulationBundle(event.target.value)}
            options={[
              ...(publishedBundle
                ? [{ value: "__published__", label: `Published • ${publishedBundle.bundle_id}` }]
                : []),
              ...bundles.map((bundle) => ({
                value: bundle.bundle_id,
                label: `${bundle.label} • ${bundle.status} • v${bundle.version}`,
              })),
            ]}
            disabled={!canManage || (!publishedBundle && !bundles.length)}
          />
          <Select
            label="Environment"
            value={selectedEnvironmentId}
            onChange={(event) => setSelectedEnvironmentId(event.target.value)}
            options={environments.map((environment) => ({
              value: environment.environment_id,
              label: `${environment.name} • ${environment.environment_kind}`,
            }))}
            disabled={!canManage || !environments.length}
          />
          <Select
            label="Agent"
            value={selectedAgentId}
            onChange={(event) => setSelectedAgentId(event.target.value)}
            options={visibleAgents.map((agent) => ({
              value: agent.agent_id,
              label: `${agent.name} • trust:${agent.trust_tier} • risk:${agent.risk_tier}`,
            }))}
            disabled={!canManage || !visibleAgents.length}
          />
          <Select
            label="Intent type"
            value={intentType}
            onChange={(event) =>
              setIntentType(
                event.target.value as "refund" | "transfer" | "generate_invoice"
              )
            }
            options={[
              { value: "refund", label: "refund" },
              { value: "transfer", label: "transfer" },
              { value: "generate_invoice", label: "generate_invoice" },
            ]}
            disabled={!canManage}
          />
          <Input
            label="Adapter type"
            value={adapterType}
            onChange={(event) => setAdapterType(event.target.value)}
            disabled={!canManage}
          />
          <Input
            label="Submitted by"
            value={submittedBy}
            onChange={(event) => setSubmittedBy(event.target.value)}
            disabled={!canManage}
          />
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-4">
          <Input
            label="Requested scope"
            value={requestedScope}
            onChange={(event) => setRequestedScope(event.target.value)}
            hint="Comma-separated scope list."
            disabled={!canManage}
          />
          <Input
            label="Reason"
            value={reason}
            onChange={(event) => setReason(event.target.value)}
            disabled={!canManage}
          />
        </div>

        <div className="mt-4">
          <Textarea
            label="Simulation payload JSON"
            value={payloadJson}
            onChange={(event) => setPayloadJson(event.target.value)}
            disabled={!canManage}
            className="min-h-[220px]"
          />
        </div>

        <div className="mt-4 flex gap-3">
          <Button
            variant="primary"
            onClick={() => void runSimulation()}
            disabled={!canManage}
            isLoading={simulating}
          >
            Run simulation
          </Button>
        </div>
      </Card>

      {result ? (
        <Card className="bg-card rounded-xl border border-border/50 p-6">
          <div className="flex items-start justify-between gap-4 mb-4">
            <div>
              <h3 className="text-lg font-semibold text-foreground">
                Simulation result
              </h3>
              <p className="text-sm text-muted-foreground mt-1">
                Decision trace from the live evaluator. No execution was submitted.
              </p>
            </div>
            <span
              className={`px-3 py-1 rounded-full text-xs font-semibold ${decisionTone(
                result.decision.final_effect
              )}`}
            >
              {result.decision.final_effect}
            </span>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-4 mb-6">
            <div>
              <span className="text-xs uppercase tracking-wide text-muted-foreground">
                Bundle
              </span>
              <strong className="block text-sm text-foreground mt-1">
                {result.bundle
                  ? `${result.bundle.bundle_id} • ${result.bundle.status}`
                  : "published"}
              </strong>
            </div>
            <div>
              <span className="text-xs uppercase tracking-wide text-muted-foreground">
                Agent
              </span>
              <strong className="block text-sm text-foreground mt-1">
                {result.resolved_agent.name}
              </strong>
              <span className="text-xs text-muted-foreground">
                trust:{result.resolved_agent.trust_tier} • risk:
                {result.resolved_agent.risk_tier}
              </span>
            </div>
            <div>
              <span className="text-xs uppercase tracking-wide text-muted-foreground">
                Environment
              </span>
              <strong className="block text-sm text-foreground mt-1">
                {result.environment.name}
              </strong>
              <span className="text-xs text-muted-foreground">
                {result.environment.environment_kind}
              </span>
            </div>
            <div>
              <span className="text-xs uppercase tracking-wide text-muted-foreground">
                Effective scope
              </span>
              <strong className="block text-sm text-foreground mt-1">
                {result.decision.effective_scope.join(", ") || "-"}
              </strong>
            </div>
            <div>
              <span className="text-xs uppercase tracking-wide text-muted-foreground">
                Execution mode
              </span>
              <strong className="block text-sm text-foreground mt-1">
                {result.execution_mode}
              </strong>
              <span className="text-xs text-muted-foreground">
                owner:{result.execution_owner}
              </span>
            </div>
          </div>

          <div className="rounded-xl border border-border/50 bg-muted/20 p-4 mb-4">
            <h4 className="text-sm font-semibold text-foreground">
              Decision summary
            </h4>
            <p className="text-sm text-muted-foreground mt-2">
              {result.decision.explanation}
            </p>
            <div className="text-xs text-muted-foreground mt-2">
              Obligations:{" "}
              {result.decision.obligations.notify.length
                ? `notify ${result.decision.obligations.notify.join(", ")}`
                : "none"}
              {result.decision.obligations.dual_approval ? " • dual approval" : ""}
              {result.decision.obligations.reason_required ? " • reason required" : ""}
            </div>
          </div>

          <div className="grid grid-cols-1 xl:grid-cols-2 gap-6">
            <div>
              <h4 className="text-sm font-semibold text-foreground mb-3">
                Matched rules
              </h4>
              <div className="space-y-3">
                {result.decision.matched_rules.map((rule) => (
                  <div
                    key={`${rule.layer}:${rule.source_id}:${rule.rule_id}`}
                    className="rounded-lg border border-border/40 bg-background/50 p-3"
                  >
                    <div className="flex items-center justify-between gap-3">
                      <strong className="text-sm text-foreground">
                        {rule.rule_id}
                      </strong>
                      <span className="text-xs text-muted-foreground">
                        {rule.layer}
                      </span>
                    </div>
                    <p className="text-xs text-muted-foreground mt-1">
                      {rule.description}
                    </p>
                    <div className="text-[11px] text-muted-foreground mt-2 font-mono">
                      {rule.source_id} • {rule.effect}
                    </div>
                  </div>
                ))}
                {!result.decision.matched_rules.length ? (
                  <p className="text-sm text-muted-foreground">
                    No rules matched. The final decision came from the default deny path.
                  </p>
                ) : null}
              </div>
            </div>

            <div>
              <h4 className="text-sm font-semibold text-foreground mb-3">
                Decision trace
              </h4>
              <div className="space-y-3">
                {result.decision.decision_trace.map((entry, index) => (
                  <div
                    key={`${entry.stage}:${entry.source_id}:${entry.rule_id ?? index}`}
                    className="rounded-lg border border-border/40 bg-background/50 p-3"
                  >
                    <div className="flex items-center justify-between gap-3">
                      <strong className="text-sm text-foreground">
                        {entry.stage}
                      </strong>
                      <span className="text-xs text-muted-foreground">
                        {entry.effect ?? entry.layer}
                      </span>
                    </div>
                    <p className="text-xs text-muted-foreground mt-1">
                      {entry.message}
                    </p>
                    <div className="text-[11px] text-muted-foreground mt-2 font-mono">
                      {entry.source_id}
                      {entry.rule_id ? ` • ${entry.rule_id}` : ""}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </Card>
      ) : null}
    </div>
  );
}
