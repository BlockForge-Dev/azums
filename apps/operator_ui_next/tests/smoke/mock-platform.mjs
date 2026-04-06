import http from "node:http";
import { randomUUID } from "node:crypto";

const tenantId = "tenant_demo";
const ingressPort = 43000;
const statusPort = 43082;

let sequence = 0;
const apiKeys = [];
const webhookKeys = [];
const requests = new Map();
const unifiedRequests = new Map();
const exceptionCases = new Map();
const exceptionDetails = new Map();

const webhookAudits = [
  {
    audit_id: "audit_webhook_accepted",
    request_id: "req_webhook_accepted",
    channel: "webhook",
    endpoint: "/webhooks/default",
    method: "POST",
    principal_id: "github-app",
    submitter_kind: "signed_webhook_sender",
    auth_scheme: "hmac_sha256",
    intent_kind: "solana.transfer.v1",
    validation_result: "accepted",
    accepted_intent_id: "intent_webhook_accepted",
    accepted_job_id: "job_webhook_accepted",
    details_json: {
      event_type: "github.push",
    },
    created_at_ms: Date.now() - 60_000,
  },
  {
    audit_id: "audit_webhook_rejected",
    request_id: "req_webhook_rejected",
    channel: "webhook",
    endpoint: "/webhooks/default",
    method: "POST",
    principal_id: "github-app",
    submitter_kind: "signed_webhook_sender",
    auth_scheme: "hmac_sha256",
    intent_kind: "solana.transfer.v1",
    validation_result: "rejected",
    rejection_reason: "invalid webhook signature",
    error_status: 401,
    error_message: "invalid webhook signature",
    details_json: {
      event_type: "github.release",
    },
    created_at_ms: Date.now() - 30_000,
  },
];

function nextId(prefix) {
  sequence += 1;
  return `${prefix}_${String(sequence).padStart(4, "0")}`;
}

function now() {
  return Date.now();
}

function jsonResponse(res, statusCode, body) {
  const payload = JSON.stringify(body);
  res.writeHead(statusCode, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
  });
  res.end(payload);
}

function textResponse(res, statusCode, body) {
  res.writeHead(statusCode, {
    "content-type": "text/plain; charset=utf-8",
    "cache-control": "no-store",
  });
  res.end(body);
}

function readJson(req) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", () => {
      if (chunks.length === 0) {
        resolve({});
        return;
      }
      try {
        resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
      } catch (error) {
        reject(error);
      }
    });
    req.on("error", reject);
  });
}

function createTransition(intentId, jobId, toState, classification, reasonCode, reason) {
  return {
    transition_id: nextId("transition"),
    tenant_id: tenantId,
    intent_id: intentId,
    job_id: jobId,
    from_state: null,
    to_state: toState,
    classification,
    reason_code: reasonCode,
    reason,
    adapter_id: "solana",
    actor: "mock-platform",
    occurred_at_ms: now(),
  };
}

function createReceiptEntry(intentId, jobId, attemptNo, state, classification, summary, details = {}) {
  return {
    receipt_id: nextId("receipt"),
    tenant_id: tenantId,
    intent_id: intentId,
    job_id: jobId,
    attempt_no: attemptNo,
    state,
    classification,
    summary,
    details,
    occurred_at_ms: now(),
  };
}

function createCallback(intentId, terminalState) {
  const delivered = terminalState !== "failed_terminal";
  return {
    callback_id: nextId("callback"),
    state: delivered ? "delivered" : "failed_terminal",
    attempts: 1,
    last_http_status: delivered ? 200 : 500,
    last_error_class: delivered ? null : "delivery_error",
    last_error_message: delivered ? null : "mock receiver unavailable",
    next_attempt_at_ms: null,
    delivered_at_ms: delivered ? now() : null,
    updated_at_ms: now(),
    attempt_history: [
      {
        attempt_no: 1,
        outcome: delivered ? "delivered" : "failed_terminal",
        failure_class: delivered ? null : "delivery_error",
        error_message: delivered ? null : "mock receiver unavailable",
        http_status: delivered ? 200 : 500,
        response_excerpt: delivered ? "ok" : "receiver unavailable",
        occurred_at_ms: now(),
      },
    ],
  };
}

function createRequestRecord(body) {
  const intentId = body?.payload?.intent_id ?? nextId("intent");
  const jobId = nextId("job");
  const scenario = body?.metadata?.["playground.demo_scenario"] ?? "success";
  const terminalState = scenario === "terminal_failure" ? "failed_terminal" : "succeeded";
  const classification = terminalState === "failed_terminal" ? "terminal_failure" : "completed";
  const transitions = [
    createTransition(intentId, jobId, "received", "accepted", "accepted", "Request accepted."),
    createTransition(intentId, jobId, "queued", "queued", "queued", "Request queued."),
    createTransition(
      intentId,
      jobId,
      terminalState,
      classification,
      terminalState,
      terminalState === "failed_terminal" ? "Mock terminal failure." : "Mock execution succeeded."
    ),
  ];
  const receiptEntries = [
    createReceiptEntry(intentId, jobId, 1, "received", "accepted", "Intent accepted."),
    createReceiptEntry(intentId, jobId, 1, "queued", "queued", "Intent queued for execution."),
    createReceiptEntry(
      intentId,
      jobId,
      1,
      terminalState,
      classification,
      terminalState === "failed_terminal"
        ? "Execution failed in the mock platform."
        : "Execution completed in the mock platform.",
      {
        scenario,
      }
    ),
  ];
  const callbacks = [createCallback(intentId, terminalState)];
  const record = {
    tenant_id: tenantId,
    intent_id: intentId,
    job_id: jobId,
    adapter_id: "solana",
    state: terminalState,
    classification,
    attempt: 1,
    max_attempts: 4,
    replay_count: 0,
    updated_at_ms: now(),
    request_id: nextId("request"),
    correlation_id: randomUUID(),
    idempotency_key: null,
    last_failure:
      terminalState === "failed_terminal"
        ? {
            code: "mock_terminal_failure",
            message: "Mock terminal failure.",
          }
        : undefined,
    receipt: {
      tenant_id: tenantId,
      intent_id: intentId,
      entries: receiptEntries,
    },
    history: {
      tenant_id: tenantId,
      intent_id: intentId,
      transitions,
    },
    callbacks: {
      tenant_id: tenantId,
      intent_id: intentId,
      callbacks,
    },
    route_rule: "mock-devnet-playground",
  };
  requests.set(intentId, record);
  return record;
}

function seedPaystackExceptionFixture() {
  const intentId = "intent_paystack_exception";
  const jobId = "job_paystack_exception";
  const requestId = "request_paystack_exception";
  const correlationId = "corr_paystack_exception";
  const executionReference = "paystack:refund:refund_9001";
  const connectorReference = "PSK_REF_12345";
  const receiptEntries = [
    {
      receipt_id: "receipt_paystack_received",
      tenant_id: tenantId,
      intent_id: intentId,
      job_id: jobId,
      receipt_version: 3,
      attempt_no: 1,
      state: "received",
      classification: "accepted",
      summary: "Paystack refund intent accepted.",
      details: {
        amount_minor: "125000",
        currency: "NGN",
        destination_reference: "BANK:044:0123456789",
        provider_status: "queued",
      },
      occurred_at_ms: now() - 180_000,
    },
    {
      receipt_id: "receipt_paystack_succeeded",
      tenant_id: tenantId,
      intent_id: intentId,
      job_id: jobId,
      receipt_version: 3,
      recon_subject_id: "recon_subject_paystack_exception",
      reconciliation_eligible: true,
      execution_correlation_id: correlationId,
      adapter_execution_reference: executionReference,
      external_observation_key: `paystack:${connectorReference}`,
      expected_fact_snapshot: {
        provider: "paystack",
        reference: connectorReference,
        amount_minor: 125000,
        currency: "NGN",
        destination_reference: "BANK:044:0123456789",
      },
      attempt_no: 1,
      state: "succeeded",
      classification: "completed",
      summary: "Paystack refund request reached protected execution.",
      details: {
        amount_minor: "125000",
        currency: "NGN",
        destination_reference: "BANK:044:0123456789",
        provider_status: "success",
      },
      connector_outcome: {
        status: "verified",
        connector_type: "paystack",
        binding_id: "binding_paystack_live",
        reference: connectorReference,
      },
      recon_linkage: {
        recon_subject_id: "recon_subject_paystack_exception",
        reconciliation_eligible: true,
        execution_correlation_id: correlationId,
        adapter_execution_reference: executionReference,
        external_observation_key: `paystack:${connectorReference}`,
        connector_type: "paystack",
        connector_binding_id: "binding_paystack_live",
        connector_reference: connectorReference,
      },
      occurred_at_ms: now() - 60_000,
    },
  ];

  const record = {
    tenant_id: tenantId,
    intent_id: intentId,
    job_id: jobId,
    adapter_id: "adapter_paystack",
    state: "succeeded",
    classification: "completed",
    attempt: 1,
    max_attempts: 3,
    replay_count: 0,
    updated_at_ms: now() - 30_000,
    request_id: requestId,
    correlation_id: correlationId,
    idempotency_key: "idem_paystack_exception",
    receipt: {
      tenant_id: tenantId,
      intent_id: intentId,
      entries: receiptEntries,
    },
    history: {
      tenant_id: tenantId,
      intent_id: intentId,
      transitions: [
        {
          transition_id: "transition_paystack_received",
          tenant_id: tenantId,
          intent_id: intentId,
          job_id: jobId,
          from_state: null,
          to_state: "received",
          classification: "accepted",
          reason_code: "accepted",
          reason: "Paystack refund accepted.",
          adapter_id: "adapter_paystack",
          actor: "mock-platform",
          occurred_at_ms: now() - 180_000,
        },
        {
          transition_id: "transition_paystack_executing",
          tenant_id: tenantId,
          intent_id: intentId,
          job_id: jobId,
          from_state: "queued",
          to_state: "executing",
          classification: "executing",
          reason_code: "dispatch_started",
          reason: "Protected Paystack execution started.",
          adapter_id: "adapter_paystack",
          actor: "mock-platform",
          occurred_at_ms: now() - 120_000,
        },
        {
          transition_id: "transition_paystack_succeeded",
          tenant_id: tenantId,
          intent_id: intentId,
          job_id: jobId,
          from_state: "executing",
          to_state: "succeeded",
          classification: "completed",
          reason_code: "paystack_refund_submitted",
          reason: "Paystack refund reported success.",
          adapter_id: "adapter_paystack",
          actor: "mock-platform",
          occurred_at_ms: now() - 60_000,
        },
      ],
    },
    callbacks: {
      tenant_id: tenantId,
      intent_id: intentId,
      callbacks: [createCallback(intentId, "succeeded")],
    },
    route_rule: "mock-paystack-refund",
  };
  requests.set(intentId, record);

  const reconciliation = {
    tenant_id: tenantId,
    intent_id: intentId,
    subject: {
      subject_id: "recon_subject_paystack_exception",
      tenant_id: tenantId,
      intent_id: intentId,
      job_id: jobId,
      adapter_id: "adapter_paystack",
      canonical_state: "succeeded",
      platform_classification: "completed",
      latest_receipt_id: "receipt_paystack_succeeded",
      latest_transition_id: "transition_paystack_succeeded",
      latest_callback_id: record.callbacks.callbacks[0].callback_id,
      latest_signal_id: "signal_paystack_terminal",
      latest_signal_kind: "finalized",
      execution_correlation_id: correlationId,
      adapter_execution_reference: executionReference,
      external_observation_key: `paystack:${connectorReference}`,
      expected_fact_snapshot: {
        reference: connectorReference,
        amount_minor: 125000,
        currency: "NGN",
        destination_reference: "BANK:044:0123456789",
      },
      dirty: false,
      recon_attempt_count: 1,
      recon_retry_count: 0,
      created_at_ms: now() - 50_000,
      updated_at_ms: now() - 20_000,
      scheduled_at_ms: now() - 40_000,
      next_reconcile_after_ms: null,
      last_reconciled_at_ms: now() - 20_000,
      last_recon_error: null,
      last_run_state: "completed",
    },
    runs: [
      {
        run_id: "recon_run_paystack_exception",
        subject_id: "recon_subject_paystack_exception",
        tenant_id: tenantId,
        intent_id: intentId,
        job_id: jobId,
        adapter_id: "adapter_paystack",
        rule_pack: "paystack",
        lifecycle_state: "completed",
        normalized_result: "unmatched",
        outcome: "unmatched",
        summary: "Observed Paystack amount differs from the expected refund amount.",
        machine_reason: "amount_mismatch",
        expected_fact_count: 4,
        observed_fact_count: 4,
        matched_fact_count: 3,
        unmatched_fact_count: 1,
        created_at_ms: now() - 40_000,
        updated_at_ms: now() - 20_000,
        completed_at_ms: now() - 20_000,
        attempt_number: 1,
        retry_scheduled_at_ms: null,
        last_error: null,
        exception_case_ids: ["case_paystack_amount_mismatch"],
      },
    ],
    latest_receipt: {
      recon_receipt_id: "recon_receipt_paystack_exception",
      run_id: "recon_run_paystack_exception",
      subject_id: "recon_subject_paystack_exception",
      normalized_result: "unmatched",
      outcome: "unmatched",
      summary: "Paystack verification mismatch detected.",
      details: {
        amount_minor_expected: "125000",
        amount_minor_observed: "130000",
        currency: "NGN",
      },
      created_at_ms: now() - 20_000,
    },
    expected_facts: [
      {
        fact_id: "expected_fact_paystack_reference",
        run_id: "recon_run_paystack_exception",
        subject_id: "recon_subject_paystack_exception",
        fact_type: "expected",
        fact_key: "reference",
        fact_value: connectorReference,
        source_kind: "receipt",
        source_table: "execution_core_receipts",
        source_id: "receipt_paystack_succeeded",
        metadata: null,
        observed_at_ms: null,
        created_at_ms: now() - 25_000,
      },
    ],
    observed_facts: [
      {
        fact_id: "observed_fact_paystack_amount",
        run_id: "recon_run_paystack_exception",
        subject_id: "recon_subject_paystack_exception",
        fact_type: "observed",
        fact_key: "amount_minor",
        fact_value: 130000,
        source_kind: "webhook",
        source_table: "paystack.webhook_events",
        source_id: "webhook_paystack_transfer_success",
        metadata: null,
        observed_at_ms: now() - 22_000,
        created_at_ms: now() - 22_000,
      },
    ],
  };

  const exceptionCase = {
    case_id: "case_paystack_amount_mismatch",
    tenant_id: tenantId,
    subject_id: "recon_subject_paystack_exception",
    intent_id: intentId,
    job_id: jobId,
    adapter_id: "adapter_paystack",
    category: "amount_mismatch",
    severity: "high",
    state: "open",
    summary: "Paystack settlement amount did not match the protected execution request.",
    machine_reason: "amount_mismatch",
    dedupe_key: "paystack:amount_mismatch:PSK_REF_12345",
    cluster_key: "paystack:amount_mismatch",
    first_seen_at_ms: now() - 25_000,
    last_seen_at_ms: now() - 20_000,
    occurrence_count: 1,
    created_at_ms: now() - 25_000,
    updated_at_ms: now() - 20_000,
    resolved_at_ms: null,
    latest_run_id: "recon_run_paystack_exception",
    latest_outcome_id: "recon_outcome_paystack_exception",
    latest_recon_receipt_id: "recon_receipt_paystack_exception",
    latest_execution_receipt_id: "receipt_paystack_succeeded",
    latest_evidence_snapshot_id: "evidence_snapshot_paystack_exception",
    last_actor: "recon_core",
    evidence: [
      {
        evidence_id: "evidence_paystack_execution",
        case_id: "case_paystack_amount_mismatch",
        evidence_type: "execution_snapshot",
        source_table: "paystack.executions",
        source_id: "pay_exec_9001",
        observed_at_ms: now() - 24_000,
        payload: {
          reference: connectorReference,
          provider_reference: "refund_9001",
          remote_id: "trx_9001",
          status: "success",
          amount: 125000,
          currency: "NGN",
          destination_reference: "BANK:044:0123456789",
        },
        created_at_ms: now() - 24_000,
      },
      {
        evidence_id: "evidence_paystack_webhook",
        case_id: "case_paystack_amount_mismatch",
        evidence_type: "provider_webhook",
        source_table: "paystack.webhook_events",
        source_id: "webhook_paystack_transfer_success",
        observed_at_ms: now() - 22_000,
        payload: {
          event: "refund.processed",
          reference: connectorReference,
          provider_reference: "refund_9001",
          remote_id: "evt_9001",
          status: "success",
          data: {
            reference: connectorReference,
            status: "success",
            gateway_response: "Refund processed by Paystack",
            amount: 130000,
            currency: "NGN",
            destination_reference: "BANK:044:0123456789",
          },
        },
        created_at_ms: now() - 22_000,
      },
    ],
  };

  const detail = {
    tenant_id: tenantId,
    case: exceptionCase,
    events: [
      {
        event_id: "exception_event_paystack_open",
        case_id: "case_paystack_amount_mismatch",
        event_type: "opened",
        from_state: null,
        to_state: "open",
        actor: "recon_core",
        reason: "Paystack verification mismatch detected after webhook observation.",
        payload: {
          machine_reason: "amount_mismatch",
        },
        created_at_ms: now() - 20_000,
      },
    ],
    resolution_history: [],
  };

  const requestExceptions = {
    tenant_id: tenantId,
    intent_id: intentId,
    cases: [exceptionCase],
  };

  const unified = {
    tenant_id: tenantId,
    intent_id: intentId,
    request: {
      tenant_id: tenantId,
      intent_id: intentId,
      job_id: jobId,
      adapter_id: "adapter_paystack",
      state: "succeeded",
      classification: "completed",
      attempt: 1,
      max_attempts: 3,
      replay_count: 0,
      updated_at_ms: now() - 20_000,
      request_id: requestId,
      correlation_id: correlationId,
      idempotency_key: "idem_paystack_exception",
    },
    receipt: record.receipt,
    history: record.history,
    callbacks: record.callbacks,
    reconciliation,
    exceptions: requestExceptions,
    dashboard_status: "mismatch_detected",
    recon_status: "unmatched",
    reconciliation_eligible: true,
    latest_execution_receipt_id: "receipt_paystack_succeeded",
    latest_recon_receipt_id: "recon_receipt_paystack_exception",
    latest_evidence_snapshot_id: "evidence_snapshot_paystack_exception",
    exception_summary: {
      total_cases: 1,
      unresolved_cases: 1,
      highest_severity: "high",
      categories: ["amount_mismatch"],
      open_case_ids: ["case_paystack_amount_mismatch"],
    },
    evidence_references: [
      {
        kind: "execution",
        label: "Paystack execution row",
        source_table: "paystack.executions",
        source_id: "pay_exec_9001",
        observed_at_ms: now() - 24_000,
      },
      {
        kind: "webhook",
        label: "Paystack webhook evidence",
        source_table: "paystack.webhook_events",
        source_id: "webhook_paystack_transfer_success",
        observed_at_ms: now() - 22_000,
      },
    ],
  };

  unifiedRequests.set(intentId, unified);
  exceptionCases.set(exceptionCase.case_id, exceptionCase);
  exceptionDetails.set(exceptionCase.case_id, detail);
}

function filterExceptionCases(url) {
  const state = (url.searchParams.get("state") ?? "").trim();
  const severity = (url.searchParams.get("severity") ?? "").trim();
  const category = (url.searchParams.get("category") ?? "").trim();
  const adapterId = (url.searchParams.get("adapter_id") ?? "").trim();
  const search = (url.searchParams.get("search") ?? "").trim().toLowerCase();
  const includeTerminal = (url.searchParams.get("include_terminal") ?? "false") === "true";
  const limit = Number(url.searchParams.get("limit") ?? "100");
  const offset = Number(url.searchParams.get("offset") ?? "0");

  let rows = [...exceptionCases.values()];

  if (!includeTerminal) {
    rows = rows.filter(
      (row) =>
        row.state !== "resolved" &&
        row.state !== "dismissed" &&
        row.state !== "false_positive"
    );
  }
  if (state) rows = rows.filter((row) => row.state === state);
  if (severity) rows = rows.filter((row) => row.severity === severity);
  if (category) rows = rows.filter((row) => row.category === category);
  if (adapterId) rows = rows.filter((row) => row.adapter_id === adapterId);
  if (search) {
    rows = rows.filter((row) =>
      [
        row.case_id,
        row.intent_id,
        row.summary,
        row.machine_reason,
        row.adapter_id,
        row.category,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase()
        .includes(search)
    );
  }

  return rows.slice(offset, offset + limit);
}

seedPaystackExceptionFixture();

function apiAuditRows() {
  return apiKeys
    .filter((key) => !key.revoked_at_ms)
    .map((key, index) => ({
      audit_id: `audit_api_${index + 1}`,
      request_id: `req_api_${index + 1}`,
      channel: "api",
      endpoint: "/api/requests",
      method: "POST",
      principal_id: "backend-service",
      submitter_kind: "api_key_holder",
      auth_scheme: "api_key",
      intent_kind: "solana.transfer.v1",
      validation_result: "accepted",
      accepted_intent_id: `intent_api_${index + 1}`,
      accepted_job_id: `job_api_${index + 1}`,
      details_json: {
        api_key_id: key.key_id,
      },
      created_at_ms: key.created_at_ms,
    }));
}

function filterAudits(url) {
  const channel = url.searchParams.get("channel");
  const validationResult = url.searchParams.get("validation_result");
  const limit = Number(url.searchParams.get("limit") ?? "100");
  const offset = Number(url.searchParams.get("offset") ?? "0");
  const all = channel === "webhook" ? webhookAudits : channel === "api" ? apiAuditRows() : [...apiAuditRows(), ...webhookAudits];
  const filtered = validationResult
    ? all.filter((audit) => audit.validation_result === validationResult)
    : all;
  return filtered.slice(offset, offset + limit);
}

async function handleIngress(req, res) {
  const url = new URL(req.url, `http://127.0.0.1:${ingressPort}`);
  const path = url.pathname;

  if (req.method === "GET" && path === "/healthz") {
    textResponse(res, 200, "ok");
    return;
  }

  if (req.method === "POST" && path === "/api/requests") {
    const body = await readJson(req);
    const record = createRequestRecord(body);
    jsonResponse(res, 200, {
      ok: true,
      tenant_id: tenantId,
      intent_id: record.intent_id,
      job_id: record.job_id,
      adapter_id: record.adapter_id,
      state: record.state,
      route_rule: record.route_rule,
    });
    return;
  }

  const apiKeyCollection = path.match(/^\/api\/internal\/tenants\/([^/]+)\/api-keys$/);
  if (apiKeyCollection) {
    if (req.method === "GET") {
      jsonResponse(res, 200, { ok: true, keys: apiKeys });
      return;
    }
    if (req.method === "POST") {
      const body = await readJson(req);
      apiKeys.push({
        key_id: body.key_id ?? nextId("api_key"),
        label: body.label ?? "default",
        key_prefix: body.key_prefix ?? "azm_mock",
        key_last4: body.key_last4 ?? "1234",
        created_at_ms: body.created_at_ms ?? now(),
        revoked_at_ms: null,
        last_used_at_ms: null,
      });
      jsonResponse(res, 200, { ok: true });
      return;
    }
  }

  const apiKeyRevoke = path.match(/^\/api\/internal\/tenants\/([^/]+)\/api-keys\/([^/]+)\/revoke$/);
  if (apiKeyRevoke && req.method === "POST") {
    const [, , keyId] = apiKeyRevoke;
    const key = apiKeys.find((candidate) => candidate.key_id === keyId);
    if (key) {
      key.revoked_at_ms = now();
    }
    jsonResponse(res, 200, { ok: true });
    return;
  }

  const webhookKeyCollection = path.match(/^\/api\/internal\/tenants\/([^/]+)\/webhook-keys$/);
  if (webhookKeyCollection) {
    if (req.method === "GET") {
      const source = (url.searchParams.get("source") ?? "default").trim();
      const includeInactive = (url.searchParams.get("include_inactive") ?? "true") !== "false";
      const rows = webhookKeys.filter(
        (key) => key.source === source && (includeInactive || key.active)
      );
      jsonResponse(res, 200, { ok: true, keys: rows });
      return;
    }
    if (req.method === "POST") {
      const body = await readJson(req);
      const keyId = nextId("webhook_key");
      const secret = `whsec_${randomUUID().replace(/-/g, "")}`;
      const record = {
        key_id: keyId,
        tenant_id: tenantId,
        source: body.source ?? "default",
        secret_last4: secret.slice(-4),
        active: true,
        created_by_principal_id: body.created_by_principal_id ?? "demo@azums.dev",
        created_at_ms: now(),
        revoked_at_ms: null,
        expires_at_ms: null,
        last_used_at_ms: null,
      };
      webhookKeys.push(record);
      jsonResponse(res, 200, {
        ok: true,
        webhook_key: {
          ...record,
          secret,
        },
        rotation: {
          rotated_previous_keys: 0,
          previous_keys_valid_until_ms: null,
          grace_seconds: body.grace_seconds ?? 900,
        },
      });
      return;
    }
  }

  const webhookKeyRevoke = path.match(/^\/api\/internal\/tenants\/([^/]+)\/webhook-keys\/([^/]+)\/revoke$/);
  if (webhookKeyRevoke && req.method === "POST") {
    const [, , keyId] = webhookKeyRevoke;
    const key = webhookKeys.find((candidate) => candidate.key_id === keyId);
    if (key) {
      key.active = false;
      key.revoked_at_ms = now();
    }
    jsonResponse(res, 200, { ok: true });
    return;
  }

  const quotaUpdate = path.match(/^\/api\/internal\/tenants\/([^/]+)\/quota$/);
  if (quotaUpdate && req.method === "POST") {
    jsonResponse(res, 200, { ok: true });
    return;
  }

  jsonResponse(res, 404, { ok: false, error: `No mock ingress route for ${req.method} ${path}` });
}

async function handleStatus(req, res) {
  const url = new URL(req.url, `http://127.0.0.1:${statusPort}`);
  const path = url.pathname;

  if ((req.method === "GET" && path === "/healthz") || (req.method === "GET" && path === "/status/health")) {
    jsonResponse(res, 200, { ok: true, status_api_reachable: true });
    return;
  }

  if (req.method === "GET" && path === "/status/reconciliation/rollout-summary") {
    jsonResponse(res, 200, {
      tenant_id: tenantId,
      window: {
        lookback_hours: Number(url.searchParams.get("lookback_hours") ?? "168"),
        started_at_ms: now() - 168 * 60 * 60 * 1000,
        generated_at_ms: now(),
      },
      intake: {
        eligible_execution_receipts: 1,
        intake_signals: 1,
        subjects_total: 1,
        dirty_subjects: 0,
        retry_scheduled_subjects: 0,
      },
      outcomes: {
        matched: 0,
        partially_matched: 0,
        unmatched: 1,
        pending_observation: 0,
        stale: 0,
        manual_review_required: 1,
      },
      exceptions: {
        total_cases: 1,
        unresolved_cases: 1,
        high_or_critical_cases: 1,
        false_positive_cases: 0,
        exception_rate: 1,
        false_positive_rate: 0,
        stale_rate: 0,
      },
      latency: {
        avg_recon_latency_ms: 2100,
        p95_recon_latency_ms: 2100,
        max_recon_latency_ms: 2100,
        avg_operator_handling_ms: null,
        p95_operator_handling_ms: null,
      },
      queries: {
        sampled_intent_id: "intent_paystack_exception",
        exception_index_query_ms: 14,
        unified_request_query_ms: 17,
      },
    });
    return;
  }

  if (req.method === "GET" && path === "/status/jobs") {
    jsonResponse(res, 200, {
      jobs: [...requests.values()].map((record) => ({
        job_id: record.job_id,
        intent_id: record.intent_id,
        adapter_id: record.adapter_id,
        state: record.state,
        classification: record.classification,
        attempt: record.attempt,
        max_attempts: record.max_attempts,
        replay_count: record.replay_count,
        updated_at_ms: record.updated_at_ms,
      })),
    });
    return;
  }

  if (req.method === "GET" && path === "/status/tenant/intake-audits") {
    jsonResponse(res, 200, {
      tenant_id: tenantId,
      audits: filterAudits(url),
    });
    return;
  }

  if (req.method === "GET" && path === "/status/tenant/callback-destination") {
    jsonResponse(res, 200, {
      tenant_id: tenantId,
      configured: false,
    });
    return;
  }

  const requestRoot = path.match(/^\/status\/requests\/([^/]+)$/);
  if (requestRoot && req.method === "GET") {
    const record = requests.get(requestRoot[1]);
    if (!record) {
      jsonResponse(res, 404, { error: "Request not found." });
      return;
    }
    jsonResponse(res, 200, {
      tenant_id: record.tenant_id,
      intent_id: record.intent_id,
      job_id: record.job_id,
      adapter_id: record.adapter_id,
      state: record.state,
      classification: record.classification,
      attempt: record.attempt,
      max_attempts: record.max_attempts,
      replay_count: record.replay_count,
      updated_at_ms: record.updated_at_ms,
      request_id: record.request_id,
      correlation_id: record.correlation_id,
      idempotency_key: record.idempotency_key,
      last_failure: record.last_failure,
    });
    return;
  }

  const requestReceipt = path.match(/^\/status\/requests\/([^/]+)\/receipt$/);
  if (requestReceipt && req.method === "GET") {
    const record = requests.get(requestReceipt[1]);
    if (!record) {
      jsonResponse(res, 404, { error: "Receipt not found." });
      return;
    }
    jsonResponse(res, 200, record.receipt);
    return;
  }

  const requestHistory = path.match(/^\/status\/requests\/([^/]+)\/history$/);
  if (requestHistory && req.method === "GET") {
    const record = requests.get(requestHistory[1]);
    if (!record) {
      jsonResponse(res, 404, { error: "History not found." });
      return;
    }
    jsonResponse(res, 200, record.history);
    return;
  }

  const requestCallbacks = path.match(/^\/status\/requests\/([^/]+)\/callbacks$/);
  if (requestCallbacks && req.method === "GET") {
    const record = requests.get(requestCallbacks[1]);
    if (!record) {
      jsonResponse(res, 404, { error: "Callbacks not found." });
      return;
    }
    jsonResponse(res, 200, record.callbacks);
    return;
  }

  const requestReplay = path.match(/^\/status\/requests\/([^/]+)\/replay$/);
  if (requestReplay && req.method === "POST") {
    const record = requests.get(requestReplay[1]);
    if (!record) {
      jsonResponse(res, 404, { error: "Replay source not found." });
      return;
    }
    const replayJobId = nextId("job");
    record.replay_count += 1;
    record.job_id = replayJobId;
    record.state = "succeeded";
    record.classification = "completed";
    record.updated_at_ms = now();
    record.last_failure = undefined;
    record.history.transitions.push(
      createTransition(record.intent_id, replayJobId, "replayed", "replayed", "replayed", "Replay requested."),
      createTransition(record.intent_id, replayJobId, "succeeded", "completed", "succeeded", "Replay completed successfully.")
    );
    record.receipt.entries.push(
      createReceiptEntry(record.intent_id, replayJobId, record.replay_count + 1, "replayed", "replayed", "Replay started."),
      createReceiptEntry(record.intent_id, replayJobId, record.replay_count + 1, "succeeded", "completed", "Replay completed successfully.")
    );
    record.callbacks.callbacks = [createCallback(record.intent_id, "succeeded")];
    jsonResponse(res, 200, {
      tenant_id: tenantId,
      intent_id: record.intent_id,
      source_job_id: requestReplay[1],
      replay_job_id: replayJobId,
      replay_count: record.replay_count,
      state: record.state,
      route_adapter_id: record.adapter_id,
      details: {
        replay_reason: "playground manual replay",
      },
    });
    return;
  }

  const callbackDetail = path.match(/^\/status\/callbacks\/([^/]+)$/);
  if (callbackDetail && req.method === "GET") {
    const callbackId = callbackDetail[1];
    const record = [...requests.values()].find((candidate) =>
      candidate.callbacks.callbacks.some((callback) => callback.callback_id === callbackId)
    );
    if (!record) {
      jsonResponse(res, 404, { error: "Callback not found." });
      return;
    }
    const callback = record.callbacks.callbacks.find((candidate) => candidate.callback_id === callbackId);
    jsonResponse(res, 200, {
      ok: true,
      callback_id: callbackId,
      intent_id: record.intent_id,
      callback,
      request: {
        tenant_id: record.tenant_id,
        intent_id: record.intent_id,
        job_id: record.job_id,
        adapter_id: record.adapter_id,
        state: record.state,
        classification: record.classification,
        attempt: record.attempt,
        max_attempts: record.max_attempts,
        replay_count: record.replay_count,
        updated_at_ms: record.updated_at_ms,
      },
      receipt: record.receipt,
      history: record.history,
    });
    return;
  }

  const unifiedRequest = path.match(/^\/status\/requests\/([^/]+)\/unified$/);
  if (unifiedRequest && req.method === "GET") {
    const unified = unifiedRequests.get(unifiedRequest[1]);
    if (!unified) {
      jsonResponse(res, 404, { error: "Unified request not found." });
      return;
    }
    jsonResponse(res, 200, unified);
    return;
  }

  if (req.method === "GET" && path === "/status/exceptions") {
    jsonResponse(res, 200, {
      tenant_id: tenantId,
      cases: filterExceptionCases(url),
    });
    return;
  }

  const exceptionDetail = path.match(/^\/status\/exceptions\/([^/]+)$/);
  if (exceptionDetail && req.method === "GET") {
    const detail = exceptionDetails.get(exceptionDetail[1]);
    if (!detail) {
      jsonResponse(res, 404, { error: "Exception case not found." });
      return;
    }
    jsonResponse(res, 200, detail);
    return;
  }

  jsonResponse(res, 404, { ok: false, error: `No mock status route for ${req.method} ${path}` });
}

const ingressServer = http.createServer((req, res) => {
  Promise.resolve(handleIngress(req, res)).catch((error) => {
    jsonResponse(res, 500, { ok: false, error: String(error) });
  });
});

const statusServer = http.createServer((req, res) => {
  Promise.resolve(handleStatus(req, res)).catch((error) => {
    jsonResponse(res, 500, { ok: false, error: String(error) });
  });
});

function shutdown() {
  ingressServer.close();
  statusServer.close();
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

ingressServer.listen(ingressPort, "127.0.0.1", () => {
  console.log(`mock ingress listening on ${ingressPort}`);
});

statusServer.listen(statusPort, "127.0.0.1", () => {
  console.log(`mock status listening on ${statusPort}`);
});
