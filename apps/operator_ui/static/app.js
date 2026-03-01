const el = {
  operatorMeta: document.getElementById("operator-meta"),
  activityLog: document.getElementById("activity-log"),
  jobsForm: document.getElementById("jobs-form"),
  jobsBody: document.getElementById("jobs-body"),
  refreshJobs: document.getElementById("refresh-jobs"),
  jobsState: document.getElementById("jobs-state"),
  jobsLimit: document.getElementById("jobs-limit"),
  jobsOffset: document.getElementById("jobs-offset"),
  requestForm: document.getElementById("request-form"),
  refreshRequest: document.getElementById("refresh-request"),
  intentIdInput: document.getElementById("intent-id-input"),
  requestSummary: document.getElementById("request-summary"),
  receiptJson: document.getElementById("receipt-json"),
  historyJson: document.getElementById("history-json"),
  callbacksJson: document.getElementById("callbacks-json"),
  replayForm: document.getElementById("replay-form"),
  replayReason: document.getElementById("replay-reason"),
  auditsForm: document.getElementById("audits-form"),
  refreshAudits: document.getElementById("refresh-audits"),
  auditValidation: document.getElementById("audit-validation"),
  auditChannel: document.getElementById("audit-channel"),
  auditLimit: document.getElementById("audit-limit"),
  auditBody: document.getElementById("audit-body"),
  callbackForm: document.getElementById("callback-form"),
  loadCallbackDestination: document.getElementById("load-callback-destination"),
  deleteCallbackDestination: document.getElementById("delete-callback-destination"),
  callbackDeliveryUrl: document.getElementById("callback-delivery-url"),
  callbackTimeoutMs: document.getElementById("callback-timeout-ms"),
  callbackAllowedHosts: document.getElementById("callback-allowed-hosts"),
  callbackEnabled: document.getElementById("callback-enabled"),
  callbackAllowPrivate: document.getElementById("callback-allow-private"),
  callbackDestinationJson: document.getElementById("callback-destination-json"),
  clearLog: document.getElementById("clear-log"),
};

const state = {
  selectedIntentId: null,
  uiConfig: null,
};

document.addEventListener("DOMContentLoaded", async () => {
  bindEvents();
  await bootstrap();
});

function bindEvents() {
  el.jobsForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    await loadJobs();
  });
  el.refreshJobs.addEventListener("click", loadJobs);
  el.jobsBody.addEventListener("click", async (event) => {
    const button = event.target.closest("button[data-intent-id]");
    if (!button) {
      return;
    }
    const intentId = button.dataset.intentId;
    el.intentIdInput.value = intentId;
    await loadIntent(intentId);
  });

  el.requestForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    await loadIntent();
  });
  el.refreshRequest.addEventListener("click", async () => {
    if (!state.selectedIntentId) {
      log("No request selected yet.", "warn");
      return;
    }
    await loadIntent(state.selectedIntentId);
  });

  el.replayForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    await triggerReplay();
  });

  el.auditsForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    await loadAudits();
  });
  el.refreshAudits.addEventListener("click", loadAudits);

  el.callbackForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    await upsertCallbackDestination();
  });
  el.loadCallbackDestination.addEventListener("click", loadCallbackDestination);
  el.deleteCallbackDestination.addEventListener("click", deleteCallbackDestination);

  el.clearLog.addEventListener("click", () => {
    el.activityLog.textContent = "";
  });
}

async function bootstrap() {
  try {
    const [config, health] = await Promise.all([uiConfig(), uiHealth()]);
    state.uiConfig = config;
    renderOperatorMeta(config, health);
    log(
      `Operator UI ready for tenant=${config.tenant_id} principal=${config.principal_id}`,
      "ok"
    );
  } catch (error) {
    log(`Failed to load operator config: ${error.message}`, "error");
  }

  await Promise.all([loadJobs(), loadAudits(), loadCallbackDestination()]);
}

function renderOperatorMeta(config, health) {
  const pieces = [
    badge(`tenant ${config.tenant_id}`),
    badge(`principal ${config.principal_id}`),
    badge(`role ${config.principal_role}`),
    badge(config.has_bearer_token ? "token configured" : "no token"),
    badge(
      health.status_api_reachable
        ? `status_api ${health.status_api_status_code ?? "ok"}`
        : "status_api unreachable"
    ),
  ];
  el.operatorMeta.innerHTML = pieces.join("");
}

function badge(text) {
  return `<span class="badge">${escapeHtml(text)}</span>`;
}

async function uiConfig() {
  const response = await fetch("/api/ui/config");
  return parseResponse(response);
}

async function uiHealth() {
  const response = await fetch("/api/ui/health");
  return parseResponse(response);
}

async function statusApi(path, { method = "GET", query, body } = {}) {
  const url = new URL(`/api/ui/status/${path}`, window.location.origin);
  if (query) {
    for (const [key, value] of Object.entries(query)) {
      if (value === undefined || value === null || value === "") {
        continue;
      }
      url.searchParams.set(key, String(value));
    }
  }

  const options = { method, headers: {} };
  if (body !== undefined) {
    options.headers["content-type"] = "application/json";
    options.body = JSON.stringify(body);
  }

  const response = await fetch(url, options);
  return parseResponse(response);
}

async function parseResponse(response) {
  const text = await response.text();
  let data = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch (_error) {
      data = { raw: text };
    }
  }

  if (!response.ok) {
    const message =
      data?.error ||
      data?.message ||
      data?.raw ||
      `request failed with status ${response.status}`;
    throw new Error(message);
  }
  return data;
}

async function loadJobs() {
  const stateFilter = el.jobsState.value.trim();
  const limit = Number(el.jobsLimit.value || 20);
  const offset = Number(el.jobsOffset.value || 0);

  try {
    const data = await statusApi("jobs", {
      query: {
        state: stateFilter || undefined,
        limit,
        offset,
      },
    });
    renderJobs(data.jobs || []);
    log(`Loaded ${data.jobs?.length || 0} jobs`, "ok");
  } catch (error) {
    renderJobs([]);
    log(`Jobs query failed: ${error.message}`, "error");
  }
}

function renderJobs(jobs) {
  if (!jobs.length) {
    el.jobsBody.innerHTML =
      '<tr><td colspan="6">No jobs found for current filter.</td></tr>';
    return;
  }

  el.jobsBody.innerHTML = jobs
    .map((job) => {
      const updated = formatTime(job.updated_at_ms);
      return `<tr>
        <td title="${escapeHtml(job.intent_id)}">${shorten(job.intent_id)}</td>
        <td>${escapeHtml(job.state)}</td>
        <td>${escapeHtml(job.classification)}</td>
        <td>${escapeHtml(String(job.attempt))}/${escapeHtml(String(job.max_attempts))}</td>
        <td>${escapeHtml(updated)}</td>
        <td><button class="btn ghost" data-intent-id="${escapeHtml(
          job.intent_id
        )}" type="button">Open</button></td>
      </tr>`;
    })
    .join("");
}

async function loadIntent(explicitIntentId) {
  const intentId = (explicitIntentId || el.intentIdInput.value).trim();
  if (!intentId) {
    log("Provide an intent id before loading request details.", "warn");
    return;
  }

  state.selectedIntentId = intentId;
  el.intentIdInput.value = intentId;

  try {
    const [request, receipt, history, callbacks] = await Promise.all([
      statusApi(`requests/${encodeURIComponent(intentId)}`),
      statusApi(`requests/${encodeURIComponent(intentId)}/receipt`),
      statusApi(`requests/${encodeURIComponent(intentId)}/history`),
      statusApi(`requests/${encodeURIComponent(intentId)}/callbacks`, {
        query: { include_attempts: true, attempt_limit: 25 },
      }),
    ]);

    renderRequestSummary(request);
    renderJson(el.receiptJson, receipt);
    renderJson(el.historyJson, history);
    renderJson(el.callbacksJson, callbacks);
    log(`Loaded request details for ${intentId}`, "ok");
  } catch (error) {
    renderRequestSummary(null);
    renderJson(el.receiptJson, { error: error.message });
    renderJson(el.historyJson, { error: error.message });
    renderJson(el.callbacksJson, { error: error.message });
    log(`Request lookup failed for ${intentId}: ${error.message}`, "error");
  }
}

function renderRequestSummary(request) {
  if (!request) {
    el.requestSummary.innerHTML = "";
    return;
  }

  const cards = [
    { label: "Intent", value: request.intent_id },
    { label: "State", value: request.state },
    { label: "Classification", value: request.classification },
    { label: "Adapter", value: request.adapter_id },
    { label: "Attempt", value: `${request.attempt}/${request.max_attempts}` },
    { label: "Replay Count", value: String(request.replay_count) },
    { label: "Updated", value: formatTime(request.updated_at_ms) },
    { label: "Request ID", value: request.request_id },
  ];

  el.requestSummary.innerHTML = cards
    .map(
      (item) => `<div class="summary-card">
        <span>${escapeHtml(item.label)}</span>
        <strong>${escapeHtml(item.value ?? "n/a")}</strong>
      </div>`
    )
    .join("");
}

async function triggerReplay() {
  if (!state.selectedIntentId) {
    log("Select a request before replay.", "warn");
    return;
  }

  const reason = el.replayReason.value.trim() || "operator replay from operator_ui";
  try {
    const replay = await statusApi(
      `requests/${encodeURIComponent(state.selectedIntentId)}/replay`,
      {
        method: "POST",
        body: { reason },
      }
    );
    renderJson(el.historyJson, replay);
    log(
      `Replay triggered: source=${replay.source_job_id} new=${replay.replay_job_id}`,
      "ok"
    );
    await loadIntent(state.selectedIntentId);
    await loadJobs();
  } catch (error) {
    log(`Replay failed: ${error.message}`, "error");
  }
}

async function loadAudits() {
  const validation = el.auditValidation.value;
  const channel = el.auditChannel.value;
  const limit = Number(el.auditLimit.value || 20);

  try {
    const data = await statusApi("tenant/intake-audits", {
      query: {
        validation_result: validation || undefined,
        channel: channel || undefined,
        limit,
        offset: 0,
      },
    });
    renderAudits(data.audits || []);
    log(`Loaded ${data.audits?.length || 0} intake audits`, "ok");
  } catch (error) {
    renderAudits([]);
    log(`Intake audit query failed: ${error.message}`, "error");
  }
}

function renderAudits(audits) {
  if (!audits.length) {
    el.auditBody.innerHTML =
      '<tr><td colspan="5">No intake audits found for current filter.</td></tr>';
    return;
  }

  el.auditBody.innerHTML = audits
    .map((row) => {
      return `<tr>
        <td title="${escapeHtml(row.request_id)}">${shorten(row.request_id)}</td>
        <td>${escapeHtml(row.validation_result)}</td>
        <td>${escapeHtml(row.rejection_reason || "-")}</td>
        <td>${escapeHtml(row.channel)}</td>
        <td>${escapeHtml(formatTime(row.created_at_ms))}</td>
      </tr>`;
    })
    .join("");
}

async function loadCallbackDestination() {
  try {
    const data = await statusApi("tenant/callback-destination");
    renderJson(el.callbackDestinationJson, data);

    if (data.configured && data.destination) {
      el.callbackDeliveryUrl.value = data.destination.delivery_url || "";
      el.callbackTimeoutMs.value = String(data.destination.timeout_ms || 10000);
      el.callbackAllowedHosts.value = (data.destination.allowed_hosts || []).join(",");
      el.callbackEnabled.checked = Boolean(data.destination.enabled);
      el.callbackAllowPrivate.checked = Boolean(
        data.destination.allow_private_destinations
      );
    }

    log(
      data.configured
        ? "Loaded callback destination configuration."
        : "No callback destination configured.",
      "ok"
    );
  } catch (error) {
    renderJson(el.callbackDestinationJson, { error: error.message });
    log(`Failed to load callback destination: ${error.message}`, "error");
  }
}

async function upsertCallbackDestination() {
  const deliveryUrl = el.callbackDeliveryUrl.value.trim();
  if (!deliveryUrl) {
    log("Delivery URL is required before upsert.", "warn");
    return;
  }

  const allowedHosts = el.callbackAllowedHosts.value
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);

  const payload = {
    delivery_url: deliveryUrl,
    timeout_ms: Number(el.callbackTimeoutMs.value || 10000),
    allow_private_destinations: Boolean(el.callbackAllowPrivate.checked),
    allowed_hosts: allowedHosts,
    enabled: Boolean(el.callbackEnabled.checked),
  };

  try {
    const data = await statusApi("tenant/callback-destination", {
      method: "POST",
      body: payload,
    });
    renderJson(el.callbackDestinationJson, data);
    log("Callback destination upserted.", "ok");
  } catch (error) {
    log(`Callback destination upsert failed: ${error.message}`, "error");
  }
}

async function deleteCallbackDestination() {
  try {
    const data = await statusApi("tenant/callback-destination", {
      method: "DELETE",
    });
    renderJson(el.callbackDestinationJson, data);
    log("Callback destination deleted.", "ok");
  } catch (error) {
    log(`Delete callback destination failed: ${error.message}`, "error");
  }
}

function renderJson(node, value) {
  node.textContent = JSON.stringify(value, null, 2);
}

function log(message, level = "info") {
  const now = new Date();
  const stamp = now.toISOString();
  const line = `[${stamp}] ${message}`;
  const className = level === "error" ? "error" : level === "warn" ? "warn" : "ok";
  const entry = document.createElement("div");
  entry.className = "log-line";
  entry.innerHTML = `<strong>${escapeHtml(className.toUpperCase())}</strong> ${escapeHtml(
    line
  )}`;
  el.activityLog.prepend(entry);
}

function formatTime(value) {
  if (!value) {
    return "-";
  }
  const date = new Date(Number(value));
  if (Number.isNaN(date.getTime())) {
    return String(value);
  }
  return date.toLocaleString();
}

function shorten(value) {
  if (!value) {
    return "-";
  }
  if (value.length <= 20) {
    return value;
  }
  return `${value.slice(0, 8)}...${value.slice(-8)}`;
}

function escapeHtml(input) {
  return String(input)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}
