mod approval;
mod connector_binding;
mod grants;
mod secret_crypto;

use adapter_contract::AdapterRegistry;
use anyhow::Context;
use axum::body::Bytes;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{Datelike, Timelike, Utc, Weekday};
use execution_core::integration::postgresq::{PostgresQConfig, PostgresQStore};
use execution_core::ports::Clock;
use execution_core::{
    AdapterId, AuthContext, Authorizer, CoreError, ExecutionCore, IntentId, IntentKind,
    NormalizedIntent, OperatorPrincipal, ReplayPolicy, RequestId, RetryPolicy, SystemClock,
    TenantId,
};
use hmac::{Hmac, Mac};
use observability::{
    apply_request_context, derive_request_context, init_metrics, init_tracing, record_http_request,
    render_metrics, ObservabilityConfig,
};
use platform_auth::{
    constant_time_eq, env_var_opt, extract_bearer_token, header_opt, parse_kv_map,
    parse_principal_tenant_map as shared_parse_principal_tenant_map, principal_tenant_allowed,
    resolve_principal_binding,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};
use uuid::Uuid;

use approval::IngressApprovalStore;
use connector_binding::{IngressConnectorBindingStore, IngressConnectorSecretBroker};
use grants::{CapabilityGrantConsumptionRequest, IngressCapabilityGrantStore};
use secret_crypto::SecretCipher;

type HmacSha256 = Hmac<Sha256>;
const FREE_PLAY_WINDOW_MS: u64 = 1000 * 60 * 60 * 24 * 30;

#[derive(Clone)]
struct AppState {
    core: Arc<ExecutionCore>,
    audit_store: Arc<IngressIntakeAuditStore>,
    environment_store: Arc<IngressEnvironmentStore>,
    agent_store: Arc<IngressAgentStore>,
    agent_action_idempotency_store: Arc<IngressAgentActionIdempotencyStore>,
    approval_store: Arc<IngressApprovalStore>,
    approval_workflow: Arc<ApprovalWorkflowConfig>,
    approval_http_client: Arc<Client>,
    capability_grant_store: Arc<IngressCapabilityGrantStore>,
    grant_workflow: Arc<GrantWorkflowConfig>,
    connector_binding_store: Arc<IngressConnectorBindingStore>,
    connector_secret_broker: Arc<IngressConnectorSecretBroker>,
    policy_bundle_store: Arc<IngressPolicyBundleStore>,
    api_key_store: Arc<IngressTenantApiKeyStore>,
    webhook_key_store: Arc<IngressTenantWebhookKeyStore>,
    quota_store: Arc<IngressTenantQuotaStore>,
    auth: Arc<IngressAuthConfig>,
    schemas: Arc<IngressSchemaRegistry>,
    clock: Arc<SystemClock>,
    execution_policy_enforcement: Arc<ExecutionPolicyEnforcement>,
}

#[derive(Clone)]
struct ExecutionPolicyEnforcement {
    enabled: bool,
    canary_tenants: HashSet<String>,
}

impl ExecutionPolicyEnforcement {
    fn from_env() -> Self {
        Self {
            enabled: env_bool("INGRESS_EXECUTION_POLICY_ENFORCEMENT_ENABLED", false),
            canary_tenants: parse_csv_set(
                env_var_opt("INGRESS_EXECUTION_POLICY_CANARY_TENANTS").as_deref(),
            ),
        }
    }

    fn is_enforced_for_tenant(&self, tenant_id: &str) -> bool {
        if !self.enabled {
            return false;
        }
        if self.canary_tenants.is_empty() {
            return true;
        }
        self.canary_tenants.contains(tenant_id)
    }
}

#[derive(Clone)]
struct ApprovalWorkflowConfig {
    slack_webhook_url: Option<String>,
    slack_signing_secret: Option<String>,
    request_ttl_ms: u64,
}

impl ApprovalWorkflowConfig {
    fn from_env() -> Self {
        let request_ttl_ms =
            env_u64("INGRESS_APPROVAL_REQUEST_TTL_SECONDS", 3600).saturating_mul(1000);
        Self {
            slack_webhook_url: env_var_opt("INGRESS_APPROVAL_SLACK_WEBHOOK_URL"),
            slack_signing_secret: env_var_opt("INGRESS_APPROVAL_SLACK_SIGNING_SECRET"),
            request_ttl_ms: if request_ttl_ms == 0 {
                3_600_000
            } else {
                request_ttl_ms
            },
        }
    }
}

#[derive(Clone)]
struct GrantWorkflowConfig {
    default_ttl_ms: u64,
    max_ttl_ms: u64,
    max_uses: u32,
}

impl GrantWorkflowConfig {
    fn from_env() -> Self {
        let default_ttl_ms =
            env_u64("INGRESS_GRANT_DEFAULT_TTL_SECONDS", 3600).saturating_mul(1000);
        let max_ttl_ms = env_u64("INGRESS_GRANT_MAX_TTL_SECONDS", 86_400).saturating_mul(1000);
        let max_uses = env_u64("INGRESS_GRANT_MAX_USES", 20).clamp(1, 1000) as u32;
        Self {
            default_ttl_ms: if default_ttl_ms == 0 {
                3_600_000
            } else {
                default_ttl_ms
            },
            max_ttl_ms: if max_ttl_ms == 0 {
                86_400_000
            } else {
                max_ttl_ms.max(default_ttl_ms)
            },
            max_uses,
        }
    }
}

#[derive(Clone)]
struct IngressIntakeAuditStore {
    pool: PgPool,
}

impl IngressIntakeAuditStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_intake_audits (
                audit_id UUID PRIMARY KEY,
                request_id TEXT NOT NULL,
                tenant_id TEXT NOT NULL,
                channel TEXT NOT NULL,
                endpoint TEXT NOT NULL,
                method TEXT NOT NULL,
                principal_id TEXT NULL,
                submitter_kind TEXT NULL,
                auth_scheme TEXT NULL,
                intent_kind TEXT NULL,
                correlation_id TEXT NULL,
                idempotency_key TEXT NULL,
                idempotency_decision TEXT NULL,
                validation_result TEXT NOT NULL,
                rejection_reason TEXT NULL,
                error_status INTEGER NULL,
                error_message TEXT NULL,
                accepted_intent_id TEXT NULL,
                accepted_job_id TEXT NULL,
                details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_intake_audits_tenant_created_idx
            ON ingress_api_intake_audits(tenant_id, created_at DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_intake_audits_request_idx
            ON ingress_api_intake_audits(request_id, created_at DESC)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress intake audit schema")?;
        }

        Ok(())
    }

    async fn record(&self, record: &IngressIntakeAuditRecord) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ingress_api_intake_audits (
                audit_id,
                request_id,
                tenant_id,
                channel,
                endpoint,
                method,
                principal_id,
                submitter_kind,
                auth_scheme,
                intent_kind,
                correlation_id,
                idempotency_key,
                idempotency_decision,
                validation_result,
                rejection_reason,
                error_status,
                error_message,
                accepted_intent_id,
                accepted_job_id,
                details_json,
                created_at_ms
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21
            )
            "#,
        )
        .bind(record.audit_id)
        .bind(&record.request_id)
        .bind(&record.tenant_id)
        .bind(&record.channel)
        .bind(&record.endpoint)
        .bind(&record.method)
        .bind(&record.principal_id)
        .bind(&record.submitter_kind)
        .bind(&record.auth_scheme)
        .bind(&record.intent_kind)
        .bind(&record.correlation_id)
        .bind(&record.idempotency_key)
        .bind(&record.idempotency_decision)
        .bind(&record.validation_result)
        .bind(&record.rejection_reason)
        .bind(record.error_status)
        .bind(&record.error_message)
        .bind(&record.accepted_intent_id)
        .bind(&record.accepted_job_id)
        .bind(&record.details_json)
        .bind(record.created_at_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to persist ingress intake audit row")?;

        Ok(())
    }
}

#[derive(Clone)]
struct IngressTenantApiKeyStore {
    pool: PgPool,
}

#[derive(Debug, Clone)]
struct TenantApiKeyRecord {
    key_id: String,
    tenant_id: String,
    label: String,
    key_prefix: String,
    key_last4: String,
    created_by_principal_id: String,
    created_at_ms: u64,
    revoked_at_ms: Option<u64>,
    last_used_at_ms: Option<u64>,
}

impl IngressTenantApiKeyStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_tenant_api_keys (
                key_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                label TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                key_prefix TEXT NOT NULL,
                key_last4 TEXT NOT NULL,
                created_by_principal_id TEXT NOT NULL,
                created_at_ms BIGINT NOT NULL,
                revoked_at_ms BIGINT NULL,
                last_used_at_ms BIGINT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_api_keys_tenant_created_idx
            ON ingress_api_tenant_api_keys(tenant_id, created_at_ms DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_api_keys_tenant_active_idx
            ON ingress_api_tenant_api_keys(tenant_id, revoked_at_ms)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress tenant api key schema")?;
        }
        Ok(())
    }

    async fn upsert_api_key(&self, record: &TenantApiKeyProvisionRequest) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ingress_api_tenant_api_keys (
                key_id,
                tenant_id,
                label,
                key_hash,
                key_prefix,
                key_last4,
                created_by_principal_id,
                created_at_ms,
                revoked_at_ms,
                last_used_at_ms,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NULL, NULL, now())
            ON CONFLICT (key_id)
            DO UPDATE SET
                label = EXCLUDED.label,
                key_hash = EXCLUDED.key_hash,
                key_prefix = EXCLUDED.key_prefix,
                key_last4 = EXCLUDED.key_last4,
                created_by_principal_id = EXCLUDED.created_by_principal_id,
                created_at_ms = EXCLUDED.created_at_ms,
                revoked_at_ms = NULL,
                updated_at = now()
            "#,
        )
        .bind(&record.key_id)
        .bind(&record.tenant_id)
        .bind(&record.label)
        .bind(&record.key_hash)
        .bind(&record.key_prefix)
        .bind(&record.key_last4)
        .bind(&record.created_by_principal_id)
        .bind(record.created_at_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to upsert ingress tenant api key row")?;

        Ok(())
    }

    async fn revoke_api_key(
        &self,
        tenant_id: &str,
        key_id: &str,
        revoked_at_ms: u64,
    ) -> anyhow::Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE ingress_api_tenant_api_keys
            SET revoked_at_ms = $3, updated_at = now()
            WHERE tenant_id = $1
              AND key_id = $2
              AND revoked_at_ms IS NULL
            "#,
        )
        .bind(tenant_id)
        .bind(key_id)
        .bind(revoked_at_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to revoke ingress tenant api key row")?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_api_keys(
        &self,
        tenant_id: &str,
        include_inactive: bool,
        limit: u32,
    ) -> anyhow::Result<Vec<TenantApiKeyRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                key_id,
                tenant_id,
                label,
                key_prefix,
                key_last4,
                created_by_principal_id,
                created_at_ms,
                revoked_at_ms,
                last_used_at_ms
            FROM ingress_api_tenant_api_keys
            WHERE tenant_id = $1
              AND ($2::boolean OR revoked_at_ms IS NULL)
            ORDER BY created_at_ms DESC
            LIMIT $3
            "#,
        )
        .bind(tenant_id)
        .bind(include_inactive)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to list ingress tenant api key rows")?;

        Ok(rows
            .into_iter()
            .map(|row| TenantApiKeyRecord {
                key_id: row.get("key_id"),
                tenant_id: row.get("tenant_id"),
                label: row.get("label"),
                key_prefix: row.get("key_prefix"),
                key_last4: row.get("key_last4"),
                created_by_principal_id: row.get("created_by_principal_id"),
                created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
                revoked_at_ms: row
                    .get::<Option<i64>, _>("revoked_at_ms")
                    .map(|value| value.max(0) as u64),
                last_used_at_ms: row
                    .get::<Option<i64>, _>("last_used_at_ms")
                    .map(|value| value.max(0) as u64),
            })
            .collect())
    }

    async fn validate_api_key(
        &self,
        tenant_id: &str,
        key_hash: &str,
        used_at_ms: u64,
    ) -> anyhow::Result<bool> {
        let row = sqlx::query_scalar::<_, String>(
            r#"
            SELECT key_id
            FROM ingress_api_tenant_api_keys
            WHERE tenant_id = $1
              AND key_hash = $2
              AND revoked_at_ms IS NULL
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await
        .context("failed to validate ingress tenant api key row")?;

        let Some(key_id) = row else {
            return Ok(false);
        };

        if let Err(error) = sqlx::query(
            r#"
            UPDATE ingress_api_tenant_api_keys
            SET last_used_at_ms = $3, updated_at = now()
            WHERE tenant_id = $1
              AND key_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(&key_id)
        .bind(used_at_ms as i64)
        .execute(&self.pool)
        .await
        {
            warn!(
                error = %error,
                tenant_id = %tenant_id,
                key_id = %key_id,
                "failed to update ingress tenant api key last_used_at"
            );
        }

        Ok(true)
    }
}

#[derive(Debug, Clone)]
struct TenantWebhookKeyRecord {
    key_id: String,
    tenant_id: String,
    source: String,
    secret_value: String,
    secret_last4: String,
    created_by_principal_id: String,
    created_at_ms: u64,
    revoked_at_ms: Option<u64>,
    expires_at_ms: Option<u64>,
    last_used_at_ms: Option<u64>,
}

#[derive(Clone)]
struct IngressTenantWebhookKeyStore {
    pool: PgPool,
}

impl IngressTenantWebhookKeyStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_tenant_webhook_keys (
                key_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                source TEXT NOT NULL,
                secret_value TEXT NOT NULL,
                secret_last4 TEXT NOT NULL,
                created_by_principal_id TEXT NOT NULL,
                created_at_ms BIGINT NOT NULL,
                revoked_at_ms BIGINT NULL,
                expires_at_ms BIGINT NULL,
                last_used_at_ms BIGINT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_webhook_keys_tenant_source_idx
            ON ingress_api_tenant_webhook_keys(tenant_id, source, created_at_ms DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_webhook_keys_active_idx
            ON ingress_api_tenant_webhook_keys(tenant_id, source, revoked_at_ms, expires_at_ms)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress tenant webhook key schema")?;
        }

        Ok(())
    }

    async fn issue_webhook_key(
        &self,
        tenant_id: &str,
        source: &str,
        created_by_principal_id: &str,
        now_ms: u64,
        grace_seconds: u64,
    ) -> anyhow::Result<(TenantWebhookKeyRecord, u64, Option<u64>)> {
        let key_id = format!("whk_{}", Uuid::new_v4().simple());
        let secret_value = format!(
            "whsec_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        let secret_last4 = secret_value
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<char>>()
            .into_iter()
            .rev()
            .collect::<String>();
        let grace_ms = grace_seconds.saturating_mul(1000);
        let expires_at_ms = if grace_ms == 0 {
            now_ms
        } else {
            now_ms.saturating_add(grace_ms)
        };

        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin tenant webhook key tx")?;

        let rotated_rows = sqlx::query(
            r#"
            UPDATE ingress_api_tenant_webhook_keys
            SET
                revoked_at_ms = COALESCE(revoked_at_ms, $3),
                expires_at_ms = COALESCE(expires_at_ms, $4),
                updated_at = now()
            WHERE tenant_id = $1
              AND source = $2
              AND revoked_at_ms IS NULL
            "#,
        )
        .bind(tenant_id)
        .bind(source)
        .bind(now_ms as i64)
        .bind(expires_at_ms as i64)
        .execute(&mut *tx)
        .await
        .context("failed to rotate existing tenant webhook keys")?
        .rows_affected();

        sqlx::query(
            r#"
            INSERT INTO ingress_api_tenant_webhook_keys (
                key_id,
                tenant_id,
                source,
                secret_value,
                secret_last4,
                created_by_principal_id,
                created_at_ms,
                revoked_at_ms,
                expires_at_ms,
                last_used_at_ms,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, NULL, NULL, NULL, now())
            "#,
        )
        .bind(&key_id)
        .bind(tenant_id)
        .bind(source)
        .bind(&secret_value)
        .bind(&secret_last4)
        .bind(created_by_principal_id)
        .bind(now_ms as i64)
        .execute(&mut *tx)
        .await
        .context("failed to insert tenant webhook key")?;

        tx.commit()
            .await
            .context("failed to commit tenant webhook key tx")?;

        Ok((
            TenantWebhookKeyRecord {
                key_id,
                tenant_id: tenant_id.to_owned(),
                source: source.to_owned(),
                secret_value,
                secret_last4,
                created_by_principal_id: created_by_principal_id.to_owned(),
                created_at_ms: now_ms,
                revoked_at_ms: None,
                expires_at_ms: None,
                last_used_at_ms: None,
            },
            rotated_rows,
            if rotated_rows > 0 {
                Some(expires_at_ms)
            } else {
                None
            },
        ))
    }

    async fn list_webhook_keys(
        &self,
        tenant_id: &str,
        source: Option<&str>,
        include_inactive: bool,
        limit: u32,
        now_ms: u64,
    ) -> anyhow::Result<Vec<TenantWebhookKeyRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                key_id,
                tenant_id,
                source,
                secret_value,
                secret_last4,
                created_by_principal_id,
                created_at_ms,
                revoked_at_ms,
                expires_at_ms,
                last_used_at_ms
            FROM ingress_api_tenant_webhook_keys
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR source = $2)
              AND (
                $3::boolean = true
                OR revoked_at_ms IS NULL
                OR (expires_at_ms IS NOT NULL AND expires_at_ms > $4)
              )
            ORDER BY created_at_ms DESC
            LIMIT $5
            "#,
        )
        .bind(tenant_id)
        .bind(source)
        .bind(include_inactive)
        .bind(now_ms as i64)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to list tenant webhook keys")?;

        Ok(rows
            .into_iter()
            .map(|row| TenantWebhookKeyRecord {
                key_id: row.get::<String, _>("key_id"),
                tenant_id: row.get::<String, _>("tenant_id"),
                source: row.get::<String, _>("source"),
                secret_value: row.get::<String, _>("secret_value"),
                secret_last4: row.get::<String, _>("secret_last4"),
                created_by_principal_id: row.get::<String, _>("created_by_principal_id"),
                created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
                revoked_at_ms: row
                    .get::<Option<i64>, _>("revoked_at_ms")
                    .map(|value| value.max(0) as u64),
                expires_at_ms: row
                    .get::<Option<i64>, _>("expires_at_ms")
                    .map(|value| value.max(0) as u64),
                last_used_at_ms: row
                    .get::<Option<i64>, _>("last_used_at_ms")
                    .map(|value| value.max(0) as u64),
            })
            .collect())
    }

    async fn revoke_webhook_key(
        &self,
        tenant_id: &str,
        key_id: &str,
        revoked_at_ms: u64,
        grace_seconds: u64,
    ) -> anyhow::Result<bool> {
        let grace_ms = grace_seconds.saturating_mul(1000);
        let expires_at_ms = if grace_ms == 0 {
            revoked_at_ms
        } else {
            revoked_at_ms.saturating_add(grace_ms)
        };

        let result = sqlx::query(
            r#"
            UPDATE ingress_api_tenant_webhook_keys
            SET
                revoked_at_ms = COALESCE(revoked_at_ms, $3),
                expires_at_ms = COALESCE(expires_at_ms, $4),
                updated_at = now()
            WHERE tenant_id = $1
              AND key_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(key_id)
        .bind(revoked_at_ms as i64)
        .bind(expires_at_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to revoke tenant webhook key")?;

        Ok(result.rows_affected() > 0)
    }

    async fn load_active_signing_candidates(
        &self,
        tenant_id: &str,
        source: &str,
        requested_key_id: Option<&str>,
        now_ms: u64,
    ) -> anyhow::Result<Vec<TenantWebhookKeyRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                key_id,
                tenant_id,
                source,
                secret_value,
                secret_last4,
                created_by_principal_id,
                created_at_ms,
                revoked_at_ms,
                expires_at_ms,
                last_used_at_ms
            FROM ingress_api_tenant_webhook_keys
            WHERE tenant_id = $1
              AND source = $2
              AND ($3::text IS NULL OR key_id = $3)
              AND (
                revoked_at_ms IS NULL
                OR (expires_at_ms IS NOT NULL AND expires_at_ms > $4)
              )
            ORDER BY created_at_ms DESC
            "#,
        )
        .bind(tenant_id)
        .bind(source)
        .bind(requested_key_id)
        .bind(now_ms as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to load webhook signing key candidates")?;

        Ok(rows
            .into_iter()
            .map(|row| TenantWebhookKeyRecord {
                key_id: row.get::<String, _>("key_id"),
                tenant_id: row.get::<String, _>("tenant_id"),
                source: row.get::<String, _>("source"),
                secret_value: row.get::<String, _>("secret_value"),
                secret_last4: row.get::<String, _>("secret_last4"),
                created_by_principal_id: row.get::<String, _>("created_by_principal_id"),
                created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
                revoked_at_ms: row
                    .get::<Option<i64>, _>("revoked_at_ms")
                    .map(|value| value.max(0) as u64),
                expires_at_ms: row
                    .get::<Option<i64>, _>("expires_at_ms")
                    .map(|value| value.max(0) as u64),
                last_used_at_ms: row
                    .get::<Option<i64>, _>("last_used_at_ms")
                    .map(|value| value.max(0) as u64),
            })
            .collect())
    }

    async fn mark_key_used(&self, key_id: &str, used_at_ms: u64) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE ingress_api_tenant_webhook_keys
            SET last_used_at_ms = $2, updated_at = now()
            WHERE key_id = $1
            "#,
        )
        .bind(key_id)
        .bind(used_at_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to update webhook key last_used_at")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuotaPlanTier {
    Developer,
    Team,
    Enterprise,
}

impl QuotaPlanTier {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "developer" => Some(Self::Developer),
            "team" => Some(Self::Team),
            "enterprise" => Some(Self::Enterprise),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Developer => "developer",
            Self::Team => "team",
            Self::Enterprise => "enterprise",
        }
    }

    fn default_free_play_limit(self) -> i64 {
        match self {
            Self::Developer => 500,
            Self::Team => 1_000,
            Self::Enterprise => 10_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuotaAccessMode {
    FreePlay,
    Paid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionPolicy {
    CustomerSigned,
    CustomerManagedSigner,
    Sponsored,
}

impl ExecutionPolicy {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "customer_signed" | "customer-signed" => Some(Self::CustomerSigned),
            "customer_managed_signer" | "customer-managed-signer" => {
                Some(Self::CustomerManagedSigner)
            }
            "sponsored" => Some(Self::Sponsored),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::CustomerSigned => "customer_signed",
            Self::CustomerManagedSigner => "customer_managed_signer",
            Self::Sponsored => "sponsored",
        }
    }
}

impl QuotaAccessMode {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "free_play" | "free-play" | "freeplay" => Some(Self::FreePlay),
            "paid" => Some(Self::Paid),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::FreePlay => "free_play",
            Self::Paid => "paid",
        }
    }
}

#[derive(Debug, Clone)]
struct TenantQuotaProfile {
    tenant_id: String,
    plan: QuotaPlanTier,
    access_mode: QuotaAccessMode,
    execution_policy: ExecutionPolicy,
    sponsored_monthly_cap_requests: i64,
    free_play_limit: i64,
    updated_by_principal_id: String,
    updated_at_ms: u64,
}

#[derive(Debug, Clone)]
struct QuotaCheckResult {
    profile: TenantQuotaProfile,
    used_requests: i64,
}

#[derive(Clone)]
struct IngressTenantQuotaStore {
    pool: PgPool,
    default_plan: QuotaPlanTier,
    default_access_mode: QuotaAccessMode,
    default_execution_policy: ExecutionPolicy,
    default_sponsored_monthly_cap_requests: i64,
    default_free_play_limit: i64,
}

impl IngressTenantQuotaStore {
    fn new(
        pool: PgPool,
        default_plan: QuotaPlanTier,
        default_access_mode: QuotaAccessMode,
        default_execution_policy: ExecutionPolicy,
        default_sponsored_monthly_cap_requests: i64,
        default_free_play_limit: i64,
    ) -> Self {
        Self {
            pool,
            default_plan,
            default_access_mode,
            default_execution_policy,
            default_sponsored_monthly_cap_requests,
            default_free_play_limit,
        }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_tenant_quota_profiles (
                tenant_id TEXT PRIMARY KEY,
                plan_tier TEXT NOT NULL,
                access_mode TEXT NOT NULL,
                execution_policy TEXT NOT NULL DEFAULT 'customer_signed',
                sponsored_monthly_cap_requests BIGINT NOT NULL DEFAULT 10000,
                free_play_limit BIGINT NOT NULL,
                updated_by_principal_id TEXT NOT NULL,
                updated_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            ALTER TABLE ingress_api_tenant_quota_profiles
            ADD COLUMN IF NOT EXISTS execution_policy TEXT NOT NULL DEFAULT 'customer_signed'
            "#,
            r#"
            ALTER TABLE ingress_api_tenant_quota_profiles
            ADD COLUMN IF NOT EXISTS sponsored_monthly_cap_requests BIGINT NOT NULL DEFAULT 10000
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_quota_profiles_updated_idx
            ON ingress_api_tenant_quota_profiles(updated_at_ms DESC)
            "#,
        ];
        for statement in ddl {
            sqlx::query(statement)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress tenant quota schema")?;
        }
        Ok(())
    }

    fn default_profile(&self, tenant_id: &str, now_ms: u64) -> TenantQuotaProfile {
        TenantQuotaProfile {
            tenant_id: tenant_id.to_owned(),
            plan: self.default_plan,
            access_mode: self.default_access_mode,
            execution_policy: self.default_execution_policy,
            sponsored_monthly_cap_requests: self.default_sponsored_monthly_cap_requests,
            free_play_limit: self.default_free_play_limit,
            updated_by_principal_id: "system:ingress_default".to_owned(),
            updated_at_ms: now_ms,
        }
    }

    async fn upsert_profile(&self, profile: &TenantQuotaProfile) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ingress_api_tenant_quota_profiles (
                tenant_id,
                plan_tier,
                access_mode,
                execution_policy,
                sponsored_monthly_cap_requests,
                free_play_limit,
                updated_by_principal_id,
                updated_at_ms,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())
            ON CONFLICT (tenant_id)
            DO UPDATE SET
                plan_tier = EXCLUDED.plan_tier,
                access_mode = EXCLUDED.access_mode,
                execution_policy = EXCLUDED.execution_policy,
                sponsored_monthly_cap_requests = EXCLUDED.sponsored_monthly_cap_requests,
                free_play_limit = EXCLUDED.free_play_limit,
                updated_by_principal_id = EXCLUDED.updated_by_principal_id,
                updated_at_ms = EXCLUDED.updated_at_ms,
                updated_at = now()
            "#,
        )
        .bind(&profile.tenant_id)
        .bind(profile.plan.as_str())
        .bind(profile.access_mode.as_str())
        .bind(profile.execution_policy.as_str())
        .bind(profile.sponsored_monthly_cap_requests)
        .bind(profile.free_play_limit)
        .bind(&profile.updated_by_principal_id)
        .bind(profile.updated_at_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to upsert ingress tenant quota profile")?;
        Ok(())
    }

    async fn profile_for_tenant(
        &self,
        tenant_id: &str,
        now_ms: u64,
    ) -> anyhow::Result<TenantQuotaProfile> {
        let row = sqlx::query_as::<_, (String, String, String, i64, i64, String, i64)>(
            r#"
            SELECT
                plan_tier,
                access_mode,
                execution_policy,
                sponsored_monthly_cap_requests,
                free_play_limit,
                updated_by_principal_id,
                updated_at_ms
            FROM ingress_api_tenant_quota_profiles
            WHERE tenant_id = $1
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load ingress tenant quota profile")?;

        let Some((
            plan_raw,
            access_mode_raw,
            execution_policy_raw,
            sponsored_monthly_cap_requests,
            free_play_limit,
            updated_by_principal_id,
            updated_at_ms,
        )) = row
        else {
            return Ok(self.default_profile(tenant_id, now_ms));
        };

        let plan = QuotaPlanTier::parse(&plan_raw).unwrap_or(self.default_plan);
        let access_mode =
            QuotaAccessMode::parse(&access_mode_raw).unwrap_or(self.default_access_mode);
        let execution_policy =
            ExecutionPolicy::parse(&execution_policy_raw).unwrap_or(self.default_execution_policy);
        let sponsored_monthly_cap_requests = if sponsored_monthly_cap_requests > 0 {
            sponsored_monthly_cap_requests
        } else if self.default_sponsored_monthly_cap_requests > 0 {
            self.default_sponsored_monthly_cap_requests
        } else {
            10_000
        };
        let fallback_limit = plan.default_free_play_limit();
        let normalized_limit = if free_play_limit > 0 {
            free_play_limit
        } else if self.default_free_play_limit > 0 {
            self.default_free_play_limit
        } else {
            fallback_limit
        };

        Ok(TenantQuotaProfile {
            tenant_id: tenant_id.to_owned(),
            plan,
            access_mode,
            execution_policy,
            sponsored_monthly_cap_requests,
            free_play_limit: normalized_limit,
            updated_by_principal_id,
            updated_at_ms: updated_at_ms.max(0) as u64,
        })
    }

    async fn count_recent_requests(
        &self,
        tenant_id: &str,
        window_start_ms: u64,
    ) -> anyhow::Result<i64> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)::BIGINT
            FROM execution_core_intents
            WHERE tenant_id = $1
              AND received_at_ms >= $2
              AND COALESCE(intent_json->'metadata'->>'metering.scope', '') <> 'playground'
            "#,
        )
        .bind(tenant_id)
        .bind(window_start_ms as i64)
        .fetch_one(&self.pool)
        .await
        .context("failed to count ingress tenant usage from execution intents")?;
        Ok(count)
    }

    async fn enforce_submit_allowed(
        &self,
        tenant_id: &str,
        now_ms: u64,
        metering_scope: Option<&str>,
    ) -> Result<QuotaCheckResult, ApiError> {
        let profile = self
            .profile_for_tenant(tenant_id, now_ms)
            .await
            .map_err(|error| {
                ApiError::service_unavailable(format!("quota profile unavailable: {error}"))
            })?;
        let playground_metering = metering_scope
            .map(|value| value.trim().eq_ignore_ascii_case("playground"))
            .unwrap_or(false);
        if playground_metering {
            return Ok(QuotaCheckResult {
                profile,
                used_requests: 0,
            });
        }
        let requires_usage_meter = matches!(profile.access_mode, QuotaAccessMode::FreePlay)
            || matches!(profile.execution_policy, ExecutionPolicy::Sponsored);
        if !requires_usage_meter {
            return Ok(QuotaCheckResult {
                profile,
                used_requests: 0,
            });
        }

        let window_start_ms = now_ms.saturating_sub(FREE_PLAY_WINDOW_MS);
        let used_requests = self
            .count_recent_requests(tenant_id, window_start_ms)
            .await
            .map_err(|error| {
                ApiError::service_unavailable(format!("quota usage unavailable: {error}"))
            })?;
        if used_requests >= profile.free_play_limit {
            return Err(ApiError::too_many_requests(format!(
                "free_play quota exceeded for tenant `{tenant_id}` ({used_requests}/{}) in the last 30 days",
                profile.free_play_limit
            )));
        }

        Ok(QuotaCheckResult {
            profile,
            used_requests,
        })
    }
}

#[derive(Debug, Clone)]
struct TenantEnvironmentRecord {
    tenant_id: String,
    environment_id: String,
    name: String,
    environment_kind: String,
    status: String,
    created_by_principal_id: String,
    updated_by_principal_id: String,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Debug, Clone)]
struct TenantEnvironmentUpsertRecord {
    tenant_id: String,
    environment_id: String,
    name: String,
    environment_kind: String,
    status: String,
    created_by_principal_id: String,
    updated_by_principal_id: String,
    now_ms: u64,
}

#[derive(Clone)]
struct IngressEnvironmentStore {
    pool: PgPool,
}

impl IngressEnvironmentStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_tenant_environments (
                tenant_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                name TEXT NOT NULL,
                environment_kind TEXT NOT NULL,
                status TEXT NOT NULL,
                created_by_principal_id TEXT NOT NULL,
                updated_by_principal_id TEXT NOT NULL,
                created_at_ms BIGINT NOT NULL,
                updated_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (tenant_id, environment_id)
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_environments_tenant_updated_idx
            ON ingress_api_tenant_environments(tenant_id, updated_at_ms DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_environments_kind_idx
            ON ingress_api_tenant_environments(tenant_id, environment_kind, status)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress tenant environment schema")?;
        }

        Ok(())
    }

    async fn upsert_environment(
        &self,
        record: &TenantEnvironmentUpsertRecord,
    ) -> anyhow::Result<TenantEnvironmentRecord> {
        let existing_created_at_ms = self
            .load_environment(&record.tenant_id, &record.environment_id)
            .await?
            .map(|value| value.created_at_ms)
            .unwrap_or(record.now_ms);
        let existing_created_by = self
            .load_environment(&record.tenant_id, &record.environment_id)
            .await?
            .map(|value| value.created_by_principal_id)
            .unwrap_or_else(|| record.created_by_principal_id.clone());

        sqlx::query(
            r#"
            INSERT INTO ingress_api_tenant_environments (
                tenant_id,
                environment_id,
                name,
                environment_kind,
                status,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now())
            ON CONFLICT (tenant_id, environment_id)
            DO UPDATE SET
                name = EXCLUDED.name,
                environment_kind = EXCLUDED.environment_kind,
                status = EXCLUDED.status,
                updated_by_principal_id = EXCLUDED.updated_by_principal_id,
                updated_at_ms = EXCLUDED.updated_at_ms,
                updated_at = now()
            "#,
        )
        .bind(&record.tenant_id)
        .bind(&record.environment_id)
        .bind(&record.name)
        .bind(&record.environment_kind)
        .bind(&record.status)
        .bind(&existing_created_by)
        .bind(&record.updated_by_principal_id)
        .bind(existing_created_at_ms as i64)
        .bind(record.now_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to upsert ingress tenant environment")?;

        self.load_environment(&record.tenant_id, &record.environment_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("environment upsert succeeded but row was not readable"))
    }

    async fn load_environment(
        &self,
        tenant_id: &str,
        environment_id: &str,
    ) -> anyhow::Result<Option<TenantEnvironmentRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                tenant_id,
                environment_id,
                name,
                environment_kind,
                status,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms
            FROM ingress_api_tenant_environments
            WHERE tenant_id = $1
              AND environment_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(environment_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load ingress tenant environment")?;

        Ok(row.map(|row| TenantEnvironmentRecord {
            tenant_id: row.get("tenant_id"),
            environment_id: row.get("environment_id"),
            name: row.get("name"),
            environment_kind: row.get("environment_kind"),
            status: row.get("status"),
            created_by_principal_id: row.get("created_by_principal_id"),
            updated_by_principal_id: row.get("updated_by_principal_id"),
            created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
            updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
        }))
    }

    async fn list_environments(
        &self,
        tenant_id: &str,
        include_inactive: bool,
        limit: u32,
    ) -> anyhow::Result<Vec<TenantEnvironmentRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                tenant_id,
                environment_id,
                name,
                environment_kind,
                status,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms
            FROM ingress_api_tenant_environments
            WHERE tenant_id = $1
              AND ($2::boolean OR status = 'active')
            ORDER BY
                CASE environment_kind WHEN 'production' THEN 0 ELSE 1 END,
                updated_at_ms DESC
            LIMIT $3
            "#,
        )
        .bind(tenant_id)
        .bind(include_inactive)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to list ingress tenant environments")?;

        Ok(rows
            .into_iter()
            .map(|row| TenantEnvironmentRecord {
                tenant_id: row.get("tenant_id"),
                environment_id: row.get("environment_id"),
                name: row.get("name"),
                environment_kind: row.get("environment_kind"),
                status: row.get("status"),
                created_by_principal_id: row.get("created_by_principal_id"),
                updated_by_principal_id: row.get("updated_by_principal_id"),
                created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
                updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
struct TenantAgentRecord {
    agent_id: String,
    tenant_id: String,
    environment_id: String,
    name: String,
    runtime_type: String,
    runtime_identity: String,
    status: String,
    trust_tier: String,
    risk_tier: String,
    owner_team: String,
    created_by_principal_id: String,
    updated_by_principal_id: String,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Debug, Clone)]
struct TenantAgentUpsertRecord {
    agent_id: String,
    tenant_id: String,
    environment_id: String,
    name: String,
    runtime_type: String,
    runtime_identity: String,
    status: String,
    trust_tier: String,
    risk_tier: String,
    owner_team: String,
    created_by_principal_id: String,
    updated_by_principal_id: String,
    now_ms: u64,
}

#[derive(Debug, Clone)]
struct ResolvedAgentIdentity {
    agent_id: String,
    environment_id: String,
    environment_kind: String,
    runtime_type: String,
    runtime_identity: String,
    status: String,
    trust_tier: String,
    risk_tier: String,
    owner_team: String,
}

#[derive(Clone)]
struct IngressAgentStore {
    pool: PgPool,
}

impl IngressAgentStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_tenant_agents (
                tenant_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                name TEXT NOT NULL,
                runtime_type TEXT NOT NULL,
                runtime_identity TEXT NOT NULL,
                status TEXT NOT NULL,
                trust_tier TEXT NOT NULL,
                risk_tier TEXT NOT NULL,
                owner_team TEXT NOT NULL,
                created_by_principal_id TEXT NOT NULL,
                updated_by_principal_id TEXT NOT NULL,
                created_at_ms BIGINT NOT NULL,
                updated_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (tenant_id, agent_id),
                CONSTRAINT ingress_api_tenant_agents_environment_fkey
                    FOREIGN KEY (tenant_id, environment_id)
                    REFERENCES ingress_api_tenant_environments(tenant_id, environment_id)
                    ON DELETE RESTRICT
            )
            "#,
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS ingress_api_tenant_agents_runtime_binding_uq
            ON ingress_api_tenant_agents(tenant_id, environment_id, runtime_type, runtime_identity)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_agents_environment_updated_idx
            ON ingress_api_tenant_agents(tenant_id, environment_id, updated_at_ms DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_agents_status_idx
            ON ingress_api_tenant_agents(tenant_id, status, risk_tier, trust_tier)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress tenant agent schema")?;
        }

        Ok(())
    }

    async fn upsert_agent(
        &self,
        record: &TenantAgentUpsertRecord,
    ) -> anyhow::Result<TenantAgentRecord> {
        let existing = self.load_agent(&record.tenant_id, &record.agent_id).await?;
        let created_at_ms = existing
            .as_ref()
            .map(|value| value.created_at_ms)
            .unwrap_or(record.now_ms);
        let created_by_principal_id = existing
            .as_ref()
            .map(|value| value.created_by_principal_id.clone())
            .unwrap_or_else(|| record.created_by_principal_id.clone());

        sqlx::query(
            r#"
            INSERT INTO ingress_api_tenant_agents (
                tenant_id,
                agent_id,
                environment_id,
                name,
                runtime_type,
                runtime_identity,
                status,
                trust_tier,
                risk_tier,
                owner_team,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms,
                updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, now()
            )
            ON CONFLICT (tenant_id, agent_id)
            DO UPDATE SET
                environment_id = EXCLUDED.environment_id,
                name = EXCLUDED.name,
                runtime_type = EXCLUDED.runtime_type,
                runtime_identity = EXCLUDED.runtime_identity,
                status = EXCLUDED.status,
                trust_tier = EXCLUDED.trust_tier,
                risk_tier = EXCLUDED.risk_tier,
                owner_team = EXCLUDED.owner_team,
                updated_by_principal_id = EXCLUDED.updated_by_principal_id,
                updated_at_ms = EXCLUDED.updated_at_ms,
                updated_at = now()
            "#,
        )
        .bind(&record.tenant_id)
        .bind(&record.agent_id)
        .bind(&record.environment_id)
        .bind(&record.name)
        .bind(&record.runtime_type)
        .bind(&record.runtime_identity)
        .bind(&record.status)
        .bind(&record.trust_tier)
        .bind(&record.risk_tier)
        .bind(&record.owner_team)
        .bind(&created_by_principal_id)
        .bind(&record.updated_by_principal_id)
        .bind(created_at_ms as i64)
        .bind(record.now_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to upsert ingress tenant agent")?;

        self.load_agent(&record.tenant_id, &record.agent_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("agent upsert succeeded but row was not readable"))
    }

    async fn load_agent(
        &self,
        tenant_id: &str,
        agent_id: &str,
    ) -> anyhow::Result<Option<TenantAgentRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                agent_id,
                tenant_id,
                environment_id,
                name,
                runtime_type,
                runtime_identity,
                status,
                trust_tier,
                risk_tier,
                owner_team,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms
            FROM ingress_api_tenant_agents
            WHERE tenant_id = $1
              AND agent_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load ingress tenant agent")?;

        Ok(row.map(|row| TenantAgentRecord {
            agent_id: row.get("agent_id"),
            tenant_id: row.get("tenant_id"),
            environment_id: row.get("environment_id"),
            name: row.get("name"),
            runtime_type: row.get("runtime_type"),
            runtime_identity: row.get("runtime_identity"),
            status: row.get("status"),
            trust_tier: row.get("trust_tier"),
            risk_tier: row.get("risk_tier"),
            owner_team: row.get("owner_team"),
            created_by_principal_id: row.get("created_by_principal_id"),
            updated_by_principal_id: row.get("updated_by_principal_id"),
            created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
            updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
        }))
    }

    async fn list_agents(
        &self,
        tenant_id: &str,
        environment_id: Option<&str>,
        include_inactive: bool,
        limit: u32,
    ) -> anyhow::Result<Vec<TenantAgentRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                agent_id,
                tenant_id,
                environment_id,
                name,
                runtime_type,
                runtime_identity,
                status,
                trust_tier,
                risk_tier,
                owner_team,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms
            FROM ingress_api_tenant_agents
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR environment_id = $2)
              AND ($3::boolean OR status = 'active')
            ORDER BY updated_at_ms DESC
            LIMIT $4
            "#,
        )
        .bind(tenant_id)
        .bind(environment_id)
        .bind(include_inactive)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to list ingress tenant agents")?;

        Ok(rows
            .into_iter()
            .map(|row| TenantAgentRecord {
                agent_id: row.get("agent_id"),
                tenant_id: row.get("tenant_id"),
                environment_id: row.get("environment_id"),
                name: row.get("name"),
                runtime_type: row.get("runtime_type"),
                runtime_identity: row.get("runtime_identity"),
                status: row.get("status"),
                trust_tier: row.get("trust_tier"),
                risk_tier: row.get("risk_tier"),
                owner_team: row.get("owner_team"),
                created_by_principal_id: row.get("created_by_principal_id"),
                updated_by_principal_id: row.get("updated_by_principal_id"),
                created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
                updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
            })
            .collect())
    }

    async fn resolve_agent_runtime(
        &self,
        tenant_id: &str,
        environment_id: &str,
        runtime_type: &str,
        runtime_identity: &str,
        requested_agent_id: Option<&str>,
    ) -> anyhow::Result<Option<TenantAgentRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                agent_id,
                tenant_id,
                environment_id,
                name,
                runtime_type,
                runtime_identity,
                status,
                trust_tier,
                risk_tier,
                owner_team,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms
            FROM ingress_api_tenant_agents
            WHERE tenant_id = $1
              AND environment_id = $2
              AND runtime_type = $3
              AND runtime_identity = $4
              AND ($5::text IS NULL OR agent_id = $5)
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(environment_id)
        .bind(runtime_type)
        .bind(runtime_identity)
        .bind(requested_agent_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to resolve ingress tenant agent runtime binding")?;

        Ok(row.map(|row| TenantAgentRecord {
            agent_id: row.get("agent_id"),
            tenant_id: row.get("tenant_id"),
            environment_id: row.get("environment_id"),
            name: row.get("name"),
            runtime_type: row.get("runtime_type"),
            runtime_identity: row.get("runtime_identity"),
            status: row.get("status"),
            trust_tier: row.get("trust_tier"),
            risk_tier: row.get("risk_tier"),
            owner_team: row.get("owner_team"),
            created_by_principal_id: row.get("created_by_principal_id"),
            updated_by_principal_id: row.get("updated_by_principal_id"),
            created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
            updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
        }))
    }
}

#[derive(Debug, Clone)]
struct AgentActionIdempotencyRecord {
    tenant_id: String,
    agent_id: String,
    environment_id: String,
    idempotency_key: String,
    request_fingerprint: String,
    action_request_id: String,
    intent_type: String,
    execution_mode: String,
    adapter_type: String,
    approval_request_id: Option<String>,
    approval_state: Option<String>,
    approval_expires_at_ms: Option<u64>,
    accepted_intent_id: Option<String>,
    accepted_job_id: Option<String>,
    accepted_adapter_id: Option<String>,
    accepted_state: Option<String>,
    route_rule: Option<String>,
}

#[derive(Debug, Clone)]
struct AgentActionIdempotencyReservation {
    tenant_id: String,
    agent_id: String,
    environment_id: String,
    idempotency_key: String,
    request_fingerprint: String,
    action_request_id: String,
    intent_type: String,
    execution_mode: String,
    adapter_type: String,
    now_ms: u64,
}

#[derive(Debug, Clone)]
enum AgentActionIdempotencyDecision {
    ReservedNew,
    ExistingAccepted(AgentActionIdempotencyRecord),
    ExistingApproval(AgentActionIdempotencyRecord),
    ExistingPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ApprovalState {
    Pending,
    Approved,
    Rejected,
    Expired,
    Escalated,
}

impl ApprovalState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
            Self::Escalated => "escalated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApprovalDecisionKind {
    Approve,
    Reject,
    Escalate,
}

impl ApprovalDecisionKind {
    fn event_type(self) -> &'static str {
        match self {
            Self::Approve => "approved",
            Self::Reject => "rejected",
            Self::Escalate => "escalated",
        }
    }
}

#[derive(Debug, Clone)]
struct ApprovalRequestRecord {
    approval_request_id: String,
    tenant_id: String,
    action_request_id: String,
    correlation_id: Option<String>,
    agent_id: String,
    environment_id: String,
    environment_kind: String,
    runtime_type: String,
    runtime_identity: String,
    trust_tier: String,
    risk_tier: String,
    owner_team: String,
    intent_type: String,
    execution_mode: String,
    adapter_type: String,
    normalized_intent_kind: String,
    normalized_payload: Value,
    idempotency_key: String,
    request_fingerprint: String,
    requested_scope: Vec<String>,
    effective_scope: Vec<String>,
    callback_config: Option<AgentActionCallbackConfig>,
    reason: String,
    submitted_by: String,
    policy_bundle_id: Option<String>,
    policy_bundle_version: Option<i64>,
    policy_explanation: String,
    obligations: PolicyRuleObligations,
    matched_rules: Vec<PolicyRuleMatch>,
    decision_trace: Vec<PolicyDecisionTraceEntry>,
    status: ApprovalState,
    required_approvals: u32,
    approvals_received: u32,
    approved_by: Vec<String>,
    expires_at_ms: u64,
    requested_at_ms: u64,
    resolved_at_ms: Option<u64>,
    resolved_by_actor_id: Option<String>,
    resolved_by_actor_source: Option<String>,
    resolution_note: Option<String>,
    slack_delivery_state: Option<String>,
    slack_delivery_error: Option<String>,
    slack_last_attempt_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct ApprovalRequestCreateRecord {
    approval_request_id: String,
    tenant_id: String,
    action_request_id: String,
    correlation_id: Option<String>,
    agent_id: String,
    environment_id: String,
    environment_kind: String,
    runtime_type: String,
    runtime_identity: String,
    trust_tier: String,
    risk_tier: String,
    owner_team: String,
    intent_type: String,
    execution_mode: String,
    adapter_type: String,
    normalized_intent_kind: String,
    normalized_payload: Value,
    idempotency_key: String,
    request_fingerprint: String,
    requested_scope: Vec<String>,
    effective_scope: Vec<String>,
    callback_config: Option<AgentActionCallbackConfig>,
    reason: String,
    submitted_by: String,
    policy_bundle_id: Option<String>,
    policy_bundle_version: Option<i64>,
    policy_explanation: String,
    obligations: PolicyRuleObligations,
    matched_rules: Vec<PolicyRuleMatch>,
    decision_trace: Vec<PolicyDecisionTraceEntry>,
    required_approvals: u32,
    requested_at_ms: u64,
    expires_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
struct ApprovalRequestView {
    approval_request_id: String,
    tenant_id: String,
    action_request_id: String,
    agent_id: String,
    environment_id: String,
    environment_kind: String,
    intent_type: String,
    execution_mode: String,
    adapter_type: String,
    requested_scope: Vec<String>,
    effective_scope: Vec<String>,
    reason: String,
    submitted_by: String,
    status: String,
    required_approvals: u32,
    approvals_received: u32,
    approved_by: Vec<String>,
    policy_bundle_id: Option<String>,
    policy_bundle_version: Option<i64>,
    policy_explanation: String,
    obligations: PolicyRuleObligations,
    matched_rules: Vec<PolicyRuleMatch>,
    decision_trace: Vec<PolicyDecisionTraceEntry>,
    expires_at_ms: u64,
    requested_at_ms: u64,
    resolved_at_ms: Option<u64>,
    resolved_by_actor_id: Option<String>,
    resolved_by_actor_source: Option<String>,
    resolution_note: Option<String>,
    slack_delivery_state: Option<String>,
    slack_delivery_error: Option<String>,
    slack_last_attempt_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ApprovalActionRequest {
    actor_id: Option<String>,
    note: Option<String>,
    grant: Option<ApprovalGrantRequest>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
struct ListApprovalsQuery {
    state: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
struct ApprovalRequestResponse {
    ok: bool,
    approval: ApprovalRequestView,
    execution_mode: String,
    execution_owner: String,
    runtime_authorized: bool,
    execution: Option<SubmitIntentResponse>,
    execution_error: Option<String>,
    grant: Option<CapabilityGrantView>,
    grant_error: Option<String>,
}

#[derive(Debug, Clone)]
struct ApprovalDecisionOutcome {
    approval: ApprovalRequestRecord,
    terminal_reached: bool,
}

#[derive(Debug, Clone)]
struct ApprovalSlackActionPayload {
    user_id: String,
    user_name: Option<String>,
    approval_request_id: String,
    decision: ApprovalDecisionKind,
}

#[derive(Debug, Clone, Deserialize)]
struct ApprovalGrantRequest {
    ttl_seconds: Option<u64>,
    max_uses: Option<u32>,
    amount_ceiling: Option<i64>,
    resource_binding: Option<Value>,
    scope: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CapabilityGrantStatus {
    Active,
    Revoked,
    Expired,
    Exhausted,
}

impl CapabilityGrantStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
            Self::Exhausted => "exhausted",
        }
    }
}

#[derive(Debug, Clone)]
struct CapabilityGrantRecord {
    grant_id: String,
    tenant_id: String,
    environment_id: String,
    agent_id: String,
    action_family: String,
    adapter_type: String,
    granted_scope: Vec<String>,
    resource_binding: Option<Value>,
    amount_ceiling: Option<i64>,
    max_uses: u32,
    uses_consumed: u32,
    status: CapabilityGrantStatus,
    source_action_request_id: String,
    source_approval_request_id: String,
    source_policy_bundle_id: Option<String>,
    source_policy_bundle_version: Option<i64>,
    created_by_actor_id: String,
    created_by_actor_source: String,
    created_at_ms: u64,
    expires_at_ms: u64,
    last_used_at_ms: Option<u64>,
    revoked_at_ms: Option<u64>,
    revoked_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct CapabilityGrantCreateRecord {
    grant_id: String,
    tenant_id: String,
    environment_id: String,
    agent_id: String,
    action_family: String,
    adapter_type: String,
    granted_scope: Vec<String>,
    resource_binding: Option<Value>,
    amount_ceiling: Option<i64>,
    max_uses: u32,
    source_action_request_id: String,
    source_approval_request_id: String,
    source_policy_bundle_id: Option<String>,
    source_policy_bundle_version: Option<i64>,
    created_by_actor_id: String,
    created_by_actor_source: String,
    created_at_ms: u64,
    expires_at_ms: u64,
}

#[derive(Debug, Clone)]
struct CapabilityGrantUseOutcome {
    grant: CapabilityGrantRecord,
    uses_remaining: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ListCapabilityGrantsQuery {
    agent_id: Option<String>,
    environment_id: Option<String>,
    status: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct RevokeCapabilityGrantRequest {
    revoked_by_actor_id: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilityGrantView {
    grant_id: String,
    tenant_id: String,
    environment_id: String,
    agent_id: String,
    action_family: String,
    adapter_type: String,
    granted_scope: Vec<String>,
    resource_binding: Option<Value>,
    amount_ceiling: Option<i64>,
    max_uses: u32,
    uses_consumed: u32,
    uses_remaining: u32,
    status: String,
    source_action_request_id: String,
    source_approval_request_id: String,
    source_policy_bundle_id: Option<String>,
    source_policy_bundle_version: Option<i64>,
    created_by_actor_id: String,
    created_by_actor_source: String,
    created_at_ms: u64,
    expires_at_ms: u64,
    last_used_at_ms: Option<u64>,
    revoked_at_ms: Option<u64>,
    revoked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilityGrantResponse {
    ok: bool,
    grant: CapabilityGrantView,
}

#[derive(Debug, Deserialize)]
struct SlackInteractivityEnvelope {
    payload: String,
}

#[derive(Debug, Deserialize)]
struct SlackInteractiveUser {
    id: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackInteractiveAction {
    action_id: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct SlackInteractiveCallbackPayload {
    user: SlackInteractiveUser,
    actions: Vec<SlackInteractiveAction>,
}

#[derive(Clone)]
struct IngressAgentActionIdempotencyStore {
    pool: PgPool,
}

impl IngressAgentActionIdempotencyStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_agent_action_idempotency (
                tenant_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                idempotency_key TEXT NOT NULL,
                request_fingerprint TEXT NOT NULL,
                action_request_id TEXT NOT NULL,
                intent_type TEXT NOT NULL,
                execution_mode TEXT NULL,
                adapter_type TEXT NOT NULL,
                approval_request_id TEXT NULL,
                approval_state TEXT NULL,
                approval_expires_at_ms BIGINT NULL,
                accepted_intent_id TEXT NULL,
                accepted_job_id TEXT NULL,
                accepted_adapter_id TEXT NULL,
                accepted_state TEXT NULL,
                route_rule TEXT NULL,
                created_at_ms BIGINT NOT NULL,
                last_seen_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (tenant_id, agent_id, environment_id, idempotency_key)
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_agent_action_idempotency_agent_idx
            ON ingress_api_agent_action_idempotency(tenant_id, agent_id, environment_id, last_seen_at_ms DESC)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_agent_action_idempotency_intent_idx
            ON ingress_api_agent_action_idempotency(tenant_id, accepted_intent_id, accepted_job_id)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_agent_action_idempotency_approval_idx
            ON ingress_api_agent_action_idempotency(tenant_id, approval_request_id, approval_state)
            "#,
            r#"
            ALTER TABLE ingress_api_agent_action_idempotency
            ADD COLUMN IF NOT EXISTS approval_request_id TEXT NULL
            "#,
            r#"
            ALTER TABLE ingress_api_agent_action_idempotency
            ADD COLUMN IF NOT EXISTS approval_state TEXT NULL
            "#,
            r#"
            ALTER TABLE ingress_api_agent_action_idempotency
            ADD COLUMN IF NOT EXISTS approval_expires_at_ms BIGINT NULL
            "#,
            r#"
            ALTER TABLE ingress_api_agent_action_idempotency
            ADD COLUMN IF NOT EXISTS execution_mode TEXT NULL
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress agent action idempotency schema")?;
        }

        Ok(())
    }

    async fn reserve_or_load(
        &self,
        reservation: &AgentActionIdempotencyReservation,
    ) -> anyhow::Result<AgentActionIdempotencyDecision> {
        let existing = self
            .load_record(
                &reservation.tenant_id,
                &reservation.agent_id,
                &reservation.environment_id,
                &reservation.idempotency_key,
            )
            .await?;

        let Some(existing) = existing else {
            sqlx::query(
                r#"
                INSERT INTO ingress_api_agent_action_idempotency (
                    tenant_id,
                    agent_id,
                    environment_id,
                    idempotency_key,
                    request_fingerprint,
                    action_request_id,
                    intent_type,
                    execution_mode,
                    adapter_type,
                    approval_request_id,
                    approval_state,
                    approval_expires_at_ms,
                    accepted_intent_id,
                    accepted_job_id,
                    accepted_adapter_id,
                    accepted_state,
                    route_rule,
                    created_at_ms,
                    last_seen_at_ms,
                    updated_at
                )
                VALUES (
                    $1, $2, $3, $4, $5, $6, $7, $8, $9,
                    NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, $10, $10, now()
                )
                ON CONFLICT (tenant_id, agent_id, environment_id, idempotency_key)
                DO NOTHING
                "#,
            )
            .bind(&reservation.tenant_id)
            .bind(&reservation.agent_id)
            .bind(&reservation.environment_id)
            .bind(&reservation.idempotency_key)
            .bind(&reservation.request_fingerprint)
            .bind(&reservation.action_request_id)
            .bind(&reservation.intent_type)
            .bind(&reservation.execution_mode)
            .bind(&reservation.adapter_type)
            .bind(reservation.now_ms as i64)
            .execute(&self.pool)
            .await
            .context("failed to reserve ingress agent action idempotency row")?;

            let reserved = self
                .load_record(
                    &reservation.tenant_id,
                    &reservation.agent_id,
                    &reservation.environment_id,
                    &reservation.idempotency_key,
                )
                .await?
                .ok_or_else(|| anyhow::anyhow!("reserved agent action idempotency row missing"))?;

            if reserved.request_fingerprint != reservation.request_fingerprint {
                anyhow::bail!("agent action idempotency reservation fingerprint mismatch");
            }
            if reserved.accepted_state.is_some()
                && (reserved.accepted_intent_id.is_some() && reserved.accepted_job_id.is_some()
                    || reserved
                        .route_rule
                        .as_deref()
                        .is_some_and(is_runtime_authorization_route_rule))
            {
                return Ok(AgentActionIdempotencyDecision::ExistingAccepted(reserved));
            }
            if reserved.approval_request_id.is_some() {
                return Ok(AgentActionIdempotencyDecision::ExistingApproval(reserved));
            }
            return Ok(AgentActionIdempotencyDecision::ReservedNew);
        };

        sqlx::query(
            r#"
            UPDATE ingress_api_agent_action_idempotency
            SET last_seen_at_ms = $5, updated_at = now()
            WHERE tenant_id = $1
              AND agent_id = $2
              AND environment_id = $3
              AND idempotency_key = $4
            "#,
        )
        .bind(&reservation.tenant_id)
        .bind(&reservation.agent_id)
        .bind(&reservation.environment_id)
        .bind(&reservation.idempotency_key)
        .bind(reservation.now_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to refresh ingress agent action idempotency row")?;

        if existing.request_fingerprint != reservation.request_fingerprint {
            anyhow::bail!(
                "agent action idempotency key is already bound to a different normalized request"
            );
        }

        if existing.accepted_state.is_some()
            && (existing.accepted_intent_id.is_some() && existing.accepted_job_id.is_some()
                || existing
                    .route_rule
                    .as_deref()
                    .is_some_and(is_runtime_authorization_route_rule))
        {
            return Ok(AgentActionIdempotencyDecision::ExistingAccepted(existing));
        }

        if existing.approval_request_id.is_some() {
            return Ok(AgentActionIdempotencyDecision::ExistingApproval(existing));
        }

        Ok(AgentActionIdempotencyDecision::ExistingPending)
    }

    async fn mark_approval_pending(
        &self,
        tenant_id: &str,
        agent_id: &str,
        environment_id: &str,
        idempotency_key: &str,
        approval_request_id: &str,
        approval_state: ApprovalState,
        approval_expires_at_ms: u64,
        now_ms: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE ingress_api_agent_action_idempotency
            SET
                approval_request_id = $5,
                approval_state = $6,
                approval_expires_at_ms = $7,
                last_seen_at_ms = $8,
                updated_at = now()
            WHERE tenant_id = $1
              AND agent_id = $2
              AND environment_id = $3
              AND idempotency_key = $4
            "#,
        )
        .bind(tenant_id)
        .bind(agent_id)
        .bind(environment_id)
        .bind(idempotency_key)
        .bind(approval_request_id)
        .bind(approval_state.as_str())
        .bind(approval_expires_at_ms as i64)
        .bind(now_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to mark ingress agent action idempotency row pending approval")?;
        Ok(())
    }

    async fn mark_approval_state(
        &self,
        tenant_id: &str,
        agent_id: &str,
        environment_id: &str,
        idempotency_key: &str,
        approval_state: ApprovalState,
        now_ms: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE ingress_api_agent_action_idempotency
            SET
                approval_state = $5,
                last_seen_at_ms = $6,
                updated_at = now()
            WHERE tenant_id = $1
              AND agent_id = $2
              AND environment_id = $3
              AND idempotency_key = $4
            "#,
        )
        .bind(tenant_id)
        .bind(agent_id)
        .bind(environment_id)
        .bind(idempotency_key)
        .bind(approval_state.as_str())
        .bind(now_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to update ingress agent action idempotency approval state")?;
        Ok(())
    }

    async fn finalize_success(
        &self,
        tenant_id: &str,
        agent_id: &str,
        environment_id: &str,
        idempotency_key: &str,
        response: &SubmitIntentResponse,
        now_ms: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE ingress_api_agent_action_idempotency
            SET
                accepted_intent_id = $5,
                accepted_job_id = $6,
                accepted_adapter_id = $7,
                accepted_state = $8,
                route_rule = $9,
                last_seen_at_ms = $10,
                updated_at = now()
            WHERE tenant_id = $1
              AND agent_id = $2
              AND environment_id = $3
              AND idempotency_key = $4
            "#,
        )
        .bind(tenant_id)
        .bind(agent_id)
        .bind(environment_id)
        .bind(idempotency_key)
        .bind(&response.intent_id)
        .bind(&response.job_id)
        .bind(&response.adapter_id)
        .bind(&response.state)
        .bind(&response.route_rule)
        .bind(now_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to finalize ingress agent action idempotency row")?;

        Ok(())
    }

    async fn finalize_runtime_authorization(
        &self,
        tenant_id: &str,
        agent_id: &str,
        environment_id: &str,
        idempotency_key: &str,
        execution_mode: AgentExecutionMode,
        adapter_type: &str,
        now_ms: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            UPDATE ingress_api_agent_action_idempotency
            SET
                accepted_adapter_id = $5,
                accepted_state = $6,
                route_rule = $7,
                last_seen_at_ms = $8,
                updated_at = now()
            WHERE tenant_id = $1
              AND agent_id = $2
              AND environment_id = $3
              AND idempotency_key = $4
            "#,
        )
        .bind(tenant_id)
        .bind(agent_id)
        .bind(environment_id)
        .bind(idempotency_key)
        .bind(adapter_type)
        .bind("runtime_authorized")
        .bind(runtime_authorization_route_rule(execution_mode))
        .bind(now_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to finalize runtime authorization idempotency row")?;

        Ok(())
    }

    async fn release_pending(
        &self,
        tenant_id: &str,
        agent_id: &str,
        environment_id: &str,
        idempotency_key: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            DELETE FROM ingress_api_agent_action_idempotency
            WHERE tenant_id = $1
              AND agent_id = $2
              AND environment_id = $3
              AND idempotency_key = $4
              AND accepted_intent_id IS NULL
              AND accepted_job_id IS NULL
            "#,
        )
        .bind(tenant_id)
        .bind(agent_id)
        .bind(environment_id)
        .bind(idempotency_key)
        .execute(&self.pool)
        .await
        .context("failed to release ingress agent action idempotency row")?;
        Ok(())
    }

    async fn load_record(
        &self,
        tenant_id: &str,
        agent_id: &str,
        environment_id: &str,
        idempotency_key: &str,
    ) -> anyhow::Result<Option<AgentActionIdempotencyRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                tenant_id,
                agent_id,
                environment_id,
                idempotency_key,
                request_fingerprint,
                action_request_id,
                intent_type,
                execution_mode,
                adapter_type,
                approval_request_id,
                approval_state,
                approval_expires_at_ms,
                accepted_intent_id,
                accepted_job_id,
                accepted_adapter_id,
                accepted_state,
                route_rule
            FROM ingress_api_agent_action_idempotency
            WHERE tenant_id = $1
              AND agent_id = $2
              AND environment_id = $3
              AND idempotency_key = $4
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(agent_id)
        .bind(environment_id)
        .bind(idempotency_key)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load ingress agent action idempotency row")?;

        Ok(row.map(|row| AgentActionIdempotencyRecord {
            tenant_id: row.get("tenant_id"),
            agent_id: row.get("agent_id"),
            environment_id: row.get("environment_id"),
            idempotency_key: row.get("idempotency_key"),
            request_fingerprint: row.get("request_fingerprint"),
            action_request_id: row.get("action_request_id"),
            intent_type: row.get("intent_type"),
            execution_mode: row.get("execution_mode"),
            adapter_type: row.get("adapter_type"),
            approval_request_id: row.get("approval_request_id"),
            approval_state: row.get("approval_state"),
            approval_expires_at_ms: row
                .get::<Option<i64>, _>("approval_expires_at_ms")
                .map(|value| value.max(0) as u64),
            accepted_intent_id: row.get("accepted_intent_id"),
            accepted_job_id: row.get("accepted_job_id"),
            accepted_adapter_id: row.get("accepted_adapter_id"),
            accepted_state: row.get("accepted_state"),
            route_rule: row.get("route_rule"),
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PolicyLayer {
    PlatformGuardrails,
    AzumsTemplate,
    TenantBundle,
}

impl PolicyLayer {
    fn as_str(self) -> &'static str {
        match self {
            Self::PlatformGuardrails => "platform_guardrails",
            Self::AzumsTemplate => "azums_template",
            Self::TenantBundle => "tenant_bundle",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PolicyEffect {
    Allow,
    Deny,
    RequireApproval,
    AllowReducedScope,
}

impl PolicyEffect {
    fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
            Self::RequireApproval => "require_approval",
            Self::AllowReducedScope => "allow_with_reduced_scope",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PolicyBusinessHoursCondition {
    #[serde(default)]
    days_utc: Vec<String>,
    start_hour_utc: u8,
    end_hour_utc: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PolicyRuleConditions {
    #[serde(default)]
    subjects: Vec<String>,
    #[serde(default)]
    actions: Vec<String>,
    #[serde(default)]
    requested_scopes: Vec<String>,
    #[serde(default)]
    environments: Vec<String>,
    #[serde(default)]
    target_systems: Vec<String>,
    #[serde(default)]
    amount_gte: Option<i64>,
    #[serde(default)]
    amount_lte: Option<i64>,
    #[serde(default)]
    sensitivities: Vec<String>,
    #[serde(default)]
    destination_classes: Vec<String>,
    #[serde(default)]
    business_hours_utc: Option<PolicyBusinessHoursCondition>,
    #[serde(default)]
    trust_tiers: Vec<String>,
    #[serde(default)]
    risk_tiers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PolicyRuleObligations {
    #[serde(default)]
    notify: Vec<String>,
    #[serde(default)]
    dual_approval: bool,
    #[serde(default)]
    reason_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyRuleDefinition {
    rule_id: String,
    description: String,
    effect: PolicyEffect,
    #[serde(default)]
    conditions: PolicyRuleConditions,
    #[serde(default)]
    obligations: PolicyRuleObligations,
    #[serde(default)]
    reduced_scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyTemplateDefinition {
    template_id: String,
    display_name: String,
    description: String,
    rules: Vec<PolicyRuleDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TenantPolicyBundleDocument {
    #[serde(default)]
    template_ids: Vec<String>,
    #[serde(default)]
    rules: Vec<PolicyRuleDefinition>,
}

#[derive(Debug, Clone)]
struct TenantPolicyBundleRecord {
    tenant_id: String,
    bundle_id: String,
    version: i64,
    label: String,
    status: String,
    template_ids: Vec<String>,
    rules: Vec<PolicyRuleDefinition>,
    created_by_principal_id: String,
    published_by_principal_id: Option<String>,
    created_at_ms: u64,
    published_at_ms: Option<u64>,
    rolled_back_from_bundle_id: Option<String>,
    rollback_reason: Option<String>,
}

#[derive(Clone)]
struct IngressPolicyBundleStore {
    pool: PgPool,
}

impl IngressPolicyBundleStore {
    fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_tenant_policy_bundles (
                tenant_id TEXT NOT NULL,
                bundle_id TEXT NOT NULL,
                version BIGINT NOT NULL,
                label TEXT NOT NULL,
                status TEXT NOT NULL,
                template_ids_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                rules_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                created_by_principal_id TEXT NOT NULL,
                published_by_principal_id TEXT NULL,
                created_at_ms BIGINT NOT NULL,
                published_at_ms BIGINT NULL,
                rolled_back_from_bundle_id TEXT NULL,
                rollback_reason TEXT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (tenant_id, bundle_id)
            )
            "#,
            r#"
            CREATE UNIQUE INDEX IF NOT EXISTS ingress_api_tenant_policy_bundles_version_uq
            ON ingress_api_tenant_policy_bundles(tenant_id, version)
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_tenant_policy_bundles_status_idx
            ON ingress_api_tenant_policy_bundles(tenant_id, status, published_at_ms DESC, created_at_ms DESC)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure ingress tenant policy bundle schema")?;
        }

        Ok(())
    }

    async fn create_bundle(
        &self,
        tenant_id: &str,
        bundle_id: &str,
        label: &str,
        document: &TenantPolicyBundleDocument,
        created_by_principal_id: &str,
        now_ms: u64,
    ) -> anyhow::Result<TenantPolicyBundleRecord> {
        let next_version = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(MAX(version), 0) + 1
            FROM ingress_api_tenant_policy_bundles
            WHERE tenant_id = $1
            "#,
        )
        .bind(tenant_id)
        .fetch_one(&self.pool)
        .await
        .context("failed to allocate tenant policy bundle version")?;

        let template_ids_json = serde_json::to_value(&document.template_ids)
            .context("failed to serialize policy template ids")?;
        let rules_json =
            serde_json::to_value(&document.rules).context("failed to serialize policy rules")?;

        sqlx::query(
            r#"
            INSERT INTO ingress_api_tenant_policy_bundles (
                tenant_id,
                bundle_id,
                version,
                label,
                status,
                template_ids_json,
                rules_json,
                created_by_principal_id,
                published_by_principal_id,
                created_at_ms,
                published_at_ms,
                rolled_back_from_bundle_id,
                rollback_reason,
                updated_at
            )
            VALUES ($1, $2, $3, $4, 'draft', $5, $6, $7, NULL, $8, NULL, NULL, NULL, now())
            "#,
        )
        .bind(tenant_id)
        .bind(bundle_id)
        .bind(next_version)
        .bind(label)
        .bind(template_ids_json)
        .bind(rules_json)
        .bind(created_by_principal_id)
        .bind(now_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to create tenant policy bundle")?;

        self.load_bundle(tenant_id, bundle_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("created tenant policy bundle row missing"))
    }

    async fn load_bundle(
        &self,
        tenant_id: &str,
        bundle_id: &str,
    ) -> anyhow::Result<Option<TenantPolicyBundleRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                tenant_id,
                bundle_id,
                version,
                label,
                status,
                template_ids_json,
                rules_json,
                created_by_principal_id,
                published_by_principal_id,
                created_at_ms,
                published_at_ms,
                rolled_back_from_bundle_id,
                rollback_reason
            FROM ingress_api_tenant_policy_bundles
            WHERE tenant_id = $1
              AND bundle_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(bundle_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load tenant policy bundle")?;

        Ok(row
            .map(map_policy_bundle_record)
            .transpose()
            .context("failed to decode tenant policy bundle row")?)
    }

    async fn list_bundles(
        &self,
        tenant_id: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<TenantPolicyBundleRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                tenant_id,
                bundle_id,
                version,
                label,
                status,
                template_ids_json,
                rules_json,
                created_by_principal_id,
                published_by_principal_id,
                created_at_ms,
                published_at_ms,
                rolled_back_from_bundle_id,
                rollback_reason
            FROM ingress_api_tenant_policy_bundles
            WHERE tenant_id = $1
            ORDER BY version DESC
            LIMIT $2
            "#,
        )
        .bind(tenant_id)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .context("failed to list tenant policy bundles")?;

        rows.into_iter()
            .map(map_policy_bundle_record)
            .collect::<anyhow::Result<Vec<_>>>()
    }

    async fn load_published_bundle(
        &self,
        tenant_id: &str,
    ) -> anyhow::Result<Option<TenantPolicyBundleRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                tenant_id,
                bundle_id,
                version,
                label,
                status,
                template_ids_json,
                rules_json,
                created_by_principal_id,
                published_by_principal_id,
                created_at_ms,
                published_at_ms,
                rolled_back_from_bundle_id,
                rollback_reason
            FROM ingress_api_tenant_policy_bundles
            WHERE tenant_id = $1
              AND status = 'published'
            ORDER BY published_at_ms DESC NULLS LAST, version DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load published tenant policy bundle")?;

        Ok(row
            .map(map_policy_bundle_record)
            .transpose()
            .context("failed to decode published tenant policy bundle row")?)
    }

    async fn publish_bundle(
        &self,
        tenant_id: &str,
        bundle_id: &str,
        published_by_principal_id: &str,
        now_ms: u64,
        rolled_back_from_bundle_id: Option<&str>,
        rollback_reason: Option<&str>,
    ) -> anyhow::Result<TenantPolicyBundleRecord> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin tenant policy publish tx")?;

        sqlx::query(
            r#"
            UPDATE ingress_api_tenant_policy_bundles
            SET status = 'superseded', updated_at = now()
            WHERE tenant_id = $1
              AND status = 'published'
              AND bundle_id <> $2
            "#,
        )
        .bind(tenant_id)
        .bind(bundle_id)
        .execute(&mut *tx)
        .await
        .context("failed to supersede previously published tenant policy bundle")?;

        let updated = sqlx::query(
            r#"
            UPDATE ingress_api_tenant_policy_bundles
            SET
                status = 'published',
                published_by_principal_id = $3,
                published_at_ms = $4,
                rolled_back_from_bundle_id = $5,
                rollback_reason = $6,
                updated_at = now()
            WHERE tenant_id = $1
              AND bundle_id = $2
            "#,
        )
        .bind(tenant_id)
        .bind(bundle_id)
        .bind(published_by_principal_id)
        .bind(now_ms as i64)
        .bind(rolled_back_from_bundle_id)
        .bind(rollback_reason)
        .execute(&mut *tx)
        .await
        .context("failed to publish tenant policy bundle")?;

        if updated.rows_affected() == 0 {
            anyhow::bail!("tenant policy bundle `{bundle_id}` not found for tenant `{tenant_id}`");
        }

        tx.commit()
            .await
            .context("failed to commit tenant policy publish tx")?;

        self.load_bundle(tenant_id, bundle_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("published tenant policy bundle row missing"))
    }
}

fn map_policy_bundle_record(
    row: sqlx::postgres::PgRow,
) -> anyhow::Result<TenantPolicyBundleRecord> {
    let template_ids =
        serde_json::from_value::<Vec<String>>(row.get::<Value, _>("template_ids_json"))
            .context("failed to decode policy template_ids_json")?;
    let rules =
        serde_json::from_value::<Vec<PolicyRuleDefinition>>(row.get::<Value, _>("rules_json"))
            .context("failed to decode policy rules_json")?;

    Ok(TenantPolicyBundleRecord {
        tenant_id: row.get("tenant_id"),
        bundle_id: row.get("bundle_id"),
        version: row.get("version"),
        label: row.get("label"),
        status: row.get("status"),
        template_ids,
        rules,
        created_by_principal_id: row.get("created_by_principal_id"),
        published_by_principal_id: row.get("published_by_principal_id"),
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
        published_at_ms: row
            .get::<Option<i64>, _>("published_at_ms")
            .map(|value| value.max(0) as u64),
        rolled_back_from_bundle_id: row.get("rolled_back_from_bundle_id"),
        rollback_reason: row.get("rollback_reason"),
    })
}

#[derive(Debug, Clone)]
struct PolicyEvaluationContext {
    tenant_id: String,
    agent_id: String,
    owner_team: String,
    trust_tier: String,
    risk_tier: String,
    environment_id: String,
    environment_kind: String,
    action: String,
    adapter_type: String,
    amount: Option<i64>,
    sensitivity: String,
    destination_class: String,
    requested_scope: Vec<String>,
    reason: String,
    submitted_by: String,
    evaluated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyRuleMatch {
    layer: String,
    source_id: String,
    rule_id: String,
    effect: String,
    description: String,
    obligations: PolicyRuleObligations,
    reduced_scope: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyDecisionTraceEntry {
    stage: String,
    layer: String,
    source_id: String,
    rule_id: Option<String>,
    effect: Option<String>,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct PolicyDecisionExplanation {
    final_effect: String,
    effective_scope: Vec<String>,
    obligations: PolicyRuleObligations,
    matched_rules: Vec<PolicyRuleMatch>,
    decision_trace: Vec<PolicyDecisionTraceEntry>,
    published_bundle_id: Option<String>,
    published_bundle_version: Option<i64>,
    explanation: String,
}

fn normalize_policy_rule(rule: &mut PolicyRuleDefinition) -> Result<(), ApiError> {
    rule.rule_id = normalize_registry_key(&rule.rule_id, "rule_id", 128)?;
    rule.description = normalize_required_name(&rule.description, "description", 256)?;
    rule.conditions.subjects = rule
        .conditions
        .subjects
        .iter()
        .map(|value| normalize_registry_key(value, "conditions.subjects", 128))
        .collect::<Result<Vec<_>, _>>()?;
    rule.conditions.actions = rule
        .conditions
        .actions
        .iter()
        .map(|value| normalize_registry_key(value, "conditions.actions", 64))
        .collect::<Result<Vec<_>, _>>()?;
    rule.conditions.requested_scopes = rule
        .conditions
        .requested_scopes
        .iter()
        .map(|value| normalize_registry_key(value, "conditions.requested_scopes", 64))
        .collect::<Result<Vec<_>, _>>()?;
    rule.conditions.environments = rule
        .conditions
        .environments
        .iter()
        .map(|value| normalize_registry_key(value, "conditions.environments", 64))
        .collect::<Result<Vec<_>, _>>()?;
    rule.conditions.target_systems = rule
        .conditions
        .target_systems
        .iter()
        .map(|value| normalize_registry_key(value, "conditions.target_systems", 64))
        .collect::<Result<Vec<_>, _>>()?;
    rule.conditions.sensitivities = rule
        .conditions
        .sensitivities
        .iter()
        .map(|value| normalize_registry_key(value, "conditions.sensitivities", 64))
        .collect::<Result<Vec<_>, _>>()?;
    rule.conditions.destination_classes = rule
        .conditions
        .destination_classes
        .iter()
        .map(|value| normalize_registry_key(value, "conditions.destination_classes", 64))
        .collect::<Result<Vec<_>, _>>()?;
    rule.conditions.trust_tiers = rule
        .conditions
        .trust_tiers
        .iter()
        .map(|value| normalize_registry_key(value, "conditions.trust_tiers", 64))
        .collect::<Result<Vec<_>, _>>()?;
    rule.conditions.risk_tiers = rule
        .conditions
        .risk_tiers
        .iter()
        .map(|value| normalize_registry_key(value, "conditions.risk_tiers", 64))
        .collect::<Result<Vec<_>, _>>()?;
    rule.obligations.notify = rule
        .obligations
        .notify
        .iter()
        .map(|value| normalize_required_name(value, "obligations.notify", 128))
        .collect::<Result<Vec<_>, _>>()?;
    rule.reduced_scope = rule
        .reduced_scope
        .iter()
        .map(|value| normalize_registry_key(value, "reduced_scope", 64))
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(hours) = rule.conditions.business_hours_utc.as_mut() {
        hours.days_utc = hours
            .days_utc
            .iter()
            .map(|value| normalize_registry_key(value, "business_hours_utc.days_utc", 16))
            .collect::<Result<Vec<_>, _>>()?;
        if hours.start_hour_utc > 23 || hours.end_hour_utc > 23 {
            return Err(ApiError::bad_request(
                "business_hours_utc hours must be in 0..=23",
            ));
        }
    }

    if matches!(rule.effect, PolicyEffect::AllowReducedScope) && rule.reduced_scope.is_empty() {
        return Err(ApiError::bad_request(
            "allow_reduced_scope rules must include reduced_scope",
        ));
    }

    Ok(())
}

fn normalize_policy_bundle_document(
    mut document: TenantPolicyBundleDocument,
) -> Result<TenantPolicyBundleDocument, ApiError> {
    document.template_ids = document
        .template_ids
        .iter()
        .map(|value| normalize_registry_key(value, "template_ids", 128))
        .collect::<Result<Vec<_>, _>>()?;
    for rule in &mut document.rules {
        normalize_policy_rule(rule)?;
    }
    Ok(document)
}

fn platform_guardrail_rules() -> Vec<PolicyRuleDefinition> {
    vec![
        PolicyRuleDefinition {
            rule_id: "guardrail_reason_required".to_owned(),
            description: "Every agent action must include a business reason.".to_owned(),
            effect: PolicyEffect::Allow,
            conditions: PolicyRuleConditions::default(),
            obligations: PolicyRuleObligations {
                reason_required: true,
                ..PolicyRuleObligations::default()
            },
            reduced_scope: Vec::new(),
        },
        PolicyRuleDefinition {
            rule_id: "guardrail_no_playground_in_production".to_owned(),
            description: "Playground scope is never allowed in production.".to_owned(),
            effect: PolicyEffect::Deny,
            conditions: PolicyRuleConditions {
                requested_scopes: vec!["playground".to_owned()],
                environments: vec!["production".to_owned()],
                ..PolicyRuleConditions::default()
            },
            obligations: PolicyRuleObligations::default(),
            reduced_scope: Vec::new(),
        },
        PolicyRuleDefinition {
            rule_id: "guardrail_low_trust_financial_prod".to_owned(),
            description: "Low-trust agents cannot execute financial actions in production."
                .to_owned(),
            effect: PolicyEffect::Deny,
            conditions: PolicyRuleConditions {
                actions: vec!["refund".to_owned(), "transfer".to_owned()],
                environments: vec!["production".to_owned()],
                trust_tiers: vec!["low".to_owned()],
                ..PolicyRuleConditions::default()
            },
            obligations: PolicyRuleObligations {
                notify: vec!["security".to_owned()],
                ..PolicyRuleObligations::default()
            },
            reduced_scope: Vec::new(),
        },
        PolicyRuleDefinition {
            rule_id: "guardrail_critical_risk_requires_approval".to_owned(),
            description: "Critical-risk agents require approval for any financial action."
                .to_owned(),
            effect: PolicyEffect::RequireApproval,
            conditions: PolicyRuleConditions {
                actions: vec!["refund".to_owned(), "transfer".to_owned()],
                risk_tiers: vec!["critical".to_owned()],
                ..PolicyRuleConditions::default()
            },
            obligations: PolicyRuleObligations {
                notify: vec!["security".to_owned(), "ops".to_owned()],
                dual_approval: true,
                reason_required: true,
            },
            reduced_scope: Vec::new(),
        },
    ]
}

fn azums_policy_templates() -> Vec<PolicyTemplateDefinition> {
    vec![
        PolicyTemplateDefinition {
            template_id: "azums.finance.reviewed.v1".to_owned(),
            display_name: "Finance Reviewed".to_owned(),
            description: "Conservative financial actions with approval in production.".to_owned(),
            rules: vec![
                PolicyRuleDefinition {
                    rule_id: "finance_reviewed_allow_nonprod_transfers".to_owned(),
                    description: "Allow transfers and refunds outside production.".to_owned(),
                    effect: PolicyEffect::Allow,
                    conditions: PolicyRuleConditions {
                        actions: vec!["transfer".to_owned(), "refund".to_owned()],
                        environments: vec![
                            "sandbox".to_owned(),
                            "staging".to_owned(),
                            "development".to_owned(),
                        ],
                        ..PolicyRuleConditions::default()
                    },
                    obligations: PolicyRuleObligations {
                        notify: vec!["ops".to_owned()],
                        reason_required: true,
                        ..PolicyRuleObligations::default()
                    },
                    reduced_scope: Vec::new(),
                },
                PolicyRuleDefinition {
                    rule_id: "finance_reviewed_approval_prod_transfers".to_owned(),
                    description: "Require approval for production transfers and refunds."
                        .to_owned(),
                    effect: PolicyEffect::RequireApproval,
                    conditions: PolicyRuleConditions {
                        actions: vec!["transfer".to_owned(), "refund".to_owned()],
                        environments: vec!["production".to_owned()],
                        ..PolicyRuleConditions::default()
                    },
                    obligations: PolicyRuleObligations {
                        notify: vec!["finance".to_owned()],
                        dual_approval: true,
                        reason_required: true,
                    },
                    reduced_scope: Vec::new(),
                },
            ],
        },
        PolicyTemplateDefinition {
            template_id: "azums.billing.invoice.v1".to_owned(),
            display_name: "Billing Invoice".to_owned(),
            description: "Allow invoice generation with audit obligations.".to_owned(),
            rules: vec![PolicyRuleDefinition {
                rule_id: "billing_invoice_allow".to_owned(),
                description: "Allow invoice generation across environments.".to_owned(),
                effect: PolicyEffect::Allow,
                conditions: PolicyRuleConditions {
                    actions: vec!["generate_invoice".to_owned()],
                    ..PolicyRuleConditions::default()
                },
                obligations: PolicyRuleObligations {
                    notify: vec!["billing".to_owned()],
                    reason_required: true,
                    ..PolicyRuleObligations::default()
                },
                reduced_scope: Vec::new(),
            }],
        },
        PolicyTemplateDefinition {
            template_id: "azums.devnet.playground.v1".to_owned(),
            display_name: "Devnet Playground".to_owned(),
            description:
                "Allow transfer actions only with playground-reduced scope outside production."
                    .to_owned(),
            rules: vec![PolicyRuleDefinition {
                rule_id: "devnet_playground_reduce_scope".to_owned(),
                description: "Reduce non-production Solana transfers to playground scope."
                    .to_owned(),
                effect: PolicyEffect::AllowReducedScope,
                conditions: PolicyRuleConditions {
                    actions: vec!["transfer".to_owned()],
                    target_systems: vec![
                        "adapter_solana".to_owned(),
                        "solana".to_owned(),
                        "solana_adapter".to_owned(),
                    ],
                    environments: vec![
                        "sandbox".to_owned(),
                        "staging".to_owned(),
                        "development".to_owned(),
                    ],
                    ..PolicyRuleConditions::default()
                },
                obligations: PolicyRuleObligations {
                    notify: vec!["ops".to_owned()],
                    reason_required: true,
                    ..PolicyRuleObligations::default()
                },
                reduced_scope: vec!["playground".to_owned()],
            }],
        },
    ]
}

fn find_policy_template(template_id: &str) -> Option<PolicyTemplateDefinition> {
    azums_policy_templates()
        .into_iter()
        .find(|template| template.template_id == template_id)
}

fn derive_action_amount(intent_type: AgentIntentType, payload: &Value) -> Option<i64> {
    match intent_type {
        AgentIntentType::Refund | AgentIntentType::Transfer | AgentIntentType::GenerateInvoice => {
            payload.get("amount").and_then(Value::as_i64)
        }
    }
}

fn derive_action_sensitivity(intent_type: AgentIntentType, payload: &Value) -> String {
    payload
        .get("sensitivity")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| match intent_type {
            AgentIntentType::Transfer | AgentIntentType::Refund => "high".to_owned(),
            AgentIntentType::GenerateInvoice => "medium".to_owned(),
        })
}

fn derive_destination_class(intent_type: AgentIntentType, payload: &Value) -> String {
    payload
        .get("destination_class")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| match intent_type {
            AgentIntentType::Transfer => "external_wallet".to_owned(),
            AgentIntentType::Refund => "payment_processor".to_owned(),
            AgentIntentType::GenerateInvoice => "accounts_receivable".to_owned(),
        })
}

fn weekday_matches(raw: &str, weekday: Weekday) -> bool {
    matches!(
        (raw, weekday),
        ("mon" | "monday", Weekday::Mon)
            | ("tue" | "tuesday", Weekday::Tue)
            | ("wed" | "wednesday", Weekday::Wed)
            | ("thu" | "thursday", Weekday::Thu)
            | ("fri" | "friday", Weekday::Fri)
            | ("sat" | "saturday", Weekday::Sat)
            | ("sun" | "sunday", Weekday::Sun)
    )
}

fn business_hours_match(condition: &PolicyBusinessHoursCondition, now_ms: u64) -> bool {
    let Some(timestamp) = chrono::DateTime::<Utc>::from_timestamp_millis(now_ms as i64) else {
        return false;
    };
    if !condition.days_utc.is_empty()
        && !condition
            .days_utc
            .iter()
            .any(|raw| weekday_matches(raw.as_str(), timestamp.weekday()))
    {
        return false;
    }
    let hour = timestamp.hour() as u8;
    if condition.start_hour_utc <= condition.end_hour_utc {
        hour >= condition.start_hour_utc && hour <= condition.end_hour_utc
    } else {
        hour >= condition.start_hour_utc || hour <= condition.end_hour_utc
    }
}

fn matches_any(candidates: &[String], actuals: &[&str]) -> bool {
    if candidates.is_empty() {
        return true;
    }
    candidates.iter().any(|candidate| {
        candidate == "*"
            || actuals
                .iter()
                .any(|actual| candidate.eq_ignore_ascii_case(actual))
    })
}

fn policy_rule_matches(rule: &PolicyRuleDefinition, context: &PolicyEvaluationContext) -> bool {
    let subject_values = vec![
        "*".to_owned(),
        context.agent_id.clone(),
        format!("tenant:{}", context.tenant_id),
        format!("agent:{}", context.agent_id),
        format!("team:{}", context.owner_team),
        format!("trust:{}", context.trust_tier),
        format!("risk:{}", context.risk_tier),
        format!("submitted_by:{}", context.submitted_by),
    ];
    let subject_refs = subject_values
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();

    if !matches_any(&rule.conditions.subjects, &subject_refs) {
        return false;
    }
    if !matches_any(&rule.conditions.actions, &[context.action.as_str()]) {
        return false;
    }
    if !rule.conditions.requested_scopes.is_empty()
        && !context
            .requested_scope
            .iter()
            .any(|scope| matches_any(&rule.conditions.requested_scopes, &[scope.as_str()]))
    {
        return false;
    }
    if !matches_any(
        &rule.conditions.environments,
        &[
            context.environment_id.as_str(),
            context.environment_kind.as_str(),
        ],
    ) {
        return false;
    }
    if !matches_any(
        &rule.conditions.target_systems,
        &[context.adapter_type.as_str()],
    ) {
        return false;
    }
    if !matches_any(
        &rule.conditions.sensitivities,
        &[context.sensitivity.as_str()],
    ) {
        return false;
    }
    if !matches_any(
        &rule.conditions.destination_classes,
        &[context.destination_class.as_str()],
    ) {
        return false;
    }
    if !matches_any(&rule.conditions.trust_tiers, &[context.trust_tier.as_str()]) {
        return false;
    }
    if !matches_any(&rule.conditions.risk_tiers, &[context.risk_tier.as_str()]) {
        return false;
    }
    if let Some(amount_gte) = rule.conditions.amount_gte {
        if context.amount.unwrap_or_default() < amount_gte {
            return false;
        }
    }
    if let Some(amount_lte) = rule.conditions.amount_lte {
        if context.amount.unwrap_or_default() > amount_lte {
            return false;
        }
    }
    if let Some(hours) = rule.conditions.business_hours_utc.as_ref() {
        if !business_hours_match(hours, context.evaluated_at_ms) {
            return false;
        }
    }
    true
}

fn merge_obligations(
    left: &PolicyRuleObligations,
    right: &PolicyRuleObligations,
) -> PolicyRuleObligations {
    let mut notify = left.notify.clone();
    for value in &right.notify {
        if !notify
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(value))
        {
            notify.push(value.clone());
        }
    }
    PolicyRuleObligations {
        notify,
        dual_approval: left.dual_approval || right.dual_approval,
        reason_required: left.reason_required || right.reason_required,
    }
}

fn reduce_scope(current: &[String], reduction: &[String]) -> Vec<String> {
    if reduction.is_empty() {
        return current.to_vec();
    }
    let reduced = current
        .iter()
        .filter(|scope| {
            reduction
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(scope))
        })
        .cloned()
        .collect::<Vec<_>>();
    if reduced.is_empty() {
        reduction.to_vec()
    } else {
        reduced
    }
}

fn evaluate_policy_layers(
    context: &PolicyEvaluationContext,
    published_bundle: Option<&TenantPolicyBundleRecord>,
) -> Result<PolicyDecisionExplanation, ApiError> {
    let mut matched_rules = Vec::new();
    let mut decision_trace = Vec::new();
    let mut obligations = PolicyRuleObligations::default();
    let mut effective_scope = context.requested_scope.clone();
    let mut deny_match = None::<PolicyRuleMatch>;
    let mut approval_match = None::<PolicyRuleMatch>;
    let mut allow_match_count = 0usize;
    let mut reduced_scope_applied = false;

    let mut evaluate_rule_set =
        |layer: PolicyLayer, source_id: &str, rules: &[PolicyRuleDefinition]| {
            for rule in rules {
                if !policy_rule_matches(rule, context) {
                    continue;
                }
                obligations = merge_obligations(&obligations, &rule.obligations);
                if matches!(rule.effect, PolicyEffect::AllowReducedScope) {
                    effective_scope = reduce_scope(&effective_scope, &rule.reduced_scope);
                    reduced_scope_applied = true;
                }
                let matched = PolicyRuleMatch {
                    layer: layer.as_str().to_owned(),
                    source_id: source_id.to_owned(),
                    rule_id: rule.rule_id.clone(),
                    effect: rule.effect.as_str().to_owned(),
                    description: rule.description.clone(),
                    obligations: rule.obligations.clone(),
                    reduced_scope: rule.reduced_scope.clone(),
                };
                decision_trace.push(PolicyDecisionTraceEntry {
                    stage: "rule_matched".to_owned(),
                    layer: layer.as_str().to_owned(),
                    source_id: source_id.to_owned(),
                    rule_id: Some(rule.rule_id.clone()),
                    effect: Some(rule.effect.as_str().to_owned()),
                    message: rule.description.clone(),
                });
                match rule.effect {
                    PolicyEffect::Deny => {
                        let _ = deny_match.get_or_insert(matched.clone());
                    }
                    PolicyEffect::RequireApproval => {
                        let _ = approval_match.get_or_insert(matched.clone());
                    }
                    PolicyEffect::Allow | PolicyEffect::AllowReducedScope => {
                        allow_match_count += 1;
                    }
                }
                matched_rules.push(matched);
            }
        };

    let guardrails = platform_guardrail_rules();
    evaluate_rule_set(
        PolicyLayer::PlatformGuardrails,
        "azums.platform_guardrails.v1",
        &guardrails,
    );

    if let Some(bundle) = published_bundle {
        for template_id in &bundle.template_ids {
            let template = find_policy_template(template_id).ok_or_else(|| {
                ApiError::service_unavailable(format!(
                    "published tenant policy bundle references unknown template `{template_id}`"
                ))
            })?;
            evaluate_rule_set(
                PolicyLayer::AzumsTemplate,
                &template.template_id,
                &template.rules,
            );
        }
        evaluate_rule_set(PolicyLayer::TenantBundle, &bundle.bundle_id, &bundle.rules);
    }

    if obligations.reason_required && context.reason.trim().is_empty() {
        return Ok(PolicyDecisionExplanation {
            final_effect: PolicyEffect::Deny.as_str().to_owned(),
            effective_scope: context.requested_scope.clone(),
            obligations,
            matched_rules,
            decision_trace: {
                decision_trace.push(PolicyDecisionTraceEntry {
                    stage: "final_decision".to_owned(),
                    layer: "policy_engine".to_owned(),
                    source_id: published_bundle
                        .map(|bundle| bundle.bundle_id.clone())
                        .unwrap_or_else(|| "published_bundle".to_owned()),
                    rule_id: None,
                    effect: Some(PolicyEffect::Deny.as_str().to_owned()),
                    message: "policy requires a reason but none was supplied".to_owned(),
                });
                decision_trace
            },
            published_bundle_id: published_bundle.map(|bundle| bundle.bundle_id.clone()),
            published_bundle_version: published_bundle.map(|bundle| bundle.version),
            explanation: "policy requires a reason but none was supplied".to_owned(),
        });
    }

    let (final_effect, explanation) = if let Some(rule) = deny_match {
        (
            PolicyEffect::Deny,
            format!(
                "denied by {} rule `{}` from `{}`",
                rule.layer, rule.rule_id, rule.source_id
            ),
        )
    } else if let Some(rule) = approval_match {
        (
            PolicyEffect::RequireApproval,
            format!(
                "requires approval by {} rule `{}` from `{}`",
                rule.layer, rule.rule_id, rule.source_id
            ),
        )
    } else if reduced_scope_applied && allow_match_count > 0 {
        (
            PolicyEffect::AllowReducedScope,
            "allowed with reduced scope after matched policy rules".to_owned(),
        )
    } else if allow_match_count > 0 {
        (
            PolicyEffect::Allow,
            "allowed by matched policy rules".to_owned(),
        )
    } else {
        (
            PolicyEffect::Deny,
            "denied because no published tenant policy rule allowed this agent action".to_owned(),
        )
    };

    decision_trace.push(PolicyDecisionTraceEntry {
        stage: "final_decision".to_owned(),
        layer: "policy_engine".to_owned(),
        source_id: published_bundle
            .map(|bundle| bundle.bundle_id.clone())
            .unwrap_or_else(|| "published_bundle".to_owned()),
        rule_id: None,
        effect: Some(final_effect.as_str().to_owned()),
        message: explanation.clone(),
    });

    Ok(PolicyDecisionExplanation {
        final_effect: final_effect.as_str().to_owned(),
        effective_scope,
        obligations,
        matched_rules,
        decision_trace,
        published_bundle_id: published_bundle.map(|bundle| bundle.bundle_id.clone()),
        published_bundle_version: published_bundle.map(|bundle| bundle.version),
        explanation,
    })
}

#[derive(Debug, Clone)]
struct IngressIntakeAuditRecord {
    audit_id: Uuid,
    request_id: String,
    tenant_id: String,
    channel: String,
    endpoint: String,
    method: String,
    principal_id: Option<String>,
    submitter_kind: Option<String>,
    auth_scheme: Option<String>,
    intent_kind: Option<String>,
    correlation_id: Option<String>,
    idempotency_key: Option<String>,
    idempotency_decision: Option<String>,
    validation_result: String,
    rejection_reason: Option<String>,
    error_status: Option<i32>,
    error_message: Option<String>,
    accepted_intent_id: Option<String>,
    accepted_job_id: Option<String>,
    details_json: Value,
    created_at_ms: u64,
}

impl IngressIntakeAuditRecord {
    fn new(
        request_id: String,
        channel: IngressChannel,
        endpoint: &str,
        method: &str,
        created_at_ms: u64,
    ) -> Self {
        Self {
            audit_id: Uuid::new_v4(),
            request_id,
            tenant_id: "__unknown__".to_owned(),
            channel: channel.as_str().to_owned(),
            endpoint: endpoint.to_owned(),
            method: method.to_owned(),
            principal_id: None,
            submitter_kind: None,
            auth_scheme: None,
            intent_kind: None,
            correlation_id: None,
            idempotency_key: None,
            idempotency_decision: None,
            validation_result: "rejected".to_owned(),
            rejection_reason: Some("intake_incomplete".to_owned()),
            error_status: None,
            error_message: None,
            accepted_intent_id: None,
            accepted_job_id: None,
            details_json: json!({}),
            created_at_ms,
        }
    }

    fn set_tenant(&mut self, tenant_id: &str) {
        self.tenant_id = tenant_id.to_owned();
    }

    fn set_submitter(&mut self, submitter: &SubmitterIdentity) {
        self.principal_id = Some(submitter.principal_id.clone());
        self.submitter_kind = Some(submitter.kind.as_str().to_owned());
        self.auth_scheme = Some(submitter.auth_scheme.clone());
    }

    fn mark_accepted(&mut self, response: &SubmitIntentResponse) {
        self.validation_result = "accepted".to_owned();
        self.rejection_reason = None;
        self.error_status = None;
        self.error_message = None;
        self.accepted_intent_id = Some(response.intent_id.clone());
        self.accepted_job_id = Some(response.job_id.clone());
        self.idempotency_decision = Some(idempotency_decision_for_route_rule(
            &response.route_rule,
            self.idempotency_key.is_some(),
        ));
        self.details_json = json!({
            "route_rule": response.route_rule,
            "adapter_id": response.adapter_id,
            "state": response.state,
        });
    }

    fn mark_rejected(&mut self, reason: impl Into<String>, err: &ApiError, details_json: Value) {
        self.validation_result = "rejected".to_owned();
        self.rejection_reason = Some(reason.into());
        self.error_status = Some(err.status.as_u16() as i32);
        self.error_message = Some(err.message.clone());
        if self.idempotency_decision.is_none()
            && err.status == StatusCode::CONFLICT
            && err.message.to_ascii_lowercase().contains("idempotency")
        {
            self.idempotency_decision = Some("conflict".to_owned());
        }
        self.details_json = details_json;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SubmitterKind {
    ApiKeyHolder,
    AgentRuntime,
    SignedWebhookSender,
    InternalService,
    WalletBackend,
}

impl SubmitterKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ApiKeyHolder => "api_key_holder",
            Self::AgentRuntime => "agent_runtime",
            Self::SignedWebhookSender => "signed_webhook_sender",
            Self::InternalService => "internal_service",
            Self::WalletBackend => "wallet_backend",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IngressChannel {
    Api,
    Webhook,
}

impl IngressChannel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Webhook => "webhook",
        }
    }
}

#[derive(Debug, Clone)]
struct SubmitterIdentity {
    principal_id: String,
    kind: SubmitterKind,
    auth_scheme: String,
    resolved_agent: Option<ResolvedAgentIdentity>,
}

#[derive(Clone)]
struct IngressAuthConfig {
    global_bearer_token: Option<String>,
    tenant_bearer_tokens: HashMap<String, String>,
    global_api_key: Option<String>,
    tenant_api_keys: HashMap<String, String>,
    webhook_signature_secrets: HashMap<String, String>,
    principal_submitter_kinds: HashMap<String, SubmitterKind>,
    principal_tenants: HashMap<String, HashSet<String>>,
    api_allowed_submitters: HashSet<SubmitterKind>,
    webhook_allowed_submitters: HashSet<SubmitterKind>,
    require_principal_id: bool,
    require_submitter_kind: bool,
    require_principal_submitter_binding: bool,
    require_principal_tenant_binding: bool,
    require_api_key_for_api_key_holder: bool,
}

impl IngressAuthConfig {
    fn from_env() -> Result<Self, anyhow::Error> {
        let cfg = Self {
            global_bearer_token: env_var_opt("INGRESS_BEARER_TOKEN"),
            tenant_bearer_tokens: parse_kv_map(env_var_opt("INGRESS_TENANT_TOKENS").as_deref()),
            global_api_key: env_var_opt("INGRESS_API_KEY"),
            tenant_api_keys: parse_kv_map(env_var_opt("INGRESS_TENANT_API_KEYS").as_deref()),
            webhook_signature_secrets: parse_kv_map(
                env_var_opt("INGRESS_WEBHOOK_SIGNATURE_SECRETS").as_deref(),
            ),
            principal_submitter_kinds: parse_principal_submitter_kind_map(
                env_var_opt("INGRESS_PRINCIPAL_SUBMITTER_BINDINGS").as_deref(),
            ),
            principal_tenants: parse_principal_tenant_map(
                env_var_opt("INGRESS_PRINCIPAL_TENANT_BINDINGS").as_deref(),
            ),
            api_allowed_submitters: parse_submitter_set(Some(
                env_or(
                    "INGRESS_API_ALLOWED_SUBMITTERS",
                    "api_key_holder,agent_runtime,internal_service,wallet_backend",
                )
                .as_str(),
            ))?,
            webhook_allowed_submitters: parse_submitter_set(Some(
                env_or(
                    "INGRESS_WEBHOOK_ALLOWED_SUBMITTERS",
                    "signed_webhook_sender,internal_service",
                )
                .as_str(),
            ))?,
            require_principal_id: env_bool("INGRESS_REQUIRE_PRINCIPAL_ID", true),
            require_submitter_kind: env_bool("INGRESS_REQUIRE_SUBMITTER_KIND", true),
            require_principal_submitter_binding: env_bool(
                "INGRESS_REQUIRE_PRINCIPAL_SUBMITTER_BINDING",
                true,
            ),
            require_principal_tenant_binding: env_bool(
                "INGRESS_REQUIRE_PRINCIPAL_TENANT_BINDING",
                true,
            ),
            require_api_key_for_api_key_holder: env_bool(
                "INGRESS_REQUIRE_API_KEY_FOR_API_KEY_HOLDER",
                true,
            ),
        };

        Ok(cfg)
    }

    fn authenticate_submitter(
        &self,
        tenant_id: &str,
        headers: &HeaderMap,
        channel: IngressChannel,
    ) -> Result<SubmitterIdentity, ApiError> {
        let kind_from_header = match header_opt(headers, "x-submitter-kind") {
            Some(value) => Some(parse_submitter_kind(&value).ok_or_else(|| {
                ApiError::bad_request(format!("invalid x-submitter-kind `{value}`"))
            })?),
            None => None,
        };
        let mut kind = match kind_from_header {
            Some(kind) => kind,
            None if self.require_submitter_kind => {
                return Err(ApiError::unauthorized("missing x-submitter-kind"));
            }
            None => SubmitterKind::InternalService,
        };

        let principal_id = match header_opt(headers, "x-principal-id") {
            Some(value) => value,
            None if matches!(kind, SubmitterKind::ApiKeyHolder)
                && self.require_api_key_for_api_key_holder =>
            {
                format!("api_key:{tenant_id}")
            }
            None if self.require_principal_id => {
                return Err(ApiError::unauthorized("missing x-principal-id"));
            }
            None => "anonymous".to_owned(),
        };

        let enforce_principal_bindings = !(matches!(kind, SubmitterKind::ApiKeyHolder)
            && self.require_api_key_for_api_key_holder);

        if enforce_principal_bindings {
            match resolve_principal_binding(&self.principal_submitter_kinds, principal_id.as_str())
            {
                Some(bound_kind) if *bound_kind == kind => {}
                Some(bound_kind) => {
                    return Err(ApiError::forbidden(format!(
                        "principal `{}` submitter kind mismatch: expected `{}` got `{}`",
                        principal_id,
                        bound_kind.as_str(),
                        kind.as_str()
                    )));
                }
                None if self.require_principal_submitter_binding => {
                    return Err(ApiError::forbidden(format!(
                        "principal `{}` is not mapped to any submitter kind",
                        principal_id
                    )));
                }
                None if kind_from_header.is_none() => {
                    kind = SubmitterKind::InternalService;
                }
                None => {}
            }

            match resolve_principal_binding(&self.principal_tenants, principal_id.as_str()) {
                Some(tenants) if principal_tenant_allowed(tenants, tenant_id) => {}
                Some(_) => {
                    return Err(ApiError::forbidden(format!(
                        "principal `{}` is not allowed for tenant `{tenant_id}`",
                        principal_id
                    )));
                }
                None if self.require_principal_tenant_binding => {
                    return Err(ApiError::forbidden(format!(
                        "principal `{}` is not bound to any tenant",
                        principal_id
                    )));
                }
                None => {}
            }
        }

        let allowed = match channel {
            IngressChannel::Api => &self.api_allowed_submitters,
            IngressChannel::Webhook => &self.webhook_allowed_submitters,
        };
        if !allowed.contains(&kind) {
            return Err(ApiError::forbidden(format!(
                "submitter kind `{}` is not allowed for {} channel",
                kind.as_str(),
                channel.as_str()
            )));
        }

        let auth_scheme = if matches!(kind, SubmitterKind::ApiKeyHolder)
            && self.require_api_key_for_api_key_holder
        {
            self.require_api_key_header(headers)?;
            "api_key".to_owned()
        } else {
            self.authenticate_bearer(tenant_id, headers)?;
            "bearer".to_owned()
        };

        Ok(SubmitterIdentity {
            principal_id,
            kind,
            auth_scheme,
            resolved_agent: None,
        })
    }

    fn authenticate_bearer(&self, tenant_id: &str, headers: &HeaderMap) -> Result<(), ApiError> {
        let token = extract_bearer_token(headers)
            .ok_or_else(|| ApiError::unauthorized("missing bearer token"))?;

        if let Some(expected) = self.tenant_bearer_tokens.get(tenant_id) {
            if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
                return Ok(());
            }
            if env_bool("INGRESS_DEBUG_AUTH_HEADER_ENABLED", false) {
                tracing::warn!(
                    tenant_id = tenant_id,
                    authorization = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    host = headers
                        .get("host")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    x_forwarded_for = headers
                        .get("x-forwarded-for")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    x_forwarded_proto = headers
                        .get("x-forwarded-proto")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    "ingress bearer auth mismatch against tenant token"
                );
            }
            return Err(ApiError::unauthorized(debug_invalid_bearer_message(
                headers,
            )));
        }

        if let Some(expected) = self.global_bearer_token.as_ref() {
            if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
                return Ok(());
            }
            if env_bool("INGRESS_DEBUG_AUTH_HEADER_ENABLED", false) {
                tracing::warn!(
                    tenant_id = tenant_id,
                    authorization = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    host = headers
                        .get("host")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    x_forwarded_for = headers
                        .get("x-forwarded-for")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    x_forwarded_proto = headers
                        .get("x-forwarded-proto")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("<missing>"),
                    "ingress bearer auth mismatch against global token"
                );
            }
            return Err(ApiError::unauthorized(debug_invalid_bearer_message(
                headers,
            )));
        }

        Err(ApiError::unauthorized(
            "no ingress bearer token configured for tenant",
        ))
    }

    fn require_api_key_header(&self, headers: &HeaderMap) -> Result<String, ApiError> {
        header_opt(headers, "x-api-key").ok_or_else(|| ApiError::unauthorized("missing x-api-key"))
    }

    fn static_api_key_matches(&self, tenant_id: &str, api_key: &str) -> bool {
        if let Some(expected) = self.tenant_api_keys.get(tenant_id) {
            return constant_time_eq(api_key.as_bytes(), expected.as_bytes());
        }

        if let Some(expected) = self.global_api_key.as_ref() {
            return constant_time_eq(api_key.as_bytes(), expected.as_bytes());
        }

        false
    }

    fn verify_webhook_signature(
        &self,
        tenant_id: &str,
        body: &[u8],
        headers: &HeaderMap,
        required: bool,
    ) -> Result<(), ApiError> {
        let Some(secret) = self.webhook_signature_secrets.get(tenant_id) else {
            if required {
                return Err(ApiError::unauthorized(
                    "no webhook signing secret configured for tenant",
                ));
            }
            return Ok(());
        };

        let signature_header = headers
            .get("x-webhook-signature")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::unauthorized("missing x-webhook-signature"))?;

        let signature_hex = signature_header
            .strip_prefix("v1=")
            .unwrap_or(signature_header);
        let expected = compute_webhook_signature(secret, body)?;
        if constant_time_eq(signature_hex.as_bytes(), expected.as_bytes()) {
            return Ok(());
        }

        Err(ApiError::unauthorized("invalid webhook signature"))
    }
}

fn debug_invalid_bearer_message(headers: &HeaderMap) -> String {
    if !env_bool("INGRESS_DEBUG_AUTH_HEADER_ENABLED", false) {
        return "invalid bearer token".to_owned();
    }

    format!(
        "invalid bearer token; authorization={}; host={}; x-forwarded-for={}; x-forwarded-proto={}",
        headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<missing>"),
        headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<missing>"),
        headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<missing>"),
        headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("<missing>")
    )
}

#[derive(Clone)]
struct IngressAuthorizer {
    allowed_adapters: HashSet<String>,
}

impl Authorizer for IngressAuthorizer {
    fn can_route_adapter(&self, _tenant_id: &TenantId, adapter_id: &AdapterId) -> bool {
        self.allowed_adapters.contains(adapter_id.as_str())
    }

    fn can_replay(&self, _principal: &OperatorPrincipal, _tenant_id: &TenantId) -> bool {
        false
    }

    fn can_trigger_manual_action(
        &self,
        _principal: &OperatorPrincipal,
        _tenant_id: &TenantId,
    ) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
struct IngressSchemaRegistry {
    strict: bool,
    by_intent: HashMap<String, BuiltinIntentSchema>,
}

impl IngressSchemaRegistry {
    fn from_env(routes: &[RouteMapping]) -> Result<Self, anyhow::Error> {
        let strict = env_bool("INGRESS_REQUIRE_SCHEMA_FOR_ALL_ROUTES", true);
        let raw = env_or(
            "INGRESS_INTENT_SCHEMAS",
            "solana.transfer.v1=solana.transfer.v1;solana.broadcast.v1=solana.broadcast.v1",
        );
        let mappings = parse_intent_schema_map(&raw)?;
        Self::from_mappings(mappings, strict, routes)
    }

    fn from_mappings(
        mappings: Vec<(String, String)>,
        strict: bool,
        routes: &[RouteMapping],
    ) -> Result<Self, anyhow::Error> {
        let mut by_intent = HashMap::new();

        for (intent_kind, schema_id) in mappings {
            let schema = BuiltinIntentSchema::from_id(&schema_id).ok_or_else(|| {
                anyhow::anyhow!("unsupported schema id `{schema_id}` for intent `{intent_kind}`")
            })?;
            by_intent.insert(intent_kind, schema);
        }

        if strict {
            for route in routes {
                if !by_intent.contains_key(route.intent_kind.as_str()) {
                    anyhow::bail!(
                        "missing schema mapping for routed intent `{}` (set INGRESS_INTENT_SCHEMAS)",
                        route.intent_kind
                    );
                }
            }
        }

        Ok(Self { strict, by_intent })
    }

    fn validate_intent_payload(&self, intent_kind: &str, payload: &Value) -> Result<(), ApiError> {
        let Some(schema) = self.by_intent.get(intent_kind).copied() else {
            if self.strict {
                return Err(ApiError::bad_request(format!(
                    "missing payload schema for intent `{intent_kind}`"
                )));
            }
            return Ok(());
        };

        schema.validate_payload(payload).map_err(|err| {
            ApiError::bad_request(format!(
                "payload does not match intent schema for `{intent_kind}`: {err}"
            ))
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum BuiltinIntentSchema {
    SolanaTransferV1,
    SolanaBroadcastV1,
}

impl BuiltinIntentSchema {
    fn from_id(id: &str) -> Option<Self> {
        match id.trim() {
            "solana.transfer.v1" => Some(Self::SolanaTransferV1),
            "solana.broadcast.v1" => Some(Self::SolanaBroadcastV1),
            _ => None,
        }
    }

    fn validate_payload(self, payload: &Value) -> Result<(), String> {
        match self {
            Self::SolanaTransferV1 => validate_solana_intent_payload(payload, false),
            Self::SolanaBroadcastV1 => validate_solana_intent_payload(payload, false),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SolanaIntentPayloadSchema {
    #[serde(default)]
    intent_id: Option<String>,
    #[serde(rename = "type", default)]
    intent_type: Option<String>,
    #[serde(default, alias = "to")]
    to_addr: Option<String>,
    amount: i64,
    #[serde(default, alias = "signed_tx_b64", alias = "signed_tx")]
    signed_tx_base64: Option<String>,
    #[serde(default)]
    skip_preflight: Option<bool>,
    #[serde(default)]
    cu_limit: Option<i64>,
    #[serde(default, alias = "cu_price")]
    cu_price_micro_lamports: Option<i64>,
    #[serde(default, alias = "blockhash")]
    blockhash_used: Option<String>,
    #[serde(default)]
    simulation_outcome: Option<String>,
    #[serde(default, alias = "provider")]
    provider_used: Option<String>,
    #[serde(default)]
    rpc_url: Option<String>,
    #[serde(default, alias = "payer", alias = "from_addr", alias = "from")]
    fee_payer: Option<String>,
    #[serde(default)]
    payer_source: Option<String>,
}

fn validate_solana_intent_payload(payload: &Value, require_signed_tx: bool) -> Result<(), String> {
    let parsed: SolanaIntentPayloadSchema =
        serde_json::from_value(payload.clone()).map_err(|err| err.to_string())?;

    ensure_non_empty_optional("intent_id", parsed.intent_id.as_deref())?;
    ensure_non_empty_optional("type", parsed.intent_type.as_deref())?;
    ensure_non_empty_optional("to_addr", parsed.to_addr.as_deref())?;
    ensure_non_empty_optional("signed_tx_base64", parsed.signed_tx_base64.as_deref())?;
    ensure_non_empty_optional("blockhash_used", parsed.blockhash_used.as_deref())?;
    ensure_non_empty_optional("simulation_outcome", parsed.simulation_outcome.as_deref())?;
    ensure_non_empty_optional("provider_used", parsed.provider_used.as_deref())?;
    ensure_non_empty_optional("rpc_url", parsed.rpc_url.as_deref())?;
    ensure_non_empty_optional("fee_payer", parsed.fee_payer.as_deref())?;
    ensure_non_empty_optional("payer_source", parsed.payer_source.as_deref())?;

    let to_addr = parsed
        .to_addr
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "payload field `to_addr` (or alias `to`) is required".to_owned())?;
    if to_addr.is_empty() {
        return Err("payload field `to_addr` must not be empty".to_owned());
    }

    if parsed.amount <= 0 {
        return Err("payload field `amount` must be a positive integer".to_owned());
    }

    if let Some(cu_limit) = parsed.cu_limit {
        if cu_limit <= 0 {
            return Err("payload field `cu_limit` must be positive when provided".to_owned());
        }
        if i32::try_from(cu_limit).is_err() {
            return Err("payload field `cu_limit` must fit into 32-bit integer".to_owned());
        }
    }

    if let Some(cu_price_micro_lamports) = parsed.cu_price_micro_lamports {
        if cu_price_micro_lamports < 0 {
            return Err(
                "payload field `cu_price_micro_lamports` must be >= 0 when provided".to_owned(),
            );
        }
    }

    if require_signed_tx && parsed.signed_tx_base64.is_none() {
        return Err(
            "payload field `signed_tx_base64` is required for this intent schema".to_owned(),
        );
    }

    let _ = parsed.skip_preflight;
    Ok(())
}

fn intent_kind_requires_signed_tx_policy(intent_kind: &str) -> bool {
    matches!(intent_kind, "solana.transfer.v1" | "solana.broadcast.v1")
}

fn signed_tx_payload_present(payload: &Value) -> bool {
    payload
        .get("signed_tx_base64")
        .or_else(|| payload.get("signed_tx_b64"))
        .or_else(|| payload.get("signed_tx"))
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

fn extract_fee_payer_hint(payload: &Value) -> Option<String> {
    [
        "fee_payer",
        "payer",
        "from_addr",
        "from",
        "payer_address",
        "fee_payer_address",
    ]
    .iter()
    .find_map(|key| {
        payload
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn resolve_signing_mode(policy: ExecutionPolicy, signed_tx_present: bool) -> &'static str {
    if signed_tx_present {
        return "customer_signed";
    }
    match policy {
        ExecutionPolicy::CustomerSigned => "customer_signed",
        ExecutionPolicy::CustomerManagedSigner => "customer_managed_signer",
        ExecutionPolicy::Sponsored => "platform_sponsored",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentExecutionMode {
    ModeBScopedRuntime,
    ModeCProtectedExecution,
}

impl AgentExecutionMode {
    fn parse(raw: &str) -> Result<Self, ApiError> {
        match normalize_registry_key(raw, "execution_mode", 64)?.as_str() {
            "mode_b_scoped_runtime" | "mode-b-scoped-runtime" | "mode_b" | "mode-b" => {
                Ok(Self::ModeBScopedRuntime)
            }
            "mode_c_protected_execution"
            | "mode-c-protected-execution"
            | "mode_c"
            | "mode-c" => Ok(Self::ModeCProtectedExecution),
            other => Err(ApiError::bad_request(format!(
                "unsupported execution_mode `{other}`"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ModeBScopedRuntime => "mode_b_scoped_runtime",
            Self::ModeCProtectedExecution => "mode_c_protected_execution",
        }
    }

    fn owner_label(self) -> &'static str {
        match self {
            Self::ModeBScopedRuntime => "customer_runtime",
            Self::ModeCProtectedExecution => "azums_protected_execution",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentIntentType {
    Refund,
    Transfer,
    GenerateInvoice,
}

impl AgentIntentType {
    fn parse(raw: &str) -> Result<Self, ApiError> {
        match normalize_registry_key(raw, "intent_type", 64)?.as_str() {
            "refund" => Ok(Self::Refund),
            "transfer" => Ok(Self::Transfer),
            "generate_invoice" | "generate-invoice" => Ok(Self::GenerateInvoice),
            other => Err(ApiError::bad_request(format!(
                "unsupported intent_type `{other}`"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Refund => "refund",
            Self::Transfer => "transfer",
            Self::GenerateInvoice => "generate_invoice",
        }
    }

    fn default_execution_mode(self) -> AgentExecutionMode {
        match self {
            Self::Refund | Self::Transfer => AgentExecutionMode::ModeCProtectedExecution,
            Self::GenerateInvoice => AgentExecutionMode::ModeBScopedRuntime,
        }
    }

    fn supports_execution_mode(self, mode: AgentExecutionMode) -> bool {
        match self {
            Self::Refund | Self::Transfer => {
                matches!(mode, AgentExecutionMode::ModeCProtectedExecution)
            }
            Self::GenerateInvoice => {
                matches!(mode, AgentExecutionMode::ModeBScopedRuntime)
            }
        }
    }
}

fn resolve_agent_execution_mode(
    intent_type: AgentIntentType,
    raw: Option<&str>,
) -> Result<AgentExecutionMode, ApiError> {
    let mode = match raw.map(str::trim).filter(|value| !value.is_empty()) {
        Some(raw) => AgentExecutionMode::parse(raw)?,
        None => intent_type.default_execution_mode(),
    };
    if !intent_type.supports_execution_mode(mode) {
        return Err(ApiError::bad_request(format!(
            "intent_type `{}` does not support execution_mode `{}`",
            intent_type.as_str(),
            mode.as_str()
        )));
    }
    Ok(mode)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentTransferPayloadSchema {
    #[serde(default, alias = "to")]
    to_addr: Option<String>,
    amount: i64,
    #[serde(default)]
    asset: Option<String>,
    #[serde(default, alias = "from")]
    from_addr: Option<String>,
    #[serde(default)]
    memo: Option<String>,
    #[serde(default, alias = "signed_tx", alias = "signed_tx_b64")]
    signed_tx_base64: Option<String>,
    #[serde(default)]
    skip_preflight: Option<bool>,
    #[serde(default)]
    cu_limit: Option<i64>,
    #[serde(default, alias = "cu_price")]
    cu_price_micro_lamports: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentRefundPayloadSchema {
    payment_reference: String,
    amount: i64,
    currency: String,
    #[serde(default)]
    destination_reference: Option<String>,
    #[serde(default)]
    reason_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentGenerateInvoicePayloadSchema {
    customer_reference: String,
    amount: i64,
    currency: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    due_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct NormalizedAgentActionRequest {
    action_request_id: String,
    tenant_id: String,
    agent_id: String,
    environment_id: String,
    intent_type: AgentIntentType,
    execution_mode: AgentExecutionMode,
    adapter_type: String,
    normalized_payload: Value,
    idempotency_key: String,
    requested_scope: Vec<String>,
    reason: String,
    callback_config: Option<AgentActionCallbackConfig>,
    submitted_by: String,
    normalized_intent_kind: String,
}

fn validate_agent_transfer_payload(payload: &Value) -> Result<(), ApiError> {
    let parsed: AgentTransferPayloadSchema = serde_json::from_value(payload.clone())
        .map_err(|err| ApiError::bad_request(format!("transfer payload schema invalid: {err}")))?;

    ensure_non_empty_optional("to_addr", parsed.to_addr.as_deref())
        .map_err(ApiError::bad_request)?;
    ensure_non_empty_optional("asset", parsed.asset.as_deref()).map_err(ApiError::bad_request)?;
    ensure_non_empty_optional("from_addr", parsed.from_addr.as_deref())
        .map_err(ApiError::bad_request)?;
    ensure_non_empty_optional("memo", parsed.memo.as_deref()).map_err(ApiError::bad_request)?;
    ensure_non_empty_optional("signed_tx_base64", parsed.signed_tx_base64.as_deref())
        .map_err(ApiError::bad_request)?;

    let to_addr = parsed
        .to_addr
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("transfer payload field `to_addr` is required"))?;
    if to_addr.is_empty() {
        return Err(ApiError::bad_request(
            "transfer payload field `to_addr` must not be empty",
        ));
    }
    if parsed.amount <= 0 {
        return Err(ApiError::bad_request(
            "transfer payload field `amount` must be a positive integer",
        ));
    }
    if let Some(cu_limit) = parsed.cu_limit {
        if cu_limit <= 0 {
            return Err(ApiError::bad_request(
                "transfer payload field `cu_limit` must be positive when provided",
            ));
        }
    }
    if let Some(cu_price_micro_lamports) = parsed.cu_price_micro_lamports {
        if cu_price_micro_lamports < 0 {
            return Err(ApiError::bad_request(
                "transfer payload field `cu_price_micro_lamports` must be >= 0 when provided",
            ));
        }
    }
    let _ = parsed.skip_preflight;
    Ok(())
}

fn validate_agent_refund_payload(payload: &Value) -> Result<(), ApiError> {
    let parsed: AgentRefundPayloadSchema = serde_json::from_value(payload.clone())
        .map_err(|err| ApiError::bad_request(format!("refund payload schema invalid: {err}")))?;
    if parsed.payment_reference.trim().is_empty() {
        return Err(ApiError::bad_request(
            "refund payload field `payment_reference` is required",
        ));
    }
    if parsed.amount <= 0 {
        return Err(ApiError::bad_request(
            "refund payload field `amount` must be a positive integer",
        ));
    }
    if parsed.currency.trim().is_empty() {
        return Err(ApiError::bad_request(
            "refund payload field `currency` is required",
        ));
    }
    if let Some(value) = parsed.destination_reference.as_deref() {
        if value.trim().is_empty() {
            return Err(ApiError::bad_request(
                "refund payload field `destination_reference` must not be empty",
            ));
        }
    }
    if let Some(value) = parsed.reason_code.as_deref() {
        if value.trim().is_empty() {
            return Err(ApiError::bad_request(
                "refund payload field `reason_code` must not be empty",
            ));
        }
    }
    Ok(())
}

fn validate_agent_generate_invoice_payload(payload: &Value) -> Result<(), ApiError> {
    let parsed: AgentGenerateInvoicePayloadSchema = serde_json::from_value(payload.clone())
        .map_err(|err| {
            ApiError::bad_request(format!("generate_invoice payload schema invalid: {err}"))
        })?;
    if parsed.customer_reference.trim().is_empty() {
        return Err(ApiError::bad_request(
            "generate_invoice payload field `customer_reference` is required",
        ));
    }
    if parsed.amount <= 0 {
        return Err(ApiError::bad_request(
            "generate_invoice payload field `amount` must be a positive integer",
        ));
    }
    if parsed.currency.trim().is_empty() {
        return Err(ApiError::bad_request(
            "generate_invoice payload field `currency` is required",
        ));
    }
    if let Some(value) = parsed.description.as_deref() {
        if value.trim().is_empty() {
            return Err(ApiError::bad_request(
                "generate_invoice payload field `description` must not be empty",
            ));
        }
    }
    let _ = parsed.due_at_ms;
    Ok(())
}

fn normalize_requested_scope(values: &[String]) -> Result<Vec<String>, ApiError> {
    if values.is_empty() {
        return Err(ApiError::bad_request("requested_scope is required"));
    }
    let mut scopes = values
        .iter()
        .map(|value| normalize_registry_key(value, "requested_scope", 64))
        .collect::<Result<Vec<_>, _>>()?;
    scopes.sort();
    scopes.dedup();
    Ok(scopes)
}

fn canonicalize_json_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(canonicalize_json_value)
                .collect::<Vec<_>>(),
        ),
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let mut out = Map::new();
            for key in keys {
                if let Some(value) = map.get(&key) {
                    out.insert(key, canonicalize_json_value(value));
                }
            }
            Value::Object(out)
        }
        _ => value.clone(),
    }
}

fn agent_action_request_fingerprint(request: &NormalizedAgentActionRequest) -> String {
    let callback_config = request
        .callback_config
        .as_ref()
        .and_then(|value| serde_json::to_value(value).ok())
        .unwrap_or(Value::Null);
    let envelope = Value::Object(Map::from_iter([
        (
            "tenant_id".to_owned(),
            Value::String(request.tenant_id.clone()),
        ),
        (
            "agent_id".to_owned(),
            Value::String(request.agent_id.clone()),
        ),
        (
            "environment_id".to_owned(),
            Value::String(request.environment_id.clone()),
        ),
        (
            "intent_type".to_owned(),
            Value::String(request.intent_type.as_str().to_owned()),
        ),
        (
            "execution_mode".to_owned(),
            Value::String(request.execution_mode.as_str().to_owned()),
        ),
        (
            "adapter_type".to_owned(),
            Value::String(request.adapter_type.clone()),
        ),
        (
            "payload".to_owned(),
            canonicalize_json_value(&request.normalized_payload),
        ),
        (
            "requested_scope".to_owned(),
            serde_json::to_value(&request.requested_scope).unwrap_or(Value::Null),
        ),
        ("reason".to_owned(), Value::String(request.reason.clone())),
        (
            "callback_config".to_owned(),
            canonicalize_json_value(&callback_config),
        ),
        (
            "submitted_by".to_owned(),
            Value::String(request.submitted_by.clone()),
        ),
    ]));
    let canonical = serde_json::to_vec(&canonicalize_json_value(&envelope)).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical);
    hex::encode(hasher.finalize())
}

fn normalize_agent_action_request(
    payload: SubmitAgentActionRequest,
    tenant_id: &str,
    resolved_agent: &ResolvedAgentIdentity,
) -> Result<NormalizedAgentActionRequest, ApiError> {
    let action_payload = payload.payload.clone();
    let action_request_id =
        normalize_registry_key(&payload.action_request_id, "action_request_id", 128)?;
    let body_tenant_id = normalize_registry_key(&payload.tenant_id, "tenant_id", 128)?;
    if body_tenant_id != tenant_id {
        return Err(ApiError::bad_request(
            "tenant_id body field does not match authenticated tenant",
        ));
    }
    let body_agent_id = normalize_registry_key(&payload.agent_id, "agent_id", 64)?;
    if body_agent_id != resolved_agent.agent_id {
        return Err(ApiError::bad_request(
            "agent_id body field does not match resolved agent identity",
        ));
    }
    let body_environment_id =
        normalize_registry_key(&payload.environment_id, "environment_id", 64)?;
    if body_environment_id != resolved_agent.environment_id {
        return Err(ApiError::bad_request(
            "environment_id body field does not match resolved environment identity",
        ));
    }
    let intent_type = AgentIntentType::parse(&payload.intent_type)?;
    let execution_mode =
        resolve_agent_execution_mode(intent_type, payload.execution_mode.as_deref())?;
    let adapter_type = normalize_registry_key(&payload.adapter_type, "adapter_type", 64)?;
    let idempotency_key = normalize_registry_key(&payload.idempotency_key, "idempotency_key", 128)?;
    let requested_scope = normalize_requested_scope(&payload.requested_scope)?;
    let reason = normalize_required_name(&payload.reason, "reason", 512)?;
    let submitted_by = normalize_required_name(&payload.submitted_by, "submitted_by", 128)?;
    if let Some(callback_config) = payload.callback_config.as_ref() {
        if let Some(url) = callback_config.url.as_deref() {
            normalize_required_name(url, "callback_config.url", 512)?;
        }
        if let Some(secret_ref) = callback_config.signing_secret_ref.as_deref() {
            normalize_required_name(secret_ref, "callback_config.signing_secret_ref", 128)?;
        }
    }

    match intent_type {
        AgentIntentType::Refund => validate_agent_refund_payload(&action_payload)?,
        AgentIntentType::Transfer => validate_agent_transfer_payload(&action_payload)?,
        AgentIntentType::GenerateInvoice => {
            validate_agent_generate_invoice_payload(&action_payload)?
        }
    }

    let (normalized_intent_kind, normalized_payload) = match (
        intent_type,
        execution_mode,
        adapter_type.as_str(),
    ) {
        (
            AgentIntentType::Transfer,
            AgentExecutionMode::ModeCProtectedExecution,
            "adapter_solana" | "solana" | "solana_adapter",
        ) => {
            let mut object = match action_payload {
                Value::Object(map) => map,
                _ => {
                    return Err(ApiError::bad_request(
                        "transfer payload must be a JSON object",
                    ))
                }
            };
            object
                .entry("intent_id".to_owned())
                .or_insert_with(|| Value::String(action_request_id.clone()));
            object
                .entry("type".to_owned())
                .or_insert_with(|| Value::String("transfer".to_owned()));
            ("solana.transfer.v1".to_owned(), Value::Object(object))
        }
        (AgentIntentType::GenerateInvoice, AgentExecutionMode::ModeBScopedRuntime, _) => (
            "runtime.generate_invoice.v1".to_owned(),
            action_payload,
        ),
        (AgentIntentType::Refund, AgentExecutionMode::ModeCProtectedExecution, adapter_type) => {
            return Err(ApiError::bad_request(format!(
                "intent_type `refund` requires protected execution, but no normalized intent mapping exists yet for adapter_type `{adapter_type}`"
            )))
        }
        (intent_type, execution_mode, adapter_type) => {
            return Err(ApiError::bad_request(format!(
                "no normalized intent mapping exists for intent_type `{}` with execution_mode `{}` and adapter_type `{adapter_type}`",
                intent_type.as_str(),
                execution_mode.as_str()
            )))
        }
    };

    Ok(NormalizedAgentActionRequest {
        action_request_id,
        tenant_id: tenant_id.to_owned(),
        agent_id: resolved_agent.agent_id.clone(),
        environment_id: resolved_agent.environment_id.clone(),
        intent_type,
        execution_mode,
        adapter_type,
        normalized_payload,
        idempotency_key,
        requested_scope,
        reason,
        callback_config: payload.callback_config,
        submitted_by,
        normalized_intent_kind,
    })
}

fn normalize_agent_action_for_policy_simulation(
    payload: PolicySimulationActionInput,
    tenant_id: &str,
    resolved_agent: &ResolvedAgentIdentity,
) -> Result<NormalizedAgentActionRequest, ApiError> {
    let body_agent_id = normalize_registry_key(&payload.agent_id, "agent_id", 64)?;
    if body_agent_id != resolved_agent.agent_id {
        return Err(ApiError::bad_request(
            "agent_id body field does not match resolved agent identity",
        ));
    }
    let body_environment_id =
        normalize_registry_key(&payload.environment_id, "environment_id", 64)?;
    if body_environment_id != resolved_agent.environment_id {
        return Err(ApiError::bad_request(
            "environment_id body field does not match resolved environment identity",
        ));
    }
    let intent_type = AgentIntentType::parse(&payload.intent_type)?;
    let execution_mode =
        resolve_agent_execution_mode(intent_type, payload.execution_mode.as_deref())?;
    let adapter_type = normalize_registry_key(&payload.adapter_type, "adapter_type", 64)?;
    let requested_scope = normalize_requested_scope(&payload.requested_scope)?;
    let reason = normalize_required_name(&payload.reason, "reason", 512)?;
    let submitted_by = normalize_required_name(&payload.submitted_by, "submitted_by", 128)?;

    match intent_type {
        AgentIntentType::Refund => validate_agent_refund_payload(&payload.payload)?,
        AgentIntentType::Transfer => validate_agent_transfer_payload(&payload.payload)?,
        AgentIntentType::GenerateInvoice => {
            validate_agent_generate_invoice_payload(&payload.payload)?
        }
    }

    Ok(NormalizedAgentActionRequest {
        action_request_id: format!("policy_simulation_{}", Uuid::new_v4().simple()),
        tenant_id: tenant_id.to_owned(),
        agent_id: resolved_agent.agent_id.clone(),
        environment_id: resolved_agent.environment_id.clone(),
        intent_type,
        execution_mode,
        adapter_type,
        normalized_payload: payload.payload,
        idempotency_key: format!("policy_simulation_{}", Uuid::new_v4().simple()),
        requested_scope,
        reason,
        callback_config: None,
        submitted_by,
        normalized_intent_kind: format!("policy.simulation.{}", intent_type.as_str()),
    })
}

fn default_scope_for_agent_intent(
    intent_type: AgentIntentType,
    raw_text: Option<&str>,
) -> Vec<String> {
    if raw_text
        .map(|value| value.to_ascii_lowercase())
        .is_some_and(|value| value.contains("playground") || value.contains("devnet"))
    {
        return vec!["playground".to_owned()];
    }
    match intent_type {
        AgentIntentType::Transfer => vec!["payments".to_owned()],
        AgentIntentType::Refund => vec!["refunds".to_owned()],
        AgentIntentType::GenerateInvoice => vec!["billing".to_owned()],
    }
}

fn default_adapter_for_agent_intent(intent_type: AgentIntentType) -> &'static str {
    match intent_type {
        AgentIntentType::Transfer => "adapter_solana",
        AgentIntentType::Refund | AgentIntentType::GenerateInvoice => "billing_adapter",
    }
}

fn agent_gateway_submitted_by(submitter: &SubmitterIdentity) -> String {
    format!("agent_gateway:{}", submitter.principal_id)
}

fn generate_gateway_action_request_id() -> String {
    format!("act_{}", Uuid::new_v4().simple())
}

fn derive_gateway_idempotency_key(seed: &Value) -> String {
    let canonical = serde_json::to_vec(&canonicalize_json_value(seed)).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(canonical);
    let digest = hex::encode(hasher.finalize());
    format!("agw_{}", &digest[..48])
}

fn free_form_parse_tokens(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|ch: char| {
                    !ch.is_ascii_alphanumeric() && !matches!(ch, '_' | '-' | '.' | ':' | '@' | '/')
                })
                .to_owned()
        })
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
}

fn infer_agent_intent_type_from_free_form(tokens: &[String]) -> Result<AgentIntentType, ApiError> {
    let has_transfer = tokens
        .iter()
        .any(|token| matches!(token.as_str(), "transfer" | "send"));
    let has_refund = tokens
        .iter()
        .any(|token| matches!(token.as_str(), "refund" | "reimburse"));
    let has_invoice = tokens
        .iter()
        .any(|token| matches!(token.as_str(), "invoice" | "bill"));
    let matches = [
        (has_transfer, AgentIntentType::Transfer, "transfer"),
        (has_refund, AgentIntentType::Refund, "refund"),
        (
            has_invoice,
            AgentIntentType::GenerateInvoice,
            "generate_invoice",
        ),
    ]
    .into_iter()
    .filter(|(matched, _, _)| *matched)
    .collect::<Vec<_>>();
    match matches.as_slice() {
        [(_, intent, _)] => Ok(*intent),
        [] => Err(ApiError::bad_request(
            "free_form_input did not contain a supported action keyword (`transfer`, `refund`, or `invoice`)",
        )),
        _ => Err(ApiError::bad_request(
            "free_form_input is ambiguous across multiple action families; provide structured_action instead",
        )),
    }
}

fn first_positive_integer_token(tokens: &[String]) -> Option<(usize, i64)> {
    tokens.iter().enumerate().find_map(|(index, token)| {
        token
            .parse::<i64>()
            .ok()
            .filter(|value| *value > 0)
            .map(|value| (index, value))
    })
}

fn token_after_keyword(tokens: &[String], keyword: &str) -> Option<String> {
    tokens
        .windows(2)
        .find(|window| window.first().is_some_and(|value| value == keyword))
        .and_then(|window| window.get(1))
        .cloned()
}

fn extract_reason_from_free_form(text: &str) -> String {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    for needle in [" because ", " for "] {
        if let Some(index) = lower.find(needle) {
            let reason = trimmed[(index + needle.len())..].trim();
            if !reason.is_empty() {
                return reason.to_owned();
            }
        }
    }
    trimmed.to_owned()
}

fn compile_transfer_payload_from_free_form(
    text: &str,
    tokens: &[String],
) -> Result<(Value, Vec<String>), ApiError> {
    let mut trace = vec!["detected transfer action from free_form_input".to_owned()];
    let (amount_index, amount) = first_positive_integer_token(tokens).ok_or_else(|| {
        ApiError::bad_request(
            "free_form_input transfer request must include a positive integer amount",
        )
    })?;
    trace.push(format!("parsed amount={amount}"));
    let to_addr = token_after_keyword(tokens, "to").ok_or_else(|| {
        ApiError::bad_request(
            "free_form_input transfer request must include a destination after `to`",
        )
    })?;
    trace.push(format!("parsed to_addr={to_addr}"));
    let asset = tokens
        .get(amount_index + 1)
        .filter(|token| token.chars().all(|ch| ch.is_ascii_alphabetic()))
        .map(|token| token.to_ascii_uppercase())
        .unwrap_or_else(|| "SOL".to_owned());
    if asset == "SOL" {
        trace.push("defaulted asset=SOL".to_owned());
    } else {
        trace.push(format!("parsed asset={asset}"));
    }
    let mut payload = Map::new();
    payload.insert("to_addr".to_owned(), Value::String(to_addr));
    payload.insert("amount".to_owned(), Value::Number(amount.into()));
    payload.insert("asset".to_owned(), Value::String(asset));
    if let Some(from_addr) = token_after_keyword(tokens, "from") {
        trace.push(format!("parsed from_addr={from_addr}"));
        payload.insert("from_addr".to_owned(), Value::String(from_addr));
    }
    if let Some(index) = text.to_ascii_lowercase().find(" memo ") {
        let memo = text[(index + 6)..].trim();
        if !memo.is_empty() {
            trace.push("parsed memo".to_owned());
            payload.insert("memo".to_owned(), Value::String(memo.to_owned()));
        }
    }
    Ok((Value::Object(payload), trace))
}

fn compile_refund_payload_from_free_form(
    tokens: &[String],
) -> Result<(Value, Vec<String>), ApiError> {
    let mut trace = vec!["detected refund action from free_form_input".to_owned()];
    let payment_reference = token_after_keyword(tokens, "payment")
        .or_else(|| token_after_keyword(tokens, "payment_reference"))
        .ok_or_else(|| {
            ApiError::bad_request(
                "free_form_input refund request must include a payment reference after `payment`",
            )
        })?;
    trace.push(format!("parsed payment_reference={payment_reference}"));
    let (amount_index, amount) = first_positive_integer_token(tokens).ok_or_else(|| {
        ApiError::bad_request(
            "free_form_input refund request must include a positive integer amount",
        )
    })?;
    trace.push(format!("parsed amount={amount}"));
    let currency = tokens
        .get(amount_index + 1)
        .filter(|token| token.chars().all(|ch| ch.is_ascii_alphabetic()))
        .map(|token| token.to_ascii_uppercase())
        .ok_or_else(|| {
            ApiError::bad_request(
                "free_form_input refund request must include a currency immediately after amount",
            )
        })?;
    trace.push(format!("parsed currency={currency}"));
    let mut payload = Map::new();
    payload.insert(
        "payment_reference".to_owned(),
        Value::String(payment_reference),
    );
    payload.insert("amount".to_owned(), Value::Number(amount.into()));
    payload.insert("currency".to_owned(), Value::String(currency));
    if let Some(destination_reference) =
        token_after_keyword(tokens, "to").or_else(|| token_after_keyword(tokens, "destination"))
    {
        trace.push(format!(
            "parsed destination_reference={destination_reference}"
        ));
        payload.insert(
            "destination_reference".to_owned(),
            Value::String(destination_reference),
        );
    }
    Ok((Value::Object(payload), trace))
}

fn compile_generate_invoice_payload_from_free_form(
    tokens: &[String],
) -> Result<(Value, Vec<String>), ApiError> {
    let mut trace = vec!["detected generate_invoice action from free_form_input".to_owned()];
    let customer_reference = token_after_keyword(tokens, "customer")
        .or_else(|| token_after_keyword(tokens, "client"))
        .ok_or_else(|| {
            ApiError::bad_request(
                "free_form_input invoice request must include a customer reference after `customer`",
            )
        })?;
    trace.push(format!("parsed customer_reference={customer_reference}"));
    let (amount_index, amount) = first_positive_integer_token(tokens).ok_or_else(|| {
        ApiError::bad_request(
            "free_form_input invoice request must include a positive integer amount",
        )
    })?;
    trace.push(format!("parsed amount={amount}"));
    let currency = tokens
        .get(amount_index + 1)
        .filter(|token| token.chars().all(|ch| ch.is_ascii_alphabetic()))
        .map(|token| token.to_ascii_uppercase())
        .ok_or_else(|| {
            ApiError::bad_request(
                "free_form_input invoice request must include a currency immediately after amount",
            )
        })?;
    trace.push(format!("parsed currency={currency}"));
    let mut payload = Map::new();
    payload.insert(
        "customer_reference".to_owned(),
        Value::String(customer_reference),
    );
    payload.insert("amount".to_owned(), Value::Number(amount.into()));
    payload.insert("currency".to_owned(), Value::String(currency));
    Ok((Value::Object(payload), trace))
}

fn compile_free_form_agent_gateway_request(
    free_form_input: &str,
    tenant_id: &str,
    resolved_agent: &ResolvedAgentIdentity,
    submitter: &SubmitterIdentity,
) -> Result<AgentGatewayCompilationView, ApiError> {
    let input = free_form_input.trim();
    if input.is_empty() {
        return Err(ApiError::bad_request("free_form_input is required"));
    }
    let tokens = free_form_parse_tokens(input);
    let intent_type = infer_agent_intent_type_from_free_form(&tokens)?;
    let execution_mode = intent_type.default_execution_mode();
    let adapter_type = default_adapter_for_agent_intent(intent_type).to_owned();
    let (payload, mut trace) = match intent_type {
        AgentIntentType::Transfer => compile_transfer_payload_from_free_form(input, &tokens)?,
        AgentIntentType::Refund => compile_refund_payload_from_free_form(&tokens)?,
        AgentIntentType::GenerateInvoice => {
            compile_generate_invoice_payload_from_free_form(&tokens)?
        }
    };
    trace.push(format!("defaulted adapter_type={adapter_type}"));
    let requested_scope = default_scope_for_agent_intent(intent_type, Some(input));
    trace.push(format!(
        "defaulted requested_scope={}",
        requested_scope.join(",")
    ));
    let action_request_id = generate_gateway_action_request_id();
    let reason = extract_reason_from_free_form(input);
    let submitted_by = agent_gateway_submitted_by(submitter);
    let idempotency_key = derive_gateway_idempotency_key(&json!({
        "tenant_id": tenant_id,
        "agent_id": resolved_agent.agent_id.clone(),
        "environment_id": resolved_agent.environment_id.clone(),
        "intent_type": intent_type.as_str(),
        "execution_mode": execution_mode.as_str(),
        "adapter_type": adapter_type.clone(),
        "payload": canonicalize_json_value(&payload),
        "requested_scope": requested_scope.clone(),
        "reason": reason.clone(),
        "free_form_input": input,
    }));
    let compiled_request = SubmitAgentActionRequest {
        action_request_id,
        tenant_id: tenant_id.to_owned(),
        agent_id: resolved_agent.agent_id.clone(),
        environment_id: resolved_agent.environment_id.clone(),
        intent_type: intent_type.as_str().to_owned(),
        execution_mode: Some(execution_mode.as_str().to_owned()),
        adapter_type,
        payload,
        idempotency_key,
        requested_scope,
        reason,
        callback_config: None,
        submitted_by,
    };
    Ok(AgentGatewayCompilationView {
        mode: "free_form".to_owned(),
        execution_mode: execution_mode.as_str().to_owned(),
        summary: format!(
            "compiled free-form input into `{}` request in `{}` for agent `{}`",
            compiled_request.intent_type,
            execution_mode.as_str(),
            resolved_agent.agent_id
        ),
        trace,
        compiled_request,
    })
}

fn compile_structured_agent_gateway_request(
    structured_action: AgentGatewayStructuredActionInput,
    tenant_id: &str,
    resolved_agent: &ResolvedAgentIdentity,
    submitter: &SubmitterIdentity,
) -> Result<AgentGatewayCompilationView, ApiError> {
    let intent_type_raw = structured_action
        .intent_type
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("structured_action.intent_type is required"))?;
    let intent_type = AgentIntentType::parse(intent_type_raw)?;
    let execution_mode =
        resolve_agent_execution_mode(intent_type, structured_action.execution_mode.as_deref())?;
    let adapter_type = structured_action
        .adapter_type
        .as_deref()
        .map(|value| normalize_registry_key(value, "structured_action.adapter_type", 64))
        .transpose()?
        .unwrap_or_else(|| default_adapter_for_agent_intent(intent_type).to_owned());
    let payload = structured_action
        .payload
        .ok_or_else(|| ApiError::bad_request("structured_action.payload is required"))?;
    let requested_scope = match structured_action.requested_scope {
        Some(scope) => normalize_requested_scope(&scope)?,
        None => default_scope_for_agent_intent(intent_type, None),
    };
    let reason = structured_action
        .reason
        .as_deref()
        .ok_or_else(|| ApiError::bad_request("structured_action.reason is required"))
        .and_then(|value| normalize_required_name(value, "structured_action.reason", 512))?;
    let submitted_by = normalize_optional_name(
        structured_action.submitted_by.as_deref(),
        "structured_action.submitted_by",
        128,
        &agent_gateway_submitted_by(submitter),
    )?;
    let action_request_id = match structured_action.action_request_id.as_deref() {
        Some(value) => normalize_registry_key(value, "structured_action.action_request_id", 128)?,
        None => generate_gateway_action_request_id(),
    };
    let callback_config = structured_action.callback_config;
    let idempotency_key = match structured_action.idempotency_key.as_deref() {
        Some(value) => normalize_registry_key(value, "structured_action.idempotency_key", 128)?,
        None => derive_gateway_idempotency_key(&json!({
            "tenant_id": tenant_id,
            "agent_id": resolved_agent.agent_id.clone(),
            "environment_id": resolved_agent.environment_id.clone(),
            "intent_type": intent_type.as_str(),
            "adapter_type": adapter_type.clone(),
            "payload": canonicalize_json_value(&payload),
            "requested_scope": requested_scope.clone(),
            "reason": reason.clone(),
        })),
    };
    let compiled_request = SubmitAgentActionRequest {
        action_request_id,
        tenant_id: tenant_id.to_owned(),
        agent_id: resolved_agent.agent_id.clone(),
        environment_id: resolved_agent.environment_id.clone(),
        intent_type: intent_type.as_str().to_owned(),
        execution_mode: Some(execution_mode.as_str().to_owned()),
        adapter_type,
        payload,
        idempotency_key,
        requested_scope,
        reason,
        callback_config,
        submitted_by,
    };
    Ok(AgentGatewayCompilationView {
        mode: "structured".to_owned(),
        execution_mode: execution_mode.as_str().to_owned(),
        summary: format!(
            "validated structured request for `{}` in `{}` and prepared handoff to agent control flow",
            compiled_request.intent_type,
            execution_mode.as_str()
        ),
        trace: vec![
            "accepted structured_action input".to_owned(),
            format!("resolved agent_id={}", resolved_agent.agent_id),
            format!("resolved environment_id={}", resolved_agent.environment_id),
        ],
        compiled_request,
    })
}

fn compile_agent_gateway_request(
    payload: AgentGatewayRequest,
    tenant_id: &str,
    resolved_agent: &ResolvedAgentIdentity,
    submitter: &SubmitterIdentity,
) -> Result<AgentGatewayCompilationView, ApiError> {
    match (
        payload
            .free_form_input
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        payload.structured_action,
    ) {
        (Some(_), Some(_)) => Err(ApiError::bad_request(
            "provide either free_form_input or structured_action, not both",
        )),
        (None, None) => Err(ApiError::bad_request(
            "one of free_form_input or structured_action is required",
        )),
        (Some(text), None) => {
            compile_free_form_agent_gateway_request(text, tenant_id, resolved_agent, submitter)
        }
        (None, Some(structured_action)) => compile_structured_agent_gateway_request(
            structured_action,
            tenant_id,
            resolved_agent,
            submitter,
        ),
    }
}

fn resolve_payer_source(policy: ExecutionPolicy, signed_tx_present: bool) -> &'static str {
    if signed_tx_present {
        return "customer_wallet";
    }
    match policy {
        ExecutionPolicy::CustomerSigned => "customer_wallet",
        ExecutionPolicy::CustomerManagedSigner => "customer_managed_signer",
        ExecutionPolicy::Sponsored => "platform_sponsored",
    }
}

fn is_playground_metering_scope(metering_scope: Option<&str>) -> bool {
    metering_scope
        .map(|value| value.trim().eq_ignore_ascii_case("playground"))
        .unwrap_or(false)
}

fn resolve_effective_execution_policy(
    base_policy: ExecutionPolicy,
    submitter: &SubmitterIdentity,
    metering_scope: Option<&str>,
) -> ExecutionPolicy {
    let playground_force_sponsored = env_bool("INGRESS_PLAYGROUND_FORCE_SPONSORED", true);
    if playground_force_sponsored
        && matches!(submitter.kind, SubmitterKind::InternalService)
        && is_playground_metering_scope(metering_scope)
    {
        return ExecutionPolicy::Sponsored;
    }
    base_policy
}

fn enforce_execution_policy_for_payload(
    effective_policy: ExecutionPolicy,
    intent_kind: &str,
    payload: &Value,
) -> Result<(), ApiError> {
    if !intent_kind_requires_signed_tx_policy(intent_kind) {
        return Ok(());
    }
    if matches!(
        effective_policy,
        ExecutionPolicy::CustomerSigned | ExecutionPolicy::CustomerManagedSigner
    ) && !signed_tx_payload_present(payload)
    {
        return Err(ApiError::bad_request(
            "EXECUTION_POLICY_SIGNED_TX_REQUIRED: tenant policy requires signed transaction payload (`signed_tx_base64`).",
        ));
    }
    Ok(())
}

fn ensure_non_empty_optional(field: &str, value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        if value.trim().is_empty() {
            return Err(format!("payload field `{field}` must not be empty"));
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SubmitIntentRequest {
    intent_kind: String,
    payload: Value,
    metadata: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct AgentActionCallbackConfig {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    signing_secret_ref: Option<String>,
    #[serde(default)]
    include_receipt: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct SubmitAgentActionRequest {
    action_request_id: String,
    tenant_id: String,
    agent_id: String,
    environment_id: String,
    intent_type: String,
    #[serde(default)]
    execution_mode: Option<String>,
    adapter_type: String,
    payload: Value,
    idempotency_key: String,
    requested_scope: Vec<String>,
    reason: String,
    #[serde(default)]
    callback_config: Option<AgentActionCallbackConfig>,
    submitted_by: String,
}

#[derive(Debug, Deserialize, Clone, Default)]
struct AgentGatewayStructuredActionInput {
    #[serde(default)]
    action_request_id: Option<String>,
    #[serde(default)]
    intent_type: Option<String>,
    #[serde(default)]
    execution_mode: Option<String>,
    #[serde(default)]
    adapter_type: Option<String>,
    #[serde(default)]
    payload: Option<Value>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    requested_scope: Option<Vec<String>>,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    callback_config: Option<AgentActionCallbackConfig>,
    #[serde(default)]
    submitted_by: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct AgentGatewayRequest {
    #[serde(default)]
    free_form_input: Option<String>,
    #[serde(default)]
    structured_action: Option<AgentGatewayStructuredActionInput>,
}

#[derive(Debug, Serialize)]
struct AgentGatewayCompilationView {
    mode: String,
    execution_mode: String,
    summary: String,
    trace: Vec<String>,
    compiled_request: SubmitAgentActionRequest,
}

#[derive(Debug, Serialize)]
struct AgentGatewayResponse {
    ok: bool,
    gateway_request_id: String,
    compilation: AgentGatewayCompilationView,
    handoff: SubmitAgentActionResponse,
}

#[derive(Debug, Clone, Serialize)]
struct SubmitIntentResponse {
    ok: bool,
    tenant_id: String,
    intent_id: String,
    job_id: String,
    adapter_id: String,
    state: String,
    route_rule: String,
}

#[derive(Debug, Serialize)]
struct SubmitAgentActionResponse {
    ok: bool,
    action_request_id: String,
    tenant_id: String,
    agent_id: String,
    environment_id: String,
    intent_type: String,
    execution_mode: String,
    execution_owner: String,
    adapter_type: String,
    idempotency_key: String,
    idempotency_decision: String,
    policy_decision: String,
    policy_explanation: String,
    effective_scope: Vec<String>,
    obligations: PolicyRuleObligations,
    matched_rules: Vec<PolicyRuleMatch>,
    decision_trace: Vec<PolicyDecisionTraceEntry>,
    policy_bundle_id: Option<String>,
    policy_bundle_version: Option<i64>,
    grant_id: Option<String>,
    grant_uses_remaining: Option<u32>,
    approval_request_id: Option<String>,
    approval_state: Option<String>,
    approval_expires_at_ms: Option<u64>,
    intent_id: Option<String>,
    job_id: Option<String>,
    adapter_id: Option<String>,
    state: Option<String>,
    route_rule: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RegisterTenantApiKeyRequest {
    key_id: String,
    label: Option<String>,
    key_value: String,
    key_prefix: String,
    key_last4: String,
    created_by_principal_id: Option<String>,
    created_at_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RegisterTenantWebhookKeyRequest {
    source: Option<String>,
    grace_seconds: Option<u64>,
    created_by_principal_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListTenantApiKeysQuery {
    include_inactive: Option<bool>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct ListTenantWebhookKeysQuery {
    source: Option<String>,
    include_inactive: Option<bool>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RevokeTenantWebhookKeyRequest {
    grace_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct TenantWebhookKeyRecordView {
    key_id: String,
    tenant_id: String,
    source: String,
    secret_last4: String,
    active: bool,
    created_by_principal_id: String,
    created_at_ms: u64,
    revoked_at_ms: Option<u64>,
    expires_at_ms: Option<u64>,
    last_used_at_ms: Option<u64>,
}

#[derive(Debug, Serialize)]
struct TenantApiKeyRecordView {
    key_id: String,
    tenant_id: String,
    label: String,
    key_prefix: String,
    key_last4: String,
    created_by_principal_id: String,
    created_at_ms: u64,
    revoked_at_ms: Option<u64>,
    last_used_at_ms: Option<u64>,
}

#[derive(Debug, Clone)]
struct ConnectorBindingRecord {
    tenant_id: String,
    environment_id: String,
    binding_id: String,
    connector_type: String,
    name: String,
    status: String,
    secret_ref: String,
    current_secret_version: u64,
    config: Value,
    secret_fields: Vec<String>,
    created_by_principal_id: String,
    updated_by_principal_id: String,
    created_at_ms: u64,
    updated_at_ms: u64,
    rotated_at_ms: u64,
    revoked_at_ms: Option<u64>,
    revoked_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct ConnectorBindingCreateRecord {
    tenant_id: String,
    environment_id: String,
    binding_id: String,
    connector_type: String,
    name: String,
    secret_ref: String,
    config: Value,
    secret_values: BTreeMap<String, String>,
    created_by_principal_id: String,
    created_at_ms: u64,
}

#[derive(Debug, Clone)]
struct ConnectorBindingRotationRecord {
    tenant_id: String,
    environment_id: String,
    binding_id: String,
    secret_values: BTreeMap<String, String>,
    rotated_by_principal_id: String,
    rotated_at_ms: u64,
    rotation_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct BrokerConnectorBindingUseRequest {
    tenant_id: String,
    environment_id: String,
    binding_id: String,
    actor_id: String,
    actor_kind: String,
    purpose: String,
    request_id: Option<String>,
    action_request_id: Option<String>,
    approval_request_id: Option<String>,
    intent_id: Option<String>,
    job_id: Option<String>,
    correlation_id: Option<String>,
    used_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct CreateConnectorBindingRequest {
    binding_id: String,
    connector_type: String,
    name: String,
    #[serde(default)]
    config: Option<Value>,
    secrets: BTreeMap<String, String>,
    created_by_principal_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RotateConnectorBindingRequest {
    secrets: BTreeMap<String, String>,
    rotated_by_principal_id: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RevokeConnectorBindingRequest {
    revoked_by_principal_id: Option<String>,
    reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ListConnectorBindingsQuery {
    include_inactive: Option<bool>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct BrokerConnectorBindingUsePayload {
    actor_id: Option<String>,
    actor_kind: Option<String>,
    purpose: String,
    request_id: Option<String>,
    action_request_id: Option<String>,
    approval_request_id: Option<String>,
    intent_id: Option<String>,
    job_id: Option<String>,
    correlation_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConnectorBindingView {
    tenant_id: String,
    environment_id: String,
    binding_id: String,
    connector_type: String,
    name: String,
    status: String,
    secret_ref: String,
    current_secret_version: u64,
    config: Value,
    secret_fields: Vec<String>,
    created_by_principal_id: String,
    updated_by_principal_id: String,
    created_at_ms: u64,
    updated_at_ms: u64,
    rotated_at_ms: u64,
    revoked_at_ms: Option<u64>,
    revoked_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConnectorBindingResponse {
    ok: bool,
    binding: ConnectorBindingView,
}

#[derive(Debug, Serialize)]
struct ConnectorBindingSecretUseResponse {
    ok: bool,
    binding: ConnectorBindingView,
    resolved_secret_version: u64,
    secrets: BTreeMap<String, String>,
}

#[derive(Debug)]
struct TenantApiKeyProvisionRequest {
    key_id: String,
    tenant_id: String,
    label: String,
    key_hash: String,
    key_prefix: String,
    key_last4: String,
    created_by_principal_id: String,
    created_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct UpsertTenantQuotaRequest {
    plan: Option<String>,
    access_mode: Option<String>,
    execution_policy: Option<String>,
    sponsored_monthly_cap_requests: Option<u64>,
    free_play_limit: Option<u64>,
    updated_by_principal_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct TenantQuotaProfileResponse {
    ok: bool,
    profile: TenantQuotaProfileView,
}

#[derive(Debug, Serialize)]
struct TenantQuotaProfileView {
    tenant_id: String,
    plan: String,
    access_mode: String,
    execution_policy: String,
    sponsored_monthly_cap_requests: u64,
    free_play_limit: u64,
    updated_by_principal_id: String,
    updated_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct UpsertTenantEnvironmentRequest {
    environment_id: String,
    name: String,
    environment_kind: String,
    status: Option<String>,
    created_by_principal_id: Option<String>,
    updated_by_principal_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListTenantEnvironmentsQuery {
    include_inactive: Option<bool>,
    limit: Option<u32>,
}

#[derive(Debug, Serialize)]
struct TenantEnvironmentResponse {
    ok: bool,
    environment: TenantEnvironmentView,
}

#[derive(Debug, Serialize)]
struct TenantEnvironmentView {
    tenant_id: String,
    environment_id: String,
    name: String,
    environment_kind: String,
    is_production: bool,
    status: String,
    created_by_principal_id: String,
    updated_by_principal_id: String,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct UpsertTenantAgentRequest {
    agent_id: String,
    environment_id: String,
    name: String,
    runtime_type: String,
    runtime_identity: String,
    status: Option<String>,
    trust_tier: Option<String>,
    risk_tier: Option<String>,
    owner_team: Option<String>,
    created_by_principal_id: Option<String>,
    updated_by_principal_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListTenantAgentsQuery {
    environment_id: Option<String>,
    include_inactive: Option<bool>,
    limit: Option<u32>,
}

#[derive(Debug, Serialize)]
struct TenantAgentResponse {
    ok: bool,
    agent: TenantAgentView,
}

#[derive(Debug, Serialize)]
struct TenantAgentView {
    agent_id: String,
    tenant_id: String,
    environment_id: String,
    name: String,
    runtime_type: String,
    runtime_identity: String,
    status: String,
    trust_tier: String,
    risk_tier: String,
    owner_team: String,
    created_by_principal_id: String,
    updated_by_principal_id: String,
    created_at_ms: u64,
    updated_at_ms: u64,
}

#[derive(Debug, Deserialize)]
struct CreateTenantPolicyBundleRequest {
    bundle_id: String,
    label: String,
    #[serde(default)]
    template_ids: Vec<String>,
    #[serde(default)]
    rules: Vec<PolicyRuleDefinition>,
}

#[derive(Debug, Deserialize)]
struct ListTenantPolicyBundlesQuery {
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RollbackTenantPolicyBundleRequest {
    target_bundle_id: String,
    rollback_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PolicySimulationActionInput {
    agent_id: String,
    environment_id: String,
    intent_type: String,
    #[serde(default)]
    execution_mode: Option<String>,
    adapter_type: String,
    payload: Value,
    requested_scope: Vec<String>,
    reason: String,
    submitted_by: String,
}

#[derive(Debug, Deserialize)]
struct SimulateTenantPolicyRequest {
    #[serde(default)]
    bundle_id: Option<String>,
    action: PolicySimulationActionInput,
}

#[derive(Debug, Serialize)]
struct TenantPolicyBundleResponse {
    ok: bool,
    bundle: TenantPolicyBundleView,
}

#[derive(Debug, Serialize)]
struct TenantPolicyBundleView {
    tenant_id: String,
    bundle_id: String,
    version: i64,
    label: String,
    status: String,
    template_ids: Vec<String>,
    rules: Vec<PolicyRuleDefinition>,
    created_by_principal_id: String,
    published_by_principal_id: Option<String>,
    created_at_ms: u64,
    published_at_ms: Option<u64>,
    rolled_back_from_bundle_id: Option<String>,
    rollback_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct PolicySimulationResponse {
    ok: bool,
    bundle: Option<TenantPolicyBundleView>,
    decision: PolicyDecisionExplanation,
    execution_mode: String,
    execution_owner: String,
    resolved_agent: TenantAgentView,
    environment: TenantEnvironmentView,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let observability = Arc::new(ObservabilityConfig::from_env("ingress_api"));
    init_tracing(observability.as_ref()).context("failed to initialize observability")?;
    let _metrics_handle = init_metrics().context("failed to initialize metrics recorder")?;

    let database_url =
        std::env::var("DATABASE_URL").context("DATABASE_URL is required for ingress_api")?;
    let bind_addr = env_or("INGRESS_API_BIND", "0.0.0.0:8081");
    let max_connections = env_u32("INGRESS_DB_MAX_CONNECTIONS", 8);

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(&database_url)
        .await
        .context("failed to connect to postgres")?;
    let audit_store = Arc::new(IngressIntakeAuditStore::new(pool.clone()));
    audit_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress intake audit schema")?;
    let api_key_store = Arc::new(IngressTenantApiKeyStore::new(pool.clone()));
    api_key_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress tenant api key schema")?;
    let webhook_key_store = Arc::new(IngressTenantWebhookKeyStore::new(pool.clone()));
    webhook_key_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress tenant webhook key schema")?;
    let environment_store = Arc::new(IngressEnvironmentStore::new(pool.clone()));
    environment_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress tenant environment schema")?;
    let agent_store = Arc::new(IngressAgentStore::new(pool.clone()));
    agent_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress tenant agent schema")?;
    let agent_action_idempotency_store =
        Arc::new(IngressAgentActionIdempotencyStore::new(pool.clone()));
    agent_action_idempotency_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress agent action idempotency schema")?;
    let approval_store = Arc::new(IngressApprovalStore::new(pool.clone()));
    approval_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress approval schema")?;
    let approval_workflow = Arc::new(ApprovalWorkflowConfig::from_env());
    let approval_http_client = Arc::new(
        Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("failed to build approval http client")?,
    );
    let capability_grant_store = Arc::new(IngressCapabilityGrantStore::new(pool.clone()));
    capability_grant_store
        .ensure_schema()
        .await
        .context("failed to ensure capability grant schema")?;
    let grant_workflow = Arc::new(GrantWorkflowConfig::from_env());
    let connector_binding_store = Arc::new(IngressConnectorBindingStore::new(pool.clone()));
    connector_binding_store
        .ensure_schema()
        .await
        .context("failed to ensure connector binding schema")?;
    let connector_secret_key = env_var_opt("INGRESS_CONNECTOR_SECRETS_KEY")
        .ok_or_else(|| anyhow::anyhow!("INGRESS_CONNECTOR_SECRETS_KEY is required"))?;
    let connector_secret_cipher = SecretCipher::from_passphrase(&connector_secret_key)
        .context("failed to initialize connector secret cipher")?;
    let connector_secret_broker = Arc::new(IngressConnectorSecretBroker::new(
        connector_binding_store.as_ref().clone(),
        connector_secret_cipher,
    ));
    let policy_bundle_store = Arc::new(IngressPolicyBundleStore::new(pool.clone()));
    policy_bundle_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress tenant policy bundle schema")?;
    let default_plan = QuotaPlanTier::parse(&env_or("INGRESS_DEFAULT_QUOTA_PLAN", "developer"))
        .unwrap_or(QuotaPlanTier::Developer);
    let default_access_mode =
        QuotaAccessMode::parse(&env_or("INGRESS_DEFAULT_QUOTA_ACCESS_MODE", "free_play"))
            .unwrap_or(QuotaAccessMode::FreePlay);
    let default_execution_policy = ExecutionPolicy::parse(&env_or(
        "INGRESS_DEFAULT_EXECUTION_POLICY",
        "customer_signed",
    ))
    .unwrap_or(ExecutionPolicy::CustomerSigned);
    let default_sponsored_monthly_cap_requests =
        env_u64("INGRESS_DEFAULT_SPONSORED_MONTHLY_CAP_REQUESTS", 10_000) as i64;
    let default_limit_from_plan = default_plan.default_free_play_limit();
    let default_free_play_limit = env_u64(
        "INGRESS_DEFAULT_FREE_PLAY_LIMIT",
        if default_limit_from_plan <= 0 {
            500
        } else {
            default_limit_from_plan as u64
        },
    ) as i64;
    let quota_store = Arc::new(IngressTenantQuotaStore::new(
        pool.clone(),
        default_plan,
        default_access_mode,
        default_execution_policy,
        if default_sponsored_monthly_cap_requests > 0 {
            default_sponsored_monthly_cap_requests
        } else {
            10_000
        },
        if default_free_play_limit > 0 {
            default_free_play_limit
        } else {
            default_limit_from_plan
        },
    ));
    quota_store
        .ensure_schema()
        .await
        .context("failed to ensure ingress tenant quota schema")?;

    let store = Arc::new(PostgresQStore::new(
        pool,
        PostgresQConfig {
            dispatch_queue: env_or("EXECUTION_DISPATCH_QUEUE", "execution.dispatch"),
            callback_queue: env_or("EXECUTION_CALLBACK_QUEUE", "execution.callback"),
            ..PostgresQConfig::default()
        },
    ));
    store
        .ensure_schema()
        .await
        .context("failed to ensure execution core schema")?;

    let routes = parse_route_map(env_or(
        "INGRESS_INTENT_ROUTES",
        "solana.transfer.v1=adapter_solana;solana.broadcast.v1=adapter_solana",
    ))?;
    let schemas = Arc::new(IngressSchemaRegistry::from_env(&routes)?);
    let mut registry = AdapterRegistry::new();
    let mut allowed_adapters = HashSet::new();
    for route in routes {
        let adapter_id = AdapterId::from(route.adapter_id.clone());
        registry.register_route(
            route.intent_kind.clone(),
            adapter_id.clone(),
            format!("ingress_route_map:{}", route.intent_kind),
        );
        allowed_adapters.insert(adapter_id.to_string());
    }

    let core = Arc::new(ExecutionCore::new(
        store,
        Arc::new(registry),
        Arc::new(IngressAuthorizer { allowed_adapters }),
        RetryPolicy::from_env(),
        ReplayPolicy::default(),
        Arc::new(SystemClock),
    ));

    let state = AppState {
        core,
        audit_store,
        environment_store,
        agent_store,
        agent_action_idempotency_store,
        approval_store,
        approval_workflow,
        approval_http_client,
        capability_grant_store,
        grant_workflow,
        connector_binding_store,
        connector_secret_broker,
        policy_bundle_store,
        api_key_store,
        webhook_key_store,
        quota_store,
        auth: Arc::new(IngressAuthConfig::from_env()?),
        schemas,
        clock: Arc::new(SystemClock),
        execution_policy_enforcement: Arc::new(ExecutionPolicyEnforcement::from_env()),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/api/requests", post(submit_request))
        .route("/api/agent/gateway/requests", post(submit_agent_gateway_request))
        .route("/api/agent/action-requests", post(submit_agent_action_request))
        .route("/api/internal/policy/templates", get(list_policy_templates))
        .route(
            "/api/internal/tenants/:tenant_id/environments/:environment_id/connector-bindings",
            get(list_connector_bindings).post(create_connector_binding),
        )
        .route(
            "/api/internal/tenants/:tenant_id/environments/:environment_id/connector-bindings/:binding_id",
            get(get_connector_binding),
        )
        .route(
            "/api/internal/tenants/:tenant_id/environments/:environment_id/connector-bindings/:binding_id/rotate",
            post(rotate_connector_binding),
        )
        .route(
            "/api/internal/tenants/:tenant_id/environments/:environment_id/connector-bindings/:binding_id/revoke",
            post(revoke_connector_binding),
        )
        .route(
            "/api/internal/tenants/:tenant_id/environments/:environment_id/connector-bindings/:binding_id/broker-use",
            post(broker_use_connector_binding),
        )
        .route(
            "/api/internal/tenants/:tenant_id/approvals",
            get(list_tenant_approvals),
        )
        .route(
            "/api/internal/tenants/:tenant_id/approvals/:approval_request_id",
            get(get_tenant_approval),
        )
        .route(
            "/api/internal/tenants/:tenant_id/approvals/:approval_request_id/approve",
            post(approve_tenant_approval),
        )
        .route(
            "/api/internal/tenants/:tenant_id/approvals/:approval_request_id/reject",
            post(reject_tenant_approval),
        )
        .route(
            "/api/internal/tenants/:tenant_id/approvals/:approval_request_id/escalate",
            post(escalate_tenant_approval),
        )
        .route(
            "/api/internal/tenants/:tenant_id/grants",
            get(list_capability_grants),
        )
        .route(
            "/api/internal/tenants/:tenant_id/grants/:grant_id",
            get(get_capability_grant),
        )
        .route(
            "/api/internal/tenants/:tenant_id/grants/:grant_id/revoke",
            post(revoke_capability_grant),
        )
        .route(
            "/api/internal/tenants/:tenant_id/environments",
            get(list_tenant_environments).post(upsert_tenant_environment),
        )
        .route(
            "/api/internal/tenants/:tenant_id/environments/:environment_id",
            get(get_tenant_environment),
        )
        .route(
            "/api/internal/tenants/:tenant_id/agents",
            get(list_tenant_agents).post(upsert_tenant_agent),
        )
        .route(
            "/api/internal/tenants/:tenant_id/agents/:agent_id",
            get(get_tenant_agent),
        )
        .route(
            "/api/internal/tenants/:tenant_id/policy/bundles",
            get(list_tenant_policy_bundles).post(create_tenant_policy_bundle),
        )
        .route(
            "/api/internal/tenants/:tenant_id/policy/bundles/:bundle_id",
            get(get_tenant_policy_bundle),
        )
        .route(
            "/api/internal/tenants/:tenant_id/policy/bundles/:bundle_id/publish",
            post(publish_tenant_policy_bundle),
        )
        .route(
            "/api/internal/tenants/:tenant_id/policy/bundles/:bundle_id/rollback",
            post(rollback_tenant_policy_bundle),
        )
        .route(
            "/api/internal/tenants/:tenant_id/policy/simulations",
            post(simulate_tenant_policy),
        )
        .route(
            "/api/internal/tenants/:tenant_id/api-keys",
            get(list_tenant_api_keys).post(register_tenant_api_key),
        )
        .route(
            "/api/internal/tenants/:tenant_id/api-keys/:key_id/revoke",
            post(revoke_tenant_api_key),
        )
        .route(
            "/api/internal/tenants/:tenant_id/webhook-keys",
            get(list_tenant_webhook_keys).post(register_tenant_webhook_key),
        )
        .route(
            "/api/internal/tenants/:tenant_id/webhook-keys/:key_id/revoke",
            post(revoke_tenant_webhook_key),
        )
        .route(
            "/api/internal/tenants/:tenant_id/quota",
            put(upsert_tenant_quota),
        )
        .route(
            "/webhooks/slack/approvals/:tenant_id",
            post(handle_slack_approval_callback),
        )
        .route("/webhooks/:source", post(submit_webhook))
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            observability.clone(),
            observability_middleware,
        ));

    let addr: SocketAddr = bind_addr
        .parse()
        .with_context(|| format!("invalid INGRESS_API_BIND `{bind_addr}`"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(bind = %addr, "ingress_api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn observability_middleware(
    State(observability): State<Arc<ObservabilityConfig>>,
    mut request: Request,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    let start = Instant::now();

    let ctx = derive_request_context(request.headers(), observability.as_ref());
    if let Err(err) = apply_request_context(request.headers_mut(), observability.as_ref(), &ctx) {
        warn!(error = %err, "failed to apply observability request headers");
    }

    let mut response = next.run(request).await;
    if let Err(err) = apply_request_context(response.headers_mut(), observability.as_ref(), &ctx) {
        warn!(error = %err, "failed to apply observability response headers");
    }

    let status = response.status().as_u16();
    record_http_request(
        observability.as_ref(),
        &method,
        &path,
        status,
        start.elapsed(),
    );

    response
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true }))
}

async fn metrics() -> Response {
    match render_metrics() {
        Some(payload) => (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                "text/plain; version=0.0.4; charset=utf-8",
            )],
            payload,
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "ok": false,
                "error": "metrics recorder is not initialized",
            })),
        )
            .into_response(),
    }
}

fn hash_tenant_api_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    hex::encode(hasher.finalize())
}

async fn authorize_submitter_api_key_if_required(
    state: &AppState,
    tenant_id: &str,
    headers: &HeaderMap,
    submitter: &SubmitterIdentity,
) -> Result<(), ApiError> {
    if !matches!(submitter.kind, SubmitterKind::ApiKeyHolder)
        || !state.auth.require_api_key_for_api_key_holder
    {
        return Ok(());
    }

    let api_key = state.auth.require_api_key_header(headers)?;
    if state.auth.static_api_key_matches(tenant_id, &api_key) {
        return Ok(());
    }

    let key_hash = hash_tenant_api_key(&api_key);
    let valid = state
        .api_key_store
        .validate_api_key(tenant_id, &key_hash, state.clock.now_ms())
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("api key store unavailable: {err}"))
        })?;

    if valid {
        Ok(())
    } else {
        Err(ApiError::unauthorized("invalid x-api-key"))
    }
}

async fn authenticate_and_resolve_agent_runtime_submitter(
    state: &AppState,
    headers: &HeaderMap,
    audit: &mut IngressIntakeAuditRecord,
) -> Result<(String, SubmitterIdentity, ResolvedAgentIdentity), ApiError> {
    let tenant_id = tenant_id_from_headers(headers)?;
    audit.set_tenant(&tenant_id);

    let mut submitter =
        state
            .auth
            .authenticate_submitter(&tenant_id, headers, IngressChannel::Api)?;
    authorize_submitter_api_key_if_required(state, &tenant_id, headers, &submitter).await?;
    if !matches!(submitter.kind, SubmitterKind::AgentRuntime) {
        return Err(ApiError::forbidden(
            "only agent_runtime submitters may call agent runtime endpoints",
        ));
    }

    let resolved_agent =
        resolve_agent_identity_for_submitter(state, &tenant_id, headers, &submitter)
            .await?
            .ok_or_else(|| {
                ApiError::forbidden(
                    "resolved agent identity is required for agent runtime requests",
                )
            })?;
    submitter.resolved_agent = Some(resolved_agent.clone());
    audit.set_submitter(&submitter);
    Ok((tenant_id, submitter, resolved_agent))
}

async fn resolve_agent_identity_for_submitter(
    state: &AppState,
    tenant_id: &str,
    headers: &HeaderMap,
    submitter: &SubmitterIdentity,
) -> Result<Option<ResolvedAgentIdentity>, ApiError> {
    if !matches!(submitter.kind, SubmitterKind::AgentRuntime) {
        return Ok(None);
    }

    let environment_id = normalize_registry_key(
        &header_opt(headers, "x-environment-id")
            .ok_or_else(|| ApiError::unauthorized("missing x-environment-id"))?,
        "x-environment-id",
        64,
    )?;
    let runtime_type = normalize_registry_key(
        &header_opt(headers, "x-agent-runtime-type")
            .ok_or_else(|| ApiError::unauthorized("missing x-agent-runtime-type"))?,
        "x-agent-runtime-type",
        64,
    )?;
    let runtime_identity = normalize_registry_key(
        &header_opt(headers, "x-agent-runtime-id")
            .ok_or_else(|| ApiError::unauthorized("missing x-agent-runtime-id"))?,
        "x-agent-runtime-id",
        128,
    )?;
    let requested_agent_id = match header_opt(headers, "x-agent-id") {
        Some(value) => Some(normalize_registry_key(&value, "x-agent-id", 64)?),
        None => None,
    };

    let environment = state
        .environment_store
        .load_environment(tenant_id, &environment_id)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "environment registry unavailable while resolving agent identity: {err}"
            ))
        })?
        .ok_or_else(|| {
            ApiError::forbidden(format!(
                "unknown environment `{environment_id}` for tenant `{tenant_id}`"
            ))
        })?;
    if environment.status != "active" {
        return Err(ApiError::forbidden(format!(
            "environment `{environment_id}` is not active for tenant `{tenant_id}`"
        )));
    }

    let agent = state
        .agent_store
        .resolve_agent_runtime(
            tenant_id,
            &environment_id,
            &runtime_type,
            &runtime_identity,
            requested_agent_id.as_deref(),
        )
        .await
        .map_err(|err| ApiError::service_unavailable(format!(
            "agent registry unavailable while resolving runtime identity: {err}"
        )))?
        .ok_or_else(|| {
            ApiError::forbidden(format!(
                "no registered agent runtime binding matched tenant `{tenant_id}`, environment `{environment_id}`, runtime `{runtime_type}`, identity `{runtime_identity}`"
            ))
        })?;
    if agent.status != "active" {
        return Err(ApiError::forbidden(format!(
            "agent `{}` is not active for tenant `{tenant_id}`",
            agent.agent_id
        )));
    }

    Ok(Some(ResolvedAgentIdentity {
        agent_id: agent.agent_id,
        environment_id: agent.environment_id,
        environment_kind: environment.environment_kind,
        runtime_type: agent.runtime_type,
        runtime_identity: agent.runtime_identity,
        status: agent.status,
        trust_tier: agent.trust_tier,
        risk_tier: agent.risk_tier,
        owner_team: agent.owner_team,
    }))
}

async fn resolve_registered_agent_identity(
    state: &AppState,
    tenant_id: &str,
    agent_id: &str,
    environment_id: &str,
) -> Result<
    (
        ResolvedAgentIdentity,
        TenantEnvironmentRecord,
        TenantAgentRecord,
    ),
    ApiError,
> {
    let environment = state
        .environment_store
        .load_environment(tenant_id, environment_id)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "environment registry unavailable while resolving policy simulation: {err}"
            ))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "unknown environment `{environment_id}` for tenant `{tenant_id}`"
            ))
        })?;
    if environment.status != "active" {
        return Err(ApiError::forbidden(format!(
            "environment `{environment_id}` is not active for tenant `{tenant_id}`"
        )));
    }

    let agent = state
        .agent_store
        .load_agent(tenant_id, agent_id)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "agent registry unavailable while resolving policy simulation: {err}"
            ))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "unknown agent `{agent_id}` for tenant `{tenant_id}`"
            ))
        })?;
    if agent.environment_id != environment.environment_id {
        return Err(ApiError::bad_request(format!(
            "agent `{agent_id}` is bound to environment `{}` not `{environment_id}`",
            agent.environment_id
        )));
    }
    if agent.status != "active" {
        return Err(ApiError::forbidden(format!(
            "agent `{agent_id}` is not active for tenant `{tenant_id}`"
        )));
    }

    Ok((
        ResolvedAgentIdentity {
            agent_id: agent.agent_id.clone(),
            environment_id: agent.environment_id.clone(),
            environment_kind: environment.environment_kind.clone(),
            runtime_type: agent.runtime_type.clone(),
            runtime_identity: agent.runtime_identity.clone(),
            status: agent.status.clone(),
            trust_tier: agent.trust_tier.clone(),
            risk_tier: agent.risk_tier.clone(),
            owner_team: agent.owner_team.clone(),
        },
        environment,
        agent,
    ))
}

fn authorize_internal_provisioning_submitter(
    state: &AppState,
    tenant_id: &str,
    headers: &HeaderMap,
) -> Result<SubmitterIdentity, ApiError> {
    let submitter = state
        .auth
        .authenticate_submitter(tenant_id, headers, IngressChannel::Api)?;
    if !matches!(submitter.kind, SubmitterKind::InternalService) {
        return Err(ApiError::forbidden(
            "internal tenant api key provisioning requires internal_service submitter",
        ));
    }
    Ok(submitter)
}

fn normalize_registry_key(raw: &str, field: &str, max_len: usize) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request(format!("{field} is required")));
    }
    if trimmed.len() > max_len {
        return Err(ApiError::bad_request(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    let normalized = trimmed.to_ascii_lowercase();
    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '@' | '/'))
    {
        return Err(ApiError::bad_request(format!(
            "{field} contains unsupported characters"
        )));
    }
    Ok(normalized)
}

fn normalize_required_name(raw: &str, field: &str, max_len: usize) -> Result<String, ApiError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ApiError::bad_request(format!("{field} is required")));
    }
    if trimmed.len() > max_len {
        return Err(ApiError::bad_request(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(trimmed.to_owned())
}

fn normalize_optional_name(
    raw: Option<&str>,
    field: &str,
    max_len: usize,
    default: &str,
) -> Result<String, ApiError> {
    let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(default.to_owned());
    };
    if value.len() > max_len {
        return Err(ApiError::bad_request(format!(
            "{field} must be at most {max_len} characters"
        )));
    }
    Ok(value.to_owned())
}

fn normalize_environment_kind(raw: &str) -> Result<String, ApiError> {
    let normalized = normalize_registry_key(raw, "environment_kind", 32)?;
    match normalized.as_str() {
        "dev" | "development" => Ok("development".to_owned()),
        "staging" => Ok("staging".to_owned()),
        "sandbox" | "test" | "testing" => Ok("sandbox".to_owned()),
        "prod" | "production" => Ok("production".to_owned()),
        _ => Err(ApiError::bad_request(format!(
            "unsupported environment_kind `{normalized}`"
        ))),
    }
}

fn normalize_registry_status(raw: Option<&str>, field: &str) -> Result<String, ApiError> {
    let Some(raw) = raw else {
        return Ok("active".to_owned());
    };
    let normalized = normalize_registry_key(raw, field, 32)?;
    match normalized.as_str() {
        "active" => Ok("active".to_owned()),
        "suspended" => Ok("suspended".to_owned()),
        "decommissioned" | "retired" | "archived" => Ok("decommissioned".to_owned()),
        _ => Err(ApiError::bad_request(format!(
            "unsupported {field} `{normalized}`"
        ))),
    }
}

fn normalize_trust_tier(raw: Option<&str>) -> Result<String, ApiError> {
    let Some(raw) = raw else {
        return Ok("standard".to_owned());
    };
    let normalized = normalize_registry_key(raw, "trust_tier", 32)?;
    match normalized.as_str() {
        "low" => Ok("low".to_owned()),
        "standard" | "default" => Ok("standard".to_owned()),
        "high" => Ok("high".to_owned()),
        "privileged" => Ok("privileged".to_owned()),
        _ => Err(ApiError::bad_request(format!(
            "unsupported trust_tier `{normalized}`"
        ))),
    }
}

fn normalize_risk_tier(raw: Option<&str>) -> Result<String, ApiError> {
    let Some(raw) = raw else {
        return Ok("medium".to_owned());
    };
    let normalized = normalize_registry_key(raw, "risk_tier", 32)?;
    match normalized.as_str() {
        "low" => Ok("low".to_owned()),
        "medium" | "standard" => Ok("medium".to_owned()),
        "high" => Ok("high".to_owned()),
        "critical" => Ok("critical".to_owned()),
        _ => Err(ApiError::bad_request(format!(
            "unsupported risk_tier `{normalized}`"
        ))),
    }
}

fn normalize_connector_type(raw: &str) -> Result<String, ApiError> {
    normalize_registry_key(raw, "connector_type", 64)
}

fn normalize_connector_binding_name(raw: &str) -> Result<String, ApiError> {
    normalize_required_name(raw, "name", 128)
}

fn normalize_connector_config(config: Option<Value>) -> Result<Value, ApiError> {
    let value = config.unwrap_or_else(|| Value::Object(Map::new()));
    match value {
        Value::Object(_) => {
            if let Some(path) = find_secret_like_config_path(&value, "$") {
                return Err(ApiError::bad_request(format!(
                    "connector binding config must not contain raw secret-like field `{path}`; use the encrypted secrets map instead"
                )));
            }
            Ok(value)
        }
        _ => Err(ApiError::bad_request(
            "connector binding config must be a JSON object",
        )),
    }
}

fn find_secret_like_config_path(value: &Value, path: &str) -> Option<String> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let normalized = key.trim().to_ascii_lowercase();
                let exempt = normalized.ends_with("_ref")
                    || normalized.ends_with("_id")
                    || normalized.ends_with("_last4")
                    || normalized.ends_with("_masked");
                let secretish = normalized.contains("secret")
                    || normalized.contains("token")
                    || normalized.contains("password")
                    || normalized.contains("private_key")
                    || normalized.contains("privatekey")
                    || normalized.contains("api_key")
                    || normalized.contains("apikey");
                let child_path = format!("{path}.{key}");
                if secretish && !exempt {
                    return Some(child_path);
                }
                if let Some(found) = find_secret_like_config_path(child, &child_path) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => items.iter().enumerate().find_map(|(index, child)| {
            find_secret_like_config_path(child, &format!("{path}[{index}]"))
        }),
        _ => None,
    }
}

fn normalize_secret_field_name(raw: &str) -> Result<String, ApiError> {
    normalize_registry_key(raw, "secret field", 64)
}

fn normalize_connector_secret_values(
    secrets: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, ApiError> {
    if secrets.is_empty() {
        return Err(ApiError::bad_request(
            "connector binding secrets must include at least one entry",
        ));
    }
    let mut normalized = BTreeMap::new();
    for (raw_key, raw_value) in secrets {
        let key = normalize_secret_field_name(&raw_key)?;
        let value = raw_value.trim();
        if value.is_empty() {
            return Err(ApiError::bad_request(format!(
                "connector secret `{key}` must not be empty"
            )));
        }
        if value.len() > 8192 {
            return Err(ApiError::bad_request(format!(
                "connector secret `{key}` exceeds the maximum supported length"
            )));
        }
        normalized.insert(key, value.to_owned());
    }
    Ok(normalized)
}

async fn ensure_tenant_environment_active(
    state: &AppState,
    tenant_id: &str,
    environment_id: &str,
) -> Result<TenantEnvironmentRecord, ApiError> {
    let environment = state
        .environment_store
        .load_environment(tenant_id, environment_id)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to load tenant environment `{environment_id}`: {err}"
            ))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "unknown environment `{environment_id}` for tenant `{tenant_id}`"
            ))
        })?;
    if environment.status != "active" {
        return Err(ApiError::forbidden(format!(
            "environment `{environment_id}` is not active for tenant `{tenant_id}`"
        )));
    }
    Ok(environment)
}

fn environment_is_production(environment_kind: &str) -> bool {
    environment_kind.eq_ignore_ascii_case("production")
}

fn tenant_environment_to_view(record: &TenantEnvironmentRecord) -> TenantEnvironmentView {
    TenantEnvironmentView {
        tenant_id: record.tenant_id.clone(),
        environment_id: record.environment_id.clone(),
        name: record.name.clone(),
        environment_kind: record.environment_kind.clone(),
        is_production: environment_is_production(&record.environment_kind),
        status: record.status.clone(),
        created_by_principal_id: record.created_by_principal_id.clone(),
        updated_by_principal_id: record.updated_by_principal_id.clone(),
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

fn tenant_agent_to_view(record: &TenantAgentRecord) -> TenantAgentView {
    TenantAgentView {
        agent_id: record.agent_id.clone(),
        tenant_id: record.tenant_id.clone(),
        environment_id: record.environment_id.clone(),
        name: record.name.clone(),
        runtime_type: record.runtime_type.clone(),
        runtime_identity: record.runtime_identity.clone(),
        status: record.status.clone(),
        trust_tier: record.trust_tier.clone(),
        risk_tier: record.risk_tier.clone(),
        owner_team: record.owner_team.clone(),
        created_by_principal_id: record.created_by_principal_id.clone(),
        updated_by_principal_id: record.updated_by_principal_id.clone(),
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
    }
}

fn connector_binding_to_view(record: &ConnectorBindingRecord) -> ConnectorBindingView {
    ConnectorBindingView {
        tenant_id: record.tenant_id.clone(),
        environment_id: record.environment_id.clone(),
        binding_id: record.binding_id.clone(),
        connector_type: record.connector_type.clone(),
        name: record.name.clone(),
        status: record.status.clone(),
        secret_ref: record.secret_ref.clone(),
        current_secret_version: record.current_secret_version,
        config: record.config.clone(),
        secret_fields: record.secret_fields.clone(),
        created_by_principal_id: record.created_by_principal_id.clone(),
        updated_by_principal_id: record.updated_by_principal_id.clone(),
        created_at_ms: record.created_at_ms,
        updated_at_ms: record.updated_at_ms,
        rotated_at_ms: record.rotated_at_ms,
        revoked_at_ms: record.revoked_at_ms,
        revoked_reason: record.revoked_reason.clone(),
    }
}

fn tenant_policy_bundle_to_view(record: &TenantPolicyBundleRecord) -> TenantPolicyBundleView {
    TenantPolicyBundleView {
        tenant_id: record.tenant_id.clone(),
        bundle_id: record.bundle_id.clone(),
        version: record.version,
        label: record.label.clone(),
        status: record.status.clone(),
        template_ids: record.template_ids.clone(),
        rules: record.rules.clone(),
        created_by_principal_id: record.created_by_principal_id.clone(),
        published_by_principal_id: record.published_by_principal_id.clone(),
        created_at_ms: record.created_at_ms,
        published_at_ms: record.published_at_ms,
        rolled_back_from_bundle_id: record.rolled_back_from_bundle_id.clone(),
        rollback_reason: record.rollback_reason.clone(),
    }
}

async fn list_policy_templates(
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let tenant_id = tenant_id_from_headers(&headers)?;
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let templates = azums_policy_templates();
    Ok(Json(json!({
        "ok": true,
        "templates": templates
    })))
}

async fn create_tenant_policy_bundle(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<CreateTenantPolicyBundleRequest>,
) -> Result<Json<TenantPolicyBundleResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let bundle_id = normalize_registry_key(&payload.bundle_id, "bundle_id", 128)?;
    let label = normalize_required_name(&payload.label, "label", 128)?;
    let document = normalize_policy_bundle_document(TenantPolicyBundleDocument {
        template_ids: payload.template_ids,
        rules: payload.rules,
    })?;
    for template_id in &document.template_ids {
        if find_policy_template(template_id).is_none() {
            return Err(ApiError::bad_request(format!(
                "unknown azums policy template `{template_id}`"
            )));
        }
    }
    let bundle = state
        .policy_bundle_store
        .create_bundle(
            &tenant_id,
            &bundle_id,
            &label,
            &document,
            &submitter.principal_id,
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to create tenant policy bundle: {err}"))
        })?;

    Ok(Json(TenantPolicyBundleResponse {
        ok: true,
        bundle: tenant_policy_bundle_to_view(&bundle),
    }))
}

async fn list_tenant_policy_bundles(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<ListTenantPolicyBundlesQuery>,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let bundles = state
        .policy_bundle_store
        .list_bundles(&tenant_id, limit)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to list tenant policy bundles: {err}"))
        })?;
    let views = bundles
        .iter()
        .map(tenant_policy_bundle_to_view)
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "ok": true,
        "bundles": views,
        "limit": limit
    })))
}

async fn get_tenant_policy_bundle(
    State(state): State<AppState>,
    Path((tenant_id, bundle_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<TenantPolicyBundleResponse>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let bundle_id = normalize_registry_key(&bundle_id, "bundle_id", 128)?;
    let bundle = state
        .policy_bundle_store
        .load_bundle(&tenant_id, &bundle_id)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to load tenant policy bundle: {err}"))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "tenant policy bundle `{bundle_id}` not found for tenant `{tenant_id}`"
            ))
        })?;
    Ok(Json(TenantPolicyBundleResponse {
        ok: true,
        bundle: tenant_policy_bundle_to_view(&bundle),
    }))
}

async fn publish_tenant_policy_bundle(
    State(state): State<AppState>,
    Path((tenant_id, bundle_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<TenantPolicyBundleResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let bundle_id = normalize_registry_key(&bundle_id, "bundle_id", 128)?;
    let bundle = state
        .policy_bundle_store
        .publish_bundle(
            &tenant_id,
            &bundle_id,
            &submitter.principal_id,
            state.clock.now_ms(),
            None,
            None,
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to publish tenant policy bundle: {err}"))
        })?;

    Ok(Json(TenantPolicyBundleResponse {
        ok: true,
        bundle: tenant_policy_bundle_to_view(&bundle),
    }))
}

async fn rollback_tenant_policy_bundle(
    State(state): State<AppState>,
    Path((tenant_id, _bundle_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<RollbackTenantPolicyBundleRequest>,
) -> Result<Json<TenantPolicyBundleResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let target_bundle_id =
        normalize_registry_key(&payload.target_bundle_id, "target_bundle_id", 128)?;
    let current = state
        .policy_bundle_store
        .load_published_bundle(&tenant_id)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to load current tenant policy bundle for rollback: {err}"
            ))
        })?;
    let rollback_reason = payload
        .rollback_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("rollback requested")
        .to_owned();
    let bundle = state
        .policy_bundle_store
        .publish_bundle(
            &tenant_id,
            &target_bundle_id,
            &submitter.principal_id,
            state.clock.now_ms(),
            current.as_ref().map(|bundle| bundle.bundle_id.as_str()),
            Some(rollback_reason.as_str()),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to roll back tenant policy bundle: {err}"
            ))
        })?;

    Ok(Json(TenantPolicyBundleResponse {
        ok: true,
        bundle: tenant_policy_bundle_to_view(&bundle),
    }))
}

async fn simulate_tenant_policy(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<SimulateTenantPolicyRequest>,
) -> Result<Json<PolicySimulationResponse>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let agent_id = normalize_registry_key(&payload.action.agent_id, "agent_id", 64)?;
    let environment_id =
        normalize_registry_key(&payload.action.environment_id, "environment_id", 64)?;
    let (resolved_agent, environment, agent) =
        resolve_registered_agent_identity(&state, &tenant_id, &agent_id, &environment_id).await?;

    let selected_bundle = match payload.bundle_id.as_deref() {
        Some(raw_bundle_id) => {
            let bundle_id = normalize_registry_key(raw_bundle_id, "bundle_id", 128)?;
            state
                .policy_bundle_store
                .load_bundle(&tenant_id, &bundle_id)
                .await
                .map_err(|err| {
                    ApiError::service_unavailable(format!(
                        "failed to load tenant policy bundle for simulation: {err}"
                    ))
                })?
                .ok_or_else(|| {
                    ApiError::not_found(format!(
                        "tenant policy bundle `{bundle_id}` not found for tenant `{tenant_id}`"
                    ))
                })?
        }
        None => match state
            .policy_bundle_store
            .load_published_bundle(&tenant_id)
            .await
        {
            Ok(bundle) => bundle.ok_or_else(|| {
                ApiError::not_found(format!(
                    "no published tenant policy bundle exists for tenant `{tenant_id}`"
                ))
            })?,
            Err(err) => {
                return Err(ApiError::service_unavailable(format!(
                    "failed to load published tenant policy bundle for simulation: {err}"
                )))
            }
        },
    };

    let normalized_action =
        normalize_agent_action_for_policy_simulation(payload.action, &tenant_id, &resolved_agent)?;
    let decision = evaluate_policy_layers(
        &build_policy_evaluation_context(&normalized_action, &resolved_agent, state.clock.now_ms()),
        Some(&selected_bundle),
    )?;

    Ok(Json(PolicySimulationResponse {
        ok: true,
        bundle: Some(tenant_policy_bundle_to_view(&selected_bundle)),
        decision,
        execution_mode: normalized_action.execution_mode.as_str().to_owned(),
        execution_owner: normalized_action.execution_mode.owner_label().to_owned(),
        resolved_agent: tenant_agent_to_view(&agent),
        environment: tenant_environment_to_view(&environment),
    }))
}

async fn list_tenant_approvals(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<ListApprovalsQuery>,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let state_filter = normalize_approval_state_filter(query.state.as_deref())?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let approvals = state
        .approval_store
        .list_requests(
            &tenant_id,
            state_filter.as_deref(),
            limit,
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to list approval requests: {err}"))
        })?
        .iter()
        .map(approval_request_to_view)
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "ok": true,
        "approvals": approvals,
        "limit": limit
    })))
}

async fn get_tenant_approval(
    State(state): State<AppState>,
    Path((tenant_id, approval_request_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<ApprovalRequestResponse>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let approval_request_id =
        normalize_registry_key(&approval_request_id, "approval_request_id", 128)?;
    let approval = state
        .approval_store
        .load_request_fresh(&tenant_id, &approval_request_id, state.clock.now_ms())
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to load approval request: {err}"))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "approval request `{approval_request_id}` not found for tenant `{tenant_id}`"
            ))
        })?;
    let grant = state
        .capability_grant_store
        .load_grant_for_approval(&tenant_id, &approval_request_id, state.clock.now_ms())
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to load capability grant for approval: {err}"
            ))
        })?;
    Ok(Json(ApprovalRequestResponse {
        ok: true,
        approval: approval_request_to_view(&approval),
        execution_mode: approval.execution_mode.clone(),
        execution_owner: AgentExecutionMode::parse(&approval.execution_mode)
            .map(|value| value.owner_label().to_owned())
            .unwrap_or_else(|_| "unknown".to_owned()),
        runtime_authorized: false,
        execution: None,
        execution_error: None,
        grant: grant.as_ref().map(capability_grant_to_view),
        grant_error: None,
    }))
}

async fn apply_tenant_approval_decision(
    state: &AppState,
    tenant_id: &str,
    approval_request_id: &str,
    decision: ApprovalDecisionKind,
    actor_id: &str,
    actor_source: &str,
    note: Option<&str>,
    grant_request: Option<&ApprovalGrantRequest>,
) -> Result<ApprovalRequestResponse, ApiError> {
    if grant_request.is_some() && !matches!(decision, ApprovalDecisionKind::Approve) {
        return Err(ApiError::bad_request(
            "grant may only be specified when approving an action request",
        ));
    }
    let pending_grant = if let Some(grant_request) = grant_request {
        let pending_approval = state
            .approval_store
            .load_request_fresh(tenant_id, approval_request_id, state.clock.now_ms())
            .await
            .map_err(|err| {
                ApiError::service_unavailable(format!(
                    "failed to load approval request for grant validation: {err}"
                ))
            })?
            .ok_or_else(|| {
                ApiError::not_found(format!(
                    "approval request `{approval_request_id}` not found for tenant `{tenant_id}`"
                ))
            })?;
        Some(normalize_grant_spec_from_approval(
            &pending_approval,
            grant_request,
            state.grant_workflow.as_ref(),
            actor_id,
            actor_source,
            state.clock.now_ms(),
        )?)
    } else {
        None
    };
    let outcome = state
        .approval_store
        .apply_decision(
            tenant_id,
            approval_request_id,
            decision,
            actor_id,
            actor_source,
            note,
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to apply approval decision: {err}"))
        })?;

    state
        .agent_action_idempotency_store
        .mark_approval_state(
            &outcome.approval.tenant_id,
            &outcome.approval.agent_id,
            &outcome.approval.environment_id,
            &outcome.approval.idempotency_key,
            outcome.approval.status,
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to update approval idempotency state: {err}"
            ))
        })?;

    let mut execution = None;
    let mut execution_error = None;
    let mut grant = None;
    let mut grant_error = None;
    if outcome.terminal_reached && matches!(outcome.approval.status, ApprovalState::Approved) {
        if let Some(grant_record) = pending_grant.as_ref() {
            match state
                .capability_grant_store
                .create_grant(grant_record)
                .await
            {
                Ok(created) => {
                    let _ = state
                        .approval_store
                        .insert_event(
                            tenant_id,
                            approval_request_id,
                            "capability_grant_created",
                            Some(actor_id),
                            Some(actor_source),
                            json!({
                                "grant_id": created.grant_id,
                                "expires_at_ms": created.expires_at_ms,
                                "max_uses": created.max_uses,
                                "granted_scope": created.granted_scope,
                                "resource_binding": created.resource_binding,
                                "amount_ceiling": created.amount_ceiling,
                            }),
                            state.clock.now_ms(),
                        )
                        .await;
                    grant = Some(created);
                }
                Err(err) => {
                    let message = format!("failed to create capability grant: {err}");
                    let _ = state
                        .approval_store
                        .insert_event(
                            tenant_id,
                            approval_request_id,
                            "capability_grant_create_failed",
                            Some(actor_id),
                            Some(actor_source),
                            json!({ "error": message }),
                            state.clock.now_ms(),
                        )
                        .await;
                    warn!(
                        tenant_id = %tenant_id,
                        approval_request_id = %approval_request_id,
                        error = %message,
                        "failed to create capability grant after approval"
                    );
                    grant_error = Some(message);
                }
            }
        }
    }
    if outcome.terminal_reached && matches!(outcome.approval.status, ApprovalState::Approved) {
        let execution_mode = AgentExecutionMode::parse(&outcome.approval.execution_mode)?;
        if execution_mode == AgentExecutionMode::ModeBScopedRuntime {
            if let Err(err) =
                authorize_approved_runtime_action(state, &outcome.approval, grant.as_ref()).await
            {
                execution_error = Some(err.message.clone());
                let _ = state
                    .approval_store
                    .insert_event(
                        tenant_id,
                        approval_request_id,
                        "runtime_authorization_failed",
                        Some(actor_id),
                        Some(actor_source),
                        json!({ "error": err.message }),
                        state.clock.now_ms(),
                    )
                    .await;
            }
        } else {
            match execute_approved_agent_action(state, &outcome.approval, grant.as_ref()).await {
                Ok(response) => execution = Some(response),
                Err(err) => {
                    execution_error = Some(err.message.clone());
                    let _ = state
                        .approval_store
                        .insert_event(
                            tenant_id,
                            approval_request_id,
                            "execution_submit_failed",
                            Some(actor_id),
                            Some(actor_source),
                            json!({ "error": err.message }),
                            state.clock.now_ms(),
                        )
                        .await;
                }
            }
        }
    }

    let approval = state
        .approval_store
        .load_request_fresh(tenant_id, approval_request_id, state.clock.now_ms())
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to reload approval request: {err}"))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "approval request `{approval_request_id}` not found for tenant `{tenant_id}`"
            ))
        })?;
    if grant.is_none() {
        grant = state
            .capability_grant_store
            .load_grant_for_approval(tenant_id, approval_request_id, state.clock.now_ms())
            .await
            .map_err(|err| {
                ApiError::service_unavailable(format!(
                    "failed to reload capability grant for approval: {err}"
                ))
            })?;
    }

    Ok(ApprovalRequestResponse {
        ok: true,
        approval: approval_request_to_view(&approval),
        execution_mode: approval.execution_mode.clone(),
        execution_owner: AgentExecutionMode::parse(&approval.execution_mode)
            .map(|value| value.owner_label().to_owned())
            .unwrap_or_else(|_| "unknown".to_owned()),
        runtime_authorized: execution.is_none()
            && execution_error.is_none()
            && approval.status == ApprovalState::Approved
            && AgentExecutionMode::parse(&approval.execution_mode)
                .is_ok_and(|mode| mode == AgentExecutionMode::ModeBScopedRuntime),
        execution,
        execution_error,
        grant: grant.as_ref().map(capability_grant_to_view),
        grant_error,
    })
}

async fn approve_tenant_approval(
    State(state): State<AppState>,
    Path((tenant_id, approval_request_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<Option<ApprovalActionRequest>>,
) -> Result<Json<ApprovalRequestResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let actor_id = parse_approval_actor_id(
        &submitter,
        payload.as_ref().and_then(|value| value.actor_id.as_deref()),
    )?;
    let approval_request_id =
        normalize_registry_key(&approval_request_id, "approval_request_id", 128)?;
    let response = apply_tenant_approval_decision(
        &state,
        &tenant_id,
        &approval_request_id,
        ApprovalDecisionKind::Approve,
        &actor_id,
        "internal_api",
        payload.as_ref().and_then(|value| value.note.as_deref()),
        payload.as_ref().and_then(|value| value.grant.as_ref()),
    )
    .await?;
    Ok(Json(response))
}

async fn reject_tenant_approval(
    State(state): State<AppState>,
    Path((tenant_id, approval_request_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<Option<ApprovalActionRequest>>,
) -> Result<Json<ApprovalRequestResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    if payload
        .as_ref()
        .and_then(|value| value.grant.as_ref())
        .is_some()
    {
        return Err(ApiError::bad_request(
            "grant may only be specified when approving an action request",
        ));
    }
    let actor_id = parse_approval_actor_id(
        &submitter,
        payload.as_ref().and_then(|value| value.actor_id.as_deref()),
    )?;
    let approval_request_id =
        normalize_registry_key(&approval_request_id, "approval_request_id", 128)?;
    let response = apply_tenant_approval_decision(
        &state,
        &tenant_id,
        &approval_request_id,
        ApprovalDecisionKind::Reject,
        &actor_id,
        "internal_api",
        payload.as_ref().and_then(|value| value.note.as_deref()),
        None,
    )
    .await?;
    Ok(Json(response))
}

async fn escalate_tenant_approval(
    State(state): State<AppState>,
    Path((tenant_id, approval_request_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<Option<ApprovalActionRequest>>,
) -> Result<Json<ApprovalRequestResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    if payload
        .as_ref()
        .and_then(|value| value.grant.as_ref())
        .is_some()
    {
        return Err(ApiError::bad_request(
            "grant may only be specified when approving an action request",
        ));
    }
    let actor_id = parse_approval_actor_id(
        &submitter,
        payload.as_ref().and_then(|value| value.actor_id.as_deref()),
    )?;
    let approval_request_id =
        normalize_registry_key(&approval_request_id, "approval_request_id", 128)?;
    let response = apply_tenant_approval_decision(
        &state,
        &tenant_id,
        &approval_request_id,
        ApprovalDecisionKind::Escalate,
        &actor_id,
        "internal_api",
        payload.as_ref().and_then(|value| value.note.as_deref()),
        None,
    )
    .await?;
    Ok(Json(response))
}

async fn handle_slack_approval_callback(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, ApiError> {
    verify_slack_signature(&state, &headers, &body)?;
    let payload = parse_slack_approval_payload(&body)?;
    let response = apply_tenant_approval_decision(
        &state,
        &tenant_id,
        &payload.approval_request_id,
        payload.decision,
        &payload.user_id,
        "slack",
        payload.user_name.as_deref(),
        None,
    )
    .await?;

    let mut text = format!(
        "Approval `{}` is now `{}`.",
        response.approval.approval_request_id, response.approval.status
    );
    if let Some(execution) = response.execution.as_ref() {
        text.push_str(&format!(
            " Execution submitted as intent `{}` job `{}`.",
            execution.intent_id, execution.job_id
        ));
    }
    if let Some(error) = response.execution_error.as_ref() {
        text.push_str(&format!(" Execution handoff failed: {error}."));
    }
    if let Some(error) = response.grant_error.as_ref() {
        text.push_str(&format!(" Capability grant creation failed: {error}."));
    }
    Ok(Json(json!({
        "response_type": "ephemeral",
        "text": text
    })))
}

async fn list_capability_grants(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<ListCapabilityGrantsQuery>,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let environment_id = query
        .environment_id
        .as_deref()
        .map(|value| normalize_registry_key(value, "environment_id", 64))
        .transpose()?;
    let agent_id = query
        .agent_id
        .as_deref()
        .map(|value| normalize_registry_key(value, "agent_id", 64))
        .transpose()?;
    let status = normalize_capability_grant_status_filter(query.status.as_deref())?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let grants = state
        .capability_grant_store
        .list_grants(
            &tenant_id,
            environment_id.as_deref(),
            agent_id.as_deref(),
            status.as_deref(),
            limit,
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to list capability grants: {err}"))
        })?
        .iter()
        .map(capability_grant_to_view)
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "ok": true,
        "grants": grants,
        "limit": limit,
    })))
}

async fn get_capability_grant(
    State(state): State<AppState>,
    Path((tenant_id, grant_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<CapabilityGrantResponse>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let grant_id = normalize_registry_key(&grant_id, "grant_id", 128)?;
    let grant = state
        .capability_grant_store
        .load_grant_fresh(&tenant_id, &grant_id, state.clock.now_ms())
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to load capability grant: {err}"))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "capability grant `{grant_id}` not found for tenant `{tenant_id}`"
            ))
        })?;
    Ok(Json(CapabilityGrantResponse {
        ok: true,
        grant: capability_grant_to_view(&grant),
    }))
}

async fn revoke_capability_grant(
    State(state): State<AppState>,
    Path((tenant_id, grant_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<Option<RevokeCapabilityGrantRequest>>,
) -> Result<Json<CapabilityGrantResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let grant_id = normalize_registry_key(&grant_id, "grant_id", 128)?;
    let actor_id = payload
        .as_ref()
        .and_then(|value| value.revoked_by_actor_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| normalize_registry_key(value, "revoked_by_actor_id", 128))
        .transpose()?
        .unwrap_or_else(|| submitter.principal_id.clone());
    let reason = payload
        .as_ref()
        .and_then(|value| value.reason.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let grant = state
        .capability_grant_store
        .revoke_grant(
            &tenant_id,
            &grant_id,
            &actor_id,
            reason.as_deref(),
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to revoke capability grant: {err}"))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "capability grant `{grant_id}` not found for tenant `{tenant_id}`"
            ))
        })?;
    Ok(Json(CapabilityGrantResponse {
        ok: true,
        grant: capability_grant_to_view(&grant),
    }))
}

async fn create_connector_binding(
    State(state): State<AppState>,
    Path((tenant_id, environment_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<CreateConnectorBindingRequest>,
) -> Result<Json<ConnectorBindingResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let environment_id = normalize_registry_key(&environment_id, "environment_id", 64)?;
    let _environment =
        ensure_tenant_environment_active(&state, &tenant_id, &environment_id).await?;
    let binding_id = normalize_registry_key(&payload.binding_id, "binding_id", 128)?;
    let connector_type = normalize_connector_type(&payload.connector_type)?;
    let name = normalize_connector_binding_name(&payload.name)?;
    let config = normalize_connector_config(payload.config)?;
    let secret_values = normalize_connector_secret_values(payload.secrets)?;
    let created_by_principal_id = payload
        .created_by_principal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(submitter.principal_id.as_str())
        .to_owned();
    let record = ConnectorBindingCreateRecord {
        tenant_id: tenant_id.clone(),
        environment_id: environment_id.clone(),
        binding_id,
        connector_type,
        name,
        secret_ref: format!("secretref_{}", Uuid::new_v4().simple()),
        config,
        secret_values,
        created_by_principal_id,
        created_at_ms: state.clock.now_ms(),
    };
    let binding = state
        .connector_secret_broker
        .create_binding(&record)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to create connector binding: {err}"))
        })?;
    Ok(Json(ConnectorBindingResponse {
        ok: true,
        binding: connector_binding_to_view(&binding),
    }))
}

async fn list_connector_bindings(
    State(state): State<AppState>,
    Path((tenant_id, environment_id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<ListConnectorBindingsQuery>,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let environment_id = normalize_registry_key(&environment_id, "environment_id", 64)?;
    let _environment =
        ensure_tenant_environment_active(&state, &tenant_id, &environment_id).await?;
    let include_inactive = query.include_inactive.unwrap_or(false);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let bindings = state
        .connector_binding_store
        .list_bindings(&tenant_id, &environment_id, include_inactive, limit)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to list connector bindings: {err}"))
        })?
        .iter()
        .map(connector_binding_to_view)
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "ok": true,
        "bindings": bindings,
        "limit": limit
    })))
}

async fn get_connector_binding(
    State(state): State<AppState>,
    Path((tenant_id, environment_id, binding_id)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<Json<ConnectorBindingResponse>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let environment_id = normalize_registry_key(&environment_id, "environment_id", 64)?;
    let binding_id = normalize_registry_key(&binding_id, "binding_id", 128)?;
    let _environment =
        ensure_tenant_environment_active(&state, &tenant_id, &environment_id).await?;
    let binding = state
        .connector_binding_store
        .load_binding(&tenant_id, &environment_id, &binding_id)
        .await
        .map_err(|err| ApiError::service_unavailable(format!(
            "failed to load connector binding: {err}"
        )))?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "connector binding `{binding_id}` not found for tenant `{tenant_id}` environment `{environment_id}`"
            ))
        })?;
    Ok(Json(ConnectorBindingResponse {
        ok: true,
        binding: connector_binding_to_view(&binding),
    }))
}

async fn rotate_connector_binding(
    State(state): State<AppState>,
    Path((tenant_id, environment_id, binding_id)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(payload): Json<RotateConnectorBindingRequest>,
) -> Result<Json<ConnectorBindingResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let environment_id = normalize_registry_key(&environment_id, "environment_id", 64)?;
    let binding_id = normalize_registry_key(&binding_id, "binding_id", 128)?;
    let _environment =
        ensure_tenant_environment_active(&state, &tenant_id, &environment_id).await?;
    let secret_values = normalize_connector_secret_values(payload.secrets)?;
    let rotated_by_principal_id = payload
        .rotated_by_principal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(submitter.principal_id.as_str())
        .to_owned();
    let binding = state
        .connector_secret_broker
        .rotate_binding(&ConnectorBindingRotationRecord {
            tenant_id: tenant_id.clone(),
            environment_id: environment_id.clone(),
            binding_id,
            secret_values,
            rotated_by_principal_id,
            rotated_at_ms: state.clock.now_ms(),
            rotation_reason: payload
                .reason
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned),
        })
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to rotate connector binding: {err}"))
        })?;
    Ok(Json(ConnectorBindingResponse {
        ok: true,
        binding: connector_binding_to_view(&binding),
    }))
}

async fn revoke_connector_binding(
    State(state): State<AppState>,
    Path((tenant_id, environment_id, binding_id)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(payload): Json<Option<RevokeConnectorBindingRequest>>,
) -> Result<Json<Value>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let environment_id = normalize_registry_key(&environment_id, "environment_id", 64)?;
    let binding_id = normalize_registry_key(&binding_id, "binding_id", 128)?;
    let _environment =
        ensure_tenant_environment_active(&state, &tenant_id, &environment_id).await?;
    let revoked_by_principal_id = payload
        .as_ref()
        .and_then(|value| value.revoked_by_principal_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(submitter.principal_id.as_str())
        .to_owned();
    let revoked_reason = payload
        .as_ref()
        .and_then(|value| value.reason.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let revoked = state
        .connector_binding_store
        .revoke_binding(
            &tenant_id,
            &environment_id,
            &binding_id,
            &revoked_by_principal_id,
            revoked_reason.as_deref(),
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to revoke connector binding: {err}"))
        })?;
    if !revoked {
        return Err(ApiError::not_found(format!(
            "connector binding `{binding_id}` not found for tenant `{tenant_id}` environment `{environment_id}`"
        )));
    }
    Ok(Json(json!({
        "ok": true,
        "binding_id": binding_id,
        "status": "revoked"
    })))
}

async fn broker_use_connector_binding(
    State(state): State<AppState>,
    Path((tenant_id, environment_id, binding_id)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(payload): Json<BrokerConnectorBindingUsePayload>,
) -> Result<Json<ConnectorBindingSecretUseResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let environment_id = normalize_registry_key(&environment_id, "environment_id", 64)?;
    let binding_id = normalize_registry_key(&binding_id, "binding_id", 128)?;
    let _environment =
        ensure_tenant_environment_active(&state, &tenant_id, &environment_id).await?;
    let purpose = normalize_required_name(&payload.purpose, "purpose", 128)?;
    let actor_id = payload
        .actor_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| normalize_registry_key(value, "actor_id", 128))
        .transpose()?
        .unwrap_or_else(|| submitter.principal_id.clone());
    let actor_kind = payload
        .actor_kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| normalize_registry_key(value, "actor_kind", 64))
        .transpose()?
        .unwrap_or_else(|| submitter.kind.as_str().to_owned());
    let use_request = BrokerConnectorBindingUseRequest {
        tenant_id: tenant_id.clone(),
        environment_id: environment_id.clone(),
        binding_id,
        actor_id,
        actor_kind,
        purpose,
        request_id: payload.request_id,
        action_request_id: payload.action_request_id,
        approval_request_id: payload.approval_request_id,
        intent_id: payload.intent_id,
        job_id: payload.job_id,
        correlation_id: payload.correlation_id,
        used_at_ms: state.clock.now_ms(),
    };
    let (binding, secrets) = state
        .connector_secret_broker
        .resolve_for_use(&use_request)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to broker connector secret use: {err}"))
        })?;
    Ok(Json(ConnectorBindingSecretUseResponse {
        ok: true,
        binding: connector_binding_to_view(&binding),
        resolved_secret_version: binding.current_secret_version,
        secrets,
    }))
}

async fn upsert_tenant_environment(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpsertTenantEnvironmentRequest>,
) -> Result<Json<TenantEnvironmentResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let now_ms = state.clock.now_ms();
    let environment_id = normalize_registry_key(&payload.environment_id, "environment_id", 64)?;
    let name = normalize_required_name(&payload.name, "name", 128)?;
    let environment_kind = normalize_environment_kind(&payload.environment_kind)?;
    let status = normalize_registry_status(payload.status.as_deref(), "status")?;
    let created_by_principal_id = payload
        .created_by_principal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(submitter.principal_id.as_str())
        .to_owned();
    let updated_by_principal_id = payload
        .updated_by_principal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(submitter.principal_id.as_str())
        .to_owned();

    let environment = state
        .environment_store
        .upsert_environment(&TenantEnvironmentUpsertRecord {
            tenant_id,
            environment_id,
            name,
            environment_kind,
            status,
            created_by_principal_id,
            updated_by_principal_id,
            now_ms,
        })
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to persist tenant environment: {err}"))
        })?;

    Ok(Json(TenantEnvironmentResponse {
        ok: true,
        environment: tenant_environment_to_view(&environment),
    }))
}

async fn list_tenant_environments(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<ListTenantEnvironmentsQuery>,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let include_inactive = query.include_inactive.unwrap_or(false);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let rows = state
        .environment_store
        .list_environments(&tenant_id, include_inactive, limit)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to list tenant environments: {err}"))
        })?;

    let environments = rows
        .iter()
        .map(tenant_environment_to_view)
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "ok": true,
        "environments": environments,
        "limit": limit
    })))
}

async fn get_tenant_environment(
    State(state): State<AppState>,
    Path((tenant_id, environment_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<TenantEnvironmentResponse>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let environment_id = normalize_registry_key(&environment_id, "environment_id", 64)?;
    let environment = state
        .environment_store
        .load_environment(&tenant_id, &environment_id)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to load tenant environment: {err}"))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "tenant environment `{environment_id}` not found for tenant `{tenant_id}`"
            ))
        })?;

    Ok(Json(TenantEnvironmentResponse {
        ok: true,
        environment: tenant_environment_to_view(&environment),
    }))
}

async fn upsert_tenant_agent(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpsertTenantAgentRequest>,
) -> Result<Json<TenantAgentResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let now_ms = state.clock.now_ms();
    let agent_id = normalize_registry_key(&payload.agent_id, "agent_id", 64)?;
    let environment_id = normalize_registry_key(&payload.environment_id, "environment_id", 64)?;
    let name = normalize_required_name(&payload.name, "name", 128)?;
    let runtime_type = normalize_registry_key(&payload.runtime_type, "runtime_type", 64)?;
    let runtime_identity =
        normalize_registry_key(&payload.runtime_identity, "runtime_identity", 128)?;
    let status = normalize_registry_status(payload.status.as_deref(), "status")?;
    let trust_tier = normalize_trust_tier(payload.trust_tier.as_deref())?;
    let risk_tier = normalize_risk_tier(payload.risk_tier.as_deref())?;
    let owner_team =
        normalize_optional_name(payload.owner_team.as_deref(), "owner_team", 128, "platform")?;
    let created_by_principal_id = payload
        .created_by_principal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(submitter.principal_id.as_str())
        .to_owned();
    let updated_by_principal_id = payload
        .updated_by_principal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(submitter.principal_id.as_str())
        .to_owned();

    state
        .environment_store
        .load_environment(&tenant_id, &environment_id)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to load tenant environment for agent binding: {err}"
            ))
        })?
        .ok_or_else(|| {
            ApiError::bad_request(format!(
                "unknown environment `{environment_id}` for tenant `{tenant_id}`"
            ))
        })?;

    let agent = state
        .agent_store
        .upsert_agent(&TenantAgentUpsertRecord {
            agent_id,
            tenant_id,
            environment_id,
            name,
            runtime_type,
            runtime_identity,
            status,
            trust_tier,
            risk_tier,
            owner_team,
            created_by_principal_id,
            updated_by_principal_id,
            now_ms,
        })
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to persist tenant agent: {err}"))
        })?;

    Ok(Json(TenantAgentResponse {
        ok: true,
        agent: tenant_agent_to_view(&agent),
    }))
}

async fn list_tenant_agents(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<ListTenantAgentsQuery>,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let environment_id = match query.environment_id.as_deref() {
        Some(raw) => Some(normalize_registry_key(raw, "environment_id", 64)?),
        None => None,
    };
    let include_inactive = query.include_inactive.unwrap_or(false);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let rows = state
        .agent_store
        .list_agents(
            &tenant_id,
            environment_id.as_deref(),
            include_inactive,
            limit,
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to list tenant agents: {err}"))
        })?;

    let agents = rows.iter().map(tenant_agent_to_view).collect::<Vec<_>>();

    Ok(Json(json!({
        "ok": true,
        "agents": agents,
        "limit": limit,
        "environment_id": environment_id
    })))
}

async fn get_tenant_agent(
    State(state): State<AppState>,
    Path((tenant_id, agent_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<TenantAgentResponse>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let agent_id = normalize_registry_key(&agent_id, "agent_id", 64)?;
    let agent = state
        .agent_store
        .load_agent(&tenant_id, &agent_id)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to load tenant agent: {err}"))
        })?
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "tenant agent `{agent_id}` not found for tenant `{tenant_id}`"
            ))
        })?;

    Ok(Json(TenantAgentResponse {
        ok: true,
        agent: tenant_agent_to_view(&agent),
    }))
}

async fn register_tenant_api_key(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<RegisterTenantApiKeyRequest>,
) -> Result<Json<Value>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;

    let key_id = payload.key_id.trim();
    if key_id.is_empty() {
        return Err(ApiError::bad_request("key_id is required"));
    }
    let key_value = payload.key_value.trim();
    if key_value.is_empty() {
        return Err(ApiError::bad_request("key_value is required"));
    }

    let record = TenantApiKeyProvisionRequest {
        key_id: key_id.to_owned(),
        tenant_id: tenant_id.clone(),
        label: payload
            .label
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("default")
            .to_owned(),
        key_hash: hash_tenant_api_key(key_value),
        key_prefix: payload.key_prefix.trim().to_owned(),
        key_last4: payload.key_last4.trim().to_owned(),
        created_by_principal_id: payload
            .created_by_principal_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(submitter.principal_id.as_str())
            .to_owned(),
        created_at_ms: payload
            .created_at_ms
            .unwrap_or_else(|| state.clock.now_ms()),
    };

    if record.key_prefix.is_empty() || record.key_last4.is_empty() {
        return Err(ApiError::bad_request(
            "key_prefix and key_last4 are required",
        ));
    }

    state
        .api_key_store
        .upsert_api_key(&record)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to upsert tenant api key: {err}"))
        })?;

    Ok(Json(json!({ "ok": true })))
}

async fn list_tenant_api_keys(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<ListTenantApiKeysQuery>,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let include_inactive = query.include_inactive.unwrap_or(false);
    let limit = query.limit.unwrap_or(100).clamp(1, 200);

    let rows = state
        .api_key_store
        .list_api_keys(&tenant_id, include_inactive, limit)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to list tenant api keys: {err}"))
        })?;

    let keys = rows
        .into_iter()
        .map(|row| TenantApiKeyRecordView {
            key_id: row.key_id,
            tenant_id: row.tenant_id,
            label: row.label,
            key_prefix: row.key_prefix,
            key_last4: row.key_last4,
            created_by_principal_id: row.created_by_principal_id,
            created_at_ms: row.created_at_ms,
            revoked_at_ms: row.revoked_at_ms,
            last_used_at_ms: row.last_used_at_ms,
        })
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "ok": true,
        "keys": keys,
        "limit": limit
    })))
}

async fn revoke_tenant_api_key(
    State(state): State<AppState>,
    Path((tenant_id, key_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let key_id = key_id.trim();
    if key_id.is_empty() {
        return Err(ApiError::bad_request("key_id is required"));
    }

    let revoked = state
        .api_key_store
        .revoke_api_key(&tenant_id, key_id, state.clock.now_ms())
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to revoke tenant api key: {err}"))
        })?;

    if !revoked {
        return Err(ApiError::not_found(format!(
            "tenant api key `{key_id}` not found for tenant `{tenant_id}`"
        )));
    }

    Ok(Json(json!({ "ok": true })))
}

async fn register_tenant_webhook_key(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<RegisterTenantWebhookKeyRequest>,
) -> Result<Json<Value>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let source = normalize_webhook_source(payload.source.as_deref()).ok_or_else(|| {
        ApiError::bad_request("source must contain only letters, numbers, '.', '-', '_' or ':'")
    })?;
    let grace_seconds = payload.grace_seconds.unwrap_or(900).clamp(0, 86_400);
    let now_ms = state.clock.now_ms();
    let created_by_principal_id = payload
        .created_by_principal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(submitter.principal_id.as_str());

    let (record, rotated_previous_keys, previous_keys_valid_until_ms) = state
        .webhook_key_store
        .issue_webhook_key(
            &tenant_id,
            &source,
            created_by_principal_id,
            now_ms,
            grace_seconds,
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to issue webhook key: {err}"))
        })?;

    Ok(Json(json!({
        "ok": true,
        "webhook_key": {
            "key_id": record.key_id,
            "tenant_id": record.tenant_id,
            "source": record.source,
            "secret": record.secret_value,
            "secret_last4": record.secret_last4,
            "created_by_principal_id": record.created_by_principal_id,
            "created_at_ms": record.created_at_ms,
        },
        "rotation": {
            "rotated_previous_keys": rotated_previous_keys,
            "previous_keys_valid_until_ms": previous_keys_valid_until_ms,
            "grace_seconds": grace_seconds,
        }
    })))
}

async fn list_tenant_webhook_keys(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<ListTenantWebhookKeysQuery>,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let source = normalize_webhook_source(query.source.as_deref());
    let include_inactive = query.include_inactive.unwrap_or(false);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let now_ms = state.clock.now_ms();

    let rows = state
        .webhook_key_store
        .list_webhook_keys(
            &tenant_id,
            source.as_deref(),
            include_inactive,
            limit,
            now_ms,
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to list webhook keys: {err}"))
        })?;

    let keys = rows
        .iter()
        .map(|row| map_webhook_key_record_view(row, now_ms))
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "ok": true,
        "keys": keys,
        "limit": limit
    })))
}

async fn revoke_tenant_webhook_key(
    State(state): State<AppState>,
    Path((tenant_id, key_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(payload): Json<Option<RevokeTenantWebhookKeyRequest>>,
) -> Result<Json<Value>, ApiError> {
    let _submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let key_id = key_id.trim();
    if key_id.is_empty() {
        return Err(ApiError::bad_request("key_id is required"));
    }
    let grace_seconds = payload
        .and_then(|value| value.grace_seconds)
        .unwrap_or(0)
        .clamp(0, 86_400);
    let now_ms = state.clock.now_ms();
    let revoked = state
        .webhook_key_store
        .revoke_webhook_key(&tenant_id, key_id, now_ms, grace_seconds)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to revoke tenant webhook key: {err}"))
        })?;

    if !revoked {
        return Err(ApiError::not_found(format!(
            "tenant webhook key `{key_id}` not found for tenant `{tenant_id}`"
        )));
    }

    Ok(Json(json!({
        "ok": true,
        "key_id": key_id,
        "grace_seconds": grace_seconds
    })))
}

fn map_webhook_key_record_view(
    row: &TenantWebhookKeyRecord,
    now_ms: u64,
) -> TenantWebhookKeyRecordView {
    let active = row.revoked_at_ms.is_none()
        || row
            .expires_at_ms
            .map(|expires_at_ms| expires_at_ms > now_ms)
            .unwrap_or(false);

    TenantWebhookKeyRecordView {
        key_id: row.key_id.clone(),
        tenant_id: row.tenant_id.clone(),
        source: row.source.clone(),
        secret_last4: row.secret_last4.clone(),
        active,
        created_by_principal_id: row.created_by_principal_id.clone(),
        created_at_ms: row.created_at_ms,
        revoked_at_ms: row.revoked_at_ms,
        expires_at_ms: row.expires_at_ms,
        last_used_at_ms: row.last_used_at_ms,
    }
}

fn quota_profile_to_view(profile: &TenantQuotaProfile) -> TenantQuotaProfileView {
    TenantQuotaProfileView {
        tenant_id: profile.tenant_id.clone(),
        plan: profile.plan.as_str().to_owned(),
        access_mode: profile.access_mode.as_str().to_owned(),
        execution_policy: profile.execution_policy.as_str().to_owned(),
        sponsored_monthly_cap_requests: profile.sponsored_monthly_cap_requests.max(0) as u64,
        free_play_limit: profile.free_play_limit.max(0) as u64,
        updated_by_principal_id: profile.updated_by_principal_id.clone(),
        updated_at_ms: profile.updated_at_ms,
    }
}

async fn upsert_tenant_quota(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<UpsertTenantQuotaRequest>,
) -> Result<Json<TenantQuotaProfileResponse>, ApiError> {
    let submitter = authorize_internal_provisioning_submitter(&state, &tenant_id, &headers)?;
    let now_ms = state.clock.now_ms();
    let current = state
        .quota_store
        .profile_for_tenant(&tenant_id, now_ms)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to load quota profile: {err}"))
        })?;

    let plan = match payload
        .plan
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(raw) => QuotaPlanTier::parse(raw)
            .ok_or_else(|| ApiError::bad_request(format!("unsupported plan `{raw}`")))?,
        None => current.plan,
    };
    let access_mode = match payload
        .access_mode
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(raw) => QuotaAccessMode::parse(raw)
            .ok_or_else(|| ApiError::bad_request(format!("unsupported access_mode `{raw}`")))?,
        None => current.access_mode,
    };
    let execution_policy = match payload
        .execution_policy
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(raw) => ExecutionPolicy::parse(raw).ok_or_else(|| {
            ApiError::bad_request(format!("unsupported execution_policy `{raw}`"))
        })?,
        None => current.execution_policy,
    };
    let sponsored_monthly_cap_requests = match payload.sponsored_monthly_cap_requests {
        Some(value) if value > 0 => value as i64,
        Some(_) => {
            return Err(ApiError::bad_request(
                "sponsored_monthly_cap_requests must be positive when set",
            ))
        }
        None => current.sponsored_monthly_cap_requests.max(10_000),
    };

    let free_play_limit = match payload.free_play_limit {
        Some(value) if value > 0 => value as i64,
        Some(_) => {
            return Err(ApiError::bad_request(
                "free_play_limit must be positive when set",
            ))
        }
        None => current.free_play_limit.max(plan.default_free_play_limit()),
    };

    let updated_by_principal_id = payload
        .updated_by_principal_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(submitter.principal_id.as_str())
        .to_owned();

    let profile = TenantQuotaProfile {
        tenant_id,
        plan,
        access_mode,
        execution_policy,
        sponsored_monthly_cap_requests,
        free_play_limit,
        updated_by_principal_id,
        updated_at_ms: now_ms,
    };
    state
        .quota_store
        .upsert_profile(&profile)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!("failed to persist quota profile: {err}"))
        })?;

    Ok(Json(TenantQuotaProfileResponse {
        ok: true,
        profile: quota_profile_to_view(&profile),
    }))
}

fn build_submit_intent_response_from_idempotency(
    record: &AgentActionIdempotencyRecord,
) -> Result<SubmitIntentResponse, ApiError> {
    Ok(SubmitIntentResponse {
        ok: true,
        tenant_id: record.tenant_id.clone(),
        intent_id: record.accepted_intent_id.clone().ok_or_else(|| {
            ApiError::internal("accepted_intent_id missing from agent action idempotency record")
        })?,
        job_id: record.accepted_job_id.clone().ok_or_else(|| {
            ApiError::internal("accepted_job_id missing from agent action idempotency record")
        })?,
        adapter_id: record.accepted_adapter_id.clone().ok_or_else(|| {
            ApiError::internal("accepted_adapter_id missing from agent action idempotency record")
        })?,
        state: record.accepted_state.clone().ok_or_else(|| {
            ApiError::internal("accepted_state missing from agent action idempotency record")
        })?,
        route_rule: record.route_rule.clone().unwrap_or_else(|| {
            format!("agent_action_idempotency_reuse:{}", record.idempotency_key)
        }),
    })
}

fn runtime_authorization_route_rule(mode: AgentExecutionMode) -> String {
    format!("runtime_authorized:{}", mode.as_str())
}

fn is_runtime_authorization_route_rule(route_rule: &str) -> bool {
    route_rule.starts_with("runtime_authorized:")
}

fn approval_request_to_view(approval: &ApprovalRequestRecord) -> ApprovalRequestView {
    ApprovalRequestView {
        approval_request_id: approval.approval_request_id.clone(),
        tenant_id: approval.tenant_id.clone(),
        action_request_id: approval.action_request_id.clone(),
        agent_id: approval.agent_id.clone(),
        environment_id: approval.environment_id.clone(),
        environment_kind: approval.environment_kind.clone(),
        intent_type: approval.intent_type.clone(),
        execution_mode: approval.execution_mode.clone(),
        adapter_type: approval.adapter_type.clone(),
        requested_scope: approval.requested_scope.clone(),
        effective_scope: approval.effective_scope.clone(),
        reason: approval.reason.clone(),
        submitted_by: approval.submitted_by.clone(),
        status: approval.status.as_str().to_owned(),
        required_approvals: approval.required_approvals,
        approvals_received: approval.approvals_received,
        approved_by: approval.approved_by.clone(),
        policy_bundle_id: approval.policy_bundle_id.clone(),
        policy_bundle_version: approval.policy_bundle_version,
        policy_explanation: approval.policy_explanation.clone(),
        obligations: approval.obligations.clone(),
        matched_rules: approval.matched_rules.clone(),
        decision_trace: approval.decision_trace.clone(),
        expires_at_ms: approval.expires_at_ms,
        requested_at_ms: approval.requested_at_ms,
        resolved_at_ms: approval.resolved_at_ms,
        resolved_by_actor_id: approval.resolved_by_actor_id.clone(),
        resolved_by_actor_source: approval.resolved_by_actor_source.clone(),
        resolution_note: approval.resolution_note.clone(),
        slack_delivery_state: approval.slack_delivery_state.clone(),
        slack_delivery_error: approval.slack_delivery_error.clone(),
        slack_last_attempt_at_ms: approval.slack_last_attempt_at_ms,
    }
}

fn capability_grant_to_view(grant: &CapabilityGrantRecord) -> CapabilityGrantView {
    CapabilityGrantView {
        grant_id: grant.grant_id.clone(),
        tenant_id: grant.tenant_id.clone(),
        environment_id: grant.environment_id.clone(),
        agent_id: grant.agent_id.clone(),
        action_family: grant.action_family.clone(),
        adapter_type: grant.adapter_type.clone(),
        granted_scope: grant.granted_scope.clone(),
        resource_binding: grant.resource_binding.clone(),
        amount_ceiling: grant.amount_ceiling,
        max_uses: grant.max_uses,
        uses_consumed: grant.uses_consumed,
        uses_remaining: grant.max_uses.saturating_sub(grant.uses_consumed),
        status: grant.status.as_str().to_owned(),
        source_action_request_id: grant.source_action_request_id.clone(),
        source_approval_request_id: grant.source_approval_request_id.clone(),
        source_policy_bundle_id: grant.source_policy_bundle_id.clone(),
        source_policy_bundle_version: grant.source_policy_bundle_version,
        created_by_actor_id: grant.created_by_actor_id.clone(),
        created_by_actor_source: grant.created_by_actor_source.clone(),
        created_at_ms: grant.created_at_ms,
        expires_at_ms: grant.expires_at_ms,
        last_used_at_ms: grant.last_used_at_ms,
        revoked_at_ms: grant.revoked_at_ms,
        revoked_reason: grant.revoked_reason.clone(),
    }
}

fn policy_decision_from_approval(approval: &ApprovalRequestRecord) -> PolicyDecisionExplanation {
    PolicyDecisionExplanation {
        final_effect: PolicyEffect::RequireApproval.as_str().to_owned(),
        effective_scope: approval.effective_scope.clone(),
        obligations: approval.obligations.clone(),
        matched_rules: approval.matched_rules.clone(),
        decision_trace: approval.decision_trace.clone(),
        published_bundle_id: approval.policy_bundle_id.clone(),
        published_bundle_version: approval.policy_bundle_version,
        explanation: approval.policy_explanation.clone(),
    }
}

fn policy_decision_satisfied_by_grant(
    base: &PolicyDecisionExplanation,
    grant: &CapabilityGrantRecord,
) -> PolicyDecisionExplanation {
    let mut decision_trace = base.decision_trace.clone();
    decision_trace.push(PolicyDecisionTraceEntry {
        stage: "grant_satisfied".to_owned(),
        layer: "capability_grant".to_owned(),
        source_id: grant.grant_id.clone(),
        rule_id: None,
        effect: Some(PolicyEffect::Allow.as_str().to_owned()),
        message: format!(
            "capability grant `{}` satisfied the approval requirement for this request",
            grant.grant_id
        ),
    });
    PolicyDecisionExplanation {
        final_effect: PolicyEffect::Allow.as_str().to_owned(),
        effective_scope: base.effective_scope.clone(),
        obligations: base.obligations.clone(),
        matched_rules: base.matched_rules.clone(),
        decision_trace,
        published_bundle_id: base.published_bundle_id.clone(),
        published_bundle_version: base.published_bundle_version,
        explanation: format!(
            "capability grant `{}` satisfied the approval requirement for this request",
            grant.grant_id
        ),
    }
}

fn build_agent_action_response(
    action: &NormalizedAgentActionRequest,
    idempotency_decision: &str,
    policy_decision: &PolicyDecisionExplanation,
    grant: Option<&CapabilityGrantUseOutcome>,
    approval: Option<&ApprovalRequestRecord>,
    response: Option<&SubmitIntentResponse>,
) -> SubmitAgentActionResponse {
    SubmitAgentActionResponse {
        ok: true,
        action_request_id: action.action_request_id.clone(),
        tenant_id: action.tenant_id.clone(),
        agent_id: action.agent_id.clone(),
        environment_id: action.environment_id.clone(),
        intent_type: action.intent_type.as_str().to_owned(),
        execution_mode: action.execution_mode.as_str().to_owned(),
        execution_owner: action.execution_mode.owner_label().to_owned(),
        adapter_type: action.adapter_type.clone(),
        idempotency_key: action.idempotency_key.clone(),
        idempotency_decision: idempotency_decision.to_owned(),
        policy_decision: policy_decision.final_effect.clone(),
        policy_explanation: policy_decision.explanation.clone(),
        effective_scope: policy_decision.effective_scope.clone(),
        obligations: policy_decision.obligations.clone(),
        matched_rules: policy_decision.matched_rules.clone(),
        decision_trace: policy_decision.decision_trace.clone(),
        policy_bundle_id: policy_decision.published_bundle_id.clone(),
        policy_bundle_version: policy_decision.published_bundle_version,
        grant_id: grant.map(|value| value.grant.grant_id.clone()),
        grant_uses_remaining: grant.map(|value| value.uses_remaining),
        approval_request_id: approval.map(|value| value.approval_request_id.clone()),
        approval_state: approval.map(|value| value.status.as_str().to_owned()),
        approval_expires_at_ms: approval.map(|value| value.expires_at_ms),
        intent_id: response.map(|value| value.intent_id.clone()),
        job_id: response.map(|value| value.job_id.clone()),
        adapter_id: response.map(|value| value.adapter_id.clone()),
        state: response.map(|value| value.state.clone()),
        route_rule: response.map(|value| value.route_rule.clone()),
    }
}

fn build_runtime_authorized_agent_action_response(
    action: &NormalizedAgentActionRequest,
    idempotency_decision: &str,
    policy_decision: &PolicyDecisionExplanation,
    grant: Option<&CapabilityGrantUseOutcome>,
    approval: Option<&ApprovalRequestRecord>,
) -> SubmitAgentActionResponse {
    SubmitAgentActionResponse {
        ok: true,
        action_request_id: action.action_request_id.clone(),
        tenant_id: action.tenant_id.clone(),
        agent_id: action.agent_id.clone(),
        environment_id: action.environment_id.clone(),
        intent_type: action.intent_type.as_str().to_owned(),
        execution_mode: action.execution_mode.as_str().to_owned(),
        execution_owner: action.execution_mode.owner_label().to_owned(),
        adapter_type: action.adapter_type.clone(),
        idempotency_key: action.idempotency_key.clone(),
        idempotency_decision: idempotency_decision.to_owned(),
        policy_decision: policy_decision.final_effect.clone(),
        policy_explanation: policy_decision.explanation.clone(),
        effective_scope: policy_decision.effective_scope.clone(),
        obligations: policy_decision.obligations.clone(),
        matched_rules: policy_decision.matched_rules.clone(),
        decision_trace: policy_decision.decision_trace.clone(),
        policy_bundle_id: policy_decision.published_bundle_id.clone(),
        policy_bundle_version: policy_decision.published_bundle_version,
        grant_id: grant.map(|value| value.grant.grant_id.clone()),
        grant_uses_remaining: grant.map(|value| value.uses_remaining),
        approval_request_id: approval.map(|value| value.approval_request_id.clone()),
        approval_state: approval.map(|value| value.status.as_str().to_owned()),
        approval_expires_at_ms: approval.map(|value| value.expires_at_ms),
        intent_id: None,
        job_id: None,
        adapter_id: None,
        state: Some("runtime_authorized".to_owned()),
        route_rule: Some(runtime_authorization_route_rule(action.execution_mode)),
    }
}

fn normalize_approval_state_filter(raw: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = normalize_registry_key(raw, "state", 32)?;
    match normalized.as_str() {
        "pending" | "approved" | "rejected" | "expired" | "escalated" => Ok(Some(normalized)),
        _ => Err(ApiError::bad_request(format!(
            "unsupported approval state `{normalized}`"
        ))),
    }
}

fn normalize_capability_grant_status_filter(raw: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = normalize_registry_key(raw, "status", 32)?;
    match normalized.as_str() {
        "active" | "revoked" | "expired" | "exhausted" => Ok(Some(normalized)),
        _ => Err(ApiError::bad_request(format!(
            "unsupported capability grant status `{normalized}`"
        ))),
    }
}

fn parse_approval_actor_id(
    submitter: &SubmitterIdentity,
    requested_actor_id: Option<&str>,
) -> Result<String, ApiError> {
    requested_actor_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| normalize_registry_key(value, "actor_id", 128))
        .transpose()?
        .or_else(|| Some(submitter.principal_id.trim().to_ascii_lowercase()))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("actor_id is required"))
}

fn ensure_scope_not_broader(
    requested_scope: &[String],
    effective_scope: &[String],
) -> Result<(), ApiError> {
    let requested = requested_scope
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    for scope in effective_scope {
        if !requested.contains(&scope.to_ascii_lowercase()) {
            return Err(ApiError::internal(
                "policy decision produced effective_scope broader than requested_scope",
            ));
        }
    }
    Ok(())
}

fn derive_action_resource_binding(intent_type: AgentIntentType, payload: &Value) -> Option<Value> {
    let mut out = Map::new();
    match intent_type {
        AgentIntentType::Transfer => {
            if let Some(value) = payload.get("to_addr").and_then(Value::as_str) {
                out.insert("to_addr".to_owned(), Value::String(value.trim().to_owned()));
            }
            if let Some(value) = payload.get("from_addr").and_then(Value::as_str) {
                out.insert(
                    "from_addr".to_owned(),
                    Value::String(value.trim().to_owned()),
                );
            }
            if let Some(value) = payload.get("asset").and_then(Value::as_str) {
                out.insert("asset".to_owned(), Value::String(value.trim().to_owned()));
            }
        }
        AgentIntentType::Refund => {
            if let Some(value) = payload.get("payment_reference").and_then(Value::as_str) {
                out.insert(
                    "payment_reference".to_owned(),
                    Value::String(value.trim().to_owned()),
                );
            }
            if let Some(value) = payload.get("destination_reference").and_then(Value::as_str) {
                out.insert(
                    "destination_reference".to_owned(),
                    Value::String(value.trim().to_owned()),
                );
            }
            if let Some(value) = payload.get("currency").and_then(Value::as_str) {
                out.insert(
                    "currency".to_owned(),
                    Value::String(value.trim().to_owned()),
                );
            }
        }
        AgentIntentType::GenerateInvoice => {
            if let Some(value) = payload.get("customer_reference").and_then(Value::as_str) {
                out.insert(
                    "customer_reference".to_owned(),
                    Value::String(value.trim().to_owned()),
                );
            }
            if let Some(value) = payload.get("currency").and_then(Value::as_str) {
                out.insert(
                    "currency".to_owned(),
                    Value::String(value.trim().to_owned()),
                );
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(canonicalize_json_value(&Value::Object(out)))
    }
}

fn normalize_optional_grant_resource_binding(
    raw: Option<Value>,
) -> Result<Option<Value>, ApiError> {
    let Some(value) = raw else {
        return Ok(None);
    };
    match value {
        Value::Object(_) => Ok(Some(canonicalize_json_value(&value))),
        _ => Err(ApiError::bad_request(
            "grant.resource_binding must be a JSON object when provided",
        )),
    }
}

fn normalize_grant_spec_from_approval(
    approval: &ApprovalRequestRecord,
    grant_request: &ApprovalGrantRequest,
    grant_workflow: &GrantWorkflowConfig,
    actor_id: &str,
    actor_source: &str,
    now_ms: u64,
) -> Result<CapabilityGrantCreateRecord, ApiError> {
    let ttl_seconds = grant_request
        .ttl_seconds
        .unwrap_or(grant_workflow.default_ttl_ms / 1000);
    if ttl_seconds == 0 {
        return Err(ApiError::bad_request("grant.ttl_seconds must be positive"));
    }
    let ttl_ms = ttl_seconds.saturating_mul(1000);
    if ttl_ms > grant_workflow.max_ttl_ms {
        return Err(ApiError::bad_request(format!(
            "grant.ttl_seconds exceeds maximum supported ttl of {} seconds",
            grant_workflow.max_ttl_ms / 1000
        )));
    }
    let max_uses = grant_request.max_uses.unwrap_or(1);
    if max_uses == 0 || max_uses > grant_workflow.max_uses {
        return Err(ApiError::bad_request(format!(
            "grant.max_uses must be between 1 and {}",
            grant_workflow.max_uses
        )));
    }
    let amount_ceiling = match grant_request.amount_ceiling {
        Some(value) if value > 0 => Some(value),
        Some(_) => {
            return Err(ApiError::bad_request(
                "grant.amount_ceiling must be positive when provided",
            ))
        }
        None => derive_action_amount(
            AgentIntentType::parse(&approval.intent_type)?,
            &approval.normalized_payload,
        ),
    };
    let granted_scope = match grant_request.scope.as_ref() {
        Some(values) => {
            let scope = normalize_requested_scope(values)?;
            ensure_scope_not_broader(&approval.effective_scope, &scope)?;
            scope
        }
        None => approval.effective_scope.clone(),
    };
    let resource_binding = match grant_request.resource_binding.clone() {
        Some(value) => Some(
            normalize_optional_grant_resource_binding(Some(value))?.ok_or_else(|| {
                ApiError::internal("normalized resource binding unexpectedly missing")
            })?,
        ),
        None => derive_action_resource_binding(
            AgentIntentType::parse(&approval.intent_type)?,
            &approval.normalized_payload,
        ),
    };
    Ok(CapabilityGrantCreateRecord {
        grant_id: format!("grant_{}", Uuid::new_v4().simple()),
        tenant_id: approval.tenant_id.clone(),
        environment_id: approval.environment_id.clone(),
        agent_id: approval.agent_id.clone(),
        action_family: approval.intent_type.clone(),
        adapter_type: approval.adapter_type.clone(),
        granted_scope,
        resource_binding,
        amount_ceiling,
        max_uses,
        source_action_request_id: approval.action_request_id.clone(),
        source_approval_request_id: approval.approval_request_id.clone(),
        source_policy_bundle_id: approval.policy_bundle_id.clone(),
        source_policy_bundle_version: approval.policy_bundle_version,
        created_by_actor_id: actor_id.to_owned(),
        created_by_actor_source: actor_source.to_owned(),
        created_at_ms: now_ms,
        expires_at_ms: now_ms.saturating_add(ttl_ms),
    })
}

struct AgentExecutionSubmitContext<'a> {
    tenant_id: &'a str,
    normalized_intent_kind: &'a str,
    normalized_payload: &'a Value,
    request_id: RequestId,
    correlation_id: Option<String>,
    idempotency_key: &'a str,
    submitter: &'a SubmitterIdentity,
    resolved_agent: &'a ResolvedAgentIdentity,
    agent_status: Option<&'a str>,
    entry_channel: &'a str,
    action_request_id: &'a str,
    intent_type: &'a str,
    execution_mode: AgentExecutionMode,
    adapter_type: &'a str,
    requested_scope: &'a [String],
    effective_scope: &'a [String],
    reason: &'a str,
    submitted_by: &'a str,
    policy_decision: &'a PolicyDecisionExplanation,
    callback_config: Option<&'a AgentActionCallbackConfig>,
    metering_scope: Option<&'a str>,
    base_execution_policy: ExecutionPolicy,
    effective_execution_policy: ExecutionPolicy,
    signing_mode: &'a str,
    payer_source: &'a str,
    fee_payer: &'a str,
    approval: Option<&'a ApprovalRequestRecord>,
    grant: Option<&'a CapabilityGrantRecord>,
}

fn build_agent_execution_auth_context(ctx: &AgentExecutionSubmitContext<'_>) -> AuthContext {
    AuthContext {
        principal_id: Some(ctx.submitter.principal_id.clone()),
        submitter_kind: Some(ctx.submitter.kind.as_str().to_owned()),
        auth_scheme: Some(ctx.submitter.auth_scheme.clone()),
        channel: Some(ctx.entry_channel.to_owned()),
        agent_id: Some(ctx.resolved_agent.agent_id.clone()),
        environment_id: Some(ctx.resolved_agent.environment_id.clone()),
        runtime_type: Some(ctx.resolved_agent.runtime_type.clone()),
        runtime_identity: Some(ctx.resolved_agent.runtime_identity.clone()),
        trust_tier: Some(ctx.resolved_agent.trust_tier.clone()),
        risk_tier: Some(ctx.resolved_agent.risk_tier.clone()),
    }
}

fn build_agent_execution_metadata(
    ctx: &AgentExecutionSubmitContext<'_>,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert("ingress.channel".to_owned(), ctx.entry_channel.to_owned());
    metadata.insert("request_id".to_owned(), ctx.request_id.to_string());
    if let Some(correlation_id) = ctx.correlation_id.as_ref() {
        metadata.insert("correlation_id".to_owned(), correlation_id.clone());
    }
    metadata.insert("idempotency_key".to_owned(), ctx.idempotency_key.to_owned());
    metadata.insert(
        "submitter.principal_id".to_owned(),
        ctx.submitter.principal_id.clone(),
    );
    metadata.insert(
        "submitter.kind".to_owned(),
        ctx.submitter.kind.as_str().to_owned(),
    );
    metadata.insert("agent.id".to_owned(), ctx.resolved_agent.agent_id.clone());
    metadata.insert(
        "agent.environment_id".to_owned(),
        ctx.resolved_agent.environment_id.clone(),
    );
    metadata.insert(
        "agent.environment_kind".to_owned(),
        ctx.resolved_agent.environment_kind.clone(),
    );
    metadata.insert(
        "agent.runtime_type".to_owned(),
        ctx.resolved_agent.runtime_type.clone(),
    );
    metadata.insert(
        "agent.runtime_identity".to_owned(),
        ctx.resolved_agent.runtime_identity.clone(),
    );
    metadata.insert(
        "agent.trust_tier".to_owned(),
        ctx.resolved_agent.trust_tier.clone(),
    );
    metadata.insert(
        "agent.risk_tier".to_owned(),
        ctx.resolved_agent.risk_tier.clone(),
    );
    metadata.insert(
        "agent.owner_team".to_owned(),
        ctx.resolved_agent.owner_team.clone(),
    );
    if let Some(agent_status) = ctx.agent_status {
        metadata.insert("agent.status".to_owned(), agent_status.to_owned());
    }
    metadata.insert(
        "agent.action_request_id".to_owned(),
        ctx.action_request_id.to_owned(),
    );
    metadata.insert("agent.intent_type".to_owned(), ctx.intent_type.to_owned());
    metadata.insert(
        "agent.execution_mode".to_owned(),
        ctx.execution_mode.as_str().to_owned(),
    );
    metadata.insert("agent.adapter_type".to_owned(), ctx.adapter_type.to_owned());
    metadata.insert(
        "agent.requested_scope".to_owned(),
        ctx.requested_scope.join(","),
    );
    metadata.insert(
        "agent.effective_scope".to_owned(),
        ctx.effective_scope.join(","),
    );
    metadata.insert("agent.reason".to_owned(), ctx.reason.to_owned());
    metadata.insert("agent.submitted_by".to_owned(), ctx.submitted_by.to_owned());
    metadata.insert(
        "policy.decision".to_owned(),
        ctx.policy_decision.final_effect.clone(),
    );
    metadata.insert(
        "policy.explanation".to_owned(),
        ctx.policy_decision.explanation.clone(),
    );
    if let Some(bundle_id) = ctx.policy_decision.published_bundle_id.as_ref() {
        metadata.insert("policy.bundle_id".to_owned(), bundle_id.clone());
    }
    if let Some(bundle_version) = ctx.policy_decision.published_bundle_version {
        metadata.insert(
            "policy.bundle_version".to_owned(),
            bundle_version.to_string(),
        );
    }
    if let Ok(serialized) = serde_json::to_string(&ctx.policy_decision.matched_rules) {
        metadata.insert("policy.matched_rules_json".to_owned(), serialized);
    }
    if let Ok(serialized) = serde_json::to_string(&ctx.policy_decision.obligations) {
        metadata.insert("policy.obligations_json".to_owned(), serialized);
    }
    if let Some(approval) = ctx.approval {
        metadata.insert(
            "approval.request_id".to_owned(),
            approval.approval_request_id.clone(),
        );
        metadata.insert(
            "approval.state".to_owned(),
            approval.status.as_str().to_owned(),
        );
        metadata.insert(
            "approval.required_approvals".to_owned(),
            approval.required_approvals.to_string(),
        );
        metadata.insert(
            "approval.approvals_received".to_owned(),
            approval.approvals_received.to_string(),
        );
        if !approval.approved_by.is_empty() {
            metadata.insert(
                "approval.approved_by".to_owned(),
                approval.approved_by.join(","),
            );
        }
    }
    if let Some(grant) = ctx.grant {
        metadata.insert("grant.id".to_owned(), grant.grant_id.clone());
        metadata.insert(
            "grant.source_action_request_id".to_owned(),
            grant.source_action_request_id.clone(),
        );
        metadata.insert(
            "grant.source_approval_request_id".to_owned(),
            grant.source_approval_request_id.clone(),
        );
        metadata.insert(
            "grant.expires_at_ms".to_owned(),
            grant.expires_at_ms.to_string(),
        );
        if let Some(bundle_id) = grant.source_policy_bundle_id.as_ref() {
            metadata.insert(
                "grant.source_policy_bundle_id".to_owned(),
                bundle_id.clone(),
            );
        }
        if let Some(bundle_version) = grant.source_policy_bundle_version {
            metadata.insert(
                "grant.source_policy_bundle_version".to_owned(),
                bundle_version.to_string(),
            );
        }
    }
    if let Some(scope) = ctx.metering_scope {
        metadata.insert("metering.scope".to_owned(), scope.to_owned());
    }
    metadata.insert("connector.outcome".to_owned(), "not_used".to_owned());
    if let Some(callback_config) = ctx.callback_config {
        if let Some(url) = callback_config.url.as_ref() {
            metadata.insert("callback.url".to_owned(), url.clone());
        }
        if let Ok(serialized) = serde_json::to_string(callback_config) {
            metadata.insert("callback.config_json".to_owned(), serialized);
        }
    }
    metadata.insert(
        "execution.mode".to_owned(),
        ctx.execution_mode.as_str().to_owned(),
    );
    metadata.insert(
        "execution.owner".to_owned(),
        ctx.execution_mode.owner_label().to_owned(),
    );
    metadata.insert(
        "execution.policy".to_owned(),
        ctx.effective_execution_policy.as_str().to_owned(),
    );
    metadata.insert(
        "execution.policy.base".to_owned(),
        ctx.base_execution_policy.as_str().to_owned(),
    );
    metadata.insert(
        "execution.signing_mode".to_owned(),
        ctx.signing_mode.to_owned(),
    );
    metadata.insert(
        "execution.payer_source".to_owned(),
        ctx.payer_source.to_owned(),
    );
    metadata.insert("execution.fee_payer".to_owned(), ctx.fee_payer.to_owned());
    metadata
}

async fn submit_agent_execution_intent(
    state: &AppState,
    ctx: AgentExecutionSubmitContext<'_>,
) -> Result<SubmitIntentResponse, ApiError> {
    let auth_context = build_agent_execution_auth_context(&ctx);
    let metadata = build_agent_execution_metadata(&ctx);
    let response = submit_normalized_intent(
        state,
        ctx.tenant_id.to_owned(),
        IntentKind::new(ctx.normalized_intent_kind.to_owned()),
        ctx.normalized_payload.clone(),
        ctx.request_id,
        ctx.correlation_id,
        Some(ctx.idempotency_key.to_owned()),
        Some(auth_context),
        metadata,
    )
    .await?;
    Ok(response.0)
}

fn build_policy_evaluation_context(
    action: &NormalizedAgentActionRequest,
    resolved_agent: &ResolvedAgentIdentity,
    now_ms: u64,
) -> PolicyEvaluationContext {
    PolicyEvaluationContext {
        tenant_id: action.tenant_id.clone(),
        agent_id: action.agent_id.clone(),
        owner_team: resolved_agent.owner_team.clone(),
        trust_tier: resolved_agent.trust_tier.clone(),
        risk_tier: resolved_agent.risk_tier.clone(),
        environment_id: action.environment_id.clone(),
        environment_kind: resolved_agent.environment_kind.clone(),
        action: action.intent_type.as_str().to_owned(),
        adapter_type: action.adapter_type.clone(),
        amount: derive_action_amount(action.intent_type, &action.normalized_payload),
        sensitivity: derive_action_sensitivity(action.intent_type, &action.normalized_payload),
        destination_class: derive_destination_class(action.intent_type, &action.normalized_payload),
        requested_scope: action.requested_scope.clone(),
        reason: action.reason.clone(),
        submitted_by: action.submitted_by.clone(),
        evaluated_at_ms: now_ms,
    }
}

fn normalized_agent_action_from_approval(
    approval: &ApprovalRequestRecord,
) -> Result<NormalizedAgentActionRequest, ApiError> {
    Ok(NormalizedAgentActionRequest {
        action_request_id: approval.action_request_id.clone(),
        tenant_id: approval.tenant_id.clone(),
        agent_id: approval.agent_id.clone(),
        environment_id: approval.environment_id.clone(),
        intent_type: AgentIntentType::parse(&approval.intent_type)?,
        execution_mode: AgentExecutionMode::parse(&approval.execution_mode)?,
        adapter_type: approval.adapter_type.clone(),
        normalized_payload: approval.normalized_payload.clone(),
        idempotency_key: approval.idempotency_key.clone(),
        requested_scope: approval.requested_scope.clone(),
        reason: approval.reason.clone(),
        callback_config: approval.callback_config.clone(),
        submitted_by: approval.submitted_by.clone(),
        normalized_intent_kind: approval.normalized_intent_kind.clone(),
    })
}

async fn execute_approved_agent_action(
    state: &AppState,
    approval: &ApprovalRequestRecord,
    grant: Option<&CapabilityGrantRecord>,
) -> Result<SubmitIntentResponse, ApiError> {
    ensure_scope_not_broader(&approval.requested_scope, &approval.effective_scope)?;
    let _ = normalized_agent_action_from_approval(approval)?;
    let metering_scope = if approval
        .effective_scope
        .iter()
        .any(|scope| scope.eq_ignore_ascii_case("playground"))
    {
        Some("playground")
    } else {
        None
    };

    let quota_check = state
        .quota_store
        .enforce_submit_allowed(&approval.tenant_id, state.clock.now_ms(), metering_scope)
        .await?;
    let submitter = SubmitterIdentity {
        principal_id: approval
            .resolved_by_actor_id
            .clone()
            .unwrap_or_else(|| "approval_workflow".to_owned()),
        kind: SubmitterKind::InternalService,
        auth_scheme: "approval_workflow".to_owned(),
        resolved_agent: None,
    };
    let policy_enforced = state
        .execution_policy_enforcement
        .is_enforced_for_tenant(&approval.tenant_id);
    let effective_execution_policy = resolve_effective_execution_policy(
        quota_check.profile.execution_policy,
        &submitter,
        metering_scope,
    );
    if policy_enforced {
        if matches!(effective_execution_policy, ExecutionPolicy::Sponsored)
            && quota_check.used_requests >= quota_check.profile.sponsored_monthly_cap_requests
        {
            return Err(ApiError::too_many_requests(format!(
                "EXECUTION_POLICY_SPONSORED_CAP_EXCEEDED: tenant `{}` reached sponsored cap ({}/{}) in the last 30 days",
                approval.tenant_id,
                quota_check.used_requests,
                quota_check.profile.sponsored_monthly_cap_requests
            )));
        }
        enforce_execution_policy_for_payload(
            effective_execution_policy,
            &approval.normalized_intent_kind,
            &approval.normalized_payload,
        )?;
    }

    let signed_tx_present = signed_tx_payload_present(&approval.normalized_payload);
    let signing_mode = resolve_signing_mode(effective_execution_policy, signed_tx_present);
    let payer_source = resolve_payer_source(effective_execution_policy, signed_tx_present);
    let fee_payer = extract_fee_payer_hint(&approval.normalized_payload)
        .unwrap_or_else(|| "unknown".to_owned());
    let resolved_agent = ResolvedAgentIdentity {
        agent_id: approval.agent_id.clone(),
        environment_id: approval.environment_id.clone(),
        environment_kind: approval.environment_kind.clone(),
        runtime_type: approval.runtime_type.clone(),
        runtime_identity: approval.runtime_identity.clone(),
        status: "approved".to_owned(),
        trust_tier: approval.trust_tier.clone(),
        risk_tier: approval.risk_tier.clone(),
        owner_team: approval.owner_team.clone(),
    };
    let policy_decision = policy_decision_from_approval(approval);
    let response = submit_agent_execution_intent(
        state,
        AgentExecutionSubmitContext {
            tenant_id: &approval.tenant_id,
            normalized_intent_kind: &approval.normalized_intent_kind,
            normalized_payload: &approval.normalized_payload,
            request_id: RequestId::from(format!("approval-{}", approval.approval_request_id)),
            correlation_id: approval
                .correlation_id
                .clone()
                .or_else(|| Some(format!("approval:{}", approval.approval_request_id))),
            idempotency_key: &approval.idempotency_key,
            submitter: &submitter,
            resolved_agent: &resolved_agent,
            agent_status: None,
            entry_channel: "approval",
            action_request_id: &approval.action_request_id,
            intent_type: &approval.intent_type,
            execution_mode: AgentExecutionMode::parse(&approval.execution_mode)?,
            adapter_type: &approval.adapter_type,
            requested_scope: &approval.requested_scope,
            effective_scope: &approval.effective_scope,
            reason: &approval.reason,
            submitted_by: &approval.submitted_by,
            policy_decision: &policy_decision,
            callback_config: approval.callback_config.as_ref(),
            metering_scope,
            base_execution_policy: quota_check.profile.execution_policy,
            effective_execution_policy,
            signing_mode,
            payer_source,
            fee_payer: &fee_payer,
            approval: Some(approval),
            grant,
        },
    )
    .await?;
    state
        .agent_action_idempotency_store
        .finalize_success(
            &approval.tenant_id,
            &approval.agent_id,
            &approval.environment_id,
            &approval.idempotency_key,
            &response,
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to finalize approval-backed idempotency row: {err}"
            ))
        })?;

    state
        .approval_store
        .insert_event(
            &approval.tenant_id,
            &approval.approval_request_id,
            "execution_submitted",
            approval.resolved_by_actor_id.as_deref(),
            approval.resolved_by_actor_source.as_deref(),
            json!({
                "intent_id": response.intent_id,
                "job_id": response.job_id,
                "adapter_id": response.adapter_id,
                "state": response.state,
                "route_rule": response.route_rule,
                "correlation_id": approval.correlation_id,
                "grant_id": grant.map(|value| value.grant_id.clone()),
            }),
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to write approval execution event: {err}"
            ))
        })?;

    Ok(response)
}

async fn authorize_approved_runtime_action(
    state: &AppState,
    approval: &ApprovalRequestRecord,
    grant: Option<&CapabilityGrantRecord>,
) -> Result<(), ApiError> {
    ensure_scope_not_broader(&approval.requested_scope, &approval.effective_scope)?;
    let _ = normalized_agent_action_from_approval(approval)?;
    let execution_mode = AgentExecutionMode::parse(&approval.execution_mode)?;
    if execution_mode != AgentExecutionMode::ModeBScopedRuntime {
        return Err(ApiError::internal(
            "runtime authorization requested for non-runtime execution mode",
        ));
    }

    state
        .agent_action_idempotency_store
        .finalize_runtime_authorization(
            &approval.tenant_id,
            &approval.agent_id,
            &approval.environment_id,
            &approval.idempotency_key,
            execution_mode,
            &approval.adapter_type,
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to finalize runtime authorization idempotency row: {err}"
            ))
        })?;

    state
        .approval_store
        .insert_event(
            &approval.tenant_id,
            &approval.approval_request_id,
            "runtime_authorized",
            approval.resolved_by_actor_id.as_deref(),
            approval.resolved_by_actor_source.as_deref(),
            json!({
                "execution_mode": execution_mode.as_str(),
                "execution_owner": execution_mode.owner_label(),
                "grant_id": grant.map(|value| value.grant_id.clone()),
                "effective_scope": approval.effective_scope,
            }),
            state.clock.now_ms(),
        )
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "failed to write runtime authorization event: {err}"
            ))
        })?;

    Ok(())
}

async fn submit_agent_action_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SubmitAgentActionRequest>,
) -> Result<Json<SubmitAgentActionResponse>, ApiError> {
    let request_id = header_opt(&headers, "x-request-id")
        .map(RequestId::from)
        .unwrap_or_else(RequestId::new);
    let mut audit = IngressIntakeAuditRecord::new(
        request_id.to_string(),
        IngressChannel::Api,
        "/api/agent/action-requests",
        "POST",
        state.clock.now_ms(),
    );

    let (tenant_id, submitter, resolved_agent) =
        match authenticate_and_resolve_agent_runtime_submitter(&state, &headers, &mut audit).await {
            Ok(value) => value,
            Err(err) => {
                let reason = match err.status {
                    StatusCode::UNAUTHORIZED => "submitter_auth_failed",
                    StatusCode::FORBIDDEN => "agent_identity_unresolved",
                    _ => "agent_submitter_invalid",
                };
                return reject_with_audit(
                    &state,
                    &mut audit,
                    err,
                    reason,
                    json!({ "stage": "agent_runtime_auth" }),
                )
                .await;
            }
        };

    let normalized_action =
        match normalize_agent_action_request(payload, &tenant_id, &resolved_agent) {
            Ok(value) => value,
            Err(err) => {
                return reject_with_audit(
                    &state,
                    &mut audit,
                    err,
                    "agent_action_invalid",
                    json!({ "stage": "normalize_agent_action" }),
                )
                .await
            }
        };
    audit.intent_kind = Some(normalized_action.normalized_intent_kind.clone());
    audit.idempotency_key = Some(normalized_action.idempotency_key.clone());
    let correlation_id = header_opt(&headers, "x-correlation-id");
    audit.correlation_id = correlation_id.clone();

    let idempotency_decision = match state
        .agent_action_idempotency_store
        .reserve_or_load(&AgentActionIdempotencyReservation {
            tenant_id: normalized_action.tenant_id.clone(),
            agent_id: normalized_action.agent_id.clone(),
            environment_id: normalized_action.environment_id.clone(),
            idempotency_key: normalized_action.idempotency_key.clone(),
            request_fingerprint: agent_action_request_fingerprint(&normalized_action),
            action_request_id: normalized_action.action_request_id.clone(),
            intent_type: normalized_action.intent_type.as_str().to_owned(),
            execution_mode: normalized_action.execution_mode.as_str().to_owned(),
            adapter_type: normalized_action.adapter_type.clone(),
            now_ms: state.clock.now_ms(),
        })
        .await
    {
        Ok(value) => value,
        Err(error) => {
            let message = error.to_string();
            let api_error = if message.contains("different normalized request") {
                ApiError::conflict(format!(
                    "idempotency conflict for key `{}`: {message}",
                    normalized_action.idempotency_key
                ))
            } else {
                ApiError::service_unavailable(format!(
                    "agent action idempotency store unavailable: {message}"
                ))
            };
            return reject_with_audit(
                &state,
                &mut audit,
                api_error,
                "agent_action_idempotency_failed",
                json!({ "stage": "idempotency_prepolicy" }),
            )
            .await;
        }
    };

    match idempotency_decision {
        AgentActionIdempotencyDecision::ExistingAccepted(record) => {
            let reused_policy = PolicyDecisionExplanation {
                final_effect: PolicyEffect::Allow.as_str().to_owned(),
                effective_scope: normalized_action.requested_scope.clone(),
                obligations: PolicyRuleObligations::default(),
                matched_rules: Vec::new(),
                decision_trace: vec![PolicyDecisionTraceEntry {
                    stage: "final_decision".to_owned(),
                    layer: "idempotency".to_owned(),
                    source_id: "existing_accepted".to_owned(),
                    rule_id: None,
                    effect: Some(PolicyEffect::Allow.as_str().to_owned()),
                    message: "reused existing accepted agent action result".to_owned(),
                }],
                published_bundle_id: None,
                published_bundle_version: None,
                explanation: "reused existing accepted agent action result".to_owned(),
            };
            if record
                .route_rule
                .as_deref()
                .is_some_and(is_runtime_authorization_route_rule)
            {
                audit.validation_result = "runtime_authorized".to_owned();
                audit.rejection_reason = None;
                audit.error_status = None;
                audit.error_message = None;
                audit.idempotency_decision = Some("reused_existing".to_owned());
                audit.details_json = json!({
                    "route_rule": record.route_rule,
                    "state": record.accepted_state,
                    "execution_mode": normalized_action.execution_mode.as_str(),
                    "execution_owner": normalized_action.execution_mode.owner_label(),
                });
                persist_intake_audit_async(&state, audit.clone());
                return Ok(Json(build_runtime_authorized_agent_action_response(
                    &normalized_action,
                    "reused_existing",
                    &reused_policy,
                    None,
                    None,
                )));
            }
            let existing = build_submit_intent_response_from_idempotency(&record)?;
            audit.mark_accepted(&existing);
            persist_intake_audit_async(&state, audit.clone());
            return Ok(Json(build_agent_action_response(
                &normalized_action,
                "reused_existing",
                &reused_policy,
                None,
                None,
                Some(&existing),
            )));
        }
        AgentActionIdempotencyDecision::ExistingApproval(record) => {
            let approval_request_id = record.approval_request_id.as_deref().ok_or_else(|| {
                ApiError::internal(
                    "approval_request_id missing from approval-bound idempotency record",
                )
            })?;
            let approval = state
                .approval_store
                .load_request_fresh(&tenant_id, approval_request_id, state.clock.now_ms())
                .await
                .map_err(|err| {
                    ApiError::service_unavailable(format!(
                        "failed to load existing approval request: {err}"
                    ))
                })?
                .ok_or_else(|| {
                    ApiError::service_unavailable(format!(
                        "approval request `{approval_request_id}` missing for idempotency key `{}`",
                        normalized_action.idempotency_key
                    ))
                })?;
            let approval_policy = policy_decision_from_approval(&approval);
            audit.validation_result = "approval_required".to_owned();
            audit.rejection_reason = Some(approval.status.as_str().to_owned());
            audit.details_json = json!({
                "stage": "idempotency_existing_approval",
                "approval_request_id": approval.approval_request_id,
                "approval_state": approval.status.as_str(),
                "approval_expires_at_ms": approval.expires_at_ms,
            });
            persist_intake_audit_async(&state, audit.clone());
            return Ok(Json(build_agent_action_response(
                &normalized_action,
                "reused_existing_approval",
                &approval_policy,
                None,
                Some(&approval),
                None,
            )));
        }
        AgentActionIdempotencyDecision::ExistingPending => {
            return reject_with_audit(
                &state,
                &mut audit,
                ApiError::conflict(format!(
                    "agent action idempotency key `{}` is already reserved and still pending",
                    normalized_action.idempotency_key
                )),
                "agent_action_idempotency_pending",
                json!({ "stage": "idempotency_prepolicy" }),
            )
            .await;
        }
        AgentActionIdempotencyDecision::ReservedNew => {}
    }

    let published_bundle = match state
        .policy_bundle_store
        .load_published_bundle(&tenant_id)
        .await
    {
        Ok(value) => value,
        Err(err) => {
            let _ = state
                .agent_action_idempotency_store
                .release_pending(
                    &normalized_action.tenant_id,
                    &normalized_action.agent_id,
                    &normalized_action.environment_id,
                    &normalized_action.idempotency_key,
                )
                .await;
            return reject_with_audit(
                &state,
                &mut audit,
                ApiError::service_unavailable(format!(
                    "failed to load tenant policy bundle: {err}"
                )),
                "tenant_policy_unavailable",
                json!({ "stage": "tenant_policy_load" }),
            )
            .await;
        }
    };

    let policy_context =
        build_policy_evaluation_context(&normalized_action, &resolved_agent, state.clock.now_ms());
    let mut policy_decision =
        match evaluate_policy_layers(&policy_context, published_bundle.as_ref()) {
            Ok(value) => value,
            Err(err) => {
                let _ = state
                    .agent_action_idempotency_store
                    .release_pending(
                        &normalized_action.tenant_id,
                        &normalized_action.agent_id,
                        &normalized_action.environment_id,
                        &normalized_action.idempotency_key,
                    )
                    .await;
                return reject_with_audit(
                    &state,
                    &mut audit,
                    err,
                    "tenant_policy_evaluation_failed",
                    json!({ "stage": "tenant_policy_eval" }),
                )
                .await;
            }
        };
    let mut matched_grant: Option<CapabilityGrantRecord> = None;
    let mut grant_consumption_request: Option<CapabilityGrantConsumptionRequest> = None;
    if policy_decision.final_effect == "require_approval" {
        let grant_lookup_request = CapabilityGrantConsumptionRequest {
            tenant_id: normalized_action.tenant_id.clone(),
            environment_id: normalized_action.environment_id.clone(),
            agent_id: normalized_action.agent_id.clone(),
            action_family: normalized_action.intent_type.as_str().to_owned(),
            adapter_type: normalized_action.adapter_type.clone(),
            requested_scope: normalized_action.requested_scope.clone(),
            resource_binding: derive_action_resource_binding(
                normalized_action.intent_type,
                &normalized_action.normalized_payload,
            ),
            amount: derive_action_amount(
                normalized_action.intent_type,
                &normalized_action.normalized_payload,
            ),
            request_id: Some(request_id.to_string()),
            action_request_id: normalized_action.action_request_id.clone(),
            correlation_id: correlation_id.clone(),
            used_at_ms: state.clock.now_ms(),
        };
        let grant_candidate = match state
            .capability_grant_store
            .find_matching_grant(&grant_lookup_request)
            .await
        {
            Ok(value) => value,
            Err(err) => {
                let _ = state
                    .agent_action_idempotency_store
                    .release_pending(
                        &normalized_action.tenant_id,
                        &normalized_action.agent_id,
                        &normalized_action.environment_id,
                        &normalized_action.idempotency_key,
                    )
                    .await;
                return reject_with_audit(
                    &state,
                    &mut audit,
                    ApiError::service_unavailable(format!(
                        "failed to evaluate capability grants: {err}"
                    )),
                    "capability_grant_lookup_failed",
                    json!({ "stage": "capability_grant_lookup" }),
                )
                .await;
            }
        };
        if let Some(grant) = grant_candidate {
            policy_decision = policy_decision_satisfied_by_grant(&policy_decision, &grant);
            matched_grant = Some(grant);
            grant_consumption_request = Some(grant_lookup_request);
        }
    }
    let effective_scope = policy_decision.effective_scope.clone();
    ensure_scope_not_broader(&normalized_action.requested_scope, &effective_scope)?;
    let metering_scope = if effective_scope
        .iter()
        .any(|scope| scope.eq_ignore_ascii_case("playground"))
    {
        Some("playground".to_owned())
    } else {
        None
    };

    if policy_decision.final_effect == "deny" {
        let _ = state
            .agent_action_idempotency_store
            .release_pending(
                &normalized_action.tenant_id,
                &normalized_action.agent_id,
                &normalized_action.environment_id,
                &normalized_action.idempotency_key,
            )
            .await;
        audit.validation_result = if policy_decision.final_effect == "require_approval" {
            "approval_required".to_owned()
        } else {
            "policy_denied".to_owned()
        };
        audit.rejection_reason = Some(policy_decision.final_effect.clone());
        audit.error_status = None;
        audit.error_message = None;
        audit.details_json = json!({
            "stage": "tenant_policy",
            "policy_decision": policy_decision.final_effect,
            "policy_explanation": policy_decision.explanation,
            "effective_scope": effective_scope,
            "obligations": policy_decision.obligations,
            "matched_rules": policy_decision.matched_rules,
            "policy_bundle_id": policy_decision.published_bundle_id,
            "policy_bundle_version": policy_decision.published_bundle_version,
        });
        persist_intake_audit_async(&state, audit.clone());
        return Ok(Json(build_agent_action_response(
            &normalized_action,
            "evaluated_new",
            &policy_decision,
            None,
            None,
            None,
        )));
    }

    if policy_decision.final_effect == "require_approval" {
        let now_ms = state.clock.now_ms();
        let approval_request_id = format!("apr_{}", Uuid::new_v4().simple());
        let approval_record = ApprovalRequestCreateRecord {
            approval_request_id: approval_request_id.clone(),
            tenant_id: normalized_action.tenant_id.clone(),
            action_request_id: normalized_action.action_request_id.clone(),
            correlation_id: correlation_id.clone(),
            agent_id: normalized_action.agent_id.clone(),
            environment_id: normalized_action.environment_id.clone(),
            environment_kind: resolved_agent.environment_kind.clone(),
            runtime_type: resolved_agent.runtime_type.clone(),
            runtime_identity: resolved_agent.runtime_identity.clone(),
            trust_tier: resolved_agent.trust_tier.clone(),
            risk_tier: resolved_agent.risk_tier.clone(),
            owner_team: resolved_agent.owner_team.clone(),
            intent_type: normalized_action.intent_type.as_str().to_owned(),
            execution_mode: normalized_action.execution_mode.as_str().to_owned(),
            adapter_type: normalized_action.adapter_type.clone(),
            normalized_intent_kind: normalized_action.normalized_intent_kind.clone(),
            normalized_payload: normalized_action.normalized_payload.clone(),
            idempotency_key: normalized_action.idempotency_key.clone(),
            request_fingerprint: agent_action_request_fingerprint(&normalized_action),
            requested_scope: normalized_action.requested_scope.clone(),
            effective_scope: effective_scope.clone(),
            callback_config: normalized_action.callback_config.clone(),
            reason: normalized_action.reason.clone(),
            submitted_by: normalized_action.submitted_by.clone(),
            policy_bundle_id: policy_decision.published_bundle_id.clone(),
            policy_bundle_version: policy_decision.published_bundle_version,
            policy_explanation: policy_decision.explanation.clone(),
            obligations: policy_decision.obligations.clone(),
            matched_rules: policy_decision.matched_rules.clone(),
            decision_trace: policy_decision.decision_trace.clone(),
            required_approvals: if policy_decision.obligations.dual_approval {
                2
            } else {
                1
            },
            requested_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(state.approval_workflow.request_ttl_ms),
        };
        let approval = match state.approval_store.create_request(&approval_record).await {
            Ok(value) => value,
            Err(err) => {
                let _ = state
                    .agent_action_idempotency_store
                    .release_pending(
                        &normalized_action.tenant_id,
                        &normalized_action.agent_id,
                        &normalized_action.environment_id,
                        &normalized_action.idempotency_key,
                    )
                    .await;
                return reject_with_audit(
                    &state,
                    &mut audit,
                    ApiError::service_unavailable(format!(
                        "failed to create approval request: {err}"
                    )),
                    "approval_request_create_failed",
                    json!({ "stage": "approval_create" }),
                )
                .await;
            }
        };
        if let Err(err) = state
            .agent_action_idempotency_store
            .mark_approval_pending(
                &approval.tenant_id,
                &approval.agent_id,
                &approval.environment_id,
                &approval.idempotency_key,
                &approval.approval_request_id,
                approval.status,
                approval.expires_at_ms,
                now_ms,
            )
            .await
        {
            let _ = state
                .agent_action_idempotency_store
                .release_pending(
                    &normalized_action.tenant_id,
                    &normalized_action.agent_id,
                    &normalized_action.environment_id,
                    &normalized_action.idempotency_key,
                )
                .await;
            return reject_with_audit(
                &state,
                &mut audit,
                ApiError::service_unavailable(format!(
                    "failed to link approval request to idempotency record: {err}"
                )),
                "approval_idempotency_link_failed",
                json!({ "stage": "approval_idempotency_link", "approval_request_id": approval.approval_request_id }),
            )
            .await;
        }
        let approval = match deliver_approval_request_to_slack(&state, &approval).await {
            Ok(()) => state
                .approval_store
                .update_slack_delivery(
                    &approval.tenant_id,
                    &approval.approval_request_id,
                    true,
                    None,
                    state.clock.now_ms(),
                )
                .await
                .map_err(|err| {
                    ApiError::service_unavailable(format!(
                        "failed to record slack delivery state: {err}"
                    ))
                })?,
            Err(err) => {
                let approval = state
                    .approval_store
                    .update_slack_delivery(
                        &approval.tenant_id,
                        &approval.approval_request_id,
                        false,
                        Some(&err.to_string()),
                        state.clock.now_ms(),
                    )
                    .await
                    .map_err(|update_err| {
                        ApiError::service_unavailable(format!(
                            "failed to record slack delivery failure: {update_err}"
                        ))
                    })?;
                let _ = state
                    .agent_action_idempotency_store
                    .mark_approval_state(
                        &approval.tenant_id,
                        &approval.agent_id,
                        &approval.environment_id,
                        &approval.idempotency_key,
                        approval.status,
                        state.clock.now_ms(),
                    )
                    .await;
                approval
            }
        };
        audit.validation_result = "approval_required".to_owned();
        audit.rejection_reason = Some(policy_decision.final_effect.clone());
        audit.error_status = None;
        audit.error_message = None;
        audit.details_json = json!({
            "stage": "tenant_policy",
            "policy_decision": policy_decision.final_effect,
            "policy_explanation": policy_decision.explanation,
            "effective_scope": effective_scope,
            "obligations": policy_decision.obligations,
            "matched_rules": policy_decision.matched_rules,
            "policy_bundle_id": policy_decision.published_bundle_id,
            "policy_bundle_version": policy_decision.published_bundle_version,
            "approval_request_id": approval.approval_request_id,
            "approval_state": approval.status.as_str(),
            "approval_expires_at_ms": approval.expires_at_ms,
            "slack_delivery_state": approval.slack_delivery_state,
            "slack_delivery_error": approval.slack_delivery_error,
        });
        persist_intake_audit_async(&state, audit.clone());
        return Ok(Json(build_agent_action_response(
            &normalized_action,
            "accepted_for_approval",
            &policy_decision_from_approval(&approval),
            None,
            Some(&approval),
            None,
        )));
    }

    let quota_check = match state
        .quota_store
        .enforce_submit_allowed(&tenant_id, state.clock.now_ms(), metering_scope.as_deref())
        .await
    {
        Ok(check) => check,
        Err(err) => {
            let _ = state
                .agent_action_idempotency_store
                .release_pending(
                    &normalized_action.tenant_id,
                    &normalized_action.agent_id,
                    &normalized_action.environment_id,
                    &normalized_action.idempotency_key,
                )
                .await;
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "quota_rejected",
                json!({ "stage": "quota_enforcement" }),
            )
            .await;
        }
    };

    let policy_enforced = state
        .execution_policy_enforcement
        .is_enforced_for_tenant(&tenant_id);
    let effective_execution_policy = resolve_effective_execution_policy(
        quota_check.profile.execution_policy,
        &submitter,
        metering_scope.as_deref(),
    );
    if policy_enforced {
        if matches!(effective_execution_policy, ExecutionPolicy::Sponsored)
            && quota_check.used_requests >= quota_check.profile.sponsored_monthly_cap_requests
        {
            let _ = state
                .agent_action_idempotency_store
                .release_pending(
                    &normalized_action.tenant_id,
                    &normalized_action.agent_id,
                    &normalized_action.environment_id,
                    &normalized_action.idempotency_key,
                )
                .await;
            return reject_with_audit(
                &state,
                &mut audit,
                ApiError::too_many_requests(format!(
                    "EXECUTION_POLICY_SPONSORED_CAP_EXCEEDED: tenant `{tenant_id}` reached sponsored cap ({}/{}) in the last 30 days",
                    quota_check.used_requests, quota_check.profile.sponsored_monthly_cap_requests
                )),
                "execution_policy_cap_exceeded",
                json!({ "stage": "execution_policy" }),
            )
            .await;
        }
        if let Err(err) = enforce_execution_policy_for_payload(
            effective_execution_policy,
            &normalized_action.normalized_intent_kind,
            &normalized_action.normalized_payload,
        ) {
            let _ = state
                .agent_action_idempotency_store
                .release_pending(
                    &normalized_action.tenant_id,
                    &normalized_action.agent_id,
                    &normalized_action.environment_id,
                    &normalized_action.idempotency_key,
                )
                .await;
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "execution_policy_rejected",
                json!({ "stage": "execution_policy" }),
            )
            .await;
        }
    }

    let signed_tx_present = signed_tx_payload_present(&normalized_action.normalized_payload);
    let signing_mode = resolve_signing_mode(effective_execution_policy, signed_tx_present);
    let payer_source = resolve_payer_source(effective_execution_policy, signed_tx_present);
    let fee_payer = extract_fee_payer_hint(&normalized_action.normalized_payload)
        .unwrap_or_else(|| "unknown".to_owned());
    let consumed_grant = if let Some(grant) = matched_grant.as_ref() {
        let grant_request = grant_consumption_request
            .as_ref()
            .ok_or_else(|| ApiError::internal("capability grant request context missing"))?;
        match state
            .capability_grant_store
            .consume_grant(&grant.grant_id, grant_request)
            .await
        {
            Ok(Some(value)) => Some(value),
            Ok(None) => {
                let _ = state
                    .agent_action_idempotency_store
                    .release_pending(
                        &normalized_action.tenant_id,
                        &normalized_action.agent_id,
                        &normalized_action.environment_id,
                        &normalized_action.idempotency_key,
                    )
                    .await;
                return reject_with_audit(
                    &state,
                    &mut audit,
                    ApiError::conflict(format!(
                        "capability grant `{}` was no longer available for this request",
                        grant.grant_id
                    )),
                    "capability_grant_unavailable",
                    json!({
                        "stage": "capability_grant_consume",
                        "grant_id": grant.grant_id,
                    }),
                )
                .await;
            }
            Err(err) => {
                let _ = state
                    .agent_action_idempotency_store
                    .release_pending(
                        &normalized_action.tenant_id,
                        &normalized_action.agent_id,
                        &normalized_action.environment_id,
                        &normalized_action.idempotency_key,
                    )
                    .await;
                return reject_with_audit(
                    &state,
                    &mut audit,
                    ApiError::service_unavailable(format!(
                        "failed to consume capability grant: {err}"
                    )),
                    "capability_grant_consume_failed",
                    json!({
                        "stage": "capability_grant_consume",
                        "grant_id": grant.grant_id,
                    }),
                )
                .await;
            }
        }
    } else {
        None
    };

    audit.details_json = json!({
        "agent_action_request_id": normalized_action.action_request_id,
        "agent_id": normalized_action.agent_id,
        "environment_id": normalized_action.environment_id,
        "intent_type": normalized_action.intent_type.as_str(),
        "execution_mode": normalized_action.execution_mode.as_str(),
        "execution_owner": normalized_action.execution_mode.owner_label(),
        "adapter_type": normalized_action.adapter_type,
        "policy_decision": policy_decision.final_effect,
        "policy_explanation": policy_decision.explanation,
        "policy_bundle_id": policy_decision.published_bundle_id,
        "policy_bundle_version": policy_decision.published_bundle_version,
        "effective_scope": effective_scope,
        "policy_obligations": policy_decision.obligations,
        "policy_matched_rules": policy_decision.matched_rules,
        "quota_plan": quota_check.profile.plan.as_str(),
        "quota_access_mode": quota_check.profile.access_mode.as_str(),
        "quota_used_requests_before_submit": quota_check.used_requests,
        "execution_policy": quota_check.profile.execution_policy.as_str(),
        "execution_policy_effective": effective_execution_policy.as_str(),
        "execution_policy_enforced": policy_enforced,
        "signed_tx_present": signed_tx_present,
        "signing_mode": signing_mode,
        "payer_source": payer_source,
        "fee_payer": fee_payer,
        "requested_scope": normalized_action.requested_scope,
        "effective_scope_before_submit": effective_scope,
        "submitted_by": normalized_action.submitted_by,
        "grant_id": consumed_grant.as_ref().map(|value| value.grant.grant_id.clone()),
        "grant_uses_remaining": consumed_grant.as_ref().map(|value| value.uses_remaining),
        "grant_source_approval_request_id": consumed_grant
            .as_ref()
            .map(|value| value.grant.source_approval_request_id.clone()),
    });

    if normalized_action.execution_mode == AgentExecutionMode::ModeBScopedRuntime {
        if let Err(error) = state
            .agent_action_idempotency_store
            .finalize_runtime_authorization(
                &normalized_action.tenant_id,
                &normalized_action.agent_id,
                &normalized_action.environment_id,
                &normalized_action.idempotency_key,
                normalized_action.execution_mode,
                &normalized_action.adapter_type,
                state.clock.now_ms(),
            )
            .await
        {
            let _ = state
                .agent_action_idempotency_store
                .release_pending(
                    &normalized_action.tenant_id,
                    &normalized_action.agent_id,
                    &normalized_action.environment_id,
                    &normalized_action.idempotency_key,
                )
                .await;
            return reject_with_audit(
                &state,
                &mut audit,
                ApiError::service_unavailable(format!(
                    "failed to finalize runtime authorization: {error}"
                )),
                "runtime_authorization_finalize_failed",
                json!({ "stage": "runtime_authorization_finalize" }),
            )
            .await;
        }

        audit.validation_result = "runtime_authorized".to_owned();
        audit.rejection_reason = None;
        audit.error_status = None;
        audit.error_message = None;
        audit.idempotency_decision = Some("accepted_new".to_owned());
        persist_intake_audit_async(&state, audit.clone());
        return Ok(Json(build_runtime_authorized_agent_action_response(
            &normalized_action,
            "accepted_new",
            &policy_decision,
            consumed_grant.as_ref(),
            None,
        )));
    }

    let result = submit_agent_execution_intent(
        &state,
        AgentExecutionSubmitContext {
            tenant_id: &tenant_id,
            normalized_intent_kind: &normalized_action.normalized_intent_kind,
            normalized_payload: &normalized_action.normalized_payload,
            request_id: request_id.clone(),
            correlation_id: correlation_id.clone(),
            idempotency_key: &normalized_action.idempotency_key,
            submitter: &submitter,
            resolved_agent: &resolved_agent,
            agent_status: Some(resolved_agent.status.as_str()),
            entry_channel: "api",
            action_request_id: &normalized_action.action_request_id,
            intent_type: normalized_action.intent_type.as_str(),
            execution_mode: normalized_action.execution_mode,
            adapter_type: &normalized_action.adapter_type,
            requested_scope: &normalized_action.requested_scope,
            effective_scope: &effective_scope,
            reason: &normalized_action.reason,
            submitted_by: &normalized_action.submitted_by,
            policy_decision: &policy_decision,
            callback_config: normalized_action.callback_config.as_ref(),
            metering_scope: metering_scope.as_deref(),
            base_execution_policy: quota_check.profile.execution_policy,
            effective_execution_policy,
            signing_mode,
            payer_source,
            fee_payer: &fee_payer,
            approval: None,
            grant: consumed_grant.as_ref().map(|value| &value.grant),
        },
    )
    .await
    .map(Json);

    match result {
        Ok(Json(response)) => {
            audit.mark_accepted(&response);
            if let Err(error) = state
                .agent_action_idempotency_store
                .finalize_success(
                    &normalized_action.tenant_id,
                    &normalized_action.agent_id,
                    &normalized_action.environment_id,
                    &normalized_action.idempotency_key,
                    &response,
                    state.clock.now_ms(),
                )
                .await
            {
                warn!(
                    error = %error,
                    tenant_id = %normalized_action.tenant_id,
                    agent_id = %normalized_action.agent_id,
                    action_request_id = %normalized_action.action_request_id,
                    "failed to finalize agent action idempotency record after successful submit"
                );
            }
            persist_intake_audit_async(&state, audit.clone());
            Ok(Json(build_agent_action_response(
                &normalized_action,
                "accepted_new",
                &policy_decision,
                consumed_grant.as_ref(),
                None,
                Some(&response),
            )))
        }
        Err(err) => {
            let _ = state
                .agent_action_idempotency_store
                .release_pending(
                    &normalized_action.tenant_id,
                    &normalized_action.agent_id,
                    &normalized_action.environment_id,
                    &normalized_action.idempotency_key,
                )
                .await;
            let reason = classify_ingress_rejection_reason(&err);
            reject_with_audit(
                &state,
                &mut audit,
                err,
                reason,
                json!({ "stage": "submit_intent" }),
            )
            .await
        }
    }
}

async fn submit_agent_gateway_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<AgentGatewayRequest>,
) -> Result<Json<AgentGatewayResponse>, ApiError> {
    let request_id = header_opt(&headers, "x-request-id")
        .map(RequestId::from)
        .unwrap_or_else(RequestId::new);
    let mut audit = IngressIntakeAuditRecord::new(
        request_id.to_string(),
        IngressChannel::Api,
        "/api/agent/gateway/requests",
        "POST",
        state.clock.now_ms(),
    );

    let (tenant_id, submitter, resolved_agent) =
        match authenticate_and_resolve_agent_runtime_submitter(&state, &headers, &mut audit).await {
            Ok(value) => value,
            Err(err) => {
                let reason = match err.status {
                    StatusCode::UNAUTHORIZED => "submitter_auth_failed",
                    StatusCode::FORBIDDEN => "agent_identity_unresolved",
                    _ => "gateway_submitter_invalid",
                };
                return reject_with_audit(
                    &state,
                    &mut audit,
                    err,
                    reason,
                    json!({ "stage": "gateway_auth" }),
                )
                .await;
            }
        };

    let compilation =
        match compile_agent_gateway_request(payload, &tenant_id, &resolved_agent, &submitter) {
            Ok(value) => value,
            Err(err) => {
                return reject_with_audit(
                    &state,
                    &mut audit,
                    err,
                    "gateway_compilation_failed",
                    json!({ "stage": "gateway_compile" }),
                )
                .await;
            }
        };

    let normalized = match normalize_agent_action_request(
        compilation.compiled_request.clone(),
        &tenant_id,
        &resolved_agent,
    ) {
        Ok(value) => value,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "gateway_handoff_invalid",
                json!({
                    "stage": "gateway_prevalidate",
                    "mode": compilation.mode.clone(),
                    "trace": compilation.trace.clone(),
                }),
            )
            .await;
        }
    };
    audit.intent_kind = Some(normalized.normalized_intent_kind);
    audit.idempotency_key = Some(compilation.compiled_request.idempotency_key.clone());
    audit.correlation_id = header_opt(&headers, "x-correlation-id");

    let handoff = match submit_agent_action_request(
        State(state.clone()),
        headers.clone(),
        Json(compilation.compiled_request.clone()),
    )
    .await
    {
        Ok(Json(response)) => response,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "gateway_handoff_failed",
                json!({
                    "stage": "gateway_handoff",
                    "mode": compilation.mode.clone(),
                    "trace": compilation.trace.clone(),
                }),
            )
            .await;
        }
    };

    audit.validation_result = "compiled".to_owned();
    audit.rejection_reason = None;
    audit.error_status = None;
    audit.error_message = None;
    audit.idempotency_decision = Some(handoff.idempotency_decision.clone());
    audit.details_json = json!({
        "stage": "gateway_handoff",
        "mode": compilation.mode.clone(),
        "summary": compilation.summary.clone(),
        "trace": compilation.trace.clone(),
        "compiled_intent_type": compilation.compiled_request.intent_type.clone(),
        "compiled_adapter_type": compilation.compiled_request.adapter_type.clone(),
        "handoff_policy_decision": handoff.policy_decision.clone(),
        "handoff_approval_request_id": handoff.approval_request_id.clone(),
        "handoff_grant_id": handoff.grant_id.clone(),
        "handoff_intent_id": handoff.intent_id.clone(),
        "handoff_job_id": handoff.job_id.clone(),
    });
    persist_intake_audit_async(&state, audit.clone());

    Ok(Json(AgentGatewayResponse {
        ok: true,
        gateway_request_id: request_id.to_string(),
        compilation,
        handoff,
    }))
}

async fn submit_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SubmitIntentRequest>,
) -> Result<Json<SubmitIntentResponse>, ApiError> {
    let request_id = header_opt(&headers, "x-request-id")
        .map(RequestId::from)
        .unwrap_or_else(RequestId::new);
    let mut audit = IngressIntakeAuditRecord::new(
        request_id.to_string(),
        IngressChannel::Api,
        "/api/requests",
        "POST",
        state.clock.now_ms(),
    );

    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(value) => value,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "missing_tenant_header",
                json!({ "stage": "tenant_header" }),
            )
            .await
        }
    };
    audit.set_tenant(&tenant_id);

    let mut submitter =
        match state
            .auth
            .authenticate_submitter(&tenant_id, &headers, IngressChannel::Api)
        {
            Ok(value) => value,
            Err(err) => {
                return reject_with_audit(
                    &state,
                    &mut audit,
                    err,
                    "submitter_auth_failed",
                    json!({ "stage": "auth" }),
                )
                .await
            }
        };
    if let Err(err) =
        authorize_submitter_api_key_if_required(&state, &tenant_id, &headers, &submitter).await
    {
        return reject_with_audit(
            &state,
            &mut audit,
            err,
            "submitter_api_key_invalid",
            json!({ "stage": "auth_api_key" }),
        )
        .await;
    }
    if matches!(submitter.kind, SubmitterKind::AgentRuntime) {
        return reject_with_audit(
            &state,
            &mut audit,
            ApiError::forbidden("agent_runtime submitters must use /api/agent/action-requests"),
            "agent_action_contract_required",
            json!({ "stage": "agent_contract" }),
        )
        .await;
    }
    let resolved_agent = match resolve_agent_identity_for_submitter(
        &state, &tenant_id, &headers, &submitter,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "agent_identity_unresolved",
                json!({ "stage": "agent_resolution" }),
            )
            .await
        }
    };
    submitter.resolved_agent = resolved_agent;
    audit.set_submitter(&submitter);
    let metering_scope = payload
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("metering.scope"))
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    let quota_check = match state
        .quota_store
        .enforce_submit_allowed(&tenant_id, state.clock.now_ms(), metering_scope.as_deref())
        .await
    {
        Ok(check) => check,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "quota_rejected",
                json!({ "stage": "quota_enforcement" }),
            )
            .await
        }
    };

    let correlation_id = header_opt(&headers, "x-correlation-id");
    let idempotency_key = header_opt(&headers, "x-idempotency-key");
    audit.correlation_id = correlation_id.clone();
    audit.idempotency_key = idempotency_key.clone();

    let intent_kind = payload.intent_kind.trim().to_owned();
    if intent_kind.is_empty() {
        return reject_with_audit(
            &state,
            &mut audit,
            ApiError::bad_request("intent_kind is required"),
            "missing_intent_kind",
            json!({ "stage": "validate_request" }),
        )
        .await;
    }
    audit.intent_kind = Some(intent_kind.clone());
    let policy_enforced = state
        .execution_policy_enforcement
        .is_enforced_for_tenant(&tenant_id);
    let effective_execution_policy = resolve_effective_execution_policy(
        quota_check.profile.execution_policy,
        &submitter,
        metering_scope.as_deref(),
    );
    if policy_enforced {
        if matches!(effective_execution_policy, ExecutionPolicy::Sponsored)
            && quota_check.used_requests >= quota_check.profile.sponsored_monthly_cap_requests
        {
            return reject_with_audit(
                &state,
                &mut audit,
                ApiError::too_many_requests(format!(
                    "EXECUTION_POLICY_SPONSORED_CAP_EXCEEDED: tenant `{tenant_id}` reached sponsored cap ({}/{}) in the last 30 days",
                    quota_check.used_requests, quota_check.profile.sponsored_monthly_cap_requests
                )),
                "execution_policy_cap_exceeded",
                json!({ "stage": "execution_policy" }),
            )
            .await;
        }
        if let Err(err) = enforce_execution_policy_for_payload(
            effective_execution_policy,
            &intent_kind,
            &payload.payload,
        ) {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "execution_policy_rejected",
                json!({ "stage": "execution_policy" }),
            )
            .await;
        }
    }
    let signed_tx_present = signed_tx_payload_present(&payload.payload);
    let signing_mode = resolve_signing_mode(effective_execution_policy, signed_tx_present);
    let payer_source = resolve_payer_source(effective_execution_policy, signed_tx_present);
    let fee_payer =
        extract_fee_payer_hint(&payload.payload).unwrap_or_else(|| "unknown".to_owned());
    audit.details_json = json!({
        "quota_plan": quota_check.profile.plan.as_str(),
        "quota_access_mode": quota_check.profile.access_mode.as_str(),
        "quota_free_play_limit": quota_check.profile.free_play_limit,
        "quota_used_requests_before_submit": quota_check.used_requests,
        "execution_policy": quota_check.profile.execution_policy.as_str(),
        "execution_policy_effective": effective_execution_policy.as_str(),
        "execution_policy_playground_override": effective_execution_policy != quota_check.profile.execution_policy,
        "execution_policy_enforced": policy_enforced,
        "sponsored_monthly_cap_requests": quota_check.profile.sponsored_monthly_cap_requests,
        "signed_tx_present": signed_tx_present,
        "signing_mode": signing_mode,
        "payer_source": payer_source,
        "fee_payer": fee_payer,
        "metering_scope": metering_scope
            .clone()
            .unwrap_or_else(|| "billable".to_owned()),
        "agent_id": submitter.resolved_agent.as_ref().map(|agent| agent.agent_id.clone()),
        "environment_id": submitter
            .resolved_agent
            .as_ref()
            .map(|agent| agent.environment_id.clone()),
        "runtime_type": submitter
            .resolved_agent
            .as_ref()
            .map(|agent| agent.runtime_type.clone()),
        "runtime_identity": submitter
            .resolved_agent
            .as_ref()
            .map(|agent| agent.runtime_identity.clone()),
        "trust_tier": submitter
            .resolved_agent
            .as_ref()
            .map(|agent| agent.trust_tier.clone()),
        "risk_tier": submitter
            .resolved_agent
            .as_ref()
            .map(|agent| agent.risk_tier.clone()),
    });

    let mut metadata = payload.metadata.unwrap_or_default();
    metadata.insert("ingress.channel".to_owned(), "api".to_owned());
    metadata.insert("request_id".to_owned(), request_id.to_string());
    if let Some(value) = correlation_id.as_ref() {
        metadata.insert("correlation_id".to_owned(), value.clone());
    }
    if let Some(value) = idempotency_key.as_ref() {
        metadata.insert("idempotency_key".to_owned(), value.clone());
    }
    metadata.insert(
        "submitter.principal_id".to_owned(),
        submitter.principal_id.clone(),
    );
    metadata.insert(
        "submitter.kind".to_owned(),
        submitter.kind.as_str().to_owned(),
    );
    if let Some(agent) = submitter.resolved_agent.as_ref() {
        metadata.insert("agent.id".to_owned(), agent.agent_id.clone());
        metadata.insert(
            "agent.environment_id".to_owned(),
            agent.environment_id.clone(),
        );
        metadata.insert(
            "agent.environment_kind".to_owned(),
            agent.environment_kind.clone(),
        );
        metadata.insert("agent.runtime_type".to_owned(), agent.runtime_type.clone());
        metadata.insert(
            "agent.runtime_identity".to_owned(),
            agent.runtime_identity.clone(),
        );
        metadata.insert("agent.trust_tier".to_owned(), agent.trust_tier.clone());
        metadata.insert("agent.risk_tier".to_owned(), agent.risk_tier.clone());
        metadata.insert("agent.owner_team".to_owned(), agent.owner_team.clone());
        metadata.insert("agent.status".to_owned(), agent.status.clone());
    }
    metadata.insert(
        "execution.policy".to_owned(),
        effective_execution_policy.as_str().to_owned(),
    );
    metadata.insert(
        "execution.policy.base".to_owned(),
        quota_check.profile.execution_policy.as_str().to_owned(),
    );
    metadata.insert("execution.signing_mode".to_owned(), signing_mode.to_owned());
    metadata.insert("execution.payer_source".to_owned(), payer_source.to_owned());
    metadata.insert("execution.fee_payer".to_owned(), fee_payer);

    let result = submit_normalized_intent(
        &state,
        tenant_id,
        IntentKind::new(intent_kind),
        payload.payload,
        request_id.clone(),
        correlation_id,
        idempotency_key,
        Some(AuthContext {
            principal_id: Some(submitter.principal_id),
            submitter_kind: Some(submitter.kind.as_str().to_owned()),
            auth_scheme: Some(submitter.auth_scheme),
            channel: Some("api".to_owned()),
            agent_id: submitter
                .resolved_agent
                .as_ref()
                .map(|agent| agent.agent_id.clone()),
            environment_id: submitter
                .resolved_agent
                .as_ref()
                .map(|agent| agent.environment_id.clone()),
            runtime_type: submitter
                .resolved_agent
                .as_ref()
                .map(|agent| agent.runtime_type.clone()),
            runtime_identity: submitter
                .resolved_agent
                .as_ref()
                .map(|agent| agent.runtime_identity.clone()),
            trust_tier: submitter
                .resolved_agent
                .as_ref()
                .map(|agent| agent.trust_tier.clone()),
            risk_tier: submitter
                .resolved_agent
                .as_ref()
                .map(|agent| agent.risk_tier.clone()),
        }),
        metadata,
    )
    .await;

    match result {
        Ok(Json(response)) => {
            audit.mark_accepted(&response);
            persist_intake_audit_async(&state, audit.clone());
            Ok(Json(response))
        }
        Err(err) => {
            let reason = classify_ingress_rejection_reason(&err);
            reject_with_audit(
                &state,
                &mut audit,
                err,
                reason,
                json!({ "stage": "submit_intent" }),
            )
            .await
        }
    }
}

async fn submit_webhook(
    State(state): State<AppState>,
    Path(source): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<SubmitIntentResponse>, ApiError> {
    let request_id = header_opt(&headers, "x-request-id")
        .map(RequestId::from)
        .unwrap_or_else(RequestId::new);
    let mut audit = IngressIntakeAuditRecord::new(
        request_id.to_string(),
        IngressChannel::Webhook,
        "/webhooks/:source",
        "POST",
        state.clock.now_ms(),
    );

    let tenant_id = match tenant_id_from_headers(&headers) {
        Ok(value) => value,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "missing_tenant_header",
                json!({ "stage": "tenant_header" }),
            )
            .await
        }
    };
    audit.set_tenant(&tenant_id);

    let submitter =
        match state
            .auth
            .authenticate_submitter(&tenant_id, &headers, IngressChannel::Webhook)
        {
            Ok(value) => value,
            Err(err) => {
                return reject_with_audit(
                    &state,
                    &mut audit,
                    err,
                    "submitter_auth_failed",
                    json!({ "stage": "auth" }),
                )
                .await
            }
        };
    if let Err(err) =
        authorize_submitter_api_key_if_required(&state, &tenant_id, &headers, &submitter).await
    {
        return reject_with_audit(
            &state,
            &mut audit,
            err,
            "submitter_api_key_invalid",
            json!({ "stage": "auth_api_key" }),
        )
        .await;
    }
    audit.set_submitter(&submitter);

    let quota_check = match state
        .quota_store
        .enforce_submit_allowed(&tenant_id, state.clock.now_ms(), None)
        .await
    {
        Ok(check) => check,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "quota_rejected",
                json!({ "stage": "quota_enforcement" }),
            )
            .await
        }
    };

    let correlation_id = header_opt(&headers, "x-correlation-id");
    let idempotency_key = header_opt(&headers, "x-idempotency-key");
    audit.correlation_id = correlation_id.clone();
    audit.idempotency_key = idempotency_key.clone();

    let normalized_source = sanitize_source(&source);
    if let Err(err) = verify_webhook_signature_for_tenant(
        &state,
        &tenant_id,
        &normalized_source,
        &headers,
        &body,
        matches!(submitter.kind, SubmitterKind::SignedWebhookSender),
    )
    .await
    {
        return reject_with_audit(
            &state,
            &mut audit,
            err,
            "webhook_signature_invalid",
            json!({ "stage": "signature_verification" }),
        )
        .await;
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(err) => {
            return reject_with_audit(
                &state,
                &mut audit,
                ApiError::bad_request(format!("invalid webhook json payload: {err}")),
                "invalid_webhook_json",
                json!({ "stage": "parse_payload" }),
            )
            .await
        }
    };

    let intent_kind = header_opt(&headers, "x-intent-kind")
        .filter(|kind| !kind.trim().is_empty())
        .unwrap_or_else(|| format!("webhook.{}.v1", sanitize_source(&source)));
    audit.intent_kind = Some(intent_kind.clone());

    let policy_enforced = state
        .execution_policy_enforcement
        .is_enforced_for_tenant(&tenant_id);
    let effective_execution_policy =
        resolve_effective_execution_policy(quota_check.profile.execution_policy, &submitter, None);
    if policy_enforced {
        if matches!(effective_execution_policy, ExecutionPolicy::Sponsored)
            && quota_check.used_requests >= quota_check.profile.sponsored_monthly_cap_requests
        {
            return reject_with_audit(
                &state,
                &mut audit,
                ApiError::too_many_requests(format!(
                    "EXECUTION_POLICY_SPONSORED_CAP_EXCEEDED: tenant `{tenant_id}` reached sponsored cap ({}/{}) in the last 30 days",
                    quota_check.used_requests, quota_check.profile.sponsored_monthly_cap_requests
                )),
                "execution_policy_cap_exceeded",
                json!({ "stage": "execution_policy" }),
            )
            .await;
        }
        if let Err(err) =
            enforce_execution_policy_for_payload(effective_execution_policy, &intent_kind, &payload)
        {
            return reject_with_audit(
                &state,
                &mut audit,
                err,
                "execution_policy_rejected",
                json!({ "stage": "execution_policy" }),
            )
            .await;
        }
    }
    let signed_tx_present = signed_tx_payload_present(&payload);
    let signing_mode = resolve_signing_mode(effective_execution_policy, signed_tx_present);
    let payer_source = resolve_payer_source(effective_execution_policy, signed_tx_present);
    let fee_payer = extract_fee_payer_hint(&payload).unwrap_or_else(|| "unknown".to_owned());
    audit.details_json = json!({
        "quota_plan": quota_check.profile.plan.as_str(),
        "quota_access_mode": quota_check.profile.access_mode.as_str(),
        "quota_free_play_limit": quota_check.profile.free_play_limit,
        "quota_used_requests_before_submit": quota_check.used_requests,
        "execution_policy": quota_check.profile.execution_policy.as_str(),
        "execution_policy_effective": effective_execution_policy.as_str(),
        "execution_policy_playground_override": effective_execution_policy != quota_check.profile.execution_policy,
        "execution_policy_enforced": policy_enforced,
        "sponsored_monthly_cap_requests": quota_check.profile.sponsored_monthly_cap_requests,
        "signed_tx_present": signed_tx_present,
        "signing_mode": signing_mode,
        "payer_source": payer_source,
        "fee_payer": fee_payer,
    });

    let mut metadata = BTreeMap::new();
    metadata.insert("ingress.channel".to_owned(), "webhook".to_owned());
    metadata.insert("webhook.source".to_owned(), normalized_source.clone());
    metadata.insert("request_id".to_owned(), request_id.to_string());
    if let Some(value) = correlation_id.as_ref() {
        metadata.insert("correlation_id".to_owned(), value.clone());
    }
    if let Some(value) = idempotency_key.as_ref() {
        metadata.insert("idempotency_key".to_owned(), value.clone());
    }
    if let Some(value) = header_opt(&headers, "x-webhook-id") {
        metadata.insert("webhook_id".to_owned(), value);
    }
    metadata.insert(
        "submitter.principal_id".to_owned(),
        submitter.principal_id.clone(),
    );
    metadata.insert(
        "submitter.kind".to_owned(),
        submitter.kind.as_str().to_owned(),
    );
    metadata.insert(
        "execution.policy".to_owned(),
        effective_execution_policy.as_str().to_owned(),
    );
    metadata.insert(
        "execution.policy.base".to_owned(),
        quota_check.profile.execution_policy.as_str().to_owned(),
    );
    metadata.insert("execution.signing_mode".to_owned(), signing_mode.to_owned());
    metadata.insert("execution.payer_source".to_owned(), payer_source.to_owned());
    metadata.insert("execution.fee_payer".to_owned(), fee_payer);

    let result = submit_normalized_intent(
        &state,
        tenant_id,
        IntentKind::new(intent_kind),
        payload,
        request_id.clone(),
        correlation_id,
        idempotency_key,
        Some(AuthContext {
            principal_id: Some(submitter.principal_id),
            submitter_kind: Some(submitter.kind.as_str().to_owned()),
            auth_scheme: Some(submitter.auth_scheme),
            channel: Some("webhook".to_owned()),
            agent_id: None,
            environment_id: None,
            runtime_type: None,
            runtime_identity: None,
            trust_tier: None,
            risk_tier: None,
        }),
        metadata,
    )
    .await;

    match result {
        Ok(Json(response)) => {
            audit.mark_accepted(&response);
            persist_intake_audit_async(&state, audit.clone());
            Ok(Json(response))
        }
        Err(err) => {
            let reason = classify_ingress_rejection_reason(&err);
            reject_with_audit(
                &state,
                &mut audit,
                err,
                reason,
                json!({ "stage": "submit_intent", "webhook_source": normalized_source }),
            )
            .await
        }
    }
}

async fn submit_normalized_intent(
    state: &AppState,
    tenant_id: String,
    kind: IntentKind,
    payload: Value,
    request_id: RequestId,
    correlation_id: Option<String>,
    idempotency_key: Option<String>,
    auth_context: Option<AuthContext>,
    metadata: BTreeMap<String, String>,
) -> Result<Json<SubmitIntentResponse>, ApiError> {
    state
        .schemas
        .validate_intent_payload(kind.as_str(), &payload)?;

    let intent = NormalizedIntent {
        request_id: Some(request_id),
        intent_id: IntentId::new(),
        tenant_id: TenantId::from(tenant_id.clone()),
        kind,
        payload,
        correlation_id,
        idempotency_key,
        auth_context,
        metadata,
        received_at_ms: state.clock.now_ms(),
    };

    let submitted = submit_intent_with_duplicate_retry(&state.core, intent)
        .await
        .map_err(map_core_error)?;

    Ok(Json(SubmitIntentResponse {
        ok: true,
        tenant_id,
        intent_id: submitted.job.intent_id.to_string(),
        job_id: submitted.job.job_id.to_string(),
        adapter_id: submitted.job.adapter_id.to_string(),
        state: format!("{:?}", submitted.job.state),
        route_rule: submitted.route_rule,
    }))
}

#[derive(Debug, Clone)]
struct RouteMapping {
    intent_kind: String,
    adapter_id: String,
}

fn parse_route_map(raw: String) -> Result<Vec<RouteMapping>, anyhow::Error> {
    let mut out = Vec::new();
    for part in raw.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (kind, adapter) = trimmed
            .split_once('=')
            .with_context(|| format!("invalid route entry `{trimmed}` (expected kind=adapter)"))?;
        let kind = kind.trim().to_owned();
        let adapter = adapter.trim().to_owned();
        if kind.is_empty() || adapter.is_empty() {
            anyhow::bail!("invalid route entry `{trimmed}`");
        }
        out.push(RouteMapping {
            intent_kind: kind,
            adapter_id: adapter,
        });
    }
    if out.is_empty() {
        anyhow::bail!("INGRESS_INTENT_ROUTES is empty");
    }
    Ok(out)
}

fn parse_intent_schema_map(raw: &str) -> Result<Vec<(String, String)>, anyhow::Error> {
    let mut out = Vec::new();
    for part in raw.split(';') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let (intent_kind, schema_id) = trimmed.split_once('=').with_context(|| {
            format!("invalid schema entry `{trimmed}` (expected intent_kind=schema_id)")
        })?;
        let intent_kind = intent_kind.trim().to_owned();
        let schema_id = schema_id.trim().to_owned();
        if intent_kind.is_empty() || schema_id.is_empty() {
            anyhow::bail!("invalid schema entry `{trimmed}`");
        }
        out.push((intent_kind, schema_id));
    }
    Ok(out)
}

fn tenant_id_from_headers(headers: &HeaderMap) -> Result<String, ApiError> {
    header_opt(headers, "x-tenant-id")
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::unauthorized("missing x-tenant-id"))
}

fn compute_webhook_signature(secret: &str, body: &[u8]) -> Result<String, ApiError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|err| ApiError::internal(format!("invalid webhook signing secret: {err}")))?;
    mac.update(body);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

fn compute_slack_signature(
    signing_secret: &str,
    timestamp: &str,
    body: &[u8],
) -> Result<String, ApiError> {
    let payload = format!("v0:{timestamp}:{}", String::from_utf8_lossy(body));
    let mut mac = HmacSha256::new_from_slice(signing_secret.as_bytes())
        .map_err(|err| ApiError::internal(format!("invalid slack signing secret: {err}")))?;
    mac.update(payload.as_bytes());
    Ok(format!("v0={}", hex::encode(mac.finalize().into_bytes())))
}

fn verify_slack_signature(
    state: &AppState,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), ApiError> {
    let signing_secret = state
        .approval_workflow
        .slack_signing_secret
        .as_deref()
        .ok_or_else(|| ApiError::service_unavailable("slack signing secret is not configured"))?;
    let timestamp = headers
        .get("x-slack-request-timestamp")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("missing x-slack-request-timestamp"))?;
    let signature = headers
        .get("x-slack-signature")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::unauthorized("missing x-slack-signature"))?;
    let timestamp_seconds = timestamp
        .parse::<i64>()
        .map_err(|_| ApiError::unauthorized("invalid x-slack-request-timestamp"))?;
    let now_seconds = (state.clock.now_ms() / 1000) as i64;
    if (now_seconds - timestamp_seconds).abs() > 300 {
        return Err(ApiError::unauthorized("stale slack signature timestamp"));
    }
    let expected = compute_slack_signature(signing_secret, timestamp, body)?;
    if constant_time_eq(signature.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        Err(ApiError::unauthorized("invalid slack signature"))
    }
}

fn parse_slack_approval_payload(body: &[u8]) -> Result<ApprovalSlackActionPayload, ApiError> {
    let envelope = serde_urlencoded::from_bytes::<SlackInteractivityEnvelope>(body)
        .map_err(|err| ApiError::bad_request(format!("invalid slack interactivity body: {err}")))?;
    let payload = serde_json::from_str::<SlackInteractiveCallbackPayload>(&envelope.payload)
        .map_err(|err| ApiError::bad_request(format!("invalid slack payload json: {err}")))?;
    let action = payload
        .actions
        .first()
        .ok_or_else(|| ApiError::bad_request("slack approval action is missing"))?;
    let decision = match action.action_id.as_str() {
        "approval:approve" => ApprovalDecisionKind::Approve,
        "approval:reject" => ApprovalDecisionKind::Reject,
        "approval:escalate" => ApprovalDecisionKind::Escalate,
        other => {
            return Err(ApiError::bad_request(format!(
                "unsupported slack approval action `{other}`"
            )))
        }
    };
    Ok(ApprovalSlackActionPayload {
        user_id: normalize_registry_key(&payload.user.id, "slack user id", 128)?,
        user_name: payload.user.username.or(payload.user.name),
        approval_request_id: normalize_registry_key(&action.value, "approval_request_id", 128)?,
        decision,
    })
}

async fn deliver_approval_request_to_slack(
    state: &AppState,
    approval: &ApprovalRequestRecord,
) -> Result<(), anyhow::Error> {
    let webhook_url = state
        .approval_workflow
        .slack_webhook_url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("slack webhook url is not configured"))?;
    let summary = format!(
        "*Approval required* for `{}` `{}` in `{}` ({})",
        approval.intent_type,
        approval.action_request_id,
        approval.environment_id,
        approval.environment_kind
    );
    let payload = json!({
        "text": summary,
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!(
                        "{}\nTenant: `{}`\nAgent: `{}`\nScope: `{}`\nReason: {}",
                        summary,
                        approval.tenant_id,
                        approval.agent_id,
                        approval.effective_scope.join(", "),
                        approval.reason
                    )
                }
            },
            {
                "type": "context",
                "elements": [
                    {
                        "type": "mrkdwn",
                        "text": format!(
                            "Approval request `{}` expires at `{}`",
                            approval.approval_request_id,
                            approval.expires_at_ms
                        )
                    }
                ]
            },
            {
                "type": "actions",
                "elements": [
                    {
                        "type": "button",
                        "text": { "type": "plain_text", "text": "Approve" },
                        "style": "primary",
                        "action_id": "approval:approve",
                        "value": approval.approval_request_id
                    },
                    {
                        "type": "button",
                        "text": { "type": "plain_text", "text": "Reject" },
                        "style": "danger",
                        "action_id": "approval:reject",
                        "value": approval.approval_request_id
                    },
                    {
                        "type": "button",
                        "text": { "type": "plain_text", "text": "Escalate" },
                        "action_id": "approval:escalate",
                        "value": approval.approval_request_id
                    }
                ]
            }
        ]
    });
    let response = state
        .approval_http_client
        .post(webhook_url)
        .json(&payload)
        .send()
        .await
        .context("failed to call slack webhook")?;
    let response = response
        .error_for_status()
        .context("slack webhook returned error status")?;
    let _ = response.bytes().await;
    Ok(())
}

async fn verify_webhook_signature_for_tenant(
    state: &AppState,
    tenant_id: &str,
    source: &str,
    headers: &HeaderMap,
    body: &[u8],
    required: bool,
) -> Result<(), ApiError> {
    let signature_hex = headers
        .get("x-webhook-signature")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.strip_prefix("v1=").unwrap_or(value).to_owned());

    if let Some(secret) = state.auth.webhook_signature_secrets.get(tenant_id) {
        let Some(signature_hex) = signature_hex.as_deref() else {
            return Err(ApiError::unauthorized("missing x-webhook-signature"));
        };
        let expected = compute_webhook_signature(secret, body)?;
        if constant_time_eq(signature_hex.as_bytes(), expected.as_bytes()) {
            return Ok(());
        }
        return Err(ApiError::unauthorized("invalid webhook signature"));
    }

    let requested_key_id = header_opt(headers, "x-webhook-key-id")
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let now_ms = state.clock.now_ms();
    let candidates = state
        .webhook_key_store
        .load_active_signing_candidates(tenant_id, source, requested_key_id.as_deref(), now_ms)
        .await
        .map_err(|err| {
            ApiError::service_unavailable(format!(
                "webhook key store unavailable while verifying signature: {err}"
            ))
        })?;

    if candidates.is_empty() {
        if required {
            return Err(ApiError::unauthorized(
                "no webhook signing secret configured for tenant/source",
            ));
        }
        return Ok(());
    }

    let Some(signature_hex) = signature_hex.as_deref() else {
        return Err(ApiError::unauthorized("missing x-webhook-signature"));
    };

    for candidate in candidates {
        let expected = compute_webhook_signature(&candidate.secret_value, body)?;
        if constant_time_eq(signature_hex.as_bytes(), expected.as_bytes()) {
            if let Err(error) = state
                .webhook_key_store
                .mark_key_used(&candidate.key_id, now_ms)
                .await
            {
                warn!(
                    error = %error,
                    tenant_id = %tenant_id,
                    source = %source,
                    key_id = %candidate.key_id,
                    "failed to update webhook key usage timestamp"
                );
            }
            return Ok(());
        }
    }

    Err(ApiError::unauthorized("invalid webhook signature"))
}

fn normalize_webhook_source(raw: Option<&str>) -> Option<String> {
    let value = raw.unwrap_or("default").trim().to_ascii_lowercase();
    if value.is_empty() || value.len() > 64 {
        return None;
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        return None;
    }
    Some(value)
}

fn sanitize_source(value: &str) -> String {
    let normalized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = normalized.trim_matches('_').to_owned();
    if cleaned.is_empty() {
        "unknown".to_owned()
    } else {
        cleaned
    }
}

async fn persist_intake_audit(state: &AppState, record: &IngressIntakeAuditRecord) {
    if let Err(err) = state.audit_store.record(record).await {
        warn!(error = %err, "failed to persist ingress intake audit row");
    }
}

fn persist_intake_audit_async(state: &AppState, record: IngressIntakeAuditRecord) {
    let audit_store = state.audit_store.clone();
    tokio::spawn(async move {
        if let Err(err) = audit_store.record(&record).await {
            warn!(error = %err, "failed to persist ingress intake audit row");
        }
    });
}

async fn reject_with_audit<T>(
    state: &AppState,
    record: &mut IngressIntakeAuditRecord,
    err: ApiError,
    reason: &str,
    details_json: Value,
) -> Result<T, ApiError> {
    record.mark_rejected(reason, &err, details_json);
    persist_intake_audit(state, record).await;
    Err(err)
}

fn classify_ingress_rejection_reason(err: &ApiError) -> &'static str {
    match err.status {
        StatusCode::BAD_REQUEST => {
            let lower = err.message.to_ascii_lowercase();
            if lower.contains("payload does not match intent schema")
                || lower.contains("missing payload schema")
            {
                "schema_validation_failed"
            } else if lower.contains("unsupported intent") {
                "unsupported_intent"
            } else {
                "bad_request"
            }
        }
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::CONFLICT => {
            if err.message.to_ascii_lowercase().contains("idempotency") {
                "idempotency_conflict"
            } else {
                "conflict"
            }
        }
        StatusCode::TOO_MANY_REQUESTS => "quota_exceeded",
        StatusCode::SERVICE_UNAVAILABLE => "core_unavailable",
        StatusCode::INTERNAL_SERVER_ERROR => "internal_error",
        _ => "ingress_rejected",
    }
}

fn idempotency_decision_for_route_rule(route_rule: &str, has_idempotency_key: bool) -> String {
    if !has_idempotency_key {
        return "not_provided".to_owned();
    }

    if route_rule.starts_with("idempotency_reuse:") {
        "reused_existing".to_owned()
    } else {
        "accepted_new".to_owned()
    }
}

fn map_core_error(err: CoreError) -> ApiError {
    match err {
        CoreError::UnsupportedIntent(kind) => {
            ApiError::bad_request(format!("unsupported intent `{}`", kind.as_str()))
        }
        CoreError::AdapterRoutingDenied {
            tenant_id,
            adapter_id,
        } => ApiError::forbidden(format!(
            "adapter `{adapter_id}` is not allowed for tenant `{tenant_id}`"
        )),
        CoreError::IllegalTransition { from, to } => {
            ApiError::conflict(format!("illegal transition {from:?} -> {to:?}"))
        }
        CoreError::JobNotFound(job_id) => ApiError::not_found(format!("job `{job_id}` not found")),
        CoreError::IntentNotFound(intent_id) => {
            ApiError::not_found(format!("intent `{intent_id}` not found"))
        }
        CoreError::TenantMismatch {
            job_id,
            expected,
            actual,
        } => ApiError::forbidden(format!(
            "tenant mismatch on job `{job_id}` (expected `{expected}`, got `{actual}`)"
        )),
        CoreError::UnauthorizedReplay { principal_id } => {
            ApiError::forbidden(format!("unauthorized replay by `{principal_id}`"))
        }
        CoreError::ReplayDenied { reason } => ApiError::conflict(reason),
        CoreError::IdempotencyConflict { key, reason } => {
            ApiError::conflict(format!("idempotency conflict for key `{key}`: {reason}"))
        }
        CoreError::UnauthorizedManualAction { principal_id } => {
            ApiError::forbidden(format!("unauthorized manual action by `{principal_id}`"))
        }
        CoreError::Store(err) => ApiError::service_unavailable(format!("store error: {err}")),
        CoreError::Routing(err) => ApiError::service_unavailable(format!("routing error: {err}")),
        CoreError::AdapterExecution(err) => {
            ApiError::service_unavailable(format!("adapter execution error: {err}"))
        }
        CoreError::Callback(err) => {
            ApiError::service_unavailable(format!("callback backend error: {err}"))
        }
    }
}

async fn submit_intent_with_duplicate_retry(
    core: &ExecutionCore,
    intent: NormalizedIntent,
) -> Result<execution_core::SubmitResult, CoreError> {
    let mut last_err = None;

    for attempt in 0..=25 {
        match core.submit_intent(intent.clone()).await {
            Ok(submitted) => return Ok(submitted),
            Err(err) if is_duplicate_submit_gap(&err) && attempt < 25 => {
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(err) => return Err(err),
        }
    }

    Err(last_err.unwrap_or_else(|| {
        CoreError::Store(execution_core::ports::StoreError::Backend(
            "duplicate submit retry exhausted without a terminal result".to_owned(),
        ))
    }))
}

fn is_duplicate_submit_gap(err: &CoreError) -> bool {
    matches!(
        err,
        CoreError::IdempotencyConflict { reason, .. }
            if reason.contains("but no execution job was found")
    )
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    ok: bool,
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn too_many_requests(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: message.into(),
        }
    }

    fn service_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                ok: false,
                error: self.message,
            }),
        )
            .into_response()
    }
}

fn parse_submitter_kind(value: &str) -> Option<SubmitterKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "api_key_holder" | "api-key-holder" => Some(SubmitterKind::ApiKeyHolder),
        "agent_runtime" | "agent-runtime" => Some(SubmitterKind::AgentRuntime),
        "signed_webhook_sender" | "signed-webhook-sender" => {
            Some(SubmitterKind::SignedWebhookSender)
        }
        "internal_service" | "internal-service" => Some(SubmitterKind::InternalService),
        "wallet_backend" | "wallet-backend" => Some(SubmitterKind::WalletBackend),
        _ => None,
    }
}

fn parse_submitter_set(raw: Option<&str>) -> Result<HashSet<SubmitterKind>, anyhow::Error> {
    let mut out = HashSet::new();
    let Some(raw) = raw else {
        return Ok(out);
    };

    for part in raw.split(|ch| ch == ';' || ch == ',' || ch == '|') {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        let kind = parse_submitter_kind(trimmed).ok_or_else(|| {
            anyhow::anyhow!("unsupported submitter kind `{trimmed}` in ingress config")
        })?;
        out.insert(kind);
    }

    Ok(out)
}

fn parse_csv_set(raw: Option<&str>) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(raw) = raw else {
        return out;
    };
    for part in raw.split(|ch| ch == ';' || ch == ',' || ch == '|') {
        let value = part.trim();
        if !value.is_empty() {
            out.insert(value.to_owned());
        }
    }
    out
}

fn parse_principal_submitter_kind_map(raw: Option<&str>) -> HashMap<String, SubmitterKind> {
    let mut out = HashMap::new();
    for (principal, kind_raw) in parse_kv_map(raw) {
        if let Some(kind) = parse_submitter_kind(&kind_raw) {
            out.insert(principal, kind);
        }
    }
    out
}

fn parse_principal_tenant_map(raw: Option<&str>) -> HashMap<String, HashSet<String>> {
    shared_parse_principal_tenant_map(raw)
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

fn env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .and_then(|value| match value.as_str() {
            "1" | "true" | "yes" | "y" | "on" => Some(true),
            "0" | "false" | "no" | "n" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::{
        compile_agent_gateway_request, compute_slack_signature, ensure_scope_not_broader,
        evaluate_policy_layers, normalize_agent_action_request, normalize_connector_config,
        normalize_connector_secret_values, normalize_environment_kind,
        normalize_grant_spec_from_approval, normalize_policy_bundle_document,
        normalize_registry_status, parse_intent_schema_map, parse_principal_tenant_map,
        parse_slack_approval_payload, parse_submitter_set, validate_solana_intent_payload,
        AgentActionCallbackConfig, AgentGatewayRequest, AgentGatewayStructuredActionInput,
        AgentIntentType, ApprovalDecisionKind, ApprovalGrantRequest, ApprovalRequestRecord,
        ApprovalState, CapabilityGrantRecord, CapabilityGrantStatus, GrantWorkflowConfig,
        IngressSchemaRegistry, PolicyDecisionExplanation, PolicyEffect, PolicyEvaluationContext,
        PolicyRuleConditions, PolicyRuleDefinition, PolicyRuleObligations, RequestId,
        ResolvedAgentIdentity, RouteMapping, SubmitAgentActionRequest, SubmitterIdentity,
        SubmitterKind, TenantPolicyBundleRecord,
    };
    use serde_json::json;
    use std::collections::HashSet;

    #[test]
    fn solana_schema_accepts_valid_payload() {
        let payload = json!({
            "intent_id": "intent_123",
            "type": "transfer",
            "to_addr": "11111111111111111111111111111111",
            "amount": 1,
            "skip_preflight": false,
            "cu_limit": 200000,
            "cu_price_micro_lamports": 0
        });
        let result = validate_solana_intent_payload(&payload, false);
        assert!(result.is_ok(), "expected valid payload: {result:?}");
    }

    #[test]
    fn solana_schema_rejects_unknown_field() {
        let payload = json!({
            "to_addr": "11111111111111111111111111111111",
            "amount": 1,
            "extra": true
        });
        let result = validate_solana_intent_payload(&payload, false);
        let err = result.expect_err("expected schema validation error");
        assert!(
            err.contains("unknown field"),
            "expected unknown field error, got: {err}"
        );
    }

    #[test]
    fn solana_schema_rejects_missing_to_addr() {
        let payload = json!({
            "amount": 1
        });
        let result = validate_solana_intent_payload(&payload, false);
        let err = result.expect_err("expected missing to_addr error");
        assert!(
            err.contains("to_addr"),
            "expected to_addr error message, got: {err}"
        );
    }

    #[test]
    fn strict_registry_requires_route_schema_mapping() {
        let routes = vec![RouteMapping {
            intent_kind: "solana.transfer.v1".to_owned(),
            adapter_id: "adapter_solana".to_owned(),
        }];
        let mappings = vec![(
            "solana.broadcast.v1".to_owned(),
            "solana.broadcast.v1".to_owned(),
        )];
        let result = IngressSchemaRegistry::from_mappings(mappings, true, &routes);
        let err = result.expect_err("expected strict registry error");
        assert!(
            err.to_string().contains("missing schema mapping"),
            "unexpected strict mapping error: {err}"
        );
    }

    #[test]
    fn parse_schema_map_parses_entries() {
        let parsed = parse_intent_schema_map(
            "solana.transfer.v1=solana.transfer.v1;solana.broadcast.v1=solana.broadcast.v1",
        )
        .expect("expected schema map parsing to succeed");
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn parse_submitter_set_accepts_supported_kinds() {
        let parsed = parse_submitter_set(Some(
            "api_key_holder,agent_runtime,internal_service,wallet_backend",
        ))
        .expect("expected submitter set parsing to succeed");
        let expected: HashSet<SubmitterKind> = HashSet::from([
            SubmitterKind::ApiKeyHolder,
            SubmitterKind::AgentRuntime,
            SubmitterKind::InternalService,
            SubmitterKind::WalletBackend,
        ]);
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parse_submitter_set_rejects_unknown_kind() {
        let err = parse_submitter_set(Some("api_key_holder,unknown"))
            .expect_err("expected unknown submitter kind to fail");
        assert!(
            err.to_string().contains("unsupported submitter kind"),
            "unexpected parse error: {err}"
        );
    }

    #[test]
    fn parse_principal_tenant_map_supports_multi_tenant_binding() {
        let parsed =
            parse_principal_tenant_map(Some("svc-a=tenant_a|tenant_b;svc-b:tenant_c,tenant_d"));
        assert_eq!(parsed.len(), 2);
        assert!(
            parsed
                .get("svc-a")
                .map(|set| set.contains("tenant_a") && set.contains("tenant_b"))
                .unwrap_or(false),
            "expected svc-a tenant bindings"
        );
        assert!(
            parsed
                .get("svc-b")
                .map(|set| set.contains("tenant_c") && set.contains("tenant_d"))
                .unwrap_or(false),
            "expected svc-b tenant bindings"
        );
    }

    #[test]
    fn environment_kind_normalizes_prod_alias() {
        let normalized =
            normalize_environment_kind("prod").expect("expected prod alias to normalize");
        assert_eq!(normalized, "production");
    }

    #[test]
    fn registry_status_normalizes_archived_to_decommissioned() {
        let normalized = normalize_registry_status(Some("archived"), "status")
            .expect("expected archived alias to normalize");
        assert_eq!(normalized, "decommissioned");
    }

    #[test]
    fn normalize_agent_action_request_accepts_solana_transfer_contract() {
        let normalized = normalize_agent_action_request(
            SubmitAgentActionRequest {
                action_request_id: "act_123".to_owned(),
                tenant_id: "tenant_a".to_owned(),
                agent_id: "agent_1".to_owned(),
                environment_id: "prod".to_owned(),
                intent_type: "transfer".to_owned(),
                execution_mode: Some("mode_c_protected_execution".to_owned()),
                adapter_type: "adapter_solana".to_owned(),
                payload: json!({
                    "to_addr": "11111111111111111111111111111111",
                    "amount": 1
                }),
                idempotency_key: "idem_123".to_owned(),
                requested_scope: vec!["payments".to_owned()],
                reason: "customer transfer".to_owned(),
                callback_config: Some(AgentActionCallbackConfig {
                    url: Some("https://callback.example.com".to_owned()),
                    signing_secret_ref: None,
                    include_receipt: Some(true),
                }),
                submitted_by: "planner".to_owned(),
            },
            "tenant_a",
            &ResolvedAgentIdentity {
                agent_id: "agent_1".to_owned(),
                environment_id: "prod".to_owned(),
                environment_kind: "production".to_owned(),
                runtime_type: "openai".to_owned(),
                runtime_identity: "rt_1".to_owned(),
                status: "active".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "medium".to_owned(),
                owner_team: "ops".to_owned(),
            },
        )
        .expect("expected valid agent action request");

        assert_eq!(normalized.intent_type, AgentIntentType::Transfer);
        assert_eq!(
            normalized.execution_mode,
            super::AgentExecutionMode::ModeCProtectedExecution
        );
        assert_eq!(normalized.normalized_intent_kind, "solana.transfer.v1");
        assert_eq!(normalized.idempotency_key, "idem_123");
        assert_eq!(
            normalized
                .normalized_payload
                .get("intent_id")
                .and_then(|value| value.as_str()),
            Some("act_123")
        );
    }

    #[test]
    fn normalize_agent_action_request_rejects_mismatched_agent_binding() {
        let err = normalize_agent_action_request(
            SubmitAgentActionRequest {
                action_request_id: "act_123".to_owned(),
                tenant_id: "tenant_a".to_owned(),
                agent_id: "agent_2".to_owned(),
                environment_id: "prod".to_owned(),
                intent_type: "transfer".to_owned(),
                execution_mode: Some("mode_c_protected_execution".to_owned()),
                adapter_type: "adapter_solana".to_owned(),
                payload: json!({
                    "to_addr": "11111111111111111111111111111111",
                    "amount": 1
                }),
                idempotency_key: "idem_123".to_owned(),
                requested_scope: vec!["payments".to_owned()],
                reason: "customer transfer".to_owned(),
                callback_config: None,
                submitted_by: "planner".to_owned(),
            },
            "tenant_a",
            &ResolvedAgentIdentity {
                agent_id: "agent_1".to_owned(),
                environment_id: "prod".to_owned(),
                environment_kind: "production".to_owned(),
                runtime_type: "openai".to_owned(),
                runtime_identity: "rt_1".to_owned(),
                status: "active".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "medium".to_owned(),
                owner_team: "ops".to_owned(),
            },
        )
        .expect_err("expected mismatched agent binding to fail");

        assert!(
            err.message.contains("agent_id"),
            "expected agent_id mismatch error, got: {}",
            err.message
        );
    }

    #[test]
    fn normalize_agent_action_request_rejects_mode_b_for_transfer() {
        let err = normalize_agent_action_request(
            SubmitAgentActionRequest {
                action_request_id: "act_123".to_owned(),
                tenant_id: "tenant_a".to_owned(),
                agent_id: "agent_1".to_owned(),
                environment_id: "prod".to_owned(),
                intent_type: "transfer".to_owned(),
                execution_mode: Some("mode_b_scoped_runtime".to_owned()),
                adapter_type: "adapter_solana".to_owned(),
                payload: json!({
                    "to_addr": "11111111111111111111111111111111",
                    "amount": 1
                }),
                idempotency_key: "idem_123".to_owned(),
                requested_scope: vec!["payments".to_owned()],
                reason: "customer transfer".to_owned(),
                callback_config: None,
                submitted_by: "planner".to_owned(),
            },
            "tenant_a",
            &ResolvedAgentIdentity {
                agent_id: "agent_1".to_owned(),
                environment_id: "prod".to_owned(),
                environment_kind: "production".to_owned(),
                runtime_type: "openai".to_owned(),
                runtime_identity: "rt_1".to_owned(),
                status: "active".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "medium".to_owned(),
                owner_team: "ops".to_owned(),
            },
        )
        .expect_err("expected transfer mode mismatch to fail");

        assert!(
            err.message.contains("does not support execution_mode"),
            "unexpected mode mismatch error: {}",
            err.message
        );
    }

    #[test]
    fn normalize_agent_action_request_accepts_generate_invoice_mode_b() {
        let normalized = normalize_agent_action_request(
            SubmitAgentActionRequest {
                action_request_id: "act_456".to_owned(),
                tenant_id: "tenant_a".to_owned(),
                agent_id: "agent_1".to_owned(),
                environment_id: "staging".to_owned(),
                intent_type: "generate_invoice".to_owned(),
                execution_mode: Some("mode_b_scoped_runtime".to_owned()),
                adapter_type: "internal_runtime".to_owned(),
                payload: json!({
                    "customer_reference": "cust_123",
                    "amount": 50,
                    "currency": "USD",
                    "description": "Monthly invoice"
                }),
                idempotency_key: "idem_456".to_owned(),
                requested_scope: vec!["billing".to_owned()],
                reason: "generate invoice".to_owned(),
                callback_config: None,
                submitted_by: "planner".to_owned(),
            },
            "tenant_a",
            &ResolvedAgentIdentity {
                agent_id: "agent_1".to_owned(),
                environment_id: "staging".to_owned(),
                environment_kind: "development".to_owned(),
                runtime_type: "openai".to_owned(),
                runtime_identity: "rt_1".to_owned(),
                status: "active".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "low".to_owned(),
                owner_team: "ops".to_owned(),
            },
        )
        .expect("expected runtime-authorized invoice normalization to succeed");

        assert_eq!(normalized.intent_type, AgentIntentType::GenerateInvoice);
        assert_eq!(
            normalized.execution_mode,
            super::AgentExecutionMode::ModeBScopedRuntime
        );
        assert_eq!(
            normalized.normalized_intent_kind,
            "runtime.generate_invoice.v1"
        );
    }

    #[test]
    fn policy_engine_denies_playground_scope_in_production() {
        let decision = evaluate_policy_layers(
            &PolicyEvaluationContext {
                tenant_id: "tenant_a".to_owned(),
                agent_id: "agent_1".to_owned(),
                owner_team: "ops".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "medium".to_owned(),
                environment_id: "prod".to_owned(),
                environment_kind: "production".to_owned(),
                action: "transfer".to_owned(),
                adapter_type: "adapter_solana".to_owned(),
                amount: Some(1),
                sensitivity: "high".to_owned(),
                destination_class: "external_wallet".to_owned(),
                requested_scope: vec!["playground".to_owned()],
                reason: "demo".to_owned(),
                submitted_by: "planner".to_owned(),
                evaluated_at_ms: 1_700_000_000_000,
            },
            None,
        )
        .expect("expected policy evaluation to succeed");

        assert_eq!(decision.final_effect, PolicyEffect::Deny.as_str());
    }

    #[test]
    fn policy_engine_applies_published_bundle_allow_rule() {
        let bundle = TenantPolicyBundleRecord {
            tenant_id: "tenant_a".to_owned(),
            bundle_id: "bundle_finance".to_owned(),
            version: 3,
            label: "Finance".to_owned(),
            status: "published".to_owned(),
            template_ids: Vec::new(),
            rules: normalize_policy_bundle_document(super::TenantPolicyBundleDocument {
                template_ids: Vec::new(),
                rules: vec![PolicyRuleDefinition {
                    rule_id: "allow_transfer".to_owned(),
                    description: "Allow transfer".to_owned(),
                    effect: PolicyEffect::Allow,
                    conditions: PolicyRuleConditions {
                        actions: vec!["transfer".to_owned()],
                        environments: vec!["production".to_owned()],
                        ..PolicyRuleConditions::default()
                    },
                    obligations: PolicyRuleObligations {
                        reason_required: true,
                        ..PolicyRuleObligations::default()
                    },
                    reduced_scope: Vec::new(),
                }],
            })
            .expect("expected policy bundle normalization")
            .rules,
            created_by_principal_id: "ops".to_owned(),
            published_by_principal_id: Some("ops".to_owned()),
            created_at_ms: 1,
            published_at_ms: Some(2),
            rolled_back_from_bundle_id: None,
            rollback_reason: None,
        };

        let decision = evaluate_policy_layers(
            &PolicyEvaluationContext {
                tenant_id: "tenant_a".to_owned(),
                agent_id: "agent_1".to_owned(),
                owner_team: "ops".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "medium".to_owned(),
                environment_id: "prod".to_owned(),
                environment_kind: "production".to_owned(),
                action: "transfer".to_owned(),
                adapter_type: "adapter_solana".to_owned(),
                amount: Some(5),
                sensitivity: "high".to_owned(),
                destination_class: "external_wallet".to_owned(),
                requested_scope: vec!["payments".to_owned()],
                reason: "customer payout".to_owned(),
                submitted_by: "planner".to_owned(),
                evaluated_at_ms: 1_700_000_000_000,
            },
            Some(&bundle),
        )
        .expect("expected policy evaluation to succeed");

        assert_eq!(decision.final_effect, PolicyEffect::Allow.as_str());
        assert_eq!(
            decision.published_bundle_id.as_deref(),
            Some("bundle_finance")
        );
    }

    #[test]
    fn policy_engine_emits_decision_trace() {
        let decision = evaluate_policy_layers(
            &PolicyEvaluationContext {
                tenant_id: "tenant_a".to_owned(),
                agent_id: "agent_1".to_owned(),
                owner_team: "ops".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "medium".to_owned(),
                environment_id: "prod".to_owned(),
                environment_kind: "production".to_owned(),
                action: "transfer".to_owned(),
                adapter_type: "adapter_solana".to_owned(),
                amount: Some(5),
                sensitivity: "high".to_owned(),
                destination_class: "external_wallet".to_owned(),
                requested_scope: vec!["playground".to_owned()],
                reason: "customer payout".to_owned(),
                submitted_by: "planner".to_owned(),
                evaluated_at_ms: 1_700_000_000_000,
            },
            None,
        )
        .expect("expected policy evaluation to succeed");

        assert!(decision
            .decision_trace
            .iter()
            .any(|entry| entry.stage == "rule_matched"
                && entry.rule_id.as_deref() == Some("guardrail_no_playground_in_production")));
        assert!(decision
            .decision_trace
            .iter()
            .any(|entry| entry.stage == "final_decision"
                && entry.effect.as_deref() == Some(PolicyEffect::Deny.as_str())));
    }

    #[test]
    fn policy_engine_can_simulate_against_draft_bundle() {
        let draft_bundle = TenantPolicyBundleRecord {
            tenant_id: "tenant_a".to_owned(),
            bundle_id: "bundle_invoice_draft".to_owned(),
            version: 4,
            label: "Invoice Draft".to_owned(),
            status: "draft".to_owned(),
            template_ids: vec!["azums.billing.invoice.v1".to_owned()],
            rules: Vec::new(),
            created_by_principal_id: "ops".to_owned(),
            published_by_principal_id: None,
            created_at_ms: 10,
            published_at_ms: None,
            rolled_back_from_bundle_id: None,
            rollback_reason: None,
        };

        let decision = evaluate_policy_layers(
            &PolicyEvaluationContext {
                tenant_id: "tenant_a".to_owned(),
                agent_id: "agent_1".to_owned(),
                owner_team: "billing".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "low".to_owned(),
                environment_id: "staging".to_owned(),
                environment_kind: "staging".to_owned(),
                action: "generate_invoice".to_owned(),
                adapter_type: "billing_adapter".to_owned(),
                amount: Some(2500),
                sensitivity: "medium".to_owned(),
                destination_class: "accounts_receivable".to_owned(),
                requested_scope: vec!["billing".to_owned()],
                reason: "issue invoice".to_owned(),
                submitted_by: "planner".to_owned(),
                evaluated_at_ms: 1_700_000_000_000,
            },
            Some(&draft_bundle),
        )
        .expect("expected draft bundle simulation to succeed");

        assert_eq!(decision.final_effect, PolicyEffect::Allow.as_str());
        assert_eq!(
            decision.published_bundle_id.as_deref(),
            Some("bundle_invoice_draft")
        );
    }

    #[test]
    fn approval_scope_guard_rejects_broader_effective_scope() {
        let err = ensure_scope_not_broader(
            &[String::from("payments")],
            &[String::from("payments"), String::from("admin")],
        )
        .expect_err("expected broader effective scope to fail");
        assert!(
            err.message.contains("effective_scope broader"),
            "unexpected scope guard error: {}",
            err.message
        );
    }

    #[test]
    fn approval_grant_normalization_defaults_to_approved_lineage() {
        let approval = ApprovalRequestRecord {
            approval_request_id: "apr_123".to_owned(),
            tenant_id: "tenant_a".to_owned(),
            action_request_id: "act_123".to_owned(),
            correlation_id: Some("corr_123".to_owned()),
            agent_id: "agent_1".to_owned(),
            environment_id: "prod".to_owned(),
            environment_kind: "production".to_owned(),
            runtime_type: "openai".to_owned(),
            runtime_identity: "rt_1".to_owned(),
            trust_tier: "high".to_owned(),
            risk_tier: "medium".to_owned(),
            owner_team: "ops".to_owned(),
            intent_type: "transfer".to_owned(),
            execution_mode: "mode_c_protected_execution".to_owned(),
            adapter_type: "adapter_solana".to_owned(),
            normalized_intent_kind: "solana.transfer.v1".to_owned(),
            normalized_payload: json!({
                "to_addr": "11111111111111111111111111111111",
                "from_addr": "22222222222222222222222222222222",
                "asset": "SOL",
                "amount": 25,
            }),
            idempotency_key: "idem_123".to_owned(),
            request_fingerprint: "fp_123".to_owned(),
            requested_scope: vec!["payments".to_owned(), "refunds".to_owned()],
            effective_scope: vec!["payments".to_owned()],
            callback_config: Some(AgentActionCallbackConfig {
                url: Some("https://callback.example.com".to_owned()),
                signing_secret_ref: None,
                include_receipt: Some(true),
            }),
            reason: "customer payout".to_owned(),
            submitted_by: "planner".to_owned(),
            policy_bundle_id: Some("bundle_finance".to_owned()),
            policy_bundle_version: Some(7),
            policy_explanation: "approval required".to_owned(),
            obligations: PolicyRuleObligations::default(),
            matched_rules: Vec::new(),
            decision_trace: Vec::new(),
            status: ApprovalState::Pending,
            required_approvals: 1,
            approvals_received: 0,
            approved_by: Vec::new(),
            expires_at_ms: 2_000,
            requested_at_ms: 1_000,
            resolved_at_ms: None,
            resolved_by_actor_id: None,
            resolved_by_actor_source: None,
            resolution_note: None,
            slack_delivery_state: None,
            slack_delivery_error: None,
            slack_last_attempt_at_ms: None,
        };
        let grant = normalize_grant_spec_from_approval(
            &approval,
            &ApprovalGrantRequest {
                ttl_seconds: Some(300),
                max_uses: Some(2),
                amount_ceiling: None,
                resource_binding: None,
                scope: None,
            },
            &GrantWorkflowConfig {
                default_ttl_ms: 3_600_000,
                max_ttl_ms: 86_400_000,
                max_uses: 20,
            },
            "ops_user",
            "internal_api",
            1_500,
        )
        .expect("expected approval-backed grant normalization to succeed");

        assert_eq!(grant.tenant_id, "tenant_a");
        assert_eq!(grant.source_action_request_id, "act_123");
        assert_eq!(grant.source_approval_request_id, "apr_123");
        assert_eq!(grant.granted_scope, vec![String::from("payments")]);
        assert_eq!(
            grant.resource_binding,
            Some(json!({
                "asset": "SOL",
                "from_addr": "22222222222222222222222222222222",
                "to_addr": "11111111111111111111111111111111",
            }))
        );
        assert_eq!(grant.amount_ceiling, Some(25));
        assert_eq!(grant.max_uses, 2);
        assert_eq!(grant.expires_at_ms, 301_500);
    }

    #[test]
    fn gateway_compiles_free_form_transfer_into_submit_request() {
        let resolved_agent = ResolvedAgentIdentity {
            agent_id: "agent_1".to_owned(),
            environment_id: "prod".to_owned(),
            environment_kind: "production".to_owned(),
            runtime_type: "openai".to_owned(),
            runtime_identity: "rt_1".to_owned(),
            status: "active".to_owned(),
            trust_tier: "high".to_owned(),
            risk_tier: "medium".to_owned(),
            owner_team: "ops".to_owned(),
        };
        let submitter = SubmitterIdentity {
            principal_id: "runtime_principal".to_owned(),
            kind: SubmitterKind::AgentRuntime,
            auth_scheme: "bearer".to_owned(),
            resolved_agent: None,
        };

        let compilation = compile_agent_gateway_request(
            AgentGatewayRequest {
                free_form_input: Some(
                    "transfer 25 SOL to 11111111111111111111111111111111 for customer payout"
                        .to_owned(),
                ),
                structured_action: None,
            },
            "tenant_a",
            &resolved_agent,
            &submitter,
        )
        .expect("expected gateway compilation to succeed");

        assert_eq!(compilation.mode, "free_form");
        assert_eq!(compilation.compiled_request.intent_type, "transfer");
        assert_eq!(compilation.compiled_request.adapter_type, "adapter_solana");
        assert_eq!(
            compilation.compiled_request.requested_scope,
            vec![String::from("payments")]
        );
        assert_eq!(
            compilation
                .compiled_request
                .payload
                .get("to_addr")
                .and_then(|value| value.as_str()),
            Some("11111111111111111111111111111111")
        );
        assert_eq!(
            compilation
                .compiled_request
                .payload
                .get("amount")
                .and_then(|value| value.as_i64()),
            Some(25)
        );
        let normalized = normalize_agent_action_request(
            compilation.compiled_request,
            "tenant_a",
            &resolved_agent,
        )
        .expect("expected compiled gateway request to normalize");
        assert_eq!(normalized.normalized_intent_kind, "solana.transfer.v1");
    }

    #[test]
    fn gateway_rejects_ambiguous_free_form_input() {
        let err = compile_agent_gateway_request(
            AgentGatewayRequest {
                free_form_input: Some("refund and invoice customer_1 for 20 usd".to_owned()),
                structured_action: None,
            },
            "tenant_a",
            &ResolvedAgentIdentity {
                agent_id: "agent_1".to_owned(),
                environment_id: "prod".to_owned(),
                environment_kind: "production".to_owned(),
                runtime_type: "openai".to_owned(),
                runtime_identity: "rt_1".to_owned(),
                status: "active".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "medium".to_owned(),
                owner_team: "ops".to_owned(),
            },
            &SubmitterIdentity {
                principal_id: "runtime_principal".to_owned(),
                kind: SubmitterKind::AgentRuntime,
                auth_scheme: "bearer".to_owned(),
                resolved_agent: None,
            },
        )
        .expect_err("expected ambiguous free-form input to fail");

        assert!(
            err.message.contains("ambiguous"),
            "unexpected gateway ambiguity error: {}",
            err.message
        );
    }

    #[test]
    fn gateway_structured_input_defaults_adapter_and_scope() {
        let compilation = compile_agent_gateway_request(
            AgentGatewayRequest {
                free_form_input: None,
                structured_action: Some(AgentGatewayStructuredActionInput {
                    action_request_id: None,
                    intent_type: Some("transfer".to_owned()),
                    execution_mode: None,
                    adapter_type: None,
                    payload: Some(json!({
                        "to_addr": "11111111111111111111111111111111",
                        "amount": 5,
                    })),
                    idempotency_key: None,
                    requested_scope: None,
                    reason: Some("customer payout".to_owned()),
                    callback_config: None,
                    submitted_by: None,
                }),
            },
            "tenant_a",
            &ResolvedAgentIdentity {
                agent_id: "agent_1".to_owned(),
                environment_id: "prod".to_owned(),
                environment_kind: "production".to_owned(),
                runtime_type: "openai".to_owned(),
                runtime_identity: "rt_1".to_owned(),
                status: "active".to_owned(),
                trust_tier: "high".to_owned(),
                risk_tier: "medium".to_owned(),
                owner_team: "ops".to_owned(),
            },
            &SubmitterIdentity {
                principal_id: "runtime_principal".to_owned(),
                kind: SubmitterKind::AgentRuntime,
                auth_scheme: "bearer".to_owned(),
                resolved_agent: None,
            },
        )
        .expect("expected structured gateway compilation to succeed");

        assert_eq!(compilation.mode, "structured");
        assert_eq!(compilation.compiled_request.adapter_type, "adapter_solana");
        assert_eq!(
            compilation.compiled_request.execution_mode.as_deref(),
            Some("mode_c_protected_execution")
        );
        assert_eq!(compilation.execution_mode, "mode_c_protected_execution");
        assert_eq!(
            compilation.compiled_request.requested_scope,
            vec![String::from("payments")]
        );
        assert!(
            compilation
                .compiled_request
                .idempotency_key
                .starts_with("agw_"),
            "expected generated gateway idempotency key"
        );
    }

    #[test]
    fn shared_agent_execution_metadata_preserves_lineage_fields() {
        let approval = ApprovalRequestRecord {
            approval_request_id: "apr_123".to_owned(),
            tenant_id: "tenant_a".to_owned(),
            action_request_id: "act_123".to_owned(),
            correlation_id: Some("corr_123".to_owned()),
            agent_id: "agent_1".to_owned(),
            environment_id: "prod".to_owned(),
            environment_kind: "production".to_owned(),
            runtime_type: "openai".to_owned(),
            runtime_identity: "rt_1".to_owned(),
            trust_tier: "high".to_owned(),
            risk_tier: "medium".to_owned(),
            owner_team: "ops".to_owned(),
            intent_type: "transfer".to_owned(),
            execution_mode: "mode_c_protected_execution".to_owned(),
            adapter_type: "adapter_solana".to_owned(),
            normalized_intent_kind: "solana.transfer.v1".to_owned(),
            normalized_payload: json!({
                "to_addr": "11111111111111111111111111111111",
                "amount": 25,
            }),
            idempotency_key: "idem_123".to_owned(),
            request_fingerprint: "fp_123".to_owned(),
            requested_scope: vec!["payments".to_owned()],
            effective_scope: vec!["payments".to_owned()],
            callback_config: None,
            reason: "customer payout".to_owned(),
            submitted_by: "planner".to_owned(),
            policy_bundle_id: Some("bundle_finance".to_owned()),
            policy_bundle_version: Some(7),
            policy_explanation: "approval required".to_owned(),
            obligations: PolicyRuleObligations::default(),
            matched_rules: Vec::new(),
            decision_trace: Vec::new(),
            status: ApprovalState::Approved,
            required_approvals: 1,
            approvals_received: 1,
            approved_by: vec!["ops_user".to_owned()],
            expires_at_ms: 2_000,
            requested_at_ms: 1_000,
            resolved_at_ms: Some(1_500),
            resolved_by_actor_id: Some("ops_user".to_owned()),
            resolved_by_actor_source: Some("internal_api".to_owned()),
            resolution_note: None,
            slack_delivery_state: None,
            slack_delivery_error: None,
            slack_last_attempt_at_ms: None,
        };
        let grant = CapabilityGrantRecord {
            grant_id: "grant_123".to_owned(),
            tenant_id: "tenant_a".to_owned(),
            environment_id: "prod".to_owned(),
            agent_id: "agent_1".to_owned(),
            action_family: "transfer".to_owned(),
            adapter_type: "adapter_solana".to_owned(),
            granted_scope: vec!["payments".to_owned()],
            resource_binding: None,
            amount_ceiling: Some(25),
            max_uses: 2,
            uses_consumed: 0,
            status: CapabilityGrantStatus::Active,
            source_action_request_id: "act_123".to_owned(),
            source_approval_request_id: "apr_123".to_owned(),
            source_policy_bundle_id: Some("bundle_finance".to_owned()),
            source_policy_bundle_version: Some(7),
            created_by_actor_id: "ops_user".to_owned(),
            created_by_actor_source: "internal_api".to_owned(),
            created_at_ms: 1_500,
            expires_at_ms: 301_500,
            last_used_at_ms: None,
            revoked_at_ms: None,
            revoked_reason: None,
        };
        let resolved_agent = ResolvedAgentIdentity {
            agent_id: "agent_1".to_owned(),
            environment_id: "prod".to_owned(),
            environment_kind: "production".to_owned(),
            runtime_type: "openai".to_owned(),
            runtime_identity: "rt_1".to_owned(),
            status: "active".to_owned(),
            trust_tier: "high".to_owned(),
            risk_tier: "medium".to_owned(),
            owner_team: "ops".to_owned(),
        };
        let submitter = SubmitterIdentity {
            principal_id: "ops_user".to_owned(),
            kind: SubmitterKind::InternalService,
            auth_scheme: "approval_workflow".to_owned(),
            resolved_agent: None,
        };
        let policy_decision = PolicyDecisionExplanation {
            final_effect: "require_approval".to_owned(),
            effective_scope: vec!["payments".to_owned()],
            obligations: PolicyRuleObligations::default(),
            matched_rules: Vec::new(),
            decision_trace: Vec::new(),
            published_bundle_id: Some("bundle_finance".to_owned()),
            published_bundle_version: Some(7),
            explanation: "approval required".to_owned(),
        };

        let metadata = super::build_agent_execution_metadata(&super::AgentExecutionSubmitContext {
            tenant_id: "tenant_a",
            normalized_intent_kind: "solana.transfer.v1",
            normalized_payload: &json!({
                "to_addr": "11111111111111111111111111111111",
                "amount": 25,
            }),
            request_id: RequestId::from("req_123".to_owned()),
            correlation_id: Some("corr_123".to_owned()),
            idempotency_key: "idem_123",
            submitter: &submitter,
            resolved_agent: &resolved_agent,
            agent_status: Some("active"),
            entry_channel: "approval",
            action_request_id: "act_123",
            intent_type: "transfer",
            adapter_type: "adapter_solana",
            requested_scope: &approval.requested_scope,
            effective_scope: &approval.effective_scope,
            reason: &approval.reason,
            submitted_by: &approval.submitted_by,
            policy_decision: &policy_decision,
            callback_config: None,
            metering_scope: None,
            base_execution_policy: super::ExecutionPolicy::Sponsored,
            effective_execution_policy: super::ExecutionPolicy::Sponsored,
            execution_mode: super::AgentExecutionMode::ModeCProtectedExecution,
            signing_mode: "platform_sponsored",
            payer_source: "platform_sponsored",
            fee_payer: "platform",
            approval: Some(&approval),
            grant: Some(&grant),
        });

        assert_eq!(metadata.get("correlation_id"), Some(&"corr_123".to_owned()));
        assert_eq!(
            metadata.get("approval.request_id"),
            Some(&"apr_123".to_owned())
        );
        assert_eq!(metadata.get("grant.id"), Some(&"grant_123".to_owned()));
        assert_eq!(
            metadata.get("grant.source_approval_request_id"),
            Some(&"apr_123".to_owned())
        );
        assert_eq!(
            metadata.get("agent.execution_mode"),
            Some(&"mode_c_protected_execution".to_owned())
        );
        assert_eq!(
            metadata.get("execution.mode"),
            Some(&"mode_c_protected_execution".to_owned())
        );
        assert_eq!(
            metadata.get("execution.owner"),
            Some(&"azums_protected_execution".to_owned())
        );
        assert_eq!(
            metadata.get("policy.decision"),
            Some(&"require_approval".to_owned())
        );
    }

    #[test]
    fn slack_approval_payload_parses_decision_and_request_id() {
        let body = b"payload=%7B%22user%22%3A%7B%22id%22%3A%22U123%22%2C%22username%22%3A%22alice%22%7D%2C%22actions%22%3A%5B%7B%22action_id%22%3A%22approval%3Aapprove%22%2C%22value%22%3A%22apr_123%22%7D%5D%7D";
        let parsed =
            parse_slack_approval_payload(body).expect("expected slack payload parsing to succeed");
        assert_eq!(parsed.user_id, "u123");
        assert_eq!(parsed.approval_request_id, "apr_123");
        assert!(matches!(parsed.decision, ApprovalDecisionKind::Approve));
    }

    #[test]
    fn slack_signature_matches_expected_format() {
        let signature =
            compute_slack_signature("secret", "1710000000", b"payload=%7B%22ok%22%3Atrue%7D")
                .expect("expected slack signature to compute");
        assert_eq!(
            signature,
            "v0=5449427581295d6e7db6188ba6608817c50ee97237f749c46246cea53922a0f8"
        );
    }

    #[test]
    fn connector_secret_values_require_non_empty_entries() {
        let err = normalize_connector_secret_values(
            [("api_key".to_owned(), "   ".to_owned())]
                .into_iter()
                .collect(),
        )
        .expect_err("expected empty connector secret to fail");
        assert!(
            err.message.contains("must not be empty"),
            "unexpected connector secret error: {}",
            err.message
        );
    }

    #[test]
    fn connector_config_must_be_object() {
        let err = normalize_connector_config(Some(json!(["bad"])))
            .expect_err("expected non-object connector config to fail");
        assert!(
            err.message.contains("JSON object"),
            "unexpected connector config error: {}",
            err.message
        );
    }

    #[test]
    fn connector_config_rejects_secret_like_fields() {
        let err = normalize_connector_config(Some(json!({
            "client_secret": "raw-secret"
        })))
        .expect_err("expected raw secret-like config field to fail");
        assert!(
            err.message.contains("raw secret-like field"),
            "unexpected connector config secret error: {}",
            err.message
        );
    }
}
