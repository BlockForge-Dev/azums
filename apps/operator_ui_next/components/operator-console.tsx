"use client";

import { FormEvent, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import { deriveFlowCoverage } from "@/lib/flow";
import { EmptyState } from "@/components/ui/empty-state";
import type {
  ActivityLogItem,
  CallbackDestinationResponse,
  CallbackDestinationUpsertRequest,
  CallbackHistoryResponse,
  FlowCard,
  HistoryResponse,
  IntakeAuditsResponse,
  JobListResponse,
  JobRow,
  Level,
  ReplayResponse,
  RequestStatusResponse,
  UiConfigResponse,
  UiHealthResponse,
  ReceiptResponse,
} from "@/lib/types";

type Tab = "receipt" | "history" | "callbacks" | "raw";
const DEFAULT_REPLAY = "operator replay from operator_ui_next";

export type OperatorConsoleView =
  | "overview"
  | "jobs"
  | "replay"
  | "request"
  | "audits"
  | "callbacks"
  | "activity"
  | "system";

export function OperatorConsole({
  view = "overview",
  initialIntent,
}: {
  view?: OperatorConsoleView;
  initialIntent?: string;
}) {
  const searchParams = useSearchParams();
  const [cfg, setCfg] = useState<UiConfigResponse | null>(null);
  const [health, setHealth] = useState<UiHealthResponse | null>(null);
  const [jobs, setJobs] = useState<JobRow[]>([]);
  const [jobsState, setJobsState] = useState("");
  const [jobsLimit, setJobsLimit] = useState("20");
  const [jobsOffset, setJobsOffset] = useState("0");
  const [jobsSearch, setJobsSearch] = useState("");
  const [jobsLoading, setJobsLoading] = useState(false);
  const [intent, setIntent] = useState(initialIntent ?? "");
  const [selectedIntent, setSelectedIntent] = useState<string | null>(initialIntent ?? null);
  const [req, setReq] = useState<RequestStatusResponse | null>(null);
  const [receipt, setReceipt] = useState<ReceiptResponse | null>(null);
  const [history, setHistory] = useState<HistoryResponse | null>(null);
  const [callbacks, setCallbacks] = useState<CallbackHistoryResponse | null>(null);
  const [requestLoading, setRequestLoading] = useState(false);
  const [tab, setTab] = useState<Tab>("receipt");
  const [replayReason, setReplayReason] = useState(DEFAULT_REPLAY);
  const [replayLoading, setReplayLoading] = useState(false);
  const [audits, setAudits] = useState<IntakeAuditsResponse["audits"]>([]);
  const [auditValidation, setAuditValidation] = useState("");
  const [auditChannel, setAuditChannel] = useState("");
  const [auditLimit, setAuditLimit] = useState("20");
  const [auditsLoading, setAuditsLoading] = useState(false);
  const [callbackConfig, setCallbackConfig] = useState<CallbackDestinationResponse | null>(null);
  const [cbUrl, setCbUrl] = useState("");
  const [cbTimeout, setCbTimeout] = useState("10000");
  const [cbHosts, setCbHosts] = useState("");
  const [cbEnabled, setCbEnabled] = useState(true);
  const [cbAllowPrivate, setCbAllowPrivate] = useState(false);
  const [cbLoading, setCbLoading] = useState(false);
  const [cbSaving, setCbSaving] = useState(false);
  const [cbDeleting, setCbDeleting] = useState(false);
  const [log, setLog] = useState<ActivityLogItem[]>([]);
  const [tenantOverride, setTenantOverride] = useState("");

  const showJobs = view === "overview" || view === "jobs" || view === "replay";
  const showInspector =
    view === "overview" || view === "replay" || view === "request" || view === "callbacks";
  const showAudits = view === "overview" || view === "audits";
  const showCallback = view === "overview" || view === "callbacks";
  const showActivity = view === "overview" || view === "activity";
  const showSystem = view === "overview" || view === "system";
  const layoutModeClass = view === "overview" ? "ops-layout-overview" : "ops-layout-focused";

  const visibleJobs = useMemo(() => {
    const q = jobsSearch.trim().toLowerCase();
    return q ? jobs.filter((j) => j.intent_id.toLowerCase().includes(q)) : jobs;
  }, [jobs, jobsSearch]);

  const flows = useMemo<FlowCard[]>(() => {
    if (!req || !receipt || !history || !callbacks) return [];
    return deriveFlowCoverage(req, receipt, history, callbacks);
  }, [req, receipt, history, callbacks]);

  useEffect(() => {
    void init();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  useEffect(() => {
    const scopedTenant = searchParams.get("tenant_id")?.trim() ?? "";
    setTenantOverride(scopedTenant);
    const qIntent = searchParams.get("intent") ?? initialIntent ?? "";
    if (!qIntent) return;
    setIntent(qIntent);
    void loadIntent(qIntent);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams, initialIntent]);

  function withTenant(path: string): string {
    if (!tenantOverride) return path;
    const separator = path.includes("?") ? "&" : "?";
    return `${path}${separator}tenant_id=${encodeURIComponent(tenantOverride)}`;
  }

  async function init() {
    try {
      const [nextCfg, nextHealth] = await Promise.all([
        apiGet<UiConfigResponse>("config"),
        apiGet<UiHealthResponse>("health"),
      ]);
      setCfg(nextCfg);
      setHealth(nextHealth);
      writeLog("ok", `UI ready for tenant=${nextCfg.tenant_id}`);
    } catch (e) {
      writeLog("error", `Failed to load metadata: ${msg(e)}`);
    }
    const tasks: Promise<void>[] = [];
    if (showJobs) tasks.push(loadJobs());
    if (showAudits) tasks.push(loadAudits());
    if (showCallback) tasks.push(loadCallbackConfig());
    await Promise.all(tasks);
  }

  function writeLog(level: Level, message: string) {
    setLog((prev) => [
      { id: `${Date.now()}-${Math.random()}`, level, message, ts: new Date().toISOString() },
      ...prev,
    ]);
  }

  async function loadJobs() {
    setJobsLoading(true);
    try {
      const q = new URLSearchParams();
      if (jobsState.trim()) q.set("state", jobsState.trim());
      q.set("limit", String(Number(jobsLimit || "20")));
      q.set("offset", String(Number(jobsOffset || "0")));
      const data = await apiGet<JobListResponse>(withTenant(`status/jobs?${q.toString()}`));
      setJobs(data.jobs ?? []);
      writeLog("ok", `Loaded ${data.jobs?.length ?? 0} jobs.`);
    } catch (e) {
      setJobs([]);
      writeLog("error", `Jobs query failed: ${msg(e)}`);
    } finally {
      setJobsLoading(false);
    }
  }

  async function loadIntent(id: string) {
    const v = id.trim();
    if (!v) return;
    setRequestLoading(true);
    setSelectedIntent(v);
    setIntent(v);
    try {
      const [r1, r2, r3, r4] = await Promise.all([
        apiGet<RequestStatusResponse>(withTenant(`status/requests/${encodeURIComponent(v)}`)),
        apiGet<ReceiptResponse>(withTenant(`status/requests/${encodeURIComponent(v)}/receipt`)),
        apiGet<HistoryResponse>(withTenant(`status/requests/${encodeURIComponent(v)}/history`)),
        apiGet<CallbackHistoryResponse>(
          withTenant(
            `status/requests/${encodeURIComponent(
              v
            )}/callbacks?include_attempts=true&attempt_limit=25`
          )
        ),
      ]);
      setReq(r1);
      setReceipt(r2);
      setHistory(r3);
      setCallbacks(r4);
      writeLog("ok", `Loaded intent ${v}.`);
    } catch (e) {
      setReq(null);
      setReceipt(null);
      setHistory(null);
      setCallbacks(null);
      writeLog("error", `Intent load failed: ${msg(e)}`);
    } finally {
      setRequestLoading(false);
    }
  }

  async function triggerReplay() {
    if (!selectedIntent) return writeLog("warn", "Select a request before replay.");
    if (!window.confirm("Trigger replay for this request?")) return;
    setReplayLoading(true);
    try {
      const out = await apiRequest<ReplayResponse>(
        `status/requests/${encodeURIComponent(selectedIntent)}/replay`,
        {
          method: "POST",
          body: JSON.stringify({ reason: replayReason.trim() || DEFAULT_REPLAY }),
        }
      );
      writeLog("ok", `Replay triggered: source=${out.source_job_id} new=${out.replay_job_id}`);
      await loadIntent(selectedIntent);
      if (showJobs) await loadJobs();
    } catch (e) {
      writeLog("error", `Replay failed: ${msg(e)}`);
    } finally {
      setReplayLoading(false);
    }
  }

  async function loadAudits() {
    setAuditsLoading(true);
    try {
      const q = new URLSearchParams();
      if (auditValidation) q.set("validation_result", auditValidation);
      if (auditChannel) q.set("channel", auditChannel);
      q.set("limit", String(Number(auditLimit || "20")));
      q.set("offset", "0");
      const data = await apiGet<IntakeAuditsResponse>(
        withTenant(`status/tenant/intake-audits?${q.toString()}`)
      );
      setAudits(data.audits ?? []);
      writeLog("ok", `Loaded ${data.audits?.length ?? 0} audits.`);
    } catch (e) {
      setAudits([]);
      writeLog("error", `Audit query failed: ${msg(e)}`);
    } finally {
      setAuditsLoading(false);
    }
  }

  async function loadCallbackConfig() {
    setCbLoading(true);
    try {
      const data = await apiGet<CallbackDestinationResponse>(
        withTenant("status/tenant/callback-destination")
      );
      setCallbackConfig(data);
      if (data.configured && data.destination) {
        setCbUrl(data.destination.delivery_url ?? "");
        setCbTimeout(String(data.destination.timeout_ms ?? 10000));
        setCbHosts((data.destination.allowed_hosts ?? []).join(","));
        setCbEnabled(Boolean(data.destination.enabled));
        setCbAllowPrivate(Boolean(data.destination.allow_private_destinations));
      }
      writeLog("ok", data.configured ? "Loaded callback config." : "No callback config set.");
    } catch (e) {
      writeLog("error", `Callback config load failed: ${msg(e)}`);
    } finally {
      setCbLoading(false);
    }
  }

  async function saveCallbackConfig(e: FormEvent) {
    e.preventDefault();
    if (!cbUrl.trim()) return writeLog("warn", "Delivery URL is required.");
    setCbSaving(true);
    try {
      const payload: CallbackDestinationUpsertRequest = {
        delivery_url: cbUrl.trim(),
        timeout_ms: Number(cbTimeout || "10000"),
        allow_private_destinations: cbAllowPrivate,
        allowed_hosts: cbHosts.split(",").map((v) => v.trim()).filter(Boolean),
        enabled: cbEnabled,
      };
      await apiRequest(withTenant("status/tenant/callback-destination"), {
        method: "POST",
        body: JSON.stringify(payload),
      });
      writeLog("ok", "Callback destination saved.");
      await loadCallbackConfig();
    } catch (e) {
      writeLog("error", `Save callback config failed: ${msg(e)}`);
    } finally {
      setCbSaving(false);
    }
  }

  async function deleteCallbackConfig() {
    if (!window.confirm("Delete callback destination configuration?")) return;
    setCbDeleting(true);
    try {
      await apiRequest(withTenant("status/tenant/callback-destination"), {
        method: "DELETE",
      });
      writeLog("ok", "Callback destination deleted.");
      await loadCallbackConfig();
    } catch (e) {
      writeLog("error", `Delete callback failed: ${msg(e)}`);
    } finally {
      setCbDeleting(false);
    }
  }

  return (
    <div className="page operator-page">
      <div className="ambient ambient-one" />
      <div className="ambient ambient-two" />
      <header className="hero ops-hero">
        <div>
          <p className="eyebrow">Durable Execution Platform</p>
          <h1>Operator Console</h1>
          <p className="subtitle">{subtitleForView(view)}</p>
        </div>
        <div className="meta-badges">
          {cfg ? <Badge text={`tenant ${cfg.tenant_id}`} tone="neutral" /> : null}
          {tenantOverride ? <Badge text={`scope ${tenantOverride}`} tone="warn" /> : null}
          {cfg ? <Badge text={`access ${cfg.principal_role}`} tone="neutral" /> : null}
          {health ? (
            <Badge
              text={
                health.status_api_reachable
                  ? `status healthy (${health.status_api_status_code ?? "ok"})`
                  : "status unavailable"
              }
              tone={health.status_api_reachable ? "success" : "error"}
            />
          ) : null}
        </div>
      </header>

      <main className={`workspace ops-workspace ${layoutModeClass}`}>
        {showSystem ? (
          <section className="panel ops-panel ops-system-panel">
            <div className="panel-head">
              <div>
                <h2>System Health</h2>
                <p className="panel-subtitle">Current metadata and status checks.</p>
              </div>
            </div>
            <div className="raw-grid">
              <article>
                <h3>Config</h3>
                <pre>{JSON.stringify(cfg, null, 2)}</pre>
              </article>
              <article>
                <h3>Health</h3>
                <pre>{JSON.stringify(health, null, 2)}</pre>
              </article>
            </div>
          </section>
        ) : null}

        {showJobs ? (
          <section className="panel ops-panel jobs-panel">
            <div className="panel-head">
              <div>
                <h2>Jobs Queue</h2>
                <p className="panel-subtitle">Filter and open intents.</p>
              </div>
              <button className="btn ghost" type="button" onClick={() => void loadJobs()}>
                {jobsLoading ? "Refreshing..." : "Refresh"}
              </button>
            </div>
            <form className="controls compact" onSubmit={(e) => { e.preventDefault(); void loadJobs(); }}>
              <label>State<input value={jobsState} onChange={(e) => setJobsState(e.target.value)} /></label>
              <label>Limit<input type="number" min={1} max={200} value={jobsLimit} onChange={(e) => setJobsLimit(e.target.value)} /></label>
              <label>Offset<input type="number" min={0} value={jobsOffset} onChange={(e) => setJobsOffset(e.target.value)} /></label>
              <label>Search intent<input value={jobsSearch} onChange={(e) => setJobsSearch(e.target.value)} /></label>
              <button className="btn primary" type="submit">Load Jobs</button>
            </form>
            <div className="stats-row"><Badge text={`${jobs.length} loaded`} tone="success" />{stateSummaryBadges(jobs)}</div>
            <div className="table-wrap"><table><thead><tr><th>Intent</th><th>State</th><th>Class</th><th>Attempt</th><th>Updated</th><th /></tr></thead><tbody>{visibleJobs.length===0 ? <tr><td colSpan={6}><EmptyState compact title="No jobs in queue" description="Adjust filters or submit requests." /></td></tr> : visibleJobs.map((j)=><tr key={`${j.intent_id}-${j.updated_at_ms}`} className={j.intent_id===selectedIntent?"selected":""}><td title={j.intent_id}>{short(j.intent_id)}</td><td><Badge text={j.state} tone={stateTone(j.state,j.classification)} /></td><td>{j.classification}</td><td>{j.attempt}/{j.max_attempts}</td><td>{fmt(j.updated_at_ms)}</td><td><button className="btn ghost" type="button" onClick={() => void loadIntent(j.intent_id)}>Open</button></td></tr>)}</tbody></table></div>
          </section>
        ) : null}

        {showInspector ? (
          <section className="panel ops-panel inspector-panel">
            <div className="panel-head">
              <div><h2>Request Inspector</h2><p className="panel-subtitle">Timeline and replay tools.</p></div>
              <button className="btn ghost" type="button" onClick={() => selectedIntent ? void loadIntent(selectedIntent) : void 0}>{requestLoading ? "Refreshing..." : "Refresh"}</button>
            </div>
            <form className="controls inline" onSubmit={(e)=>{e.preventDefault(); void loadIntent(intent);}}><label className="wide">Intent ID<input value={intent} onChange={(e)=>setIntent(e.target.value)} /></label><button className="btn primary" type="submit">Load Request</button></form>
            <div className="summary-grid">{req ? <><Summary k="Intent" v={req.intent_id} /><Summary k="State" v={req.state} /><Summary k="Classification" v={req.classification} /><Summary k="Adapter" v={req.adapter_id ?? "-"} /><Summary k="Attempt" v={`${req.attempt}/${req.max_attempts}`} /><Summary k="Replay Count" v={String(req.replay_count ?? 0)} /><Summary k="Updated" v={fmt(req.updated_at_ms)} /><Summary k="Request ID" v={req.request_id ?? "-"} /></> : <EmptyState title="No request selected" description="Open one intent to inspect details." />}</div>
            <div className="flow-strip">{flows.length===0 ? <article className="flow-card"><div className="flow-card-head"><h3>Flow Coverage</h3><Badge text="No data" tone="neutral" /></div><p>Select a request to evaluate flow A/B/C/D.</p></article> : flows.map((f)=><article className="flow-card" key={f.code}><div className="flow-card-head"><h3>Flow {f.code}</h3><Badge text={flowLabel(f.status)} tone={flowTone(f.status)} /></div><p><strong>{f.title}</strong></p><p>{f.detail}</p></article>)}</div>
            <div className="tabs"><button type="button" className={`tab ${tab==="receipt"?"active":""}`} onClick={() => setTab("receipt")}>Receipt</button><button type="button" className={`tab ${tab==="history"?"active":""}`} onClick={() => setTab("history")}>History</button><button type="button" className={`tab ${tab==="callbacks"?"active":""}`} onClick={() => setTab("callbacks")}>Callbacks</button><button type="button" className={`tab ${tab==="raw"?"active":""}`} onClick={() => setTab("raw")}>Raw</button></div>
            <div className="tab-body">
              {tab==="receipt" ? <div className="timeline">{receipt?.entries?.length ? receipt.entries.map((e)=><Event key={e.receipt_id} state={e.state} cls={e.classification} summary={e.summary} meta={[`attempt:${e.attempt_no}`,`time:${fmt(e.occurred_at_ms)}`]} />) : <EmptyState title="No receipt entries" description="No receipt timeline yet." />}</div> : null}
              {tab==="history" ? <div className="timeline">{history?.transitions?.length ? history.transitions.map((t)=><Event key={t.transition_id} state={t.to_state} cls={t.classification} summary={`${t.from_state ?? "start"} -> ${t.to_state}`} meta={[`reason:${t.reason_code}`,`time:${fmt(t.occurred_at_ms)}`]} />) : <EmptyState title="No transitions" description="No state transitions yet." />}</div> : null}
              {tab==="callbacks" ? <div className="callback-grid">{callbacks?.callbacks?.length ? callbacks.callbacks.map((c)=><article key={c.callback_id} className="callback-card"><h3>{c.callback_id}</h3><div className="callback-meta"><Badge text={c.state} tone={stateTone(c.state)} /><Badge text={`attempts ${c.attempts}`} tone="neutral" /><Badge text={`http ${c.last_http_status ?? "-"}`} tone={c.last_http_status && c.last_http_status >= 400 ? "error":"neutral"} /></div></article>) : <EmptyState title="No callbacks recorded" description="No callback delivery records for this intent." />}</div> : null}
              {tab==="raw" ? <div className="raw-grid"><article><h3>Receipt</h3><pre>{JSON.stringify(receipt,null,2)}</pre></article><article><h3>History</h3><pre>{JSON.stringify(history,null,2)}</pre></article><article><h3>Callbacks</h3><pre>{JSON.stringify(callbacks,null,2)}</pre></article></div> : null}
            </div>
            <form className="controls inline replay-row" onSubmit={(e)=>{e.preventDefault(); void triggerReplay();}}><label className="wide">Replay reason<input value={replayReason} onChange={(e)=>setReplayReason(e.target.value)} /></label><button className="btn danger" disabled={replayLoading} type="submit">{replayLoading ? "Triggering..." : "Trigger Replay"}</button></form>
          </section>
        ) : null}

        {showAudits ? (
          <section className="panel ops-panel audits-panel">
            <div className="panel-head"><div><h2>Ingress Intake Audits</h2><p className="panel-subtitle">Validation outcomes at ingress.</p></div><button className="btn ghost" type="button" onClick={() => void loadAudits()}>{auditsLoading ? "Refreshing..." : "Refresh"}</button></div>
            <form className="controls compact" onSubmit={(e)=>{e.preventDefault(); void loadAudits();}}><label>Validation<select value={auditValidation} onChange={(e)=>setAuditValidation(e.target.value)}><option value="">all</option><option value="accepted">accepted</option><option value="rejected">rejected</option></select></label><label>Channel<select value={auditChannel} onChange={(e)=>setAuditChannel(e.target.value)}><option value="">all</option><option value="api">api</option><option value="webhook">webhook</option></select></label><label>Limit<input type="number" min={1} max={200} value={auditLimit} onChange={(e)=>setAuditLimit(e.target.value)} /></label><button className="btn primary" type="submit">Load Audits</button></form>
            <div className="table-wrap"><table><thead><tr><th>Request</th><th>Result</th><th>Reason</th><th>Channel</th><th>Created</th></tr></thead><tbody>{audits.length===0 ? <tr><td colSpan={5}><EmptyState compact title="No audits found" description="Ingress validation events appear here." /></td></tr> : audits.map((a)=><tr key={`${a.request_id}-${a.created_at_ms}`}><td title={a.request_id}>{short(a.request_id)}</td><td><Badge text={a.validation_result} tone={a.validation_result==="accepted"?"success":"warn"} /></td><td>{a.rejection_reason ?? "-"}</td><td>{a.channel}</td><td>{fmt(a.created_at_ms)}</td></tr>)}</tbody></table></div>
          </section>
        ) : null}

        {showCallback ? (
          <section className="panel ops-panel callback-panel">
            <div className="panel-head"><div><h2>Callback Destination</h2><p className="panel-subtitle">Manage callback delivery target.</p></div><button className="btn ghost" type="button" onClick={() => void loadCallbackConfig()}>{cbLoading ? "Loading..." : "Load"}</button></div>
            <form className="controls compact" onSubmit={(e)=>void saveCallbackConfig(e)}><label className="wide">Delivery URL<input type="url" value={cbUrl} onChange={(e)=>setCbUrl(e.target.value)} placeholder="https://example.com/callback" /></label><label>Timeout ms<input type="number" min={100} max={120000} value={cbTimeout} onChange={(e)=>setCbTimeout(e.target.value)} /></label><label>Allowed hosts<input value={cbHosts} onChange={(e)=>setCbHosts(e.target.value)} placeholder="example.com,api.example.com" /></label><label className="check"><input type="checkbox" checked={cbEnabled} onChange={(e)=>setCbEnabled(e.target.checked)} />Enabled</label><label className="check"><input type="checkbox" checked={cbAllowPrivate} onChange={(e)=>setCbAllowPrivate(e.target.checked)} />Allow private destinations</label><button className="btn primary" disabled={cbSaving} type="submit">{cbSaving?"Saving...":"Upsert Destination"}</button><button className="btn danger" type="button" disabled={cbDeleting} onClick={() => void deleteCallbackConfig()}>{cbDeleting?"Deleting...":"Delete Destination"}</button></form>
            <pre>{JSON.stringify(callbackConfig, null, 2)}</pre>
          </section>
        ) : null}

        {showActivity ? (
          <section className="panel ops-panel activity-panel">
            <div className="panel-head"><div><h2>Activity Log</h2><p className="panel-subtitle">Recent UI actions and API outcomes.</p></div><button className="btn ghost" type="button" onClick={() => setLog([])}>Clear</button></div>
            <div className="activity-log">{log.length===0 ? <EmptyState compact title="No activity yet" description="Actions will appear here." /> : log.map((l)=><div className="log-line" key={l.id}><span className={`log-level ${l.level}`}>{l.level}</span>[{l.ts}] {l.message}</div>)}</div>
          </section>
        ) : null}
      </main>
    </div>
  );
}

function Summary({ k, v }: { k: string; v: string }) {
  return <div className="summary-card"><span>{k}</span><strong>{v}</strong></div>;
}

function Event({ state, cls, summary, meta }: { state: string; cls: string; summary: string; meta: string[] }) {
  return <article className="event-card"><div className="event-top"><Badge text={state} tone={stateTone(state, cls)} /><Badge text={cls} tone={classTone(cls)} /></div><p className="event-summary">{summary}</p><div className="event-meta">{meta.map((m)=><span className="kv" key={m}>{m}</span>)}</div></article>;
}

function Badge({ text, tone }: { text: string; tone: "neutral" | "success" | "warn" | "error" }) { return <span className={`badge ${tone}`}>{text}</span>; }

function stateSummaryBadges(jobs: JobRow[]) {
  const m = new Map<string, number>();
  for (const j of jobs) m.set(j.state, (m.get(j.state) ?? 0) + 1);
  return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0])).map(([k, v]) => <Badge key={k} text={`${k}: ${v}`} tone={stateTone(k)} />);
}

function stateTone(state: string, cls?: string): "neutral" | "success" | "warn" | "error" {
  const s = state.toLowerCase();
  const c = (cls ?? "").toLowerCase();
  if (s.includes("succeed") || s === "delivered") return "success";
  if (s.includes("fail") || s.includes("dead") || c.includes("terminal") || c.includes("blocked")) return "error";
  if (s.includes("retry") || c.includes("retry")) return "warn";
  return "neutral";
}

function classTone(cls: string): "neutral" | "success" | "warn" | "error" { return stateTone("", cls); }
function flowLabel(s: FlowCard["status"]) { return s === "observed" ? "Observed" : s === "partial" ? "Partial" : "Not observed"; }
function flowTone(s: FlowCard["status"]): "neutral" | "success" | "warn" | "error" { return s === "observed" ? "success" : s === "partial" ? "warn" : "neutral"; }
function short(v: string) { return v.length <= 20 ? v : `${v.slice(0,8)}...${v.slice(-8)}`; }
function fmt(v: number | null | undefined) { if (v == null) return "-"; const d = new Date(Number(v)); return Number.isNaN(d.getTime()) ? String(v) : d.toLocaleString(); }
function msg(e: unknown) { return e instanceof Error ? e.message : String(e); }

function subtitleForView(view: OperatorConsoleView): string {
  switch (view) {
    case "jobs":
      return "Jobs-focused operator workflow.";
    case "replay":
      return "Replay and lineage controls.";
    case "request":
      return "Detailed request timeline and callbacks.";
    case "audits":
      return "Ingress intake audit stream.";
    case "callbacks":
      return "Callback configuration and delivery operations.";
    case "activity":
      return "Operator activity stream.";
    case "system":
      return "System metadata and health.";
    default:
      return "Full operator surface.";
  }
}

async function apiGet<T>(path: string): Promise<T> { return apiRequest<T>(path, { method: "GET" }); }

async function apiRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (!headers.has("content-type")) headers.set("content-type", "application/json");
  const res = await fetch(`/api/ui/${path}`, { cache: "no-store", ...init, headers });
  const txt = await res.text();
  const payload = parseJson(txt);
  if (!res.ok) {
    let message = `Request failed with ${res.status}`;
    if (txt) message = txt;
    if (payload && typeof payload === "object") {
      const obj = payload as Record<string, unknown>;
      if (typeof obj.error === "string" && obj.error.trim()) message = obj.error;
      else if (typeof obj.message === "string" && obj.message.trim()) message = obj.message;
    }
    throw new Error(normalizeApiError(message));
  }
  return payload as T;
}

function parseJson(input: string): unknown { if (!input) return null; try { return JSON.parse(input); } catch { return { raw: input }; } }
function normalizeApiError(message: string) { if (message.includes("is not mapped to any role") || message.includes("principal user:")) return "Workspace identity is still syncing. Refresh in a few seconds and try again."; return message; }
