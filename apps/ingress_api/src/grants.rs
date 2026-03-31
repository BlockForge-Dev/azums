use anyhow::Context;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    canonicalize_json_value, CapabilityGrantCreateRecord, CapabilityGrantRecord,
    CapabilityGrantStatus, CapabilityGrantUseOutcome,
};

#[derive(Debug, Clone)]
pub struct CapabilityGrantConsumptionRequest {
    pub tenant_id: String,
    pub environment_id: String,
    pub agent_id: String,
    pub action_family: String,
    pub adapter_type: String,
    pub requested_scope: Vec<String>,
    pub resource_binding: Option<Value>,
    pub amount: Option<i64>,
    pub request_id: Option<String>,
    pub action_request_id: String,
    pub correlation_id: Option<String>,
    pub used_at_ms: u64,
}

#[derive(Clone)]
pub struct IngressCapabilityGrantStore {
    pool: PgPool,
}

impl IngressCapabilityGrantStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_capability_grants (
                grant_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                action_family TEXT NOT NULL,
                adapter_type TEXT NOT NULL,
                granted_scope_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                resource_binding_json JSONB NULL,
                amount_ceiling BIGINT NULL,
                max_uses INTEGER NOT NULL,
                uses_consumed INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                source_action_request_id TEXT NOT NULL,
                source_approval_request_id TEXT NOT NULL,
                source_policy_bundle_id TEXT NULL,
                source_policy_bundle_version BIGINT NULL,
                created_by_actor_id TEXT NOT NULL,
                created_by_actor_source TEXT NOT NULL,
                created_at_ms BIGINT NOT NULL,
                expires_at_ms BIGINT NOT NULL,
                last_used_at_ms BIGINT NULL,
                revoked_at_ms BIGINT NULL,
                revoked_reason TEXT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_capability_grants_lookup_idx
            ON ingress_api_capability_grants(tenant_id, environment_id, agent_id, action_family, status, expires_at_ms)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_capability_grants_source_idx
            ON ingress_api_capability_grants(tenant_id, source_action_request_id, source_approval_request_id)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_capability_grant_events (
                event_id UUID PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                grant_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                actor_id TEXT NULL,
                actor_source TEXT NULL,
                details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_capability_grant_events_lookup_idx
            ON ingress_api_capability_grant_events(tenant_id, grant_id, created_at_ms DESC)
            "#,
        ];
        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure capability grant schema")?;
        }
        Ok(())
    }

    pub async fn create_grant(
        &self,
        record: &CapabilityGrantCreateRecord,
    ) -> anyhow::Result<CapabilityGrantRecord> {
        let granted_scope_json =
            serde_json::to_value(&record.granted_scope).context("serialize granted scope")?;
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin create capability grant tx")?;
        sqlx::query(
            r#"
            INSERT INTO ingress_api_capability_grants (
                grant_id,
                tenant_id,
                environment_id,
                agent_id,
                action_family,
                adapter_type,
                granted_scope_json,
                resource_binding_json,
                amount_ceiling,
                max_uses,
                uses_consumed,
                status,
                source_action_request_id,
                source_approval_request_id,
                source_policy_bundle_id,
                source_policy_bundle_version,
                created_by_actor_id,
                created_by_actor_source,
                created_at_ms,
                expires_at_ms,
                last_used_at_ms,
                revoked_at_ms,
                revoked_reason,
                updated_at
            )
            VALUES (
                $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,0,'active',$11,$12,$13,$14,$15,$16,$17,$18,NULL,NULL,NULL,now()
            )
            "#,
        )
        .bind(&record.grant_id)
        .bind(&record.tenant_id)
        .bind(&record.environment_id)
        .bind(&record.agent_id)
        .bind(&record.action_family)
        .bind(&record.adapter_type)
        .bind(granted_scope_json)
        .bind(record.resource_binding.as_ref().map(canonicalize_json_value))
        .bind(record.amount_ceiling)
        .bind(record.max_uses as i32)
        .bind(&record.source_action_request_id)
        .bind(&record.source_approval_request_id)
        .bind(&record.source_policy_bundle_id)
        .bind(record.source_policy_bundle_version)
        .bind(&record.created_by_actor_id)
        .bind(&record.created_by_actor_source)
        .bind(record.created_at_ms as i64)
        .bind(record.expires_at_ms as i64)
        .execute(&mut *tx)
        .await
        .context("failed to insert capability grant")?;

        self.insert_event_tx(
            &mut tx,
            &record.tenant_id,
            &record.grant_id,
            "created",
            Some(&record.created_by_actor_id),
            Some(&record.created_by_actor_source),
            json!({
                "environment_id": record.environment_id,
                "agent_id": record.agent_id,
                "action_family": record.action_family,
                "adapter_type": record.adapter_type,
                "granted_scope": record.granted_scope,
                "resource_binding": record.resource_binding,
                "amount_ceiling": record.amount_ceiling,
                "max_uses": record.max_uses,
                "expires_at_ms": record.expires_at_ms,
                "source_action_request_id": record.source_action_request_id,
                "source_approval_request_id": record.source_approval_request_id,
            }),
            record.created_at_ms,
        )
        .await?;

        tx.commit()
            .await
            .context("failed to commit create capability grant tx")?;

        self.load_grant_fresh(&record.tenant_id, &record.grant_id, record.created_at_ms)
            .await?
            .ok_or_else(|| anyhow::anyhow!("capability grant row missing after create"))
    }

    pub async fn load_grant(
        &self,
        tenant_id: &str,
        grant_id: &str,
    ) -> anyhow::Result<Option<CapabilityGrantRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                grant_id,
                tenant_id,
                environment_id,
                agent_id,
                action_family,
                adapter_type,
                granted_scope_json,
                resource_binding_json,
                amount_ceiling,
                max_uses,
                uses_consumed,
                status,
                source_action_request_id,
                source_approval_request_id,
                source_policy_bundle_id,
                source_policy_bundle_version,
                created_by_actor_id,
                created_by_actor_source,
                created_at_ms,
                expires_at_ms,
                last_used_at_ms,
                revoked_at_ms,
                revoked_reason
            FROM ingress_api_capability_grants
            WHERE tenant_id = $1
              AND grant_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(grant_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load capability grant")?;
        row.map(map_capability_grant_record)
            .transpose()
            .context("failed to decode capability grant")
    }

    pub async fn load_grant_fresh(
        &self,
        tenant_id: &str,
        grant_id: &str,
        now_ms: u64,
    ) -> anyhow::Result<Option<CapabilityGrantRecord>> {
        self.expire_if_needed(tenant_id, grant_id, now_ms).await?;
        self.load_grant(tenant_id, grant_id).await
    }

    pub async fn list_grants(
        &self,
        tenant_id: &str,
        environment_id: Option<&str>,
        agent_id: Option<&str>,
        status: Option<&str>,
        limit: u32,
        now_ms: u64,
    ) -> anyhow::Result<Vec<CapabilityGrantRecord>> {
        self.expire_overdue(tenant_id, now_ms).await?;
        let rows = sqlx::query(
            r#"
            SELECT
                grant_id,
                tenant_id,
                environment_id,
                agent_id,
                action_family,
                adapter_type,
                granted_scope_json,
                resource_binding_json,
                amount_ceiling,
                max_uses,
                uses_consumed,
                status,
                source_action_request_id,
                source_approval_request_id,
                source_policy_bundle_id,
                source_policy_bundle_version,
                created_by_actor_id,
                created_by_actor_source,
                created_at_ms,
                expires_at_ms,
                last_used_at_ms,
                revoked_at_ms,
                revoked_reason
            FROM ingress_api_capability_grants
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR environment_id = $2)
              AND ($3::text IS NULL OR agent_id = $3)
              AND ($4::text IS NULL OR status = $4)
            ORDER BY created_at_ms DESC
            LIMIT $5
            "#,
        )
        .bind(tenant_id)
        .bind(environment_id)
        .bind(agent_id)
        .bind(status)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to list capability grants")?;
        rows.into_iter()
            .map(map_capability_grant_record)
            .collect::<anyhow::Result<Vec<_>>>()
            .context("failed to decode capability grants")
    }

    pub async fn revoke_grant(
        &self,
        tenant_id: &str,
        grant_id: &str,
        actor_id: &str,
        reason: Option<&str>,
        now_ms: u64,
    ) -> anyhow::Result<Option<CapabilityGrantRecord>> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin revoke capability grant tx")?;
        let updated = sqlx::query(
            r#"
            UPDATE ingress_api_capability_grants
            SET
                status = 'revoked',
                revoked_at_ms = $3,
                revoked_reason = $4,
                updated_at = now()
            WHERE tenant_id = $1
              AND grant_id = $2
              AND status = 'active'
            "#,
        )
        .bind(tenant_id)
        .bind(grant_id)
        .bind(now_ms as i64)
        .bind(reason)
        .execute(&mut *tx)
        .await
        .context("failed to revoke capability grant")?
        .rows_affected();
        if updated > 0 {
            self.insert_event_tx(
                &mut tx,
                tenant_id,
                grant_id,
                "revoked",
                Some(actor_id),
                Some("internal_api"),
                json!({ "reason": reason }),
                now_ms,
            )
            .await?;
        }
        tx.commit()
            .await
            .context("failed to commit revoke capability grant tx")?;
        self.load_grant_fresh(tenant_id, grant_id, now_ms).await
    }

    #[allow(dead_code)]
    pub async fn consume_matching_grant(
        &self,
        request: &CapabilityGrantConsumptionRequest,
    ) -> anyhow::Result<Option<CapabilityGrantUseOutcome>> {
        self.expire_overdue(&request.tenant_id, request.used_at_ms)
            .await?;
        let rows = sqlx::query(
            r#"
            SELECT
                grant_id,
                tenant_id,
                environment_id,
                agent_id,
                action_family,
                adapter_type,
                granted_scope_json,
                resource_binding_json,
                amount_ceiling,
                max_uses,
                uses_consumed,
                status,
                source_action_request_id,
                source_approval_request_id,
                source_policy_bundle_id,
                source_policy_bundle_version,
                created_by_actor_id,
                created_by_actor_source,
                created_at_ms,
                expires_at_ms,
                last_used_at_ms,
                revoked_at_ms,
                revoked_reason
            FROM ingress_api_capability_grants
            WHERE tenant_id = $1
              AND environment_id = $2
              AND agent_id = $3
              AND action_family = $4
              AND adapter_type = $5
              AND status = 'active'
            ORDER BY expires_at_ms ASC, created_at_ms ASC
            LIMIT 100
            "#,
        )
        .bind(&request.tenant_id)
        .bind(&request.environment_id)
        .bind(&request.agent_id)
        .bind(&request.action_family)
        .bind(&request.adapter_type)
        .fetch_all(&self.pool)
        .await
        .context("failed to query matching capability grants")?;

        let requested_scope_lower = request
            .requested_scope
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let requested_binding = request
            .resource_binding
            .as_ref()
            .map(canonicalize_json_value);

        for row in rows {
            let grant = map_capability_grant_record(row)?;
            if !grant_matches_request(
                &grant,
                &requested_scope_lower,
                requested_binding.as_ref(),
                request.amount,
                request.used_at_ms,
            ) {
                continue;
            }
            if let Some(outcome) = self.consume_grant_by_id(&grant.grant_id, request).await? {
                return Ok(Some(outcome));
            }
        }

        Ok(None)
    }

    pub async fn find_matching_grant(
        &self,
        request: &CapabilityGrantConsumptionRequest,
    ) -> anyhow::Result<Option<CapabilityGrantRecord>> {
        self.expire_overdue(&request.tenant_id, request.used_at_ms)
            .await?;
        let rows = sqlx::query(
            r#"
            SELECT
                grant_id,
                tenant_id,
                environment_id,
                agent_id,
                action_family,
                adapter_type,
                granted_scope_json,
                resource_binding_json,
                amount_ceiling,
                max_uses,
                uses_consumed,
                status,
                source_action_request_id,
                source_approval_request_id,
                source_policy_bundle_id,
                source_policy_bundle_version,
                created_by_actor_id,
                created_by_actor_source,
                created_at_ms,
                expires_at_ms,
                last_used_at_ms,
                revoked_at_ms,
                revoked_reason
            FROM ingress_api_capability_grants
            WHERE tenant_id = $1
              AND environment_id = $2
              AND agent_id = $3
              AND action_family = $4
              AND adapter_type = $5
              AND status = 'active'
            ORDER BY expires_at_ms ASC, created_at_ms ASC
            LIMIT 100
            "#,
        )
        .bind(&request.tenant_id)
        .bind(&request.environment_id)
        .bind(&request.agent_id)
        .bind(&request.action_family)
        .bind(&request.adapter_type)
        .fetch_all(&self.pool)
        .await
        .context("failed to query matching capability grants")?;

        let requested_scope_lower = request
            .requested_scope
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let requested_binding = request
            .resource_binding
            .as_ref()
            .map(canonicalize_json_value);

        for row in rows {
            let grant = map_capability_grant_record(row)?;
            if grant_matches_request(
                &grant,
                &requested_scope_lower,
                requested_binding.as_ref(),
                request.amount,
                request.used_at_ms,
            ) {
                return Ok(Some(grant));
            }
        }

        Ok(None)
    }

    pub async fn consume_grant(
        &self,
        grant_id: &str,
        request: &CapabilityGrantConsumptionRequest,
    ) -> anyhow::Result<Option<CapabilityGrantUseOutcome>> {
        self.consume_grant_by_id(grant_id, request).await
    }

    pub async fn load_grant_for_approval(
        &self,
        tenant_id: &str,
        approval_request_id: &str,
        now_ms: u64,
    ) -> anyhow::Result<Option<CapabilityGrantRecord>> {
        self.expire_overdue(tenant_id, now_ms).await?;
        let row = sqlx::query(
            r#"
            SELECT
                grant_id,
                tenant_id,
                environment_id,
                agent_id,
                action_family,
                adapter_type,
                granted_scope_json,
                resource_binding_json,
                amount_ceiling,
                max_uses,
                uses_consumed,
                status,
                source_action_request_id,
                source_approval_request_id,
                source_policy_bundle_id,
                source_policy_bundle_version,
                created_by_actor_id,
                created_by_actor_source,
                created_at_ms,
                expires_at_ms,
                last_used_at_ms,
                revoked_at_ms,
                revoked_reason
            FROM ingress_api_capability_grants
            WHERE tenant_id = $1
              AND source_approval_request_id = $2
            ORDER BY created_at_ms DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(approval_request_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load capability grant for approval")?;
        row.map(map_capability_grant_record)
            .transpose()
            .context("failed to decode capability grant for approval")
    }

    async fn consume_grant_by_id(
        &self,
        grant_id: &str,
        request: &CapabilityGrantConsumptionRequest,
    ) -> anyhow::Result<Option<CapabilityGrantUseOutcome>> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin consume capability grant tx")?;
        let row = sqlx::query(
            r#"
            SELECT
                grant_id,
                tenant_id,
                environment_id,
                agent_id,
                action_family,
                adapter_type,
                granted_scope_json,
                resource_binding_json,
                amount_ceiling,
                max_uses,
                uses_consumed,
                status,
                source_action_request_id,
                source_approval_request_id,
                source_policy_bundle_id,
                source_policy_bundle_version,
                created_by_actor_id,
                created_by_actor_source,
                created_at_ms,
                expires_at_ms,
                last_used_at_ms,
                revoked_at_ms,
                revoked_reason
            FROM ingress_api_capability_grants
            WHERE tenant_id = $1
              AND grant_id = $2
            FOR UPDATE
            "#,
        )
        .bind(&request.tenant_id)
        .bind(grant_id)
        .fetch_optional(&mut *tx)
        .await
        .context("failed to lock capability grant")?;
        let Some(row) = row else {
            tx.commit().await.context("commit empty consume grant tx")?;
            return Ok(None);
        };
        let mut grant = map_capability_grant_record(row)?;
        let requested_scope_lower = request
            .requested_scope
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let requested_binding = request
            .resource_binding
            .as_ref()
            .map(canonicalize_json_value);
        if !grant_matches_request(
            &grant,
            &requested_scope_lower,
            requested_binding.as_ref(),
            request.amount,
            request.used_at_ms,
        ) {
            tx.commit()
                .await
                .context("commit mismatch consume grant tx")?;
            return Ok(None);
        }

        if request.used_at_ms >= grant.expires_at_ms {
            grant.status = CapabilityGrantStatus::Expired;
        } else {
            grant.uses_consumed = grant.uses_consumed.saturating_add(1);
            grant.last_used_at_ms = Some(request.used_at_ms);
            if grant.uses_consumed >= grant.max_uses {
                grant.status = CapabilityGrantStatus::Exhausted;
            }
        }

        sqlx::query(
            r#"
            UPDATE ingress_api_capability_grants
            SET
                uses_consumed = $3,
                status = $4,
                last_used_at_ms = $5,
                updated_at = now()
            WHERE tenant_id = $1
              AND grant_id = $2
            "#,
        )
        .bind(&request.tenant_id)
        .bind(grant_id)
        .bind(grant.uses_consumed as i32)
        .bind(grant.status.as_str())
        .bind(grant.last_used_at_ms.map(|value| value as i64))
        .execute(&mut *tx)
        .await
        .context("failed to update consumed capability grant")?;

        self.insert_event_tx(
            &mut tx,
            &request.tenant_id,
            grant_id,
            if matches!(grant.status, CapabilityGrantStatus::Expired) {
                "expired"
            } else {
                "used"
            },
            Some(&request.agent_id),
            Some("grant_runtime"),
            json!({
                "request_id": request.request_id,
                "action_request_id": request.action_request_id,
                "correlation_id": request.correlation_id,
                "requested_scope": request.requested_scope,
                "amount": request.amount,
                "uses_consumed": grant.uses_consumed,
                "max_uses": grant.max_uses,
                "status": grant.status.as_str(),
            }),
            request.used_at_ms,
        )
        .await?;

        tx.commit()
            .await
            .context("failed to commit consume capability grant tx")?;

        let uses_remaining = grant.max_uses.saturating_sub(grant.uses_consumed);
        Ok(Some(CapabilityGrantUseOutcome {
            grant,
            uses_remaining,
        }))
    }

    async fn expire_if_needed(
        &self,
        tenant_id: &str,
        grant_id: &str,
        now_ms: u64,
    ) -> anyhow::Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin expire capability grant tx")?;
        let updated = sqlx::query(
            r#"
            UPDATE ingress_api_capability_grants
            SET
                status = 'expired',
                updated_at = now()
            WHERE tenant_id = $1
              AND grant_id = $2
              AND status = 'active'
              AND expires_at_ms <= $3
            "#,
        )
        .bind(tenant_id)
        .bind(grant_id)
        .bind(now_ms as i64)
        .execute(&mut *tx)
        .await
        .context("failed to expire capability grant")?
        .rows_affected();
        if updated > 0 {
            self.insert_event_tx(
                &mut tx,
                tenant_id,
                grant_id,
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
            .context("failed to commit expire capability grant tx")?;
        Ok(())
    }

    async fn expire_overdue(&self, tenant_id: &str, now_ms: u64) -> anyhow::Result<()> {
        let rows = sqlx::query_scalar::<_, String>(
            r#"
            SELECT grant_id
            FROM ingress_api_capability_grants
            WHERE tenant_id = $1
              AND status = 'active'
              AND expires_at_ms <= $2
            "#,
        )
        .bind(tenant_id)
        .bind(now_ms as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to query overdue capability grants")?;
        for grant_id in rows {
            self.expire_if_needed(tenant_id, &grant_id, now_ms).await?;
        }
        Ok(())
    }

    async fn insert_event_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &str,
        grant_id: &str,
        event_type: &str,
        actor_id: Option<&str>,
        actor_source: Option<&str>,
        details: Value,
        created_at_ms: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ingress_api_capability_grant_events (
                event_id,
                tenant_id,
                grant_id,
                event_type,
                actor_id,
                actor_source,
                details_json,
                created_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(grant_id)
        .bind(event_type)
        .bind(actor_id)
        .bind(actor_source)
        .bind(details)
        .bind(created_at_ms as i64)
        .execute(&mut **tx)
        .await
        .context("failed to insert capability grant event")?;
        Ok(())
    }
}

fn grant_matches_request(
    grant: &CapabilityGrantRecord,
    requested_scope_lower: &[String],
    requested_binding: Option<&Value>,
    amount: Option<i64>,
    now_ms: u64,
) -> bool {
    if !matches!(grant.status, CapabilityGrantStatus::Active) {
        return false;
    }
    if now_ms >= grant.expires_at_ms {
        return false;
    }
    if grant.uses_consumed >= grant.max_uses {
        return false;
    }
    let granted_scope_lower = grant
        .granted_scope
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if !requested_scope_lower
        .iter()
        .all(|scope| granted_scope_lower.iter().any(|allowed| allowed == scope))
    {
        return false;
    }
    if let Some(ceiling) = grant.amount_ceiling {
        if amount.unwrap_or(i64::MAX) > ceiling {
            return false;
        }
    }
    if let Some(binding) = grant.resource_binding.as_ref() {
        let Some(requested_binding) = requested_binding else {
            return false;
        };
        if canonicalize_json_value(binding) != *requested_binding {
            return false;
        }
    }
    true
}

fn map_capability_grant_record(
    row: sqlx::postgres::PgRow,
) -> anyhow::Result<CapabilityGrantRecord> {
    let granted_scope = serde_json::from_value::<Vec<String>>(row.get("granted_scope_json"))
        .context("decode granted_scope_json")?;
    let status = match row.get::<String, _>("status").as_str() {
        "active" => CapabilityGrantStatus::Active,
        "revoked" => CapabilityGrantStatus::Revoked,
        "expired" => CapabilityGrantStatus::Expired,
        "exhausted" => CapabilityGrantStatus::Exhausted,
        other => anyhow::bail!("unsupported capability grant status `{other}`"),
    };
    Ok(CapabilityGrantRecord {
        grant_id: row.get("grant_id"),
        tenant_id: row.get("tenant_id"),
        environment_id: row.get("environment_id"),
        agent_id: row.get("agent_id"),
        action_family: row.get("action_family"),
        adapter_type: row.get("adapter_type"),
        granted_scope,
        resource_binding: row.get("resource_binding_json"),
        amount_ceiling: row.get("amount_ceiling"),
        max_uses: row.get::<i32, _>("max_uses").max(0) as u32,
        uses_consumed: row.get::<i32, _>("uses_consumed").max(0) as u32,
        status,
        source_action_request_id: row.get("source_action_request_id"),
        source_approval_request_id: row.get("source_approval_request_id"),
        source_policy_bundle_id: row.get("source_policy_bundle_id"),
        source_policy_bundle_version: row.get("source_policy_bundle_version"),
        created_by_actor_id: row.get("created_by_actor_id"),
        created_by_actor_source: row.get("created_by_actor_source"),
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
        expires_at_ms: row.get::<i64, _>("expires_at_ms").max(0) as u64,
        last_used_at_ms: row
            .get::<Option<i64>, _>("last_used_at_ms")
            .map(|value| value.max(0) as u64),
        revoked_at_ms: row
            .get::<Option<i64>, _>("revoked_at_ms")
            .map(|value| value.max(0) as u64),
        revoked_reason: row.get("revoked_reason"),
    })
}

#[cfg(test)]
mod tests {
    use super::{grant_matches_request, CapabilityGrantRecord, CapabilityGrantStatus};
    use serde_json::json;

    fn sample_grant() -> CapabilityGrantRecord {
        CapabilityGrantRecord {
            grant_id: "grant_123".to_owned(),
            tenant_id: "tenant_a".to_owned(),
            environment_id: "prod".to_owned(),
            agent_id: "agent_1".to_owned(),
            action_family: "transfer".to_owned(),
            adapter_type: "adapter_solana".to_owned(),
            granted_scope: vec!["payments".to_owned()],
            resource_binding: Some(json!({
                "asset": "SOL",
                "to_addr": "11111111111111111111111111111111",
            })),
            amount_ceiling: Some(100),
            max_uses: 2,
            uses_consumed: 0,
            status: CapabilityGrantStatus::Active,
            source_action_request_id: "act_123".to_owned(),
            source_approval_request_id: "apr_123".to_owned(),
            source_policy_bundle_id: Some("bundle_finance".to_owned()),
            source_policy_bundle_version: Some(2),
            created_by_actor_id: "ops".to_owned(),
            created_by_actor_source: "internal_api".to_owned(),
            created_at_ms: 1,
            expires_at_ms: 10_000,
            last_used_at_ms: None,
            revoked_at_ms: None,
            revoked_reason: None,
        }
    }

    #[test]
    fn grant_matches_request_accepts_matching_scope_binding_and_amount() {
        let grant = sample_grant();
        assert!(grant_matches_request(
            &grant,
            &[String::from("payments")],
            Some(&json!({
                "to_addr": "11111111111111111111111111111111",
                "asset": "SOL",
            })),
            Some(50),
            5_000,
        ));
    }

    #[test]
    fn grant_matches_request_rejects_binding_and_amount_mismatch() {
        let grant = sample_grant();
        assert!(!grant_matches_request(
            &grant,
            &[String::from("payments")],
            Some(&json!({
                "to_addr": "22222222222222222222222222222222",
                "asset": "SOL",
            })),
            Some(50),
            5_000,
        ));
        assert!(!grant_matches_request(
            &grant,
            &[String::from("payments")],
            Some(&json!({
                "to_addr": "11111111111111111111111111111111",
                "asset": "SOL",
            })),
            Some(500),
            5_000,
        ));
    }
}
