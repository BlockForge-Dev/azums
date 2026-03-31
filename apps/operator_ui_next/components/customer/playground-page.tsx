"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { readSession } from "@/lib/app-state";
import { apiGet, apiRequest, formatMs } from "@/lib/client-api";
import { Button, Badge, Card, CardHeader, Spinner } from "@/components/ui";
import type {
  CallbackHistoryResponse,
  HistoryResponse,
  ReceiptResponse,
  ReplayResponse,
  SubmitIntentResponse,
} from "@/lib/types";

type PlaygroundDemoScenario = "success" | "retry_then_success" | "terminal_failure" | "real";
type PlaygroundTab = "build" | "receipt" | "history" | "callbacks" | "replay";

const DEFAULT_TO_ADDRESS = "11111111111111111111111111111111";

function middleEllipsis(value: string, start = 18, end = 12) {
  if (!value || value.length <= start + end + 3) return value;
  return `${value.slice(0, start)}...${value.slice(-end)}`;
}

function replayBadgeVariant(state: string) {
  const normalized = state.toLowerCase();
  if (normalized === "succeeded") return "success" as const;
  if (normalized.includes("failed") || normalized.includes("dead")) return "error" as const;
  return "warn" as const;
}

export function PlaygroundPage() {
  const [session, setSession] = useState<Awaited<ReturnType<typeof readSession>>>(null);
  const [tab, setTab] = useState<PlaygroundTab>("build");

  const [submitting, setSubmitting] = useState(false);
  const [loadingTab, setLoadingTab] = useState(false);
  const [intentKind, setIntentKind] = useState("solana.transfer.v1");
  const [demoScenario, setDemoScenario] = useState<PlaygroundDemoScenario>("success");
  const [toAddress, setToAddress] = useState(DEFAULT_TO_ADDRESS);
  const [amount, setAmount] = useState("1");
  const [signedTxBase64, setSignedTxBase64] = useState("");
  const [intentId, setIntentId] = useState("");

  const [submitResult, setSubmitResult] = useState<SubmitIntentResponse | null>(null);
  const [receipt, setReceipt] = useState<ReceiptResponse | null>(null);
  const [history, setHistory] = useState<HistoryResponse | null>(null);
  const [callbacks, setCallbacks] = useState<CallbackHistoryResponse | null>(null);

  const [replaying, setReplaying] = useState(false);
  const [replayResult, setReplayResult] = useState<ReplayResponse | null>(null);
  const [confirmReplay, setConfirmReplay] = useState(false);

  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    void readSession().then(setSession);
  }, []);

  const canSubmit = useMemo(() => {
    const numericAmount = Number(amount);
    return toAddress.trim().length > 0 && Number.isFinite(numericAmount) && numericAmount > 0;
  }, [amount, toAddress]);

  async function loadReceipt(nextIntentId: string) {
    const encoded = encodeURIComponent(nextIntentId);
    const nextReceipt = await apiGet<ReceiptResponse>(`status/requests/${encoded}/receipt`);
    setReceipt(nextReceipt);
  }

  async function loadHistory(nextIntentId: string) {
    const encoded = encodeURIComponent(nextIntentId);
    const nextHistory = await apiGet<HistoryResponse>(`status/requests/${encoded}/history`);
    setHistory(nextHistory);
  }

  async function loadCallbacks(nextIntentId: string) {
    const encoded = encodeURIComponent(nextIntentId);
    const nextCallbacks = await apiGet<CallbackHistoryResponse>(
      `status/requests/${encoded}/callbacks?include_attempts=true&attempt_limit=10`
    );
    setCallbacks(nextCallbacks);
  }

  async function refreshActiveTab(nextIntentId = intentId, nextTab = tab) {
    if (!nextIntentId) return;
    setLoadingTab(true);
    setError(null);
    try {
      if (nextTab === "receipt") {
        await loadReceipt(nextIntentId);
      } else if (nextTab === "history") {
        await loadHistory(nextIntentId);
      } else if (nextTab === "callbacks") {
        await loadCallbacks(nextIntentId);
      } else if (nextTab === "replay") {
        await Promise.all([loadReceipt(nextIntentId), loadHistory(nextIntentId)]);
      }
    } catch (loadError: unknown) {
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      setLoadingTab(false);
    }
  }

  async function submitIntent() {
    if (!canSubmit) {
      setError("Provide a valid destination address and a positive amount.");
      return;
    }

    setSubmitting(true);
    setError(null);
    setMessage(null);
    setReplayResult(null);

    try {
      const payload: Record<string, unknown> = {
        to_addr: toAddress.trim(),
        amount: Number(amount),
      };
      if (signedTxBase64.trim()) {
        payload.signed_tx_base64 = signedTxBase64.trim();
      }

      const response = await apiRequest<SubmitIntentResponse>("ingress/requests", {
        method: "POST",
        headers: {
          "x-azums-submit-surface": "playground",
        },
        body: JSON.stringify({
          intent_kind: intentKind,
          payload,
          metadata: {
            "playground.demo_scenario": demoScenario,
          },
        }),
      });

      setSubmitResult(response);
      setIntentId(response.intent_id);
      setMessage(`Submitted ${middleEllipsis(response.intent_id, 12, 10)} on devnet Playground.`);
      await loadReceipt(response.intent_id);
      setTab("receipt");
    } catch (submitError: unknown) {
      setError(submitError instanceof Error ? submitError.message : String(submitError));
    } finally {
      setSubmitting(false);
    }
  }

  async function handleReplay() {
    if (!intentId || !confirmReplay) return;

    setReplaying(true);
    setError(null);
    setMessage(null);

    try {
      const encoded = encodeURIComponent(intentId);
      const result = await apiRequest<ReplayResponse>(`status/requests/${encoded}/replay`, {
        method: "POST",
        body: JSON.stringify({
          reason: "playground manual replay",
        }),
      });
      setReplayResult(result);
      setMessage(`Replay requested for ${middleEllipsis(intentId, 12, 10)}.`);
      await Promise.all([loadReceipt(intentId), loadHistory(intentId), loadCallbacks(intentId)]);
    } catch (replayError: unknown) {
      setError(replayError instanceof Error ? replayError.message : String(replayError));
    } finally {
      setReplaying(false);
      setConfirmReplay(false);
    }
  }

  function handleTabChange(nextTab: PlaygroundTab) {
    setTab(nextTab);
    void refreshActiveTab(intentId, nextTab);
  }

  return (
    <div className="flex flex-col gap-6 max-w-7xl mx-auto p-6">
      <div className="bg-gradient-to-br from-card to-card/80 border border-border rounded-2xl p-8">
        <div className="flex-1">
          <p className="text-xs font-semibold uppercase tracking-wider text-primary mb-2">Playground</p>
          <h2 className="text-2xl font-bold text-foreground mb-2">Run an intent and inspect the execution clearly.</h2>
          <p className="text-sm text-muted-foreground">
            Submit the real ingress contract through the Playground, force devnet-safe routing,
            and inspect durable receipt, history, callbacks, and replay without counting against workspace usage.
          </p>
        </div>
        <div className="flex items-center gap-2 flex-wrap mt-4">
          <Badge>Devnet only</Badge>
          <Badge>Real ingress</Badge>
          <Badge>Replay tools</Badge>
        </div>
      </div>

      {error ? (
        <div className="bg-destructive/10 border border-destructive/30 rounded-xl p-4 text-destructive text-sm">
          {error}
        </div>
      ) : null}
      {message ? (
        <div className="bg-primary/10 border border-primary/30 rounded-xl p-4 text-primary text-sm">
          {message}
        </div>
      ) : null}

      <div className="flex gap-1 border-b border-border">
        {(["build", "receipt", "history", "callbacks", "replay"] as PlaygroundTab[]).map((t) => (
          <button
            key={t}
            data-testid={`playground-tab-${t}`}
            onClick={() => handleTabChange(t)}
            className={`px-4 py-2 text-sm font-medium capitalize transition-colors relative ${
              tab === t ? "text-primary" : "text-muted-foreground hover:text-foreground"
            }`}
          >
            {t}
            {tab === t ? <span className="absolute bottom-0 left-0 right-0 h-0.5 bg-primary" /> : null}
          </button>
        ))}
      </div>

      {tab === "build" ? (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <Card>
            <CardHeader
              title="Request builder"
              subtitle="Configure the real ingress payload and submit it through the Playground surface."
            />

            <div className="space-y-4">
              <div>
                <label className="text-sm font-medium text-foreground block mb-2">Intent type</label>
                <select
                  data-testid="playground-intent-kind"
                  value={intentKind}
                  onChange={(event) => setIntentKind(event.target.value)}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-foreground"
                >
                  <option value="solana.transfer.v1">Send transaction</option>
                  <option value="solana.broadcast.v1">Submit signed payload</option>
                </select>
              </div>

              <div>
                <label className="text-sm font-medium text-foreground block mb-2">Demo scenario</label>
                <select
                  data-testid="playground-demo-scenario"
                  value={demoScenario}
                  onChange={(event) => setDemoScenario(event.target.value as PlaygroundDemoScenario)}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-foreground"
                >
                  <option value="success">Success</option>
                  <option value="retry_then_success">Retry then success</option>
                  <option value="terminal_failure">Terminal failure</option>
                  <option value="real">Real execution path</option>
                </select>
              </div>

              <div>
                <label className="text-sm font-medium text-foreground block mb-2">Destination address</label>
                <input
                  data-testid="playground-to-address"
                  value={toAddress}
                  onChange={(event) => setToAddress(event.target.value)}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-foreground"
                  placeholder="11111111111111111111111111111111"
                />
              </div>

              <div>
                <label className="text-sm font-medium text-foreground block mb-2">Amount</label>
                <input
                  data-testid="playground-amount"
                  type="number"
                  min={1}
                  value={amount}
                  onChange={(event) => setAmount(event.target.value)}
                  className="w-full px-3 py-2 bg-input border border-border rounded-lg text-foreground"
                />
              </div>

              <div>
                <label className="text-sm font-medium text-foreground block mb-2">
                  Signed transaction base64 <span className="text-muted-foreground">optional</span>
                </label>
                <textarea
                  data-testid="playground-signed-tx"
                  value={signedTxBase64}
                  onChange={(event) => setSignedTxBase64(event.target.value)}
                  className="w-full min-h-[110px] px-3 py-2 bg-input border border-border rounded-lg text-foreground font-mono text-xs"
                  placeholder="Leave empty to let the platform handle signing when allowed."
                />
              </div>

              <Button data-testid="playground-submit" onClick={submitIntent} disabled={submitting || !canSubmit} className="w-full">
                {submitting ? <Spinner size="sm" className="mr-2" /> : null}
                {submitting ? "Submitting..." : "Submit intent"}
              </Button>
            </div>
          </Card>

          <Card>
            <CardHeader
              title="Execution view"
              subtitle="Real ingress submission and durable state come back here."
            />

            {submitResult ? (
              <div className="space-y-4">
                <div className="bg-muted/30 rounded-lg p-4 border border-border/50">
                  <div className="flex items-center justify-between mb-2">
                    <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Result</p>
                    <Badge variant={submitResult.ok ? "success" : "error"}>
                      {submitResult.state}
                    </Badge>
                  </div>
                  <pre className="text-xs font-mono text-foreground overflow-auto">
                    {JSON.stringify(submitResult, null, 2)}
                  </pre>
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-xs text-muted-foreground">
                  <div>Intent: {middleEllipsis(submitResult.intent_id, 12, 10)}</div>
                  <div>Tenant: {session?.tenant_id ?? submitResult.tenant_id}</div>
                </div>

                <div className="flex gap-2 flex-wrap">
                  <Button variant="ghost" size="small" onClick={() => handleTabChange("receipt")}>View Receipt</Button>
                  <Button variant="ghost" size="small" onClick={() => handleTabChange("history")}>View History</Button>
                  <Button variant="ghost" size="small" onClick={() => handleTabChange("callbacks")}>View Callbacks</Button>
                  <Button variant="ghost" size="small" onClick={() => handleTabChange("replay")}>Open Replay</Button>
                </div>
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">
                Submit an intent to see durable backend results here.
              </p>
            )}
          </Card>
        </div>
      ) : null}

      {tab === "receipt" ? (
        <Card>
          <CardHeader
            title="Receipt"
            subtitle={intentId ? `Intent ID: ${intentId}` : "No intent submitted yet"}
            action={
              intentId ? (
                <div className="flex gap-2">
                  <Button variant="ghost" size="small" onClick={() => void refreshActiveTab(intentId, "receipt")}>
                    {loadingTab ? "Refreshing..." : "Refresh"}
                  </Button>
                  <Button variant="ghost" size="small" onClick={() => setTab("replay")}>
                    Replay tools
                  </Button>
                </div>
              ) : undefined
            }
          />

          {receipt ? (
            <pre className="text-xs font-mono text-foreground bg-muted/30 rounded-lg p-4 overflow-auto">
              {JSON.stringify(receipt, null, 2)}
            </pre>
          ) : (
            <p className="text-sm text-muted-foreground">
              {intentId ? "Loading receipt..." : "Submit an intent first to view receipt."}
            </p>
          )}
        </Card>
      ) : null}

      {tab === "history" ? (
        <Card>
          <CardHeader
            title="History / Attempts"
            subtitle={intentId ? `Intent ID: ${intentId}` : "No intent submitted yet"}
            action={
              intentId ? (
                <Button variant="ghost" size="small" onClick={() => void refreshActiveTab(intentId, "history")}>
                  {loadingTab ? "Refreshing..." : "Refresh"}
                </Button>
              ) : undefined
            }
          />

          {history ? (
            <div className="space-y-3">
              {history.transitions.map((transition, index) => (
                <div key={`${transition.transition_id}-${index}`} className="bg-muted/30 rounded-lg p-4 border border-border/50">
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-sm font-medium text-foreground">
                      {transition.from_state ? `${transition.from_state} → ${transition.to_state}` : transition.to_state}
                    </span>
                    <Badge variant={transition.classification === "retryable_failure" ? "warn" : transition.classification === "terminal_failure" ? "error" : "success"}>
                      {transition.reason_code}
                    </Badge>
                  </div>
                  <p className="text-xs text-muted-foreground">
                    {formatMs(transition.occurred_at_ms)} - {transition.reason}
                  </p>
                </div>
              ))}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              {intentId ? "Loading history..." : "Submit an intent first to view history."}
            </p>
          )}
        </Card>
      ) : null}

      {tab === "callbacks" ? (
        <Card>
          <CardHeader
            title="Callbacks"
            subtitle={intentId ? `Intent ID: ${intentId}` : "No intent submitted yet"}
            action={
              intentId ? (
                <Button variant="ghost" size="small" onClick={() => void refreshActiveTab(intentId, "callbacks")}>
                  {loadingTab ? "Refreshing..." : "Refresh"}
                </Button>
              ) : undefined
            }
          />

          {callbacks ? (
            <div className="space-y-3">
              {callbacks.callbacks.length > 0 ? (
                callbacks.callbacks.map((callback, index) => (
                  <div key={`${callback.callback_id}-${index}`} className="bg-muted/30 rounded-lg p-4 border border-border/50">
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-sm font-medium text-foreground">Callback {callback.callback_id}</span>
                      <Badge variant={callback.state === "delivered" ? "success" : callback.state === "failed_terminal" ? "error" : "warn"}>
                        {callback.state}
                      </Badge>
                    </div>
                    <p className="text-xs text-muted-foreground mb-2">
                      {formatMs(callback.updated_at_ms)} - HTTP {callback.last_http_status ?? "N/A"}
                    </p>
                    {callback.attempt_history?.length ? (
                      <div className="space-y-2">
                        {callback.attempt_history.map((attempt) => (
                          <div key={`${callback.callback_id}-${attempt.attempt_no}`} className="bg-background/50 rounded-md border border-border/50 p-3 text-xs">
                            <div className="flex items-center justify-between text-foreground mb-1">
                              <span>Attempt {attempt.attempt_no}</span>
                              <span>{attempt.outcome}</span>
                            </div>
                            <div className="text-muted-foreground">
                              {formatMs(attempt.occurred_at_ms)}
                              {attempt.http_status ? ` · HTTP ${attempt.http_status}` : ""}
                              {attempt.error_message ? ` · ${attempt.error_message}` : ""}
                            </div>
                          </div>
                        ))}
                      </div>
                    ) : null}
                  </div>
                ))
              ) : (
                <p className="text-sm text-muted-foreground">No callbacks for this intent.</p>
              )}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              {intentId ? "Loading callbacks..." : "Submit an intent first to view callbacks."}
            </p>
          )}
        </Card>
      ) : null}

      {tab === "replay" ? (
        <Card>
          <CardHeader
            title="Replay"
            subtitle="Replay the intent through the real status API."
            action={
              intentId ? (
                <Button variant="ghost" size="small" onClick={() => void refreshActiveTab(intentId, "replay")}>
                  {loadingTab ? "Refreshing..." : "Refresh"}
                </Button>
              ) : undefined
            }
          />

          <div className="space-y-4">
            <p className="text-sm text-muted-foreground">
              Replay creates a new execution path while preserving lineage. Use it to validate retry and recovery behavior without leaving the customer surface.
            </p>

            {intentId ? (
              <div className="space-y-4">
                <p className="text-sm text-foreground">
                  Intent ID: <code className="bg-muted px-2 py-1 rounded">{intentId}</code>
                </p>

                {!confirmReplay ? (
                  <Button data-testid="playground-confirm-replay" onClick={() => setConfirmReplay(true)}>Confirm Replay</Button>
                ) : (
                  <div className="flex gap-2 items-center">
                    <Button data-testid="playground-replay-now" onClick={handleReplay} disabled={replaying}>
                      {replaying ? <Spinner size="sm" className="mr-2" /> : null}
                      {replaying ? "Replaying..." : "Replay Now"}
                    </Button>
                    <Button variant="ghost" onClick={() => setConfirmReplay(false)}>
                      Cancel
                    </Button>
                  </div>
                )}

                {replayResult ? (
                  <div className="bg-muted/30 rounded-lg p-4 border border-border/50">
                    <div className="flex items-center justify-between mb-2">
                      <p className="text-xs font-medium uppercase tracking-wider text-muted-foreground">Replay Result</p>
                      <Badge variant={replayBadgeVariant(replayResult.state)}>{replayResult.state}</Badge>
                    </div>
                    <pre className="text-xs font-mono text-foreground overflow-auto">
                      {JSON.stringify(replayResult, null, 2)}
                    </pre>
                  </div>
                ) : null}
              </div>
            ) : (
              <p className="text-sm text-muted-foreground">Submit an intent first to enable replay.</p>
            )}
          </div>
        </Card>
      ) : null}

      <Card>
        <CardHeader
          title="Next steps"
          subtitle="Move from Playground testing into real integration setup."
        />
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
          <Link href="/app/api-keys" className="bg-gradient-to-br from-primary/10 to-primary/5 border border-primary/30 rounded-xl p-5 transition-all duration-200 hover:border-primary/50 hover:shadow-lg hover:shadow-primary/5 group">
            <h4 className="text-sm font-semibold text-primary mb-1">Generate API key</h4>
            <p className="text-xs text-muted-foreground">Create keys for your apps</p>
          </Link>
          <Link href="/app/callbacks" className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5 group">
            <h4 className="text-sm font-semibold text-foreground mb-1 group-hover:text-primary transition-colors">Configure callbacks</h4>
            <p className="text-xs text-muted-foreground">Set up delivery endpoints</p>
          </Link>
          <Link href="/app/workspaces" className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5 group">
            <h4 className="text-sm font-semibold text-foreground mb-1 group-hover:text-primary transition-colors">Move to staging</h4>
            <p className="text-xs text-muted-foreground">Review workspace posture and billing readiness</p>
          </Link>
          <Link href="/docs" className="bg-card border border-border rounded-xl p-5 transition-all duration-200 hover:border-primary/30 hover:shadow-lg hover:shadow-primary/5 group">
            <h4 className="text-sm font-semibold text-foreground mb-1 group-hover:text-primary transition-colors">Open docs</h4>
            <p className="text-xs text-muted-foreground">Read the integration guide</p>
          </Link>
        </div>
      </Card>
    </div>
  );
}
