use anyhow::Context;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    AgentActionCallbackConfig, ApprovalDecisionKind, ApprovalDecisionOutcome,
    ApprovalRequestCreateRecord, ApprovalRequestRecord, ApprovalState, PolicyDecisionTraceEntry,
    PolicyRuleMatch, PolicyRuleObligations,
};

const APPROVAL_REQUEST_SELECT: &str = r#"
SELECT
    approval_request_id,
    tenant_id,
    action_request_id,
    correlation_id,
    agent_id,
    environment_id,
    environment_kind,
    runtime_type,
    runtime_identity,
    trust_tier,
    risk_tier,
    owner_team,
    intent_type,
    execution_mode,
    adapter_type,
    normalized_intent_kind,
    normalized_payload_json,
    idempotency_key,
    request_fingerprint,
    requested_scope_json,
    effective_scope_json,
    callback_config_json,
    reason,
    submitted_by,
    policy_bundle_id,
    policy_bundle_version,
    policy_explanation,
    obligations_json,
    matched_rules_json,
    decision_trace_json,
    status,
    required_approvals,
    approvals_received,
    approved_by_json,
    expires_at_ms,
    requested_at_ms,
    resolved_at_ms,
    resolved_by_actor_id,
    resolved_by_actor_source,
    resolution_note,
    slack_delivery_state,
    slack_delivery_error,
    slack_last_attempt_at_ms
FROM ingress_api_approval_requests
"#;

#[derive(Clone)]
pub struct IngressApprovalStore {
    pool: PgPool,
}

impl IngressApprovalStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_approval_requests (
                approval_request_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                action_request_id TEXT NOT NULL,
                correlation_id TEXT NULL,
                agent_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                environment_kind TEXT NOT NULL,
                runtime_type TEXT NOT NULL,
                runtime_identity TEXT NOT NULL,
                trust_tier TEXT NOT NULL,
                risk_tier TEXT NOT NULL,
                owner_team TEXT NOT NULL,
                intent_type TEXT NOT NULL,
                execution_mode TEXT NOT NULL DEFAULT 'mode_c_protected_execution',
                adapter_type TEXT NOT NULL,
                normalized_intent_kind TEXT NOT NULL,
                normalized_payload_json JSONB NOT NULL,
                idempotency_key TEXT NOT NULL,
                request_fingerprint TEXT NOT NULL,
                requested_scope_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                effective_scope_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                callback_config_json JSONB NULL,
                reason TEXT NOT NULL,
                submitted_by TEXT NOT NULL,
                policy_bundle_id TEXT NULL,
                policy_bundle_version BIGINT NULL,
                policy_explanation TEXT NOT NULL,
                obligations_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                matched_rules_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                decision_trace_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                status TEXT NOT NULL,
                required_approvals INTEGER NOT NULL DEFAULT 1,
                approvals_received INTEGER NOT NULL DEFAULT 0,
                approved_by_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                expires_at_ms BIGINT NOT NULL,
                requested_at_ms BIGINT NOT NULL,
                resolved_at_ms BIGINT NULL,
                resolved_by_actor_id TEXT NULL,
                resolved_by_actor_source TEXT NULL,
                resolution_note TEXT NULL,
                slack_delivery_state TEXT NULL,
                slack_delivery_error TEXT NULL,
                slack_last_attempt_at_ms BIGINT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                UNIQUE (tenant_id, action_request_id)
            )
            "#,
            r#"
            ALTER TABLE ingress_api_approval_requests
            ADD COLUMN IF NOT EXISTS correlation_id TEXT NULL
            "#,
            r#"
            ALTER TABLE ingress_api_approval_requests
            ADD COLUMN IF NOT EXISTS execution_mode TEXT NOT NULL DEFAULT 'mode_c_protected_execution'
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_approval_requests_tenant_status_idx
            ON ingress_api_approval_requests(tenant_id, status, requested_at_ms DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_approval_requests_tenant_agent_idx
            ON ingress_api_approval_requests(tenant_id, agent_id, environment_id, requested_at_ms DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_approval_requests_expiry_idx
            ON ingress_api_approval_requests(status, expires_at_ms)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_approval_events (
                event_id UUID PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                approval_request_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                actor_id TEXT NULL,
                actor_source TEXT NULL,
                details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_approval_events_request_idx
            ON ingress_api_approval_events(approval_request_id, created_at DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_approval_events_tenant_idx
            ON ingress_api_approval_events(tenant_id, created_at DESC)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress approval schema")?;
        }

        Ok(())
    }

    pub async fn create_request(
        &self,
        record: &ApprovalRequestCreateRecord,
    ) -> anyhow::Result<ApprovalRequestRecord> {
        let requested_scope_json =
            serde_json::to_value(&record.requested_scope).context("serialize requested scope")?;
        let effective_scope_json =
            serde_json::to_value(&record.effective_scope).context("serialize effective scope")?;
        let callback_config_json = record
            .callback_config
            .as_ref()
            .map(serde_json::to_value)
            .transpose()
            .context("serialize callback config")?;
        let obligations_json =
            serde_json::to_value(&record.obligations).context("serialize obligations")?;
        let matched_rules_json =
            serde_json::to_value(&record.matched_rules).context("serialize matched rules")?;
        let decision_trace_json =
            serde_json::to_value(&record.decision_trace).context("serialize decision trace")?;

        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin approval request tx")?;

        sqlx::query(
            r#"
            INSERT INTO ingress_api_approval_requests (
                approval_request_id,
                tenant_id,
                action_request_id,
                correlation_id,
                agent_id,
                environment_id,
                environment_kind,
                runtime_type,
                runtime_identity,
                trust_tier,
                risk_tier,
                owner_team,
                intent_type,
                execution_mode,
                adapter_type,
                normalized_intent_kind,
                normalized_payload_json,
                idempotency_key,
                request_fingerprint,
                requested_scope_json,
                effective_scope_json,
                callback_config_json,
                reason,
                submitted_by,
                policy_bundle_id,
                policy_bundle_version,
                policy_explanation,
                obligations_json,
                matched_rules_json,
                decision_trace_json,
                status,
                required_approvals,
                approvals_received,
                approved_by_json,
                expires_at_ms,
                requested_at_ms,
                resolved_at_ms,
                resolved_by_actor_id,
                resolved_by_actor_source,
                resolution_note,
                slack_delivery_state,
                slack_delivery_error,
                slack_last_attempt_at_ms,
                updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20,
                $21, $22, $23, $24, $25, $26, $27, $28, $29, $30,
                $31, $32, 0, '[]'::jsonb, $33, $34, NULL, NULL, NULL, NULL, NULL, NULL, NULL, now()
            )
            "#,
        )
        .bind(&record.approval_request_id)
        .bind(&record.tenant_id)
        .bind(&record.action_request_id)
        .bind(&record.correlation_id)
        .bind(&record.agent_id)
        .bind(&record.environment_id)
        .bind(&record.environment_kind)
        .bind(&record.runtime_type)
        .bind(&record.runtime_identity)
        .bind(&record.trust_tier)
        .bind(&record.risk_tier)
        .bind(&record.owner_team)
        .bind(&record.intent_type)
        .bind(&record.execution_mode)
        .bind(&record.adapter_type)
        .bind(&record.normalized_intent_kind)
        .bind(&record.normalized_payload)
        .bind(&record.idempotency_key)
        .bind(&record.request_fingerprint)
        .bind(requested_scope_json)
        .bind(effective_scope_json)
        .bind(callback_config_json)
        .bind(&record.reason)
        .bind(&record.submitted_by)
        .bind(&record.policy_bundle_id)
        .bind(record.policy_bundle_version)
        .bind(&record.policy_explanation)
        .bind(obligations_json)
        .bind(matched_rules_json)
        .bind(decision_trace_json)
        .bind(ApprovalState::Pending.as_str())
        .bind(record.required_approvals as i32)
        .bind(record.expires_at_ms as i64)
        .bind(record.requested_at_ms as i64)
        .execute(&mut *tx)
        .await
        .context("failed to insert approval request")?;

        self.insert_event_tx(
            &mut tx,
            &record.tenant_id,
            &record.approval_request_id,
            "requested",
            None,
            None,
            json!({
                "action_request_id": record.action_request_id,
                "correlation_id": record.correlation_id,
                "agent_id": record.agent_id,
                "environment_id": record.environment_id,
                "intent_type": record.intent_type,
                "execution_mode": record.execution_mode,
                "adapter_type": record.adapter_type,
                "effective_scope": record.effective_scope,
                "required_approvals": record.required_approvals,
            }),
            record.requested_at_ms,
        )
        .await?;

        tx.commit()
            .await
            .context("failed to commit approval request tx")?;

        self.load_request(&record.tenant_id, &record.approval_request_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("approval request insert row missing"))
    }

    pub async fn load_request(
        &self,
        tenant_id: &str,
        approval_request_id: &str,
    ) -> anyhow::Result<Option<ApprovalRequestRecord>> {
        let row = sqlx::query(&format!(
            "{APPROVAL_REQUEST_SELECT} WHERE tenant_id = $1 AND approval_request_id = $2"
        ))
        .bind(tenant_id)
        .bind(approval_request_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load approval request")?;

        row.map(map_approval_request_record)
            .transpose()
            .context("failed to decode approval request row")
    }

    pub async fn load_request_fresh(
        &self,
        tenant_id: &str,
        approval_request_id: &str,
        now_ms: u64,
    ) -> anyhow::Result<Option<ApprovalRequestRecord>> {
        self.expire_if_needed(tenant_id, approval_request_id, now_ms)
            .await?;
        self.load_request(tenant_id, approval_request_id).await
    }

    pub async fn list_requests(
        &self,
        tenant_id: &str,
        state: Option<&str>,
        limit: u32,
        now_ms: u64,
    ) -> anyhow::Result<Vec<ApprovalRequestRecord>> {
        self.expire_overdue(tenant_id, now_ms).await?;
        let rows = sqlx::query(&format!(
            "{APPROVAL_REQUEST_SELECT} WHERE tenant_id = $1 AND ($2::text IS NULL OR status = $2) ORDER BY requested_at_ms DESC LIMIT $3"
        ))
        .bind(tenant_id)
        .bind(state)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to list approval requests")?;

        rows.into_iter()
            .map(map_approval_request_record)
            .collect::<anyhow::Result<Vec<_>>>()
            .context("failed to decode approval request rows")
    }

    pub async fn update_slack_delivery(
        &self,
        tenant_id: &str,
        approval_request_id: &str,
        delivered: bool,
        error_message: Option<&str>,
        now_ms: u64,
    ) -> anyhow::Result<ApprovalRequestRecord> {
        let next_state = if delivered {
            None
        } else {
            Some(ApprovalState::Escalated.as_str())
        };
        sqlx::query(
            r#"
            UPDATE ingress_api_approval_requests
            SET
                status = COALESCE($3, status),
                slack_delivery_state = $4,
                slack_delivery_error = $5,
                slack_last_attempt_at_ms = $6,
                updated_at = now()
            WHERE tenant_id = $1
              AND approval_request_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(approval_request_id)
        .bind(next_state)
        .bind(if delivered { "delivered" } else { "failed" })
        .bind(error_message)
        .bind(now_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to update approval slack delivery state")?;

        self.insert_event(
            tenant_id,
            approval_request_id,
            if delivered {
                "slack_delivered"
            } else {
                "slack_delivery_failed"
            },
            None,
            Some("slack"),
            if delivered {
                json!({})
            } else {
                json!({ "error": error_message })
            },
            now_ms,
        )
        .await?;

        self.load_request(tenant_id, approval_request_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("approval request missing after slack delivery update"))
    }

    pub async fn expire_if_needed(
        &self,
        tenant_id: &str,
        approval_request_id: &str,
        now_ms: u64,
    ) -> anyhow::Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin approval expiry tx")?;
        let updated = sqlx::query(
            r#"
            UPDATE ingress_api_approval_requests
            SET
                status = 'expired',
                resolved_at_ms = COALESCE(resolved_at_ms, $3),
                resolution_note = COALESCE(resolution_note, 'approval expired'),
                updated_at = now()
            WHERE tenant_id = $1
              AND approval_request_id = $2
              AND status IN ('pending', 'escalated')
              AND expires_at_ms <= $3
            "#,
        )
        .bind(tenant_id)
        .bind(approval_request_id)
        .bind(now_ms as i64)
        .execute(&mut *tx)
        .await
        .context("failed to expire approval request")?
        .rows_affected();

        if updated > 0 {
            self.insert_event_tx(
                &mut tx,
                tenant_id,
                approval_request_id,
                "expired",
                None,
                Some("system"),
                json!({ "expired_at_ms": now_ms }),
                now_ms,
            )
            .await?;
        }

        tx.commit()
            .await
            .context("failed to commit approval expiry tx")?;
        Ok(())
    }

    pub async fn expire_overdue(&self, tenant_id: &str, now_ms: u64) -> anyhow::Result<()> {
        let rows = sqlx::query_scalar::<_, String>(
            r#"
            SELECT approval_request_id
            FROM ingress_api_approval_requests
            WHERE tenant_id = $1
              AND status IN ('pending', 'escalated')
              AND expires_at_ms <= $2
            "#,
        )
        .bind(tenant_id)
        .bind(now_ms as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to query overdue approval requests")?;

        for approval_request_id in rows {
            self.expire_if_needed(tenant_id, &approval_request_id, now_ms)
                .await?;
        }
        Ok(())
    }

    pub async fn apply_decision(
        &self,
        tenant_id: &str,
        approval_request_id: &str,
        decision: ApprovalDecisionKind,
        actor_id: &str,
        actor_source: &str,
        note: Option<&str>,
        now_ms: u64,
    ) -> anyhow::Result<ApprovalDecisionOutcome> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin approval decision tx")?;
        let row = sqlx::query(&format!(
            "{APPROVAL_REQUEST_SELECT} WHERE tenant_id = $1 AND approval_request_id = $2 FOR UPDATE"
        ))
        .bind(tenant_id)
        .bind(approval_request_id)
        .fetch_optional(&mut *tx)
        .await
        .context("failed to lock approval request")?
        .ok_or_else(|| anyhow::anyhow!("approval request not found"))?;

        let mut approval = map_approval_request_record(row)?;

        if matches!(
            approval.status,
            ApprovalState::Pending | ApprovalState::Escalated
        ) && approval.expires_at_ms <= now_ms
        {
            approval.status = ApprovalState::Expired;
            approval.resolved_at_ms = Some(now_ms);
            approval.resolution_note = Some("approval expired".to_owned());
            sqlx::query(
                r#"
                UPDATE ingress_api_approval_requests
                SET
                    status = 'expired',
                    resolved_at_ms = $3,
                    resolution_note = COALESCE(resolution_note, 'approval expired'),
                    updated_at = now()
                WHERE tenant_id = $1
                  AND approval_request_id = $2
                "#,
            )
            .bind(tenant_id)
            .bind(approval_request_id)
            .bind(now_ms as i64)
            .execute(&mut *tx)
            .await
            .context("failed to mark approval expired")?;
            self.insert_event_tx(
                &mut tx,
                tenant_id,
                approval_request_id,
                "expired",
                None,
                Some("system"),
                json!({ "expired_at_ms": now_ms }),
                now_ms,
            )
            .await?;
            tx.commit()
                .await
                .context("failed to commit approval expiry decision tx")?;
            return Ok(ApprovalDecisionOutcome {
                approval,
                terminal_reached: false,
            });
        }

        if matches!(
            approval.status,
            ApprovalState::Rejected | ApprovalState::Expired | ApprovalState::Approved
        ) {
            anyhow::bail!(
                "approval request `{approval_request_id}` is already in terminal state `{}`",
                approval.status.as_str()
            );
        }

        let note_value = note
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let mut terminal_reached = false;

        match decision {
            ApprovalDecisionKind::Approve => {
                if approval
                    .approved_by
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(actor_id))
                {
                    anyhow::bail!(
                        "actor `{actor_id}` has already approved approval request `{approval_request_id}`"
                    );
                }
                approval.approved_by.push(actor_id.to_owned());
                approval.approvals_received = approval.approvals_received.saturating_add(1);
                if approval.approvals_received >= approval.required_approvals {
                    approval.status = ApprovalState::Approved;
                    approval.resolved_at_ms = Some(now_ms);
                    approval.resolved_by_actor_id = Some(actor_id.to_owned());
                    approval.resolved_by_actor_source = Some(actor_source.to_owned());
                    approval.resolution_note = note_value.clone();
                    terminal_reached = true;
                }
            }
            ApprovalDecisionKind::Reject => {
                approval.status = ApprovalState::Rejected;
                approval.resolved_at_ms = Some(now_ms);
                approval.resolved_by_actor_id = Some(actor_id.to_owned());
                approval.resolved_by_actor_source = Some(actor_source.to_owned());
                approval.resolution_note = note_value.clone();
                terminal_reached = true;
            }
            ApprovalDecisionKind::Escalate => {
                approval.status = ApprovalState::Escalated;
                approval.resolved_by_actor_id = Some(actor_id.to_owned());
                approval.resolved_by_actor_source = Some(actor_source.to_owned());
                approval.resolution_note = note_value.clone();
            }
        }

        sqlx::query(
            r#"
            UPDATE ingress_api_approval_requests
            SET
                status = $3,
                approvals_received = $4,
                approved_by_json = $5,
                resolved_at_ms = $6,
                resolved_by_actor_id = $7,
                resolved_by_actor_source = $8,
                resolution_note = $9,
                updated_at = now()
            WHERE tenant_id = $1
              AND approval_request_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(approval_request_id)
        .bind(approval.status.as_str())
        .bind(approval.approvals_received as i32)
        .bind(serde_json::to_value(&approval.approved_by).context("serialize approved_by")?)
        .bind(approval.resolved_at_ms.map(|value| value as i64))
        .bind(&approval.resolved_by_actor_id)
        .bind(&approval.resolved_by_actor_source)
        .bind(&approval.resolution_note)
        .execute(&mut *tx)
        .await
        .context("failed to update approval decision state")?;

        self.insert_event_tx(
            &mut tx,
            tenant_id,
            approval_request_id,
            decision.event_type(),
            Some(actor_id),
            Some(actor_source),
            json!({
                "status": approval.status.as_str(),
                "note": note_value,
                "approvals_received": approval.approvals_received,
                "required_approvals": approval.required_approvals,
            }),
            now_ms,
        )
        .await?;

        tx.commit()
            .await
            .context("failed to commit approval decision tx")?;

        Ok(ApprovalDecisionOutcome {
            approval,
            terminal_reached,
        })
    }

    pub async fn insert_event(
        &self,
        tenant_id: &str,
        approval_request_id: &str,
        event_type: &str,
        actor_id: Option<&str>,
        actor_source: Option<&str>,
        details: Value,
        created_at_ms: u64,
    ) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await.context("begin approval event tx")?;
        self.insert_event_tx(
            &mut tx,
            tenant_id,
            approval_request_id,
            event_type,
            actor_id,
            actor_source,
            details,
            created_at_ms,
        )
        .await?;
        tx.commit().await.context("commit approval event tx")?;
        Ok(())
    }

    async fn insert_event_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &str,
        approval_request_id: &str,
        event_type: &str,
        actor_id: Option<&str>,
        actor_source: Option<&str>,
        details: Value,
        created_at_ms: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ingress_api_approval_events (
                event_id,
                tenant_id,
                approval_request_id,
                event_type,
                actor_id,
                actor_source,
                details_json,
                created_at_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(approval_request_id)
        .bind(event_type)
        .bind(actor_id)
        .bind(actor_source)
        .bind(details)
        .bind(created_at_ms as i64)
        .execute(&mut **tx)
        .await
        .context("failed to insert approval event")?;
        Ok(())
    }
}

fn map_approval_request_record(
    row: sqlx::postgres::PgRow,
) -> anyhow::Result<ApprovalRequestRecord> {
    let requested_scope =
        serde_json::from_value::<Vec<String>>(row.get::<Value, _>("requested_scope_json"))
            .context("decode requested_scope_json")?;
    let effective_scope =
        serde_json::from_value::<Vec<String>>(row.get::<Value, _>("effective_scope_json"))
            .context("decode effective_scope_json")?;
    let callback_config = row
        .get::<Option<Value>, _>("callback_config_json")
        .map(serde_json::from_value::<AgentActionCallbackConfig>)
        .transpose()
        .context("decode callback_config_json")?;
    let obligations =
        serde_json::from_value::<PolicyRuleObligations>(row.get::<Value, _>("obligations_json"))
            .context("decode obligations_json")?;
    let matched_rules =
        serde_json::from_value::<Vec<PolicyRuleMatch>>(row.get::<Value, _>("matched_rules_json"))
            .context("decode matched_rules_json")?;
    let decision_trace = serde_json::from_value::<Vec<PolicyDecisionTraceEntry>>(
        row.get::<Value, _>("decision_trace_json"),
    )
    .context("decode decision_trace_json")?;
    let approved_by =
        serde_json::from_value::<Vec<String>>(row.get::<Value, _>("approved_by_json"))
            .context("decode approved_by_json")?;
    let status = match row.get::<String, _>("status").as_str() {
        "pending" => ApprovalState::Pending,
        "approved" => ApprovalState::Approved,
        "rejected" => ApprovalState::Rejected,
        "expired" => ApprovalState::Expired,
        "escalated" => ApprovalState::Escalated,
        other => anyhow::bail!("unsupported approval state `{other}`"),
    };

    Ok(ApprovalRequestRecord {
        approval_request_id: row.get("approval_request_id"),
        tenant_id: row.get("tenant_id"),
        action_request_id: row.get("action_request_id"),
        correlation_id: row.get("correlation_id"),
        agent_id: row.get("agent_id"),
        environment_id: row.get("environment_id"),
        environment_kind: row.get("environment_kind"),
        runtime_type: row.get("runtime_type"),
        runtime_identity: row.get("runtime_identity"),
        trust_tier: row.get("trust_tier"),
        risk_tier: row.get("risk_tier"),
        owner_team: row.get("owner_team"),
        intent_type: row.get("intent_type"),
        execution_mode: row.get("execution_mode"),
        adapter_type: row.get("adapter_type"),
        normalized_intent_kind: row.get("normalized_intent_kind"),
        normalized_payload: row.get("normalized_payload_json"),
        idempotency_key: row.get("idempotency_key"),
        request_fingerprint: row.get("request_fingerprint"),
        requested_scope,
        effective_scope,
        callback_config,
        reason: row.get("reason"),
        submitted_by: row.get("submitted_by"),
        policy_bundle_id: row.get("policy_bundle_id"),
        policy_bundle_version: row.get("policy_bundle_version"),
        policy_explanation: row.get("policy_explanation"),
        obligations,
        matched_rules,
        decision_trace,
        status,
        required_approvals: row.get::<i32, _>("required_approvals").max(0) as u32,
        approvals_received: row.get::<i32, _>("approvals_received").max(0) as u32,
        approved_by,
        expires_at_ms: row.get::<i64, _>("expires_at_ms").max(0) as u64,
        requested_at_ms: row.get::<i64, _>("requested_at_ms").max(0) as u64,
        resolved_at_ms: row
            .get::<Option<i64>, _>("resolved_at_ms")
            .map(|value| value.max(0) as u64),
        resolved_by_actor_id: row.get("resolved_by_actor_id"),
        resolved_by_actor_source: row.get("resolved_by_actor_source"),
        resolution_note: row.get("resolution_note"),
        slack_delivery_state: row.get("slack_delivery_state"),
        slack_delivery_error: row.get("slack_delivery_error"),
        slack_last_attempt_at_ms: row
            .get::<Option<i64>, _>("slack_last_attempt_at_ms")
            .map(|value| value.max(0) as u64),
    })
}
