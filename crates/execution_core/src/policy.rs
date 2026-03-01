use crate::model::{
    AdapterOutcome, CanonicalState, FailureInfo, PlatformClassification, TimestampMs,
};

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub jitter_percent: u8,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay_ms: 1_000,
            max_delay_ms: 60_000,
            jitter_percent: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryDecision {
    RetryAt {
        next_attempt: u32,
        run_at_ms: TimestampMs,
        delay_ms: u64,
    },
    Exhausted,
}

impl RetryPolicy {
    pub fn decide(
        &self,
        now_ms: TimestampMs,
        failed_attempt: u32,
        adapter_retry_after_ms: Option<u64>,
    ) -> RetryDecision {
        if failed_attempt >= self.max_attempts {
            return RetryDecision::Exhausted;
        }

        let base_delay = adapter_retry_after_ms
            .unwrap_or_else(|| self.exponential_backoff(failed_attempt))
            .clamp(1, self.max_delay_ms.max(1));
        let delay_ms = self.apply_jitter(base_delay, failed_attempt);

        RetryDecision::RetryAt {
            next_attempt: failed_attempt + 1,
            run_at_ms: now_ms.saturating_add(delay_ms),
            delay_ms,
        }
    }

    fn exponential_backoff(&self, failed_attempt: u32) -> u64 {
        let exponent = failed_attempt.saturating_sub(1).min(20);
        let multiplier = 1u64.checked_shl(exponent).unwrap_or(u64::MAX);
        self.base_delay_ms
            .saturating_mul(multiplier)
            .clamp(1, self.max_delay_ms.max(1))
    }

    fn apply_jitter(&self, delay_ms: u64, failed_attempt: u32) -> u64 {
        if self.jitter_percent == 0 {
            return delay_ms;
        }

        let window = delay_ms.saturating_mul(self.jitter_percent as u64) / 100;
        if window == 0 {
            return delay_ms;
        }

        let seed = failed_attempt as u64 * 1_103_515_245 + 12_345;
        let jitter = seed % (window + 1);
        let min = delay_ms.saturating_sub(window / 2);
        min.saturating_add(jitter).max(1)
    }
}

#[derive(Debug, Clone)]
pub struct ReplayPolicy {
    pub max_replays_per_intent: u32,
    pub replayable_states: Vec<CanonicalState>,
}

impl Default for ReplayPolicy {
    fn default() -> Self {
        Self {
            max_replays_per_intent: 5,
            replayable_states: vec![
                CanonicalState::FailedTerminal,
                CanonicalState::DeadLettered,
            ],
        }
    }
}

impl ReplayPolicy {
    pub fn can_replay(&self, state: CanonicalState, replay_count: u32) -> bool {
        replay_count < self.max_replays_per_intent && self.replayable_states.contains(&state)
    }
}

pub fn transition_allowed(from: Option<CanonicalState>, to: CanonicalState) -> bool {
    use CanonicalState::*;

    matches!(
        (from, to),
        (None, Received)
            | (Some(Received), Validated)
            | (Some(Received), Rejected)
            | (Some(Validated), Queued)
            | (Some(Validated), Replayed)
            | (Some(Replayed), Queued)
            | (Some(Queued), Leased)
            | (Some(Leased), Executing)
            | (Some(Executing), Succeeded)
            | (Some(Executing), RetryScheduled)
            | (Some(Executing), FailedTerminal)
            | (Some(Executing), DeadLettered)
            | (Some(RetryScheduled), Queued)
    )
}

pub fn classify_adapter_outcome(
    outcome: &AdapterOutcome,
) -> (PlatformClassification, Option<FailureInfo>) {
    match outcome {
        AdapterOutcome::InProgress { .. } => (PlatformClassification::Success, None),
        AdapterOutcome::Succeeded { .. } => (PlatformClassification::Success, None),
        AdapterOutcome::RetryableFailure {
            code,
            message,
            retry_after_ms,
            provider_details,
        } => (
            PlatformClassification::RetryableFailure,
            Some(FailureInfo {
                code: code.clone(),
                message: message.clone(),
                classification: PlatformClassification::RetryableFailure,
                caller_can_fix: false,
                operator_can_fix: true,
                retry_after_ms: *retry_after_ms,
                provider_details: provider_details.clone(),
            }),
        ),
        AdapterOutcome::TerminalFailure {
            code,
            message,
            provider_details,
        } => (
            PlatformClassification::TerminalFailure,
            Some(FailureInfo {
                code: code.clone(),
                message: message.clone(),
                classification: PlatformClassification::TerminalFailure,
                caller_can_fix: true,
                operator_can_fix: true,
                retry_after_ms: None,
                provider_details: provider_details.clone(),
            }),
        ),
        AdapterOutcome::Blocked { code, message } => (
            PlatformClassification::Blocked,
            Some(FailureInfo {
                code: code.clone(),
                message: message.clone(),
                classification: PlatformClassification::Blocked,
                caller_can_fix: false,
                operator_can_fix: true,
                retry_after_ms: None,
                provider_details: None,
            }),
        ),
        AdapterOutcome::ManualReview { code, message } => (
            PlatformClassification::ManualReview,
            Some(FailureInfo {
                code: code.clone(),
                message: message.clone(),
                classification: PlatformClassification::ManualReview,
                caller_can_fix: false,
                operator_can_fix: true,
                retry_after_ms: None,
                provider_details: None,
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_guard_rejects_illegal_transition() {
        assert!(!transition_allowed(
            Some(CanonicalState::Received),
            CanonicalState::Executing
        ));
    }

    #[test]
    fn retry_policy_exhausts_at_max_attempts() {
        let policy = RetryPolicy {
            max_attempts: 2,
            ..RetryPolicy::default()
        };
        let decision = policy.decide(1_000, 2, None);
        assert_eq!(decision, RetryDecision::Exhausted);
    }
}
