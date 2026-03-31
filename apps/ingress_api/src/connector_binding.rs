use anyhow::Context;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::{
    secret_crypto::SecretCipher, BrokerConnectorBindingUseRequest, ConnectorBindingCreateRecord,
    ConnectorBindingRecord, ConnectorBindingRotationRecord,
};

#[derive(Clone)]
pub struct IngressConnectorBindingStore {
    pool: PgPool,
}

impl IngressConnectorBindingStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn ensure_schema(&self) -> anyhow::Result<()> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_connector_bindings (
                tenant_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                binding_id TEXT NOT NULL,
                connector_type TEXT NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                secret_ref TEXT NOT NULL,
                current_secret_version BIGINT NOT NULL,
                config_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                secret_fields_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                created_by_principal_id TEXT NOT NULL,
                updated_by_principal_id TEXT NOT NULL,
                created_at_ms BIGINT NOT NULL,
                updated_at_ms BIGINT NOT NULL,
                rotated_at_ms BIGINT NOT NULL,
                revoked_at_ms BIGINT NULL,
                revoked_reason TEXT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (tenant_id, environment_id, binding_id),
                UNIQUE (tenant_id, environment_id, secret_ref)
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_connector_bindings_tenant_env_status_idx
            ON ingress_api_connector_bindings(tenant_id, environment_id, status, updated_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_connector_secret_versions (
                tenant_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                binding_id TEXT NOT NULL,
                secret_ref TEXT NOT NULL,
                version BIGINT NOT NULL,
                encrypted_secret_blob TEXT NOT NULL,
                secret_fields_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                created_by_principal_id TEXT NOT NULL,
                created_at_ms BIGINT NOT NULL,
                rotation_reason TEXT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                PRIMARY KEY (tenant_id, environment_id, secret_ref, version)
            )
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_connector_binding_events (
                event_id UUID PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                binding_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_connector_binding_events_lookup_idx
            ON ingress_api_connector_binding_events(tenant_id, environment_id, binding_id, created_at_ms DESC)
            "#,
            r#"
            CREATE TABLE IF NOT EXISTS ingress_api_connector_secret_use_audits (
                audit_id UUID PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                environment_id TEXT NOT NULL,
                binding_id TEXT NOT NULL,
                secret_ref TEXT NOT NULL,
                secret_version BIGINT NOT NULL,
                actor_id TEXT NOT NULL,
                actor_kind TEXT NOT NULL,
                purpose TEXT NOT NULL,
                request_id TEXT NULL,
                action_request_id TEXT NULL,
                approval_request_id TEXT NULL,
                intent_id TEXT NULL,
                job_id TEXT NULL,
                correlation_id TEXT NULL,
                field_names_json JSONB NOT NULL DEFAULT '[]'::jsonb,
                outcome TEXT NOT NULL,
                details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at_ms BIGINT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )
            "#,
            r#"
            CREATE INDEX IF NOT EXISTS ingress_api_connector_secret_use_audits_lookup_idx
            ON ingress_api_connector_secret_use_audits(tenant_id, environment_id, binding_id, created_at_ms DESC)
            "#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .context("failed to ensure connector binding schema")?;
        }

        Ok(())
    }

    pub async fn create_binding(
        &self,
        cipher: &SecretCipher,
        record: &ConnectorBindingCreateRecord,
    ) -> anyhow::Result<ConnectorBindingRecord> {
        let encrypted_secret_blob = encrypt_secret_payload(cipher, &record.secret_values)?;
        let secret_fields_json = serde_json::to_value(secret_field_names(&record.secret_values))
            .context("serialize secret field names")?;
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin connector binding tx")?;

        sqlx::query(
            r#"
            INSERT INTO ingress_api_connector_bindings (
                tenant_id,
                environment_id,
                binding_id,
                connector_type,
                name,
                status,
                secret_ref,
                current_secret_version,
                config_json,
                secret_fields_json,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms,
                rotated_at_ms,
                revoked_at_ms,
                revoked_reason,
                updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, 'active', $6, 1, $7, $8, $9, $9, $10, $10, $10, NULL, NULL, now()
            )
            "#,
        )
        .bind(&record.tenant_id)
        .bind(&record.environment_id)
        .bind(&record.binding_id)
        .bind(&record.connector_type)
        .bind(&record.name)
        .bind(&record.secret_ref)
        .bind(&record.config)
        .bind(&secret_fields_json)
        .bind(&record.created_by_principal_id)
        .bind(record.created_at_ms as i64)
        .execute(&mut *tx)
        .await
        .context("failed to insert connector binding")?;

        sqlx::query(
            r#"
            INSERT INTO ingress_api_connector_secret_versions (
                tenant_id,
                environment_id,
                binding_id,
                secret_ref,
                version,
                encrypted_secret_blob,
                secret_fields_json,
                created_by_principal_id,
                created_at_ms,
                rotation_reason
            )
            VALUES ($1, $2, $3, $4, 1, $5, $6, $7, $8, NULL)
            "#,
        )
        .bind(&record.tenant_id)
        .bind(&record.environment_id)
        .bind(&record.binding_id)
        .bind(&record.secret_ref)
        .bind(&encrypted_secret_blob)
        .bind(&secret_fields_json)
        .bind(&record.created_by_principal_id)
        .bind(record.created_at_ms as i64)
        .execute(&mut *tx)
        .await
        .context("failed to insert connector secret version")?;

        self.insert_event_tx(
            &mut tx,
            &record.tenant_id,
            &record.environment_id,
            &record.binding_id,
            "created",
            &record.created_by_principal_id,
            json!({
                "connector_type": record.connector_type,
                "secret_ref": record.secret_ref,
                "secret_version": 1,
                "secret_fields": secret_field_names(&record.secret_values),
            }),
            record.created_at_ms,
        )
        .await?;

        tx.commit()
            .await
            .context("failed to commit connector binding tx")?;

        self.load_binding(
            &record.tenant_id,
            &record.environment_id,
            &record.binding_id,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("connector binding row missing after create"))
    }

    pub async fn list_bindings(
        &self,
        tenant_id: &str,
        environment_id: &str,
        include_inactive: bool,
        limit: u32,
    ) -> anyhow::Result<Vec<ConnectorBindingRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT
                tenant_id,
                environment_id,
                binding_id,
                connector_type,
                name,
                status,
                secret_ref,
                current_secret_version,
                config_json,
                secret_fields_json,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms,
                rotated_at_ms,
                revoked_at_ms,
                revoked_reason
            FROM ingress_api_connector_bindings
            WHERE tenant_id = $1
              AND environment_id = $2
              AND ($3 OR status = 'active')
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
        .context("failed to list connector bindings")?;

        rows.into_iter()
            .map(map_connector_binding_record)
            .collect::<anyhow::Result<Vec<_>>>()
            .context("failed to decode connector bindings")
    }

    pub async fn load_binding(
        &self,
        tenant_id: &str,
        environment_id: &str,
        binding_id: &str,
    ) -> anyhow::Result<Option<ConnectorBindingRecord>> {
        let row = sqlx::query(
            r#"
            SELECT
                tenant_id,
                environment_id,
                binding_id,
                connector_type,
                name,
                status,
                secret_ref,
                current_secret_version,
                config_json,
                secret_fields_json,
                created_by_principal_id,
                updated_by_principal_id,
                created_at_ms,
                updated_at_ms,
                rotated_at_ms,
                revoked_at_ms,
                revoked_reason
            FROM ingress_api_connector_bindings
            WHERE tenant_id = $1
              AND environment_id = $2
              AND binding_id = $3
            "#,
        )
        .bind(tenant_id)
        .bind(environment_id)
        .bind(binding_id)
        .fetch_optional(&self.pool)
        .await
        .context("failed to load connector binding")?;

        row.map(map_connector_binding_record)
            .transpose()
            .context("failed to decode connector binding")
    }

    pub async fn rotate_binding(
        &self,
        cipher: &SecretCipher,
        rotation: &ConnectorBindingRotationRecord,
    ) -> anyhow::Result<ConnectorBindingRecord> {
        let existing = self
            .load_binding(
                &rotation.tenant_id,
                &rotation.environment_id,
                &rotation.binding_id,
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("connector binding not found"))?;
        if existing.status != "active" {
            anyhow::bail!("connector binding `{}` is not active", rotation.binding_id);
        }

        let next_version = existing.current_secret_version.saturating_add(1);
        let encrypted_secret_blob = encrypt_secret_payload(cipher, &rotation.secret_values)?;
        let secret_fields = secret_field_names(&rotation.secret_values);
        let secret_fields_json =
            serde_json::to_value(&secret_fields).context("serialize rotated secret fields")?;

        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin connector rotation tx")?;

        sqlx::query(
            r#"
            INSERT INTO ingress_api_connector_secret_versions (
                tenant_id,
                environment_id,
                binding_id,
                secret_ref,
                version,
                encrypted_secret_blob,
                secret_fields_json,
                created_by_principal_id,
                created_at_ms,
                rotation_reason
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
        )
        .bind(&rotation.tenant_id)
        .bind(&rotation.environment_id)
        .bind(&rotation.binding_id)
        .bind(&existing.secret_ref)
        .bind(next_version as i64)
        .bind(&encrypted_secret_blob)
        .bind(&secret_fields_json)
        .bind(&rotation.rotated_by_principal_id)
        .bind(rotation.rotated_at_ms as i64)
        .bind(&rotation.rotation_reason)
        .execute(&mut *tx)
        .await
        .context("failed to insert rotated secret version")?;

        sqlx::query(
            r#"
            UPDATE ingress_api_connector_bindings
            SET
                current_secret_version = $4,
                secret_fields_json = $5,
                updated_by_principal_id = $6,
                updated_at_ms = $7,
                rotated_at_ms = $7,
                updated_at = now()
            WHERE tenant_id = $1
              AND environment_id = $2
              AND binding_id = $3
            "#,
        )
        .bind(&rotation.tenant_id)
        .bind(&rotation.environment_id)
        .bind(&rotation.binding_id)
        .bind(next_version as i64)
        .bind(&secret_fields_json)
        .bind(&rotation.rotated_by_principal_id)
        .bind(rotation.rotated_at_ms as i64)
        .execute(&mut *tx)
        .await
        .context("failed to update connector binding for rotation")?;

        self.insert_event_tx(
            &mut tx,
            &rotation.tenant_id,
            &rotation.environment_id,
            &rotation.binding_id,
            "rotated",
            &rotation.rotated_by_principal_id,
            json!({
                "secret_ref": existing.secret_ref,
                "secret_version": next_version,
                "secret_fields": secret_fields,
                "rotation_reason": rotation.rotation_reason,
            }),
            rotation.rotated_at_ms,
        )
        .await?;

        tx.commit()
            .await
            .context("failed to commit connector rotation tx")?;

        self.load_binding(
            &rotation.tenant_id,
            &rotation.environment_id,
            &rotation.binding_id,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("connector binding row missing after rotation"))
    }

    pub async fn revoke_binding(
        &self,
        tenant_id: &str,
        environment_id: &str,
        binding_id: &str,
        revoked_by_principal_id: &str,
        revoked_reason: Option<&str>,
        revoked_at_ms: u64,
    ) -> anyhow::Result<bool> {
        let mut tx = self
            .pool
            .begin()
            .await
            .context("failed to begin connector revoke tx")?;
        let updated = sqlx::query(
            r#"
            UPDATE ingress_api_connector_bindings
            SET
                status = 'revoked',
                updated_by_principal_id = $4,
                updated_at_ms = $5,
                revoked_at_ms = $5,
                revoked_reason = $6,
                updated_at = now()
            WHERE tenant_id = $1
              AND environment_id = $2
              AND binding_id = $3
              AND status <> 'revoked'
            "#,
        )
        .bind(tenant_id)
        .bind(environment_id)
        .bind(binding_id)
        .bind(revoked_by_principal_id)
        .bind(revoked_at_ms as i64)
        .bind(revoked_reason)
        .execute(&mut *tx)
        .await
        .context("failed to revoke connector binding")?
        .rows_affected();

        if updated > 0 {
            self.insert_event_tx(
                &mut tx,
                tenant_id,
                environment_id,
                binding_id,
                "revoked",
                revoked_by_principal_id,
                json!({
                    "revoked_reason": revoked_reason,
                }),
                revoked_at_ms,
            )
            .await?;
        }

        tx.commit()
            .await
            .context("failed to commit connector revoke tx")?;

        Ok(updated > 0)
    }

    pub async fn resolve_secret_payload(
        &self,
        cipher: &SecretCipher,
        use_request: &BrokerConnectorBindingUseRequest,
    ) -> anyhow::Result<(ConnectorBindingRecord, BTreeMap<String, String>)> {
        let binding = self
            .load_binding(
                &use_request.tenant_id,
                &use_request.environment_id,
                &use_request.binding_id,
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("connector binding not found"))?;
        if binding.status != "active" {
            anyhow::bail!(
                "connector binding `{}` is not active",
                use_request.binding_id
            );
        }

        let encrypted_secret_blob = sqlx::query_scalar::<_, String>(
            r#"
            SELECT encrypted_secret_blob
            FROM ingress_api_connector_secret_versions
            WHERE tenant_id = $1
              AND environment_id = $2
              AND secret_ref = $3
              AND version = $4
            "#,
        )
        .bind(&use_request.tenant_id)
        .bind(&use_request.environment_id)
        .bind(&binding.secret_ref)
        .bind(binding.current_secret_version as i64)
        .fetch_one(&self.pool)
        .await
        .context("failed to load encrypted connector secret")?;

        let decrypted_json = cipher
            .decrypt(&encrypted_secret_blob)
            .context("failed to decrypt connector secret payload")?;
        let secret_values = serde_json::from_str::<BTreeMap<String, String>>(&decrypted_json)
            .context("failed to decode decrypted connector secret payload")?;

        self.record_secret_use(
            use_request,
            &binding,
            "resolved",
            json!({
                "secret_fields": binding.secret_fields,
            }),
        )
        .await?;

        Ok((binding, secret_values))
    }

    pub async fn record_secret_use(
        &self,
        use_request: &BrokerConnectorBindingUseRequest,
        binding: &ConnectorBindingRecord,
        outcome: &str,
        details: Value,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ingress_api_connector_secret_use_audits (
                audit_id,
                tenant_id,
                environment_id,
                binding_id,
                secret_ref,
                secret_version,
                actor_id,
                actor_kind,
                purpose,
                request_id,
                action_request_id,
                approval_request_id,
                intent_id,
                job_id,
                correlation_id,
                field_names_json,
                outcome,
                details_json,
                created_at_ms
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19
            )
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(&use_request.tenant_id)
        .bind(&use_request.environment_id)
        .bind(&use_request.binding_id)
        .bind(&binding.secret_ref)
        .bind(binding.current_secret_version as i64)
        .bind(&use_request.actor_id)
        .bind(&use_request.actor_kind)
        .bind(&use_request.purpose)
        .bind(&use_request.request_id)
        .bind(&use_request.action_request_id)
        .bind(&use_request.approval_request_id)
        .bind(&use_request.intent_id)
        .bind(&use_request.job_id)
        .bind(&use_request.correlation_id)
        .bind(serde_json::to_value(&binding.secret_fields).context("serialize used secret fields")?)
        .bind(outcome)
        .bind(details)
        .bind(use_request.used_at_ms as i64)
        .execute(&self.pool)
        .await
        .context("failed to insert connector secret use audit")?;
        Ok(())
    }

    async fn insert_event_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &str,
        environment_id: &str,
        binding_id: &str,
        event_type: &str,
        actor_id: &str,
        details: Value,
        created_at_ms: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO ingress_api_connector_binding_events (
                event_id,
                tenant_id,
                environment_id,
                binding_id,
                event_type,
                actor_id,
                details_json,
                created_at_ms
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(environment_id)
        .bind(binding_id)
        .bind(event_type)
        .bind(actor_id)
        .bind(details)
        .bind(created_at_ms as i64)
        .execute(&mut **tx)
        .await
        .context("failed to insert connector binding event")?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct IngressConnectorSecretBroker {
    store: IngressConnectorBindingStore,
    cipher: SecretCipher,
}

impl IngressConnectorSecretBroker {
    pub fn new(store: IngressConnectorBindingStore, cipher: SecretCipher) -> Self {
        Self { store, cipher }
    }

    pub async fn create_binding(
        &self,
        record: &ConnectorBindingCreateRecord,
    ) -> anyhow::Result<ConnectorBindingRecord> {
        self.store.create_binding(&self.cipher, record).await
    }

    pub async fn rotate_binding(
        &self,
        record: &ConnectorBindingRotationRecord,
    ) -> anyhow::Result<ConnectorBindingRecord> {
        self.store.rotate_binding(&self.cipher, record).await
    }

    pub async fn resolve_for_use(
        &self,
        request: &BrokerConnectorBindingUseRequest,
    ) -> anyhow::Result<(ConnectorBindingRecord, BTreeMap<String, String>)> {
        self.store
            .resolve_secret_payload(&self.cipher, request)
            .await
    }
}

fn encrypt_secret_payload(
    cipher: &SecretCipher,
    secret_values: &BTreeMap<String, String>,
) -> anyhow::Result<String> {
    let raw = serde_json::to_string(secret_values).context("serialize connector secret payload")?;
    cipher
        .encrypt(&raw)
        .context("encrypt connector secret payload")
}

fn secret_field_names(secret_values: &BTreeMap<String, String>) -> Vec<String> {
    secret_values.keys().cloned().collect::<Vec<_>>()
}

fn map_connector_binding_record(
    row: sqlx::postgres::PgRow,
) -> anyhow::Result<ConnectorBindingRecord> {
    let secret_fields = serde_json::from_value::<Vec<String>>(row.get("secret_fields_json"))
        .context("decode connector secret_fields_json")?;
    Ok(ConnectorBindingRecord {
        tenant_id: row.get("tenant_id"),
        environment_id: row.get("environment_id"),
        binding_id: row.get("binding_id"),
        connector_type: row.get("connector_type"),
        name: row.get("name"),
        status: row.get("status"),
        secret_ref: row.get("secret_ref"),
        current_secret_version: row.get::<i64, _>("current_secret_version").max(0) as u64,
        config: row.get("config_json"),
        secret_fields,
        created_by_principal_id: row.get("created_by_principal_id"),
        updated_by_principal_id: row.get("updated_by_principal_id"),
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
        updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
        rotated_at_ms: row.get::<i64, _>("rotated_at_ms").max(0) as u64,
        revoked_at_ms: row
            .get::<Option<i64>, _>("revoked_at_ms")
            .map(|value| value.max(0) as u64),
        revoked_reason: row.get("revoked_reason"),
    })
}
