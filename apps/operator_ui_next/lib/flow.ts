import type {
  CallbackHistoryResponse,
  FlowCard,
  HistoryResponse,
  RequestStatusResponse,
  ReceiptResponse,
} from "@/lib/types";

function normalize(value: unknown): string {
  return String(value ?? "").toLowerCase();
}

function isTruthyString(value: unknown): boolean {
  return normalize(value) === "true";
}

export function deriveFlowCoverage(
  request: RequestStatusResponse,
  receipt: ReceiptResponse,
  history: HistoryResponse,
  callbacks: CallbackHistoryResponse
): FlowCard[] {
  const entries = Array.isArray(receipt.entries) ? receipt.entries : [];
  const transitions = Array.isArray(history.transitions) ? history.transitions : [];
  const callbackRows = Array.isArray(callbacks.callbacks) ? callbacks.callbacks : [];

  const states = new Set(entries.map((entry) => normalize(entry.state)));
  const reasons = new Set(
    [
      ...entries.map((entry) => entry.details?.reason_code),
      ...transitions.map((transition) => transition.reason_code),
    ]
      .filter(Boolean)
      .map((reason) => normalize(reason))
  );

  const jobIds = new Set(entries.map((entry) => entry.job_id).filter(Boolean));
  const attemptNos = entries
    .map((entry) => Number(entry.attempt_no))
    .filter((value) => Number.isFinite(value));
  const maxAttempt = attemptNos.length > 0 ? Math.max(...attemptNos) : 0;
  const requestState = normalize(request.state);

  const hasCorePath = ["received", "validated", "queued", "leased", "executing"].every(
    (state) => states.has(state)
  );
  const hasTerminal = ["succeeded", "failed_terminal", "dead_lettered", "finalized"].some(
    (state) => states.has(state) || requestState === state
  );
  const callbackDelivered = callbackRows.some(
    (callback) => normalize(callback.state) === "delivered"
  );

  const flowAObserved = hasCorePath || hasTerminal;
  const flowAPassed = hasCorePath && hasTerminal;
  const flowADetail = flowAPassed
    ? `Inbound path observed to terminal state (${request.state}); callbacks=${callbackRows.length}${
        callbackDelivered ? " delivered" : ""
      }.`
    : `Core path incomplete for this intent; final=${request.state}.`;

  const executionCount = entries.filter((entry) => normalize(entry.state) === "executing").length;
  const hasRetryScheduled = states.has("retry_scheduled") || reasons.has("retry_scheduled");
  const hasRetryDue = reasons.has("retry_due");
  const hasReexecution = executionCount >= 2 || maxAttempt >= 2;
  const flowBObserved = hasRetryScheduled || hasRetryDue || hasReexecution;
  const flowBPassed = hasRetryScheduled && hasRetryDue && hasReexecution;
  const flowBDetail = flowBPassed
    ? `Retry behavior observed (${executionCount} execution states, max attempt ${maxAttempt}).`
    : `Retry markers: scheduled=${hasRetryScheduled}, due=${hasRetryDue}, re-execution=${hasReexecution}.`;

  const terminalEntry = [...entries].reverse().find((entry) => {
    const state = normalize(entry.state);
    return state === "failed_terminal" || state === "dead_lettered";
  });
  const terminalDetails = terminalEntry?.details ?? {};
  const hasFailureCode = Boolean(terminalDetails.failure_code || request.last_failure?.code);
  const hasFixability =
    "caller_can_fix" in terminalDetails ||
    "operator_can_fix" in terminalDetails ||
    isTruthyString(terminalDetails.caller_can_fix) ||
    isTruthyString(terminalDetails.operator_can_fix);
  const hasTerminalFailure =
    requestState === "failed_terminal" ||
    requestState === "dead_lettered" ||
    states.has("failed_terminal") ||
    states.has("dead_lettered");
  const flowCObserved = hasTerminalFailure || hasFailureCode;
  const flowCPassed = hasTerminalFailure && (hasFailureCode || hasFixability);
  const flowCDetail = flowCPassed
    ? `Terminal failure classified with operator/caller context.`
    : `Terminal failure markers are partial for this intent.`;

  const replayCount = Number(request.replay_count ?? 0);
  const hasReplayMarker =
    states.has("replayed") ||
    [...reasons].some((reason) => reason.startsWith("replay_")) ||
    entries.some((entry) => Boolean(entry.details?.replay_of_job_id));
  const hasReplayLineage = replayCount > 0 || jobIds.size > 1;
  const flowDObserved = hasReplayMarker || hasReplayLineage;
  const flowDPassed = hasReplayMarker && hasReplayLineage;
  const flowDDetail = flowDPassed
    ? `Replay lineage observed (${jobIds.size} job ids, replay_count=${replayCount}).`
    : `Replay markers: replay_event=${hasReplayMarker}, lineage=${hasReplayLineage}.`;

  return [
    {
      code: "A",
      title: "Inbound to Result",
      status: flowAPassed ? "observed" : flowAObserved ? "partial" : "not_observed",
      detail: flowADetail,
    },
    {
      code: "B",
      title: "Retry Flow",
      status: flowBPassed ? "observed" : flowBObserved ? "partial" : "not_observed",
      detail: flowBDetail,
    },
    {
      code: "C",
      title: "Terminal Failure",
      status: flowCPassed ? "observed" : flowCObserved ? "partial" : "not_observed",
      detail: flowCDetail,
    },
    {
      code: "D",
      title: "Replay Flow",
      status: flowDPassed ? "observed" : flowDObserved ? "partial" : "not_observed",
      detail: flowDDetail,
    },
  ];
}