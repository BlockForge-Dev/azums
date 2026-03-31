"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { EmptyState } from "@/components/ui/empty-state";
import { apiGet, apiRequest, formatMs, shortId } from "@/lib/client-api";
import type {
  IntakeAuditsResponse,
  JobListResponse,
  JobRow,
  OperatorActivityResponse,
  OperatorDeliveriesResponse,
  OperatorDeliveryRedriveResponse,
  OperatorOverviewResponse,
  OperatorSecurityResponse,
  ReplayResponse,
  WorkspaceDetailResponse,
  WorkspaceListResponse,
  WorkspaceSummary,
} from "@/lib/types";

export type CustomerOperatorView =
  | "overview"
  | "jobs"
  | "replay"
  | "dead_letters"
  | "deliveries"
  | "intake_audits"
  | "adapter_health"
  | "security"
  | "workspaces"
  | "activity";

const DEFAULT_REPLAY_REASON = "Operator replay with explicit tenant admin confirmation";

export function CustomerOperatorPage({ view }: { view: CustomerOperatorView }) {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [overview, setOverview] = useState<OperatorOverviewResponse | null>(null);
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [deliveries, setDeliveries] = useState<OperatorDeliveriesResponse | null>(null);
  const [audits, setAudits] = useState<IntakeAuditsResponse | null>(null);
  const [activity, setActivity] = useState<OperatorActivityResponse | null>(null);
  const [security, setSecurity] = useState<OperatorSecurityResponse | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceSummary[]>([]);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState("");
  const [workspaceDetail, setWorkspaceDetail] = useState<WorkspaceDetailResponse | null>(null);
  const [selectedIntent, setSelectedIntent] = useState("");
  const [jobStateFilter, setJobStateFilter] = useState("");
  const [jobAdapterFilter, setJobAdapterFilter] = useState("");
  const [jobAttemptFilter, setJobAttemptFilter] = useState("");
  const [jobSearchFilter, setJobSearchFilter] = useState("");
  const [replayReason, setReplayReason] = useState(DEFAULT_REPLAY_REASON);
  const [replayConfirm, setReplayConfirm] = useState(false);
  const [replaying, setReplaying] = useState(false);
  const [deliveryStateFilter, setDeliveryStateFilter] = useState("");
  const [redrivingId, setRedrivingId] = useState<string | null>(null);
  const [auditValidationFilter, setAuditValidationFilter] = useState("");
  const [auditChannelFilter, setAuditChannelFilter] = useState("");

  useEffect(() => {
    void loadView();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  useEffect(() => {
    if (view !== "workspaces" || !selectedWorkspaceId) return;
    void loadWorkspaceDetail(selectedWorkspaceId);
  }, [selectedWorkspaceId, view]);

  const filteredJobs = useMemo(() => {
    const stateFilter = jobStateFilter.trim().toLowerCase();
    const adapterFilter = jobAdapterFilter.trim().toLowerCase();
    const search = jobSearchFilter.trim().toLowerCase();
    const attempt = Number(jobAttemptFilter || "0");
    return jobs.filter((job) => {
      if (stateFilter && !job.state.toLowerCase().includes(stateFilter)) return false;
      if (adapterFilter && !job.adapter_id.toLowerCase().includes(adapterFilter)) return false;
      if (attempt > 0 && job.attempt !== attempt) return false;
      if (
        search &&
        !`${job.intent_id} ${job.job_id} ${job.failure_code ?? ""} ${job.failure_message ?? ""}`
          .toLowerCase()
          .includes(search)
      ) {
        return false;
      }
      return true;
    });
  }, [jobAdapterFilter, jobAttemptFilter, jobSearchFilter, jobStateFilter, jobs]);

  const deadLetterJobs = useMemo(
    () => filteredJobs.filter((job) => job.state.toLowerCase().includes("dead")),
    [filteredJobs]
  );

  const replayCandidates = useMemo(
    () =>
      filteredJobs.filter((job) => {
        const state = job.state.toLowerCase();
        return state.includes("failed") || state.includes("dead") || state.includes("blocked");
      }),
    [filteredJobs]
  );

  useEffect(() => {
    if (!selectedIntent && filteredJobs.length > 0) {
      setSelectedIntent(filteredJobs[0].intent_id);
    }
  }, [filteredJobs, selectedIntent]);

  async function loadView() {
    setLoading(true);
    setError(null);
    setMessage(null);
    try {
      switch (view) {
        case "overview": {
          const [nextOverview, jobList] = await Promise.all([
            apiGet<OperatorOverviewResponse>("operator/overview"),
            apiGet<JobListResponse>("status/jobs?limit=240&offset=0"),
          ]);
          setOverview(nextOverview);
          setJobs(jobList.jobs ?? []);
          break;
        }
        case "jobs":
        case "replay":
        case "dead_letters": {
          const jobList = await apiGet<JobListResponse>("status/jobs?limit=240&offset=0");
          setJobs(jobList.jobs ?? []);
          break;
        }
        case "deliveries": {
          const nextDeliveries = await apiGet<OperatorDeliveriesResponse>(
            `operator/deliveries?limit=160&offset=0${
              deliveryStateFilter ? `&state=${encodeURIComponent(deliveryStateFilter)}` : ""
            }`
          );
          setDeliveries(nextDeliveries);
          break;
        }
        case "intake_audits": {
          const params = new URLSearchParams({ limit: "200", offset: "0" });
          if (auditValidationFilter) params.set("validation_result", auditValidationFilter);
          if (auditChannelFilter) params.set("channel", auditChannelFilter);
          setAudits(
            await apiGet<IntakeAuditsResponse>(`status/tenant/intake-audits?${params.toString()}`)
          );
          break;
        }
        case "adapter_health":
          setOverview(await apiGet<OperatorOverviewResponse>("operator/overview"));
          break;
        case "security":
          setSecurity(await apiGet<OperatorSecurityResponse>("operator/security"));
          break;
        case "workspaces": {
          const nextWorkspaces = await apiGet<WorkspaceListResponse>("account/workspaces");
          setWorkspaces(nextWorkspaces.workspaces ?? []);
          const currentWorkspace =
            selectedWorkspaceId || nextWorkspaces.workspaces.find((row) => row.is_current)?.workspace_id;
          if (currentWorkspace) setSelectedWorkspaceId(currentWorkspace);
          break;
        }
        case "activity":
          setActivity(await apiGet<OperatorActivityResponse>("operator/activity?limit=120"));
          break;
      }
    } catch (loadError: unknown) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoading(false);
    }
  }

  async function loadWorkspaceDetail(workspaceId: string) {
    try {
      setWorkspaceDetail(
        await apiGet<WorkspaceDetailResponse>(
          `account/workspaces/${encodeURIComponent(workspaceId)}/detail`
        )
      );
    } catch (loadError: unknown) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
      setWorkspaceDetail(null);
    }
  }

  async function triggerReplay() {
    if (!selectedIntent.trim()) {
      setError("Select an intent to replay.");
      return;
    }
    if (!replayConfirm) {
      setError("Confirm replay before continuing.");
      return;
    }
    if (replayReason.trim().length < 8) {
      setError("Replay reason must be at least 8 characters.");
      return;
    }
    if (!window.confirm("Replay creates a new execution path. Continue?")) return;
    setReplaying(true);
    setError(null);
    setMessage(null);
    try {
      const response = await apiRequest<ReplayResponse>(
        `status/requests/${encodeURIComponent(selectedIntent)}/replay`,
        {
          method: "POST",
          body: JSON.stringify({ reason: replayReason.trim() }),
        }
      );
      setMessage(`Replay scheduled for ${response.intent_id}. New job ${shortId(response.replay_job_id)}.`);
      await loadView();
    } catch (replayError: unknown) {
      setError(replayError instanceof Error ? replayError.message : String(replayError));
    } finally {
      setReplaying(false);
    }
  }

  async function redriveDelivery(callbackId: string) {
    const reason = window.prompt(
      "Reason for delivery redrive",
      "Tenant operator requested callback delivery retry"
    );
    if (!reason || reason.trim().length < 8) return;
    if (!window.confirm(`Redrive callback delivery ${callbackId}?`)) return;
    setRedrivingId(callbackId);
    setError(null);
    setMessage(null);
    try {
      const response = await apiRequest<OperatorDeliveryRedriveResponse>(
        `operator/deliveries/${encodeURIComponent(callbackId)}/redrive`,
        {
          method: "POST",
          body: JSON.stringify({ reason }),
        }
      );
      setMessage(response.message);
      await loadView();
    } catch (redriveError: unknown) {
      setError(redriveError instanceof Error ? redriveError.message : String(redriveError));
    } finally {
      setRedrivingId(null);
    }
  }

  function exportDeadLetters() {
    if (deadLetterJobs.length === 0) return;
    const header = [
      "intent_id",
      "job_id",
      "state",
      "classification",
      "adapter_id",
      "attempt",
      "max_attempts",
      "updated_at",
      "failure_code",
      "failure_message",
    ];
    const lines = deadLetterJobs.map((job) =>
      [
        sanitizeCsv(job.intent_id),
        sanitizeCsv(job.job_id),
        sanitizeCsv(job.state),
        sanitizeCsv(job.classification),
        sanitizeCsv(job.adapter_id),
        String(job.attempt),
        String(job.max_attempts),
        sanitizeCsv(formatMs(job.updated_at_ms)),
        sanitizeCsv(job.failure_code ?? ""),
        sanitizeCsv(job.failure_message ?? ""),
      ].join(",")
    );
    downloadCsv("azums-dead-letters.csv", [header.join(","), ...lines].join("\n"));
  }

  return (
    <div className="stack">
      <section className="surface hero-surface">
        <p className="eyebrow">Customer Operator</p>
        <h2>{titleForView(view)}</h2>
        <p>{subtitleForView(view)}</p>
      </section>

      <section className="surface">
        <div className="controls inline">
          <button className="btn ghost" type="button" onClick={() => void loadView()}>
            {loading ? "Refreshing..." : "Refresh"}
          </button>
          {view === "dead_letters" ? (
            <button className="btn ghost" type="button" onClick={exportDeadLetters}>
              Export DLQ
            </button>
          ) : null}
        </div>
        {error ? <p className="inline-error">{error}</p> : null}
        {message ? <p className="inline-message">{message}</p> : null}
      </section>

      {view === "overview" ? (
        <>
          <section className="surface summary-grid">
            <Summary label="Total jobs" value={String(overview?.backlog.total_jobs ?? 0)} />
            <Summary label="Queued backlog" value={String(overview?.backlog.queued ?? 0)} />
            <Summary label="Executing" value={String(overview?.backlog.executing ?? 0)} />
            <Summary label="Retry scheduled" value={String(overview?.backlog.retry_scheduled ?? 0)} />
            <Summary label="Terminal failures" value={String(overview?.backlog.failed_terminal ?? 0)} />
            <Summary label="Dead letters" value={String(overview?.backlog.dead_lettered ?? 0)} />
          </section>

          <section className="surface">
            <h3>Failure class distribution</h3>
            <div className="summary-grid">
              {(overview?.failure_classes ?? []).slice(0, 6).map((row) => (
                <Summary key={row.label} label={row.label} value={String(row.count)} />
              ))}
              {(overview?.failure_classes ?? []).length === 0 ? (
                <EmptyState compact title="No failure data" description="Failure class counts will appear here." />
              ) : null}
            </div>
          </section>

          <section className="surface">
            <h3>Callback failure trends</h3>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Bucket</th>
                    <th>Delivered</th>
                    <th>Retrying</th>
                    <th>Terminal failures</th>
                  </tr>
                </thead>
                <tbody>
                  {(overview?.callback_failure_trends ?? []).map((row) => (
                    <tr key={row.bucket}>
                      <td>{row.bucket}</td>
                      <td>{row.delivered}</td>
                      <td>{row.retrying}</td>
                      <td>{row.terminal_failures}</td>
                    </tr>
                  ))}
                  {(overview?.callback_failure_trends ?? []).length === 0 ? (
                    <tr>
                      <td colSpan={4}>No callback trend rows yet.</td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>

          <section className="surface">
            <h3>Top failing intents</h3>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Intent</th>
                    <th>State</th>
                    <th>Classification</th>
                    <th>Adapter</th>
                    <th>Attempt</th>
                    <th>Updated</th>
                  </tr>
                </thead>
                <tbody>
                  {(overview?.top_failing_intents ?? []).map((row) => (
                    <tr key={row.job_id}>
                      <td title={row.intent_id}>
                        <Link href={`/ops/requests/${encodeURIComponent(row.intent_id)}`}>
                          {shortId(row.intent_id)}
                        </Link>
                      </td>
                      <td>
                        <span className={`badge ${toneForState(row.state, row.classification)}`}>
                          {row.state}
                        </span>
                      </td>
                      <td>{row.classification}</td>
                      <td>{row.adapter_id}</td>
                      <td>
                        {row.attempt}/{row.max_attempts}
                      </td>
                      <td>{formatMs(row.updated_at_ms)}</td>
                    </tr>
                  ))}
                  {(overview?.top_failing_intents ?? []).length === 0 ? (
                    <tr>
                      <td colSpan={6}>No failing intents recorded.</td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>
        </>
      ) : null}

      {view === "jobs" || view === "replay" || view === "dead_letters" ? (
        <section className="surface">
          <div className="controls compact">
            <label>
              State
              <input value={jobStateFilter} onChange={(event) => setJobStateFilter(event.target.value)} />
            </label>
            <label>
              Adapter
              <input
                value={jobAdapterFilter}
                onChange={(event) => setJobAdapterFilter(event.target.value)}
              />
            </label>
            <label>
              Attempt
              <input
                type="number"
                min={0}
                value={jobAttemptFilter}
                onChange={(event) => setJobAttemptFilter(event.target.value)}
              />
            </label>
            <label className="wide">
              Search
              <input
                value={jobSearchFilter}
                onChange={(event) => setJobSearchFilter(event.target.value)}
                placeholder="intent, job, failure code"
              />
            </label>
          </div>
        </section>
      ) : null}

      {view === "jobs" ? (
        <section className="surface">
          <h3>Tenant jobs</h3>
          <JobTable rows={filteredJobs} />
        </section>
      ) : null}

      {view === "replay" ? (
        <>
          <section className="surface">
            <h3>Safe replay</h3>
            <div className="controls inline">
              <label className="wide">
                Intent
                <input
                  value={selectedIntent}
                  onChange={(event) => setSelectedIntent(event.target.value)}
                  placeholder="intent_id"
                />
              </label>
              <label className="wide">
                Reason
                <input value={replayReason} onChange={(event) => setReplayReason(event.target.value)} />
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={replayConfirm}
                  onChange={(event) => setReplayConfirm(event.target.checked)}
                />
                Confirm replay lineage impact
              </label>
              <button className="btn danger" type="button" disabled={replaying} onClick={() => void triggerReplay()}>
                {replaying ? "Scheduling..." : "Replay request"}
              </button>
            </div>
          </section>

          <section className="surface">
            <h3>Replay candidates</h3>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Intent</th>
                    <th>State</th>
                    <th>Classification</th>
                    <th>Attempts</th>
                    <th>Replay count</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {replayCandidates.map((job) => (
                    <tr key={job.job_id}>
                      <td title={job.intent_id}>{shortId(job.intent_id)}</td>
                      <td>{job.state}</td>
                      <td>{job.classification}</td>
                      <td>
                        {job.attempt}/{job.max_attempts}
                      </td>
                      <td>{job.replay_count ?? 0}</td>
                      <td>
                        <button
                          className="btn ghost"
                          type="button"
                          onClick={() => setSelectedIntent(job.intent_id)}
                        >
                          Select
                        </button>
                      </td>
                    </tr>
                  ))}
                  {replayCandidates.length === 0 ? (
                    <tr>
                      <td colSpan={6}>No replay candidates currently visible for this tenant.</td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>
        </>
      ) : null}

      {view === "dead_letters" ? (
        <section className="surface">
          <h3>Dead-letter queue</h3>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Intent</th>
                  <th>Classification</th>
                  <th>Why dead-lettered</th>
                  <th>Updated</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {deadLetterJobs.map((job) => (
                  <tr key={job.job_id}>
                    <td title={job.intent_id}>{shortId(job.intent_id)}</td>
                    <td>{job.classification}</td>
                    <td>{job.failure_message ?? job.failure_code ?? "-"}</td>
                    <td>{formatMs(job.updated_at_ms)}</td>
                    <td>
                      <Link href={`/ops/requests/${encodeURIComponent(job.intent_id)}`}>Open request</Link>
                    </td>
                  </tr>
                ))}
                {deadLetterJobs.length === 0 ? (
                  <tr>
                    <td colSpan={5}>No dead-lettered jobs found.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}

      {view === "deliveries" ? (
        <>
          <section className="surface">
            <div className="controls inline">
              <label>
                Delivery state
                <input
                  value={deliveryStateFilter}
                  onChange={(event) => setDeliveryStateFilter(event.target.value)}
                  placeholder="delivered, retry_scheduled, terminal_failure"
                />
              </label>
              <button className="btn ghost" type="button" onClick={() => void loadView()}>
                Apply
              </button>
            </div>
          </section>

          <section className="surface">
            <h3>Failure clustering</h3>
            <div className="summary-grid">
              {(deliveries?.failure_clusters ?? []).map((cluster) => (
                <Summary key={cluster.key} label={cluster.key} value={String(cluster.count)} />
              ))}
              {(deliveries?.failure_clusters ?? []).length === 0 ? (
                <EmptyState compact title="No delivery failures" description="No failed delivery clusters yet." />
              ) : null}
            </div>
          </section>

          <section className="surface">
            <h3>Delivery attempts</h3>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Callback</th>
                    <th>Intent</th>
                    <th>State</th>
                    <th>Attempts</th>
                    <th>Last HTTP</th>
                    <th>Failure</th>
                    <th>Updated</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {(deliveries?.deliveries ?? []).map((row) => (
                    <tr key={row.callback_id}>
                      <td title={row.callback_id}>{shortId(row.callback_id)}</td>
                      <td title={row.intent_id}>
                        <Link href={`/ops/requests/${encodeURIComponent(row.intent_id)}`}>
                          {shortId(row.intent_id)}
                        </Link>
                      </td>
                      <td>{row.state}</td>
                      <td>{row.attempts}</td>
                      <td>{row.last_http_status ?? "-"}</td>
                      <td>{row.last_error_class ?? row.last_error_message ?? "-"}</td>
                      <td>{formatMs(row.updated_at_ms)}</td>
                      <td>
                        <button
                          className="btn ghost"
                          type="button"
                          disabled={
                            redrivingId === row.callback_id || row.state.toLowerCase() === "delivered"
                          }
                          onClick={() => void redriveDelivery(row.callback_id)}
                        >
                          {redrivingId === row.callback_id ? "Redriving..." : "Redrive"}
                        </button>
                      </td>
                    </tr>
                  ))}
                  {(deliveries?.deliveries ?? []).length === 0 ? (
                    <tr>
                      <td colSpan={8}>No callback deliveries recorded for this tenant.</td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>
        </>
      ) : null}

      {view === "intake_audits" ? (
        <>
          <section className="surface">
            <div className="controls inline">
              <label>
                Validation
                <select
                  value={auditValidationFilter}
                  onChange={(event) => setAuditValidationFilter(event.target.value)}
                >
                  <option value="">all</option>
                  <option value="accepted">accepted</option>
                  <option value="rejected">rejected</option>
                </select>
              </label>
              <label>
                Channel
                <select
                  value={auditChannelFilter}
                  onChange={(event) => setAuditChannelFilter(event.target.value)}
                >
                  <option value="">all</option>
                  <option value="api">api</option>
                  <option value="webhook">webhook</option>
                </select>
              </label>
              <button className="btn ghost" type="button" onClick={() => void loadView()}>
                Apply
              </button>
            </div>
          </section>

          <section className="surface">
            <h3>Tenant intake audits</h3>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Request</th>
                    <th>Channel</th>
                    <th>Result</th>
                    <th>Correlation</th>
                    <th>Reason</th>
                    <th>Created</th>
                  </tr>
                </thead>
                <tbody>
                  {(audits?.audits ?? []).map((audit) => (
                    <tr key={`${audit.request_id}-${audit.created_at_ms}`}>
                      <td title={audit.request_id}>{shortId(audit.request_id)}</td>
                      <td>{audit.channel}</td>
                      <td>{audit.validation_result}</td>
                      <td>{audit.correlation_id ?? "-"}</td>
                      <td>{audit.rejection_reason ?? audit.error_message ?? "-"}</td>
                      <td>{formatMs(audit.created_at_ms)}</td>
                    </tr>
                  ))}
                  {(audits?.audits ?? []).length === 0 ? (
                    <tr>
                      <td colSpan={6}>No intake audits found.</td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>
        </>
      ) : null}

      {view === "adapter_health" ? (
        <section className="surface">
          <h3>Adapter health</h3>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Adapter</th>
                  <th>Total</th>
                  <th>Success</th>
                  <th>Failure</th>
                  <th>Retrying</th>
                  <th>Queued</th>
                  <th>Last failure</th>
                </tr>
              </thead>
              <tbody>
                {(overview?.adapter_health ?? []).map((row) => (
                  <tr key={row.adapter_id}>
                    <td>{row.adapter_id}</td>
                    <td>{row.total_jobs}</td>
                    <td>{row.success_jobs}</td>
                    <td>{row.failure_jobs}</td>
                    <td>{row.retrying_jobs}</td>
                    <td>{row.queued_jobs}</td>
                    <td>{formatMs(row.last_failure_at_ms ?? null)}</td>
                  </tr>
                ))}
                {(overview?.adapter_health ?? []).length === 0 ? (
                  <tr>
                    <td colSpan={7}>No adapter health rows available yet.</td>
                  </tr>
                ) : null}
              </tbody>
            </table>
          </div>
          <p className="hint-line">
            RPC latency and confirmation lag instrumentation will populate here once adapter metrics are emitted.
          </p>
        </section>
      ) : null}

      {view === "security" ? (
        <>
          <section className="surface summary-grid">
            <Summary
              label="Callback destination"
              value={security?.callback_destination.configured ? "configured" : "not configured"}
            />
            <Summary
              label="Private destinations"
              value={security?.callback_destination.allow_private_destinations ? "allowed" : "blocked"}
            />
            <Summary
              label="Allowed hosts"
              value={String(security?.callback_destination.allowed_hosts.length ?? 0)}
            />
            <Summary
              label="Suspicious auth failures"
              value={String(security?.suspicious_auth_failures.length ?? 0)}
            />
          </section>

          <section className="surface">
            <h3>Allowed hosts policy</h3>
            <p>{security?.callback_destination.delivery_url ?? "No callback destination configured."}</p>
            <div className="event-meta">
              {(security?.callback_destination.allowed_hosts ?? []).map((host) => (
                <span key={host} className="kv">
                  {host}
                </span>
              ))}
            </div>
          </section>

          <section className="surface">
            <h3>Suspicious auth failures</h3>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Request</th>
                    <th>Channel</th>
                    <th>Principal</th>
                    <th>Reason</th>
                    <th>Created</th>
                  </tr>
                </thead>
                <tbody>
                  {(security?.suspicious_auth_failures ?? []).map((row) => (
                    <tr key={`${row.request_id}-${row.created_at_ms}`}>
                      <td title={row.request_id}>{shortId(row.request_id)}</td>
                      <td>{row.channel}</td>
                      <td>{row.principal_id ?? "-"}</td>
                      <td>{row.reason}</td>
                      <td>{formatMs(row.created_at_ms)}</td>
                    </tr>
                  ))}
                  {(security?.suspicious_auth_failures ?? []).length === 0 ? (
                    <tr>
                      <td colSpan={5}>No suspicious auth failures detected.</td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>

          <section className="surface">
            <h3>Key rotation prompts</h3>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Workspace</th>
                    <th>Key</th>
                    <th>Created</th>
                    <th>Last used</th>
                    <th>Recommendation</th>
                  </tr>
                </thead>
                <tbody>
                  {(security?.key_rotation_prompts ?? []).map((row) => (
                    <tr key={row.key_id}>
                      <td>{row.workspace_name}</td>
                      <td>
                        {row.key_name} ({row.prefix}...{row.last4})
                      </td>
                      <td>{formatMs(row.created_at_ms)}</td>
                      <td>{formatMs(row.last_used_at_ms ?? null)}</td>
                      <td>{row.recommendation}</td>
                    </tr>
                  ))}
                  {(security?.key_rotation_prompts ?? []).length === 0 ? (
                    <tr>
                      <td colSpan={5}>No key rotation prompts at the moment.</td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>
        </>
      ) : null}

      {view === "workspaces" ? (
        <>
          <section className="surface">
            <div className="controls inline">
              <label className="wide">
                Workspace
                <select
                  value={selectedWorkspaceId}
                  onChange={(event) => setSelectedWorkspaceId(event.target.value)}
                >
                  {(workspaces ?? []).map((workspace) => (
                    <option key={workspace.workspace_id} value={workspace.workspace_id}>
                      {workspace.environment.toUpperCase()} | {workspace.workspace_name}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </section>

          <section className="surface">
            <h3>Workspace mapping</h3>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Workspace</th>
                    <th>Environment</th>
                    <th>Role</th>
                    <th>Current</th>
                  </tr>
                </thead>
                <tbody>
                  {(workspaces ?? []).map((workspace) => (
                    <tr key={workspace.workspace_id}>
                      <td>{workspace.workspace_name}</td>
                      <td>{workspace.environment}</td>
                      <td>{workspace.role}</td>
                      <td>{workspace.is_current ? "yes" : "no"}</td>
                    </tr>
                  ))}
                  {workspaces.length === 0 ? (
                    <tr>
                      <td colSpan={4}>No workspaces are visible for this tenant.</td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>

          {workspaceDetail ? (
            <section className="surface">
              <h3>Environment settings</h3>
              <div className="summary-grid">
                <Summary label="Plan" value={workspaceDetail.billing.plan} />
                <Summary label="Access mode" value={workspaceDetail.billing.access_mode} />
                <Summary label="Execution policy" value={workspaceDetail.settings.execution_policy} />
                <Summary
                  label="Sponsored cap"
                  value={String(workspaceDetail.settings.sponsored_monthly_cap_requests)}
                />
                <Summary
                  label="Replay from customer app"
                  value={workspaceDetail.settings.allow_replay_from_customer_app ? "enabled" : "disabled"}
                />
                <Summary
                  label="Retention days"
                  value={String(workspaceDetail.settings.request_retention_days)}
                />
              </div>
            </section>
          ) : null}
        </>
      ) : null}

      {view === "activity" ? (
        <>
          <section className="surface">
            <h3>Combined activity feed</h3>
            <div className="timeline">
              {(activity?.feed ?? []).map((entry) => (
                <article key={entry.id} className="event-card">
                  <div className="event-top">
                    <span className="badge neutral">{entry.kind}</span>
                    <span className={`badge ${entry.allowed ? "success" : "warn"}`}>
                      {entry.allowed ? "allowed" : "blocked"}
                    </span>
                  </div>
                  <p className="event-summary">{entry.action}</p>
                  <div className="event-meta">
                    <span className="kv">principal:{entry.principal_id}</span>
                    <span className="kv">role:{entry.principal_role}</span>
                    <span className="kv">target:{entry.target ?? "-"}</span>
                    <span className="kv">time:{formatMs(entry.created_at_ms)}</span>
                  </div>
                </article>
              ))}
              {(activity?.feed ?? []).length === 0 ? (
                <EmptyState compact title="No activity yet" description="Query audits and operator actions appear here." />
              ) : null}
            </div>
          </section>

          <section className="surface">
            <h3>Replay and operator actions</h3>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Action</th>
                    <th>Target</th>
                    <th>Reason</th>
                    <th>Allowed</th>
                    <th>Created</th>
                  </tr>
                </thead>
                <tbody>
                  {(activity?.operator_actions ?? []).map((row) => (
                    <tr key={row.action_id}>
                      <td>{row.action_type}</td>
                      <td>{shortId(row.target_intent_id)}</td>
                      <td>{row.reason}</td>
                      <td>{row.allowed ? "yes" : "no"}</td>
                      <td>{formatMs(row.created_at_ms)}</td>
                    </tr>
                  ))}
                  {(activity?.operator_actions ?? []).length === 0 ? (
                    <tr>
                      <td colSpan={5}>No operator actions recorded.</td>
                    </tr>
                  ) : null}
                </tbody>
              </table>
            </div>
          </section>
        </>
      ) : null}
    </div>
  );
}

function Summary({ label, value }: { label: string; value: string }) {
  return (
    <article className="surface kpi">
      <span className="kpi-label">{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

function JobTable({ rows }: { rows: JobRow[] }) {
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            <th>Intent</th>
            <th>Job</th>
            <th>State</th>
            <th>Adapter</th>
            <th>Attempt</th>
            <th>Failure</th>
            <th>Updated</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((job) => (
            <tr key={job.job_id}>
              <td title={job.intent_id}>
                <Link href={`/ops/requests/${encodeURIComponent(job.intent_id)}`}>
                  {shortId(job.intent_id)}
                </Link>
              </td>
              <td title={job.job_id}>{shortId(job.job_id)}</td>
              <td>
                <span className={`badge ${toneForState(job.state, job.classification)}`}>
                  {job.state}
                </span>
              </td>
              <td>{job.adapter_id}</td>
              <td>
                {job.attempt}/{job.max_attempts}
              </td>
              <td>{job.failure_message ?? job.failure_code ?? "-"}</td>
              <td>{formatMs(job.updated_at_ms)}</td>
            </tr>
          ))}
          {rows.length === 0 ? (
            <tr>
              <td colSpan={7}>No jobs found for the current filter set.</td>
            </tr>
          ) : null}
        </tbody>
      </table>
    </div>
  );
}

function titleForView(view: CustomerOperatorView): string {
  switch (view) {
    case "overview":
      return "Overview";
    case "jobs":
      return "Jobs";
    case "replay":
      return "Replay";
    case "dead_letters":
      return "Dead Letters";
    case "deliveries":
      return "Deliveries";
    case "intake_audits":
      return "Intake Audits";
    case "adapter_health":
      return "Adapter Health";
    case "security":
      return "Security";
    case "workspaces":
      return "Workspaces";
    case "activity":
      return "Activity";
  }
}

function subtitleForView(view: CustomerOperatorView): string {
  switch (view) {
    case "overview":
      return "Tenant-scoped operator metrics: backlog, failure classes, delivery trends, and top failing intents.";
    case "jobs":
      return "List tenant jobs with state, adapter, and attempt filtering.";
    case "replay":
      return "Safe replay with explicit reason and confirm. Lineage stays attached to request detail.";
    case "dead_letters":
      return "Dead-lettered jobs, why they exhausted policy, and export or open-request controls.";
    case "deliveries":
      return "Tenant-scoped callback delivery history, failure clustering, and delivery-only redrive.";
    case "intake_audits":
      return "Accepted and rejected requests at ingress with validation and correlation context.";
    case "adapter_health":
      return "Current adapter job health summary for this tenant.";
    case "security":
      return "Callback destination policy, suspicious auth failures, and key rotation prompts.";
    case "workspaces":
      return "Workspace-to-environment mapping and settings across the tenant.";
    case "activity":
      return "Combined query audit and operator action feed, including replay activity.";
  }
}

function toneForState(state: string, classification?: string): "neutral" | "success" | "warn" | "error" {
  const stateLc = state.toLowerCase();
  const classLc = (classification ?? "").toLowerCase();
  if (stateLc.includes("succeed") || classLc.includes("success")) return "success";
  if (
    stateLc.includes("dead") ||
    stateLc.includes("fail") ||
    classLc.includes("terminal") ||
    classLc.includes("blocked")
  ) {
    return "error";
  }
  if (stateLc.includes("retry") || classLc.includes("retry")) return "warn";
  return "neutral";
}

function sanitizeCsv(value: string): string {
  return `"${value.replaceAll("\"", "\"\"")}"`;
}

function downloadCsv(filename: string, content: string) {
  const blob = new Blob([content], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
