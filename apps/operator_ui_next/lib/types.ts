export type Level = "ok" | "warn" | "error";

export type JsonMap = Record<string, string | number | boolean | null | undefined>;

export interface UiConfigResponse {
  ok: boolean;
  public_base_url?: string;
  status_base_url: string;
  ingress_base_url: string;
  tenant_id: string;
  principal_id: string;
  principal_role: string;
  has_bearer_token: boolean;
  reconciliation_rollout_mode: "hidden" | "operator_only" | "customer_visible";
  reconciliation_operator_visible: boolean;
  reconciliation_customer_visible: boolean;
  requires_email_verification: boolean;
  email_delivery_configured: boolean;
  password_reset_enabled: boolean;
}

export interface UiHealthResponse {
  ok: boolean;
  status_api_reachable: boolean;
  status_api_status_code?: number;
  status_api_error?: string;
}

export interface JobRow {
  job_id: string;
  intent_id: string;
  adapter_id: string;
  state: string;
  classification: string;
  attempt: number;
  max_attempts: number;
  replay_count?: number;
  replay_of_job_id?: string | null;
  next_retry_at_ms?: number | null;
  created_at_ms?: number;
  failure_code?: string | null;
  failure_message?: string | null;
  updated_at_ms: number;
}

export interface JobListResponse {
  jobs: JobRow[];
}

export interface FailureInfo {
  code?: string;
  message?: string;
}

export interface RequestStatusResponse {
  tenant_id: string;
  intent_id: string;
  job_id?: string;
  adapter_id?: string;
  state: string;
  classification: string;
  attempt: number;
  max_attempts: number;
  replay_count?: number;
  updated_at_ms: number;
  request_id?: string;
  correlation_id?: string | null;
  idempotency_key?: string | null;
  last_failure?: FailureInfo;
}

export interface ReceiptEntry {
  receipt_id: string;
  tenant_id: string;
  intent_id: string;
  job_id: string;
  receipt_version?: number;
  recon_subject_id?: string | null;
  reconciliation_eligible?: boolean;
  execution_correlation_id?: string | null;
  adapter_execution_reference?: string | null;
  external_observation_key?: string | null;
  expected_fact_snapshot?: Record<string, unknown> | null;
  attempt_no: number;
  state: string;
  classification: string;
  summary: string;
  details?: Record<string, string>;
  connector_outcome?: {
    status: string;
    connector_type?: string | null;
    binding_id?: string | null;
    reference?: string | null;
  } | null;
  recon_linkage?: {
    recon_subject_id?: string | null;
    reconciliation_eligible: boolean;
    execution_correlation_id?: string | null;
    adapter_execution_reference?: string | null;
    external_observation_key?: string | null;
    connector_type?: string | null;
    connector_binding_id?: string | null;
    connector_reference?: string | null;
  } | null;
  occurred_at_ms: number;
}

export interface ReceiptResponse {
  tenant_id: string;
  intent_id: string;
  entries: ReceiptEntry[];
}

export interface ReceiptLookupResponse {
  tenant_id: string;
  receipt_id: string;
  intent_id: string;
  entry: ReceiptEntry;
}

export interface HistoryTransition {
  transition_id: string;
  tenant_id: string;
  intent_id: string;
  job_id: string;
  from_state?: string | null;
  to_state: string;
  classification: string;
  reason_code: string;
  reason: string;
  adapter_id?: string;
  actor?: string | Record<string, string>;
  occurred_at_ms: number;
}

export interface HistoryResponse {
  tenant_id: string;
  intent_id: string;
  transitions: HistoryTransition[];
}

export interface CallbackAttempt {
  attempt_no: number;
  outcome: string;
  failure_class?: string | null;
  error_message?: string | null;
  http_status?: number | null;
  response_excerpt?: string | null;
  occurred_at_ms: number;
}

export interface CallbackItem {
  callback_id: string;
  state: string;
  attempts: number;
  last_http_status?: number | null;
  last_error_class?: string | null;
  last_error_message?: string | null;
  next_attempt_at_ms?: number | null;
  delivered_at_ms?: number | null;
  updated_at_ms: number;
  attempt_history?: CallbackAttempt[];
}

export interface CallbackHistoryResponse {
  tenant_id: string;
  intent_id: string;
  callbacks: CallbackItem[];
}

export interface ReplayResponse {
  tenant_id: string;
  intent_id: string;
  source_job_id: string;
  replay_job_id: string;
  replay_count: number;
  state: string;
}

export interface IntakeAudit {
  audit_id?: string;
  request_id: string;
  channel: string;
  endpoint?: string;
  method?: string;
  principal_id?: string | null;
  submitter_kind?: string | null;
  auth_scheme?: string | null;
  intent_kind?: string | null;
  correlation_id?: string | null;
  idempotency_key?: string | null;
  idempotency_decision?: string | null;
  validation_result: string;
  rejection_reason?: string | null;
  error_status?: number | null;
  error_message?: string | null;
  accepted_intent_id?: string | null;
  accepted_job_id?: string | null;
  details_json?: Record<string, unknown>;
  created_at_ms: number;
}

export interface IntakeAuditsResponse {
  tenant_id: string;
  audits: IntakeAudit[];
}

export interface CallbackDestination {
  delivery_url: string;
  timeout_ms: number;
  allow_private_destinations: boolean;
  allowed_hosts: string[];
  enabled: boolean;
  has_bearer_token: boolean;
  has_signature_secret: boolean;
  signature_key_id?: string | null;
  updated_by_principal_id?: string;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface CallbackDestinationResponse {
  tenant_id: string;
  configured: boolean;
  destination?: CallbackDestination;
}

export interface CallbackDestinationUpsertRequest {
  delivery_url: string;
  timeout_ms: number;
  allow_private_destinations: boolean;
  allowed_hosts: string[];
  enabled: boolean;
}

export interface ActivityLogItem {
  id: string;
  level: Level;
  message: string;
  ts: string;
}

export type FlowStatus = "observed" | "partial" | "not_observed";

export interface FlowCard {
  code: "A" | "B" | "C" | "D";
  title: string;
  status: FlowStatus;
  detail: string;
}

export interface SubmitIntentRequest {
  intent_kind: string;
  payload: Record<string, unknown>;
  metadata?: Record<string, string>;
}

export interface SubmitIntentResponse {
  ok: boolean;
  tenant_id: string;
  intent_id: string;
  job_id: string;
  adapter_id: string;
  state: string;
  route_rule: string;
}

export interface SearchResultItem {
  kind: "request" | "receipt" | "callback" | string;
  object_id: string;
  intent_id?: string;
  title: string;
  subtitle: string;
  href: string;
  updated_at_ms: number;
  score: number;
}

export interface SearchResponse {
  ok: boolean;
  query: string;
  results: SearchResultItem[];
}

export interface CallbackDetailResponse {
  ok: boolean;
  callback_id: string;
  intent_id: string;
  callback: CallbackItem;
  request: RequestStatusResponse;
  receipt: ReceiptResponse;
  history: HistoryResponse;
}

export interface ReconciliationSubjectRecord {
  subject_id: string;
  tenant_id: string;
  intent_id: string;
  job_id: string;
  adapter_id: string;
  canonical_state: string;
  platform_classification: string;
  latest_receipt_id?: string | null;
  latest_transition_id?: string | null;
  latest_callback_id?: string | null;
  latest_signal_id?: string | null;
  latest_signal_kind?: string | null;
  execution_correlation_id?: string | null;
  adapter_execution_reference?: string | null;
  external_observation_key?: string | null;
  expected_fact_snapshot?: Record<string, unknown> | null;
  dirty: boolean;
  recon_attempt_count: number;
  recon_retry_count: number;
  created_at_ms: number;
  updated_at_ms: number;
  scheduled_at_ms?: number | null;
  next_reconcile_after_ms?: number | null;
  last_reconciled_at_ms?: number | null;
  last_recon_error?: string | null;
  last_run_state?: string | null;
}

export interface ReconciliationRunRecord {
  run_id: string;
  subject_id: string;
  tenant_id: string;
  intent_id: string;
  job_id: string;
  adapter_id: string;
  rule_pack: string;
  lifecycle_state: string;
  normalized_result?: string | null;
  outcome: string;
  summary: string;
  machine_reason: string;
  expected_fact_count: number;
  observed_fact_count: number;
  matched_fact_count: number;
  unmatched_fact_count: number;
  created_at_ms: number;
  updated_at_ms: number;
  completed_at_ms?: number | null;
  attempt_number: number;
  retry_scheduled_at_ms?: number | null;
  last_error?: string | null;
  exception_case_ids: string[];
}

export interface ReconciliationReceiptRecord {
  recon_receipt_id: string;
  run_id: string;
  subject_id: string;
  normalized_result?: string | null;
  outcome: string;
  summary: string;
  details: Record<string, string>;
  created_at_ms: number;
}

export interface ReconciliationFactRecord {
  fact_id: string;
  run_id: string;
  subject_id: string;
  fact_type: string;
  fact_key: string;
  fact_value: unknown;
  source_kind?: string | null;
  source_table?: string | null;
  source_id?: string | null;
  metadata: Record<string, unknown> | null;
  observed_at_ms?: number | null;
  created_at_ms: number;
}

export interface RequestReconciliationResponse {
  tenant_id: string;
  intent_id: string;
  subject: ReconciliationSubjectRecord | null;
  runs: ReconciliationRunRecord[];
  latest_receipt: ReconciliationReceiptRecord | null;
  expected_facts: ReconciliationFactRecord[];
  observed_facts: ReconciliationFactRecord[];
}

export interface ExceptionEvidenceRecord {
  evidence_id: string;
  case_id: string;
  evidence_type: string;
  source_table?: string | null;
  source_id?: string | null;
  observed_at_ms?: number | null;
  payload: Record<string, unknown> | null;
  created_at_ms: number;
}

export interface ExceptionCaseRecord {
  case_id: string;
  tenant_id: string;
  subject_id: string;
  intent_id: string;
  job_id: string;
  adapter_id: string;
  category: string;
  severity: string;
  state: string;
  summary: string;
  machine_reason: string;
  dedupe_key: string;
  cluster_key: string;
  first_seen_at_ms: number;
  last_seen_at_ms: number;
  occurrence_count: number;
  created_at_ms: number;
  updated_at_ms: number;
  resolved_at_ms?: number | null;
  latest_run_id?: string | null;
  latest_outcome_id?: string | null;
  latest_recon_receipt_id?: string | null;
  latest_execution_receipt_id?: string | null;
  latest_evidence_snapshot_id?: string | null;
  last_actor?: string | null;
  evidence: ExceptionEvidenceRecord[];
}

export interface ExceptionEventRecord {
  event_id: string;
  case_id: string;
  event_type: string;
  from_state?: string | null;
  to_state?: string | null;
  actor: string;
  reason: string;
  payload: Record<string, unknown> | null;
  created_at_ms: number;
}

export interface ExceptionResolutionRecord {
  resolution_id: string;
  case_id: string;
  resolution_state: string;
  actor: string;
  reason: string;
  payload: Record<string, unknown> | null;
  created_at_ms: number;
}

export interface RequestExceptionsResponse {
  tenant_id: string;
  intent_id: string;
  cases: ExceptionCaseRecord[];
}

export interface ExceptionIndexResponse {
  tenant_id: string;
  cases: ExceptionCaseRecord[];
}

export interface ExceptionDetailResponse {
  tenant_id: string;
  case: ExceptionCaseRecord;
  events: ExceptionEventRecord[];
  resolution_history: ExceptionResolutionRecord[];
}

export interface ExceptionStateTransitionRequest {
  state: string;
  reason: string;
  payload?: Record<string, unknown> | null;
}

export interface ExceptionStateTransitionResponse {
  ok: boolean;
  case: ExceptionCaseRecord;
}

export interface OperatorActionRequest {
  reason: string;
  payload?: Record<string, unknown> | null;
}

export interface ReconActionResponse {
  ok: boolean;
  action: string;
  action_id: string;
  subject: ReconciliationSubjectRecord;
}

export interface ExceptionActionResponse {
  ok: boolean;
  action: string;
  case: ExceptionCaseRecord;
}

export interface ReplayReviewResponse {
  ok: boolean;
  handoff: string;
  replay: ReplayResponse;
}

export interface UnifiedExceptionSummary {
  total_cases: number;
  unresolved_cases: number;
  highest_severity?: string | null;
  categories: string[];
  open_case_ids: string[];
}

export interface UnifiedEvidenceReferenceRecord {
  kind: string;
  label: string;
  source_table?: string | null;
  source_id?: string | null;
  observed_at_ms?: number | null;
}

export type UnifiedDashboardStatus =
  | "matched"
  | "pending_verification"
  | "mismatch_detected"
  | "manual_review_required";

export interface UnifiedRequestStatusResponse {
  tenant_id: string;
  intent_id: string;
  request: RequestStatusResponse;
  receipt: ReceiptResponse;
  history: HistoryResponse;
  callbacks: CallbackHistoryResponse;
  reconciliation: RequestReconciliationResponse;
  exceptions: RequestExceptionsResponse;
  dashboard_status: UnifiedDashboardStatus | string;
  recon_status?: string | null;
  reconciliation_eligible: boolean;
  latest_execution_receipt_id?: string | null;
  latest_recon_receipt_id?: string | null;
  latest_evidence_snapshot_id?: string | null;
  exception_summary: UnifiedExceptionSummary;
  evidence_references: UnifiedEvidenceReferenceRecord[];
}

export interface ReconciliationRolloutWindow {
  lookback_hours: number;
  started_at_ms: number;
  generated_at_ms: number;
}

export interface ReconciliationRolloutIntakeMetrics {
  eligible_execution_receipts: number;
  intake_signals: number;
  subjects_total: number;
  dirty_subjects: number;
  retry_scheduled_subjects: number;
}

export interface ReconciliationRolloutOutcomeMetrics {
  matched: number;
  partially_matched: number;
  unmatched: number;
  pending_observation: number;
  stale: number;
  manual_review_required: number;
}

export interface ReconciliationRolloutExceptionMetrics {
  total_cases: number;
  unresolved_cases: number;
  high_or_critical_cases: number;
  false_positive_cases: number;
  exception_rate: number;
  false_positive_rate: number;
  stale_rate: number;
}

export interface ReconciliationRolloutLatencyMetrics {
  avg_recon_latency_ms?: number | null;
  p95_recon_latency_ms?: number | null;
  max_recon_latency_ms?: number | null;
  avg_operator_handling_ms?: number | null;
  p95_operator_handling_ms?: number | null;
}

export interface ReconciliationRolloutQueryMetrics {
  sampled_intent_id?: string | null;
  exception_index_query_ms?: number | null;
  unified_request_query_ms?: number | null;
}

export interface ReconciliationRolloutSummaryResponse {
  tenant_id: string;
  window: ReconciliationRolloutWindow;
  intake: ReconciliationRolloutIntakeMetrics;
  outcomes: ReconciliationRolloutOutcomeMetrics;
  exceptions: ReconciliationRolloutExceptionMetrics;
  latency: ReconciliationRolloutLatencyMetrics;
  queries: ReconciliationRolloutQueryMetrics;
}

export interface NotificationItem {
  tenant_id?: string;
  intent_id: string;
  state: string;
  classification: string;
  updated_at_ms: number;
}

export interface NotificationStreamEnvelope {
  event: string;
  generated_at_ms: number;
  notifications: NotificationItem[];
}

export interface UsageSummarySnapshot {
  workspace_id: string;
  plan: string;
  access_mode: string;
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

export interface WorkspaceSummary {
  workspace_id: string;
  workspace_name: string;
  tenant_id: string;
  role: string;
  environment: "sandbox" | "staging" | "production";
  is_current: boolean;
}

export interface WorkspaceListResponse {
  ok: boolean;
  workspaces: WorkspaceSummary[];
}

export interface WorkspaceDetailResponse {
  ok: boolean;
  workspace: WorkspaceSummary;
  members: Array<{
    id: string;
    email: string;
    role: string;
    status: string;
    added_at_ms: number;
    invite_expires_at_ms?: number | null;
  }>;
  api_keys: Array<{
    id: string;
    name: string;
    prefix: string;
    last4: string;
    created_at_ms: number;
    revoked_at_ms: number | null;
    last_used_at_ms: number | null;
  }>;
  settings: {
    callback_default_enabled: boolean;
    request_retention_days: number;
    allow_replay_from_customer_app: boolean;
    execution_policy: "customer_signed" | "customer_managed_signer" | "sponsored";
    sponsored_monthly_cap_requests: number;
    updated_at_ms: number;
  };
  billing: {
    plan: string;
    access_mode: string;
    billing_email: string;
    card_brand: string | null;
    card_last4: string | null;
    payment_provider: string | null;
    payment_reference: string | null;
    payment_verified_at_ms: number | null;
    updated_at_ms: number;
  };
  usage: UsageSummarySnapshot;
  invoices: Array<{
    id: string;
    period: string;
    amount_usd: number;
    status: string;
    issued_at_ms: number;
  }>;
}

export interface AdminTenantSummary {
  tenant_id: string;
  workspace_count: number;
  principal_count: number;
  api_key_count: number;
  active_webhook_keys: number;
}

export interface AdminWorkspaceSummary {
  workspace_id: string;
  workspace_name: string;
  tenant_id: string;
  principals: number;
  plan: string;
  access_mode: string;
}

export interface AdminJobSummary {
  tenant_id: string;
  intent_id: string;
  state: string;
  classification: string;
  adapter_id?: string | null;
  updated_at_ms: number;
}

export interface AdminIncidentSummary {
  incident_id: string;
  kind: string;
  severity: string;
  tenant_id: string;
  intent_id: string;
  classification: string;
  state: string;
  updated_at_ms: number;
}

export interface AdminAdapterHealth {
  adapter_id: string;
  total_jobs: number;
  success_jobs: number;
  failure_jobs: number;
  retrying_jobs: number;
}

export interface AdminOverviewResponse {
  ok: boolean;
  generated_at_ms: number;
  tenants: AdminTenantSummary[];
  workspaces: AdminWorkspaceSummary[];
  dead_letters: AdminJobSummary[];
  incidents: AdminIncidentSummary[];
  adapter_health: AdminAdapterHealth[];
}

export interface OperatorBacklogSummary {
  total_jobs: number;
  queued: number;
  leased: number;
  executing: number;
  retry_scheduled: number;
  failed_terminal: number;
  dead_lettered: number;
}

export interface OperatorCountRow {
  label: string;
  count: number;
}

export interface OperatorCallbackTrendPoint {
  bucket: string;
  delivered: number;
  retrying: number;
  terminal_failures: number;
}

export interface OperatorAdapterHealth {
  adapter_id: string;
  total_jobs: number;
  success_jobs: number;
  failure_jobs: number;
  retrying_jobs: number;
  queued_jobs: number;
  last_failure_at_ms?: number | null;
}

export interface OperatorFailingIntent {
  intent_id: string;
  job_id: string;
  state: string;
  classification: string;
  adapter_id: string;
  attempt: number;
  max_attempts: number;
  replay_count: number;
  failure_code?: string | null;
  failure_message?: string | null;
  updated_at_ms: number;
}

export interface OperatorOverviewResponse {
  ok: boolean;
  generated_at_ms: number;
  tenant_id: string;
  backlog: OperatorBacklogSummary;
  failure_classes: OperatorCountRow[];
  callback_failure_trends: OperatorCallbackTrendPoint[];
  adapter_health: OperatorAdapterHealth[];
  top_failing_intents: OperatorFailingIntent[];
}

export interface OperatorDeliverySummary {
  callback_id: string;
  intent_id: string;
  job_id: string;
  state: string;
  attempts: number;
  last_http_status?: number | null;
  last_error_class?: string | null;
  last_error_message?: string | null;
  next_attempt_at_ms?: number | null;
  delivered_at_ms?: number | null;
  updated_at_ms: number;
  latest_attempt_outcome?: string | null;
  latest_attempt_at_ms?: number | null;
}

export interface OperatorDeliveryCluster {
  key: string;
  count: number;
  latest_at_ms: number;
}

export interface OperatorDeliveriesResponse {
  ok: boolean;
  tenant_id: string;
  deliveries: OperatorDeliverySummary[];
  failure_clusters: OperatorDeliveryCluster[];
}

export interface OperatorDeliveryRedriveResponse {
  ok: boolean;
  tenant_id: string;
  callback_id: string;
  state: string;
  message: string;
}

export interface OperatorQueryAuditRecord {
  audit_id: string;
  principal_id: string;
  principal_role: string;
  method: string;
  endpoint: string;
  resource_id?: string | null;
  request_id?: string | null;
  allowed: boolean;
  details_json: Record<string, unknown> | null;
  created_at_ms: number;
}

export interface OperatorActionAuditRecord {
  action_id: string;
  principal_id: string;
  principal_role: string;
  action_type: string;
  target_intent_id: string;
  allowed: boolean;
  reason: string;
  result_json?: Record<string, unknown> | null;
  created_at_ms: number;
}

export interface OperatorActivityFeedItem {
  id: string;
  kind: string;
  principal_id: string;
  principal_role: string;
  action: string;
  target?: string | null;
  allowed: boolean;
  details?: Record<string, unknown> | null;
  created_at_ms: number;
}

export interface OperatorActivityResponse {
  ok: boolean;
  tenant_id: string;
  query_audits: OperatorQueryAuditRecord[];
  operator_actions: OperatorActionAuditRecord[];
  feed: OperatorActivityFeedItem[];
}

export interface OperatorSecurityCallbackDestination {
  configured: boolean;
  delivery_url?: string | null;
  enabled: boolean;
  allow_private_destinations: boolean;
  allowed_hosts: string[];
  updated_by_principal_id?: string | null;
  updated_at_ms?: number | null;
}

export interface OperatorSecurityIncident {
  request_id: string;
  channel: string;
  principal_id?: string | null;
  reason: string;
  created_at_ms: number;
}

export interface OperatorKeyRotationPrompt {
  workspace_id: string;
  workspace_name: string;
  key_id: string;
  key_name: string;
  prefix: string;
  last4: string;
  created_at_ms: number;
  last_used_at_ms?: number | null;
  recommendation: string;
}

export interface OperatorSecurityResponse {
  ok: boolean;
  tenant_id: string;
  callback_destination: OperatorSecurityCallbackDestination;
  suspicious_auth_failures: OperatorSecurityIncident[];
  key_rotation_prompts: OperatorKeyRotationPrompt[];
}
