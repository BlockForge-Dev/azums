"use client";

import { apiGet, apiRequest } from "@/lib/client-api";
import type { PlanTier } from "@/lib/plans";

export type WorkspaceRole = "owner" | "admin" | "developer" | "viewer";
export type BillingAccessMode = "free_play" | "paid";
export type WorkspaceExecutionPolicy =
  | "customer_signed"
  | "customer_managed_signer"
  | "sponsored";
export type WorkspaceCapability =
  | "write_requests"
  | "manage_workspace"
  | "view_billing"
  | "access_operator";

export interface OnboardingState {
  workspace_created: boolean;
  api_key_generated: boolean;
  submitted_request: boolean;
  viewed_receipt: boolean;
  configured_callback: boolean;
}

export interface SessionRecord {
  id: string;
  email: string;
  full_name: string;
  workspace_id: string;
  workspace_name: string;
  tenant_id: string;
  role: WorkspaceRole;
  plan: PlanTier;
  created_at_ms: number;
  email_verified_at_ms: number | null;
  onboarding: OnboardingState;
}

export interface ApiKeyRecord {
  id: string;
  name: string;
  prefix: string;
  last4: string;
  created_at_ms: number;
  revoked_at_ms: number | null;
  last_used_at_ms: number | null;
}

export interface ApiKeyCreateResult {
  key: ApiKeyRecord;
  token: string;
}

export interface WebhookKeyRecord {
  key_id: string;
  tenant_id: string;
  source: string;
  secret_last4: string;
  active: boolean;
  created_by_principal_id: string;
  created_at_ms: number;
  revoked_at_ms: number | null;
  expires_at_ms: number | null;
  last_used_at_ms: number | null;
}

export interface WebhookKeyCreateResult {
  webhook_key: {
    key_id: string;
    tenant_id: string;
    source: string;
    secret: string;
    secret_last4: string;
    created_by_principal_id: string;
    created_at_ms: number;
  };
  rotation: {
    rotated_previous_keys: number;
    previous_keys_valid_until_ms: number | null;
    grace_seconds: number;
  };
}

export type WorkspaceEnvironment = "sandbox" | "staging" | "production";

export interface WorkspaceRecord {
  workspace_id: string;
  workspace_name: string;
  tenant_id: string;
  role: WorkspaceRole;
  environment: WorkspaceEnvironment;
  is_current: boolean;
}

export interface TeamMemberRecord {
  id: string;
  email: string;
  role: WorkspaceRole;
  status: "active" | "invited";
  added_at_ms: number;
  invite_expires_at_ms?: number | null;
}

export interface InviteSummary {
  email: string;
  workspace_id: string;
  workspace_name: string;
  role: WorkspaceRole;
  expires_at_ms: number;
}

export interface BillingProfile {
  plan: PlanTier;
  access_mode: BillingAccessMode;
  billing_email: string;
  card_brand: string | null;
  card_last4: string | null;
  payment_provider: string | null;
  payment_reference: string | null;
  payment_verified_at_ms: number | null;
  payment_currency?: string | null;
  payment_amount?: number | null;
  payment_amount_usd?: number | null;
  payment_fx_rate_to_usd?: number | null;
  updated_at_ms: number;
}

export interface BillingProviderConfig {
  provider: string;
  ready: boolean;
  has_secret_key: boolean;
  has_webhook_hash: boolean;
  base_url: string;
  expected_currency: string | null;
  supported_currencies?: string[];
  webhook_path: string;
}

export interface BillingAuditEvent {
  event_id: string;
  workspace_id: string;
  actor_email: string;
  actor_role: WorkspaceRole;
  changed_at_ms: number;
  plan_before: PlanTier;
  plan_after: PlanTier;
  access_mode_before: BillingAccessMode;
  access_mode_after: BillingAccessMode;
  payment_method_updated: boolean;
}

export interface WorkspaceSettings {
  callback_default_enabled: boolean;
  request_retention_days: number;
  allow_replay_from_customer_app: boolean;
  execution_policy: WorkspaceExecutionPolicy;
  sponsored_monthly_cap_requests: number;
  updated_at_ms: number;
}

export type PolicyEffect =
  | "allow"
  | "deny"
  | "require_approval"
  | "allow_with_reduced_scope";

export interface PolicyBusinessHoursCondition {
  days_utc: string[];
  start_hour_utc: number;
  end_hour_utc: number;
}

export interface PolicyRuleConditions {
  subjects: string[];
  actions: string[];
  requested_scopes: string[];
  environments: string[];
  target_systems: string[];
  amount_gte?: number | null;
  amount_lte?: number | null;
  sensitivities: string[];
  destination_classes: string[];
  business_hours_utc?: PolicyBusinessHoursCondition | null;
  trust_tiers: string[];
  risk_tiers: string[];
}

export interface PolicyRuleObligations {
  notify: string[];
  dual_approval: boolean;
  reason_required: boolean;
}

export interface PolicyRuleDefinition {
  rule_id: string;
  description: string;
  effect: PolicyEffect;
  conditions: PolicyRuleConditions;
  obligations: PolicyRuleObligations;
  reduced_scope: string[];
}

export interface PolicyTemplateDefinition {
  template_id: string;
  display_name: string;
  description: string;
  rules: PolicyRuleDefinition[];
}

export interface TenantEnvironmentRecord {
  tenant_id: string;
  environment_id: string;
  name: string;
  environment_kind: string;
  is_production: boolean;
  status: string;
  created_by_principal_id: string;
  updated_by_principal_id: string;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface TenantAgentRecord {
  agent_id: string;
  tenant_id: string;
  environment_id: string;
  name: string;
  runtime_type: string;
  runtime_identity: string;
  status: string;
  trust_tier: string;
  risk_tier: string;
  owner_team: string;
  created_by_principal_id: string;
  updated_by_principal_id: string;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface TenantPolicyBundleRecord {
  tenant_id: string;
  bundle_id: string;
  version: number;
  label: string;
  status: string;
  template_ids: string[];
  rules: PolicyRuleDefinition[];
  created_by_principal_id: string;
  published_by_principal_id?: string | null;
  created_at_ms: number;
  published_at_ms?: number | null;
  rolled_back_from_bundle_id?: string | null;
  rollback_reason?: string | null;
}

export interface PolicyRuleMatch {
  layer: string;
  source_id: string;
  rule_id: string;
  effect: string;
  description: string;
  obligations: PolicyRuleObligations;
  reduced_scope: string[];
}

export interface PolicyDecisionTraceEntry {
  stage: string;
  layer: string;
  source_id: string;
  rule_id?: string | null;
  effect?: string | null;
  message: string;
}

export interface PolicyDecisionExplanation {
  final_effect: string;
  effective_scope: string[];
  obligations: PolicyRuleObligations;
  matched_rules: PolicyRuleMatch[];
  decision_trace: PolicyDecisionTraceEntry[];
  published_bundle_id?: string | null;
  published_bundle_version?: number | null;
  explanation: string;
}

export interface PolicySimulationResult {
  ok: boolean;
  bundle: TenantPolicyBundleRecord | null;
  decision: PolicyDecisionExplanation;
  execution_mode: string;
  execution_owner: string;
  resolved_agent: TenantAgentRecord;
  environment: TenantEnvironmentRecord;
}

export interface UsageSummary {
  workspace_id: string;
  plan: PlanTier;
  access_mode: BillingAccessMode;
  monthly_price_usd: number;
  free_play_limit: number;
  used_requests: number;
  remaining_requests: number | null;
  window_start_ms: number;
  window_end_ms: number;
  paid_unlimited: boolean;
  metering_source: string;
  metering_warning: string | null;
}

export interface InvoiceRecord {
  id: string;
  period: string;
  amount_usd: number;
  status: "paid" | "open";
  issued_at_ms: number;
}

interface SessionEnvelopeResponse {
  ok: boolean;
  authenticated: boolean;
  session: SessionRecord | null;
}

interface SessionUpdatedResponse {
  ok: boolean;
  session: SessionRecord;
}

interface SignupResponse {
  ok: boolean;
  session: SessionRecord | null;
  requires_email_verification: boolean;
  verification_sent: boolean;
}

interface ApiKeysResponse {
  ok: boolean;
  keys: ApiKeyRecord[];
}

interface ApiKeyCreateResponse {
  ok: boolean;
  key: ApiKeyRecord;
  token: string;
}

interface WebhookKeysResponse {
  ok: boolean;
  keys: WebhookKeyRecord[];
}

interface WebhookKeyCreateResponse {
  ok: boolean;
  webhook_key: WebhookKeyCreateResult["webhook_key"];
  rotation: WebhookKeyCreateResult["rotation"];
}

interface WorkspaceListResponse {
  ok: boolean;
  workspaces: WorkspaceRecord[];
}

interface TeamMembersResponse {
  ok: boolean;
  members: TeamMemberRecord[];
}

interface TeamMemberResponse {
  ok: boolean;
  member: TeamMemberRecord;
}

interface TeamMemberInviteResponse {
  ok: boolean;
  member: TeamMemberRecord;
  invite_token: string;
  invite_path: string;
  invite_expires_at_ms: number;
}

interface InviteLookupResponse {
  ok: boolean;
  invite: InviteSummary;
}

interface BillingResponse {
  ok: boolean;
  profile: BillingProfile;
}

interface BillingProviderConfigResponse {
  ok: boolean;
  flutterwave: BillingProviderConfig;
}

interface BillingAuditResponse {
  ok: boolean;
  events: BillingAuditEvent[];
}

interface WorkspaceSettingsResponse {
  ok: boolean;
  settings: WorkspaceSettings;
}

interface EnvironmentsResponse {
  ok: boolean;
  environments: TenantEnvironmentRecord[];
  limit?: number;
}

interface AgentsResponse {
  ok: boolean;
  agents: TenantAgentRecord[];
  limit?: number;
}

interface PolicyTemplatesResponse {
  ok: boolean;
  templates: PolicyTemplateDefinition[];
}

interface PolicyBundleResponse {
  ok: boolean;
  bundle: TenantPolicyBundleRecord;
}

interface PolicyBundlesResponse {
  ok: boolean;
  bundles: TenantPolicyBundleRecord[];
  limit?: number;
}

interface UsageSummaryResponse {
  ok: boolean;
  workspace_id: string;
  plan: PlanTier;
  access_mode: BillingAccessMode;
  monthly_price_usd: number;
  free_play_limit: number;
  used_requests: number;
  remaining_requests: number | null;
  window_start_ms: number;
  window_end_ms: number;
  paid_unlimited: boolean;
  metering_source?: string;
  metering_warning?: string | null;
}

interface InvoiceResponse {
  ok: boolean;
  invoices: InvoiceRecord[];
}

interface OkResponse {
  ok: boolean;
}

export async function readSession(): Promise<SessionRecord | null> {
  try {
    const out = await apiGet<SessionEnvelopeResponse>("account/session");
    if (!out.authenticated) {
      return null;
    }
    return out.session;
  } catch {
    return null;
  }
}

export async function clearSession(): Promise<void> {
  await apiRequest<OkResponse>("account/logout", {
    method: "POST",
  });
}

export async function signup(input: {
  full_name: string;
  email: string;
  password: string;
  workspace_name: string;
  plan: PlanTier;
}): Promise<{
  session: SessionRecord | null;
  requires_email_verification: boolean;
  verification_sent: boolean;
}> {
  const out = await apiRequest<SignupResponse>("account/signup", {
    method: "POST",
    body: JSON.stringify(input),
  });
  return {
    session: out.session,
    requires_email_verification: out.requires_email_verification,
    verification_sent: out.verification_sent,
  };
}

export async function login(input: {
  email: string;
  password: string;
}): Promise<SessionRecord> {
  const out = await apiRequest<SessionUpdatedResponse>("account/login", {
    method: "POST",
    body: JSON.stringify(input),
  });
  return out.session;
}

export async function markOnboardingStep(
  step: keyof OnboardingState
): Promise<SessionRecord | null> {
  try {
    const out = await apiRequest<SessionUpdatedResponse>("account/onboarding", {
      method: "POST",
      body: JSON.stringify({ step }),
    });
    return out.session;
  } catch {
    return null;
  }
}

export function onboardingProgress(session: SessionRecord): {
  completed: number;
  total: number;
  percent: number;
} {
  const all = Object.values(session.onboarding);
  const completed = all.filter(Boolean).length;
  const total = all.length;
  return {
    completed,
    total,
    percent: Math.round((completed / total) * 100),
  };
}

export async function listApiKeys(): Promise<ApiKeyRecord[]> {
  const out = await apiGet<ApiKeysResponse>("account/api-keys");
  return out.keys ?? [];
}

export async function createApiKey(name: string): Promise<ApiKeyCreateResult> {
  const out = await apiRequest<ApiKeyCreateResponse>("account/api-keys", {
    method: "POST",
    body: JSON.stringify({
      name,
    }),
  });
  return {
    key: out.key,
    token: out.token,
  };
}

export function canWriteRequests(role: WorkspaceRole): boolean {
  return role === "owner" || role === "admin" || role === "developer";
}

export function canManageWorkspace(role: WorkspaceRole): boolean {
  return role === "owner" || role === "admin";
}

export function canAccessOperator(role: WorkspaceRole): boolean {
  return role === "owner" || role === "admin";
}

export function canViewBilling(role: WorkspaceRole): boolean {
  return role === "owner" || role === "admin";
}

export function hasWorkspaceCapability(
  role: WorkspaceRole,
  capability: WorkspaceCapability
): boolean {
  switch (capability) {
    case "write_requests":
      return canWriteRequests(role);
    case "manage_workspace":
      return canManageWorkspace(role);
    case "view_billing":
      return canViewBilling(role);
    case "access_operator":
      return canAccessOperator(role);
    default:
      return false;
  }
}

export function capabilityLabel(capability: WorkspaceCapability): string {
  switch (capability) {
    case "write_requests":
      return "request write access";
    case "manage_workspace":
      return "workspace admin access";
    case "view_billing":
      return "billing admin access";
    case "access_operator":
      return "operator console access";
    default:
      return "required access";
  }
}

export async function revokeApiKey(keyId: string): Promise<void> {
  await apiRequest<OkResponse>(`account/api-keys/${encodeURIComponent(keyId)}/revoke`, {
    method: "POST",
  });
}

export async function listWebhookKeys(
  source = "default",
  includeInactive = true
): Promise<WebhookKeyRecord[]> {
  const params = new URLSearchParams({
    source,
    include_inactive: includeInactive ? "true" : "false",
    limit: "100",
  });
  const out = await apiGet<WebhookKeysResponse>(`account/webhook-keys?${params.toString()}`);
  return out.keys ?? [];
}

export async function createWebhookKey(input: {
  source: string;
  grace_seconds?: number;
}): Promise<WebhookKeyCreateResult> {
  const out = await apiRequest<WebhookKeyCreateResponse>("account/webhook-keys", {
    method: "POST",
    body: JSON.stringify({
      source: input.source,
      grace_seconds: input.grace_seconds,
    }),
  });
  return {
    webhook_key: out.webhook_key,
    rotation: out.rotation,
  };
}

export async function revokeWebhookKey(
  keyId: string,
  graceSeconds = 0
): Promise<void> {
  await apiRequest<OkResponse>(
    `account/webhook-keys/${encodeURIComponent(keyId)}/revoke`,
    {
      method: "POST",
      body: JSON.stringify({ grace_seconds: graceSeconds }),
    }
  );
}

export async function listWorkspaces(): Promise<WorkspaceRecord[]> {
  const out = await apiGet<WorkspaceListResponse>("account/workspaces");
  return out.workspaces ?? [];
}

export async function switchWorkspace(input: {
  workspace_id?: string;
  environment?: WorkspaceEnvironment;
}): Promise<SessionRecord> {
  const out = await apiRequest<SessionUpdatedResponse>("account/workspaces/switch", {
    method: "POST",
    body: JSON.stringify(input),
  });
  return out.session;
}

export async function listTeamMembers(): Promise<TeamMemberRecord[]> {
  const out = await apiGet<TeamMembersResponse>("account/team-members");
  return out.members ?? [];
}

export async function inviteTeamMember(
  email: string,
  role: WorkspaceRole
): Promise<{
  member: TeamMemberRecord;
  invite_token: string;
  invite_path: string;
  invite_expires_at_ms: number;
}> {
  const out = await apiRequest<TeamMemberInviteResponse>("account/team-members", {
    method: "POST",
    body: JSON.stringify({
      email,
      role,
    }),
  });
  return {
    member: out.member,
    invite_token: out.invite_token,
    invite_path: out.invite_path,
    invite_expires_at_ms: out.invite_expires_at_ms,
  };
}

export async function removeTeamMember(memberId: string): Promise<void> {
  await apiRequest<OkResponse>(`account/team-members/${encodeURIComponent(memberId)}`, {
    method: "DELETE",
  });
}

export async function updateTeamRole(
  memberId: string,
  role: WorkspaceRole
): Promise<TeamMemberRecord> {
  const out = await apiRequest<TeamMemberResponse>(
    `account/team-members/${encodeURIComponent(memberId)}`,
    {
      method: "PATCH",
      body: JSON.stringify({ role }),
    }
  );
  return out.member;
}

export async function getBillingProfile(): Promise<BillingProfile | null> {
  try {
    const out = await apiGet<BillingResponse>("account/billing");
    return out.profile;
  } catch {
    return null;
  }
}

export async function updateBillingProfile(
  update: Partial<Omit<BillingProfile, "updated_at_ms">> & {
    flutterwave_transaction_id?: string;
  }
): Promise<BillingProfile> {
  const out = await apiRequest<BillingResponse>("account/billing", {
    method: "PUT",
    body: JSON.stringify(update),
  });
  return out.profile;
}

export async function getBillingProviderConfig(): Promise<BillingProviderConfig | null> {
  try {
    const out = await apiGet<BillingProviderConfigResponse>("account/billing/providers");
    return out.flutterwave;
  } catch {
    return null;
  }
}

export async function listBillingAuditEvents(): Promise<BillingAuditEvent[]> {
  const out = await apiGet<BillingAuditResponse>("account/billing-audit");
  return out.events ?? [];
}

export async function getWorkspaceSettings(): Promise<WorkspaceSettings | null> {
  try {
    const out = await apiGet<WorkspaceSettingsResponse>("account/settings");
    return out.settings;
  } catch {
    return null;
  }
}

export async function updateWorkspaceSettings(
  update: Partial<Omit<WorkspaceSettings, "updated_at_ms">>
): Promise<WorkspaceSettings> {
  const out = await apiRequest<WorkspaceSettingsResponse>("account/settings", {
    method: "PUT",
    body: JSON.stringify(update),
  });
  return out.settings;
}

export async function listEnvironments(
  includeInactive = true,
  limit = 100
): Promise<TenantEnvironmentRecord[]> {
  const params = new URLSearchParams({
    include_inactive: includeInactive ? "true" : "false",
    limit: String(limit),
  });
  const out = await apiGet<EnvironmentsResponse>(
    `account/environments?${params.toString()}`
  );
  return out.environments ?? [];
}

export async function listAgents(input?: {
  environment_id?: string;
  include_inactive?: boolean;
  limit?: number;
}): Promise<TenantAgentRecord[]> {
  const params = new URLSearchParams({
    include_inactive: input?.include_inactive === false ? "false" : "true",
    limit: String(input?.limit ?? 100),
  });
  if (input?.environment_id) {
    params.set("environment_id", input.environment_id);
  }
  const out = await apiGet<AgentsResponse>(`account/agents?${params.toString()}`);
  return out.agents ?? [];
}

export async function listPolicyTemplates(): Promise<PolicyTemplateDefinition[]> {
  const out = await apiGet<PolicyTemplatesResponse>("account/policy/templates");
  return out.templates ?? [];
}

export async function listPolicyBundles(limit = 100): Promise<TenantPolicyBundleRecord[]> {
  const params = new URLSearchParams({ limit: String(limit) });
  const out = await apiGet<PolicyBundlesResponse>(
    `account/policy/bundles?${params.toString()}`
  );
  return out.bundles ?? [];
}

export async function createPolicyBundle(input: {
  bundle_id: string;
  label: string;
  template_ids: string[];
  rules: PolicyRuleDefinition[];
}): Promise<TenantPolicyBundleRecord> {
  const out = await apiRequest<PolicyBundleResponse>("account/policy/bundles", {
    method: "POST",
    body: JSON.stringify(input),
  });
  return out.bundle;
}

export async function publishPolicyBundle(
  bundleId: string
): Promise<TenantPolicyBundleRecord> {
  const out = await apiRequest<PolicyBundleResponse>(
    `account/policy/bundles/${encodeURIComponent(bundleId)}/publish`,
    {
      method: "POST",
      body: JSON.stringify({}),
    }
  );
  return out.bundle;
}

export async function rollbackPolicyBundle(input: {
  current_bundle_id: string;
  target_bundle_id: string;
  rollback_reason?: string;
}): Promise<TenantPolicyBundleRecord> {
  const out = await apiRequest<PolicyBundleResponse>(
    `account/policy/bundles/${encodeURIComponent(input.current_bundle_id)}/rollback`,
    {
      method: "POST",
      body: JSON.stringify({
        target_bundle_id: input.target_bundle_id,
        rollback_reason: input.rollback_reason,
      }),
    }
  );
  return out.bundle;
}

export async function simulatePolicyBundle(input: {
  bundle_id?: string;
  action: {
    agent_id: string;
    environment_id: string;
    intent_type: "refund" | "transfer" | "generate_invoice";
    adapter_type: string;
    payload: Record<string, unknown>;
    requested_scope: string[];
    reason: string;
    submitted_by: string;
  };
}): Promise<PolicySimulationResult> {
  return apiRequest<PolicySimulationResult>("account/policy/simulations", {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function getUsageSummary(): Promise<UsageSummary | null> {
  try {
    const out = await apiGet<UsageSummaryResponse>("account/usage");
    return {
      workspace_id: out.workspace_id,
      plan: out.plan,
      access_mode: out.access_mode,
      monthly_price_usd: out.monthly_price_usd,
      free_play_limit: out.free_play_limit,
      used_requests: out.used_requests,
      remaining_requests: out.remaining_requests,
      window_start_ms: out.window_start_ms,
      window_end_ms: out.window_end_ms,
      paid_unlimited: out.paid_unlimited,
      metering_source: out.metering_source ?? "unknown",
      metering_warning: out.metering_warning ?? null,
    };
  } catch {
    return null;
  }
}

export async function listInvoices(): Promise<InvoiceRecord[]> {
  const out = await apiGet<InvoiceResponse>("account/invoices");
  return out.invoices ?? [];
}

export async function lookupInvite(token: string): Promise<InviteSummary> {
  const query = new URLSearchParams({ token });
  const out = await apiGet<InviteLookupResponse>(`account/invite?${query.toString()}`);
  return out.invite;
}

export async function acceptInvite(input: {
  token: string;
  full_name: string;
  password: string;
}): Promise<SessionRecord> {
  const out = await apiRequest<SessionUpdatedResponse>("account/invite/accept", {
    method: "POST",
    body: JSON.stringify(input),
  });
  return out.session;
}

export async function requestEmailVerification(email: string): Promise<void> {
  await apiRequest<OkResponse>("account/email-verification/request", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

export async function confirmEmailVerification(token: string): Promise<void> {
  await apiRequest<OkResponse>("account/email-verification/confirm", {
    method: "POST",
    body: JSON.stringify({ token }),
  });
}

export async function requestPasswordReset(email: string): Promise<void> {
  await apiRequest<OkResponse>("account/password-reset/request", {
    method: "POST",
    body: JSON.stringify({ email }),
  });
}

export async function confirmPasswordReset(token: string, password: string): Promise<void> {
  await apiRequest<OkResponse>("account/password-reset/confirm", {
    method: "POST",
    body: JSON.stringify({ token, password }),
  });
}
