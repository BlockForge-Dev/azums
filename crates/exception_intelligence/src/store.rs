use crate::classifier::{ClassifiedExceptionDraft, ExceptionClassifier, ExceptionContext};
use crate::error::ExceptionIntelligenceError;
use crate::model::{
    ExceptionCase, ExceptionCaseDetail, ExceptionCategory, ExceptionDraft, ExceptionEvent,
    ExceptionEvidence, ExceptionResolutionRecord, ExceptionSearchQuery, ExceptionSeverity,
    ExceptionState,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone)]
pub struct PostgresExceptionStore {
    pool: PgPool,
}

#[derive(Debug, Clone, Default)]
struct ExceptionLinkage {
    latest_outcome_id: Option<String>,
    latest_outcome_payload: Option<Value>,
    latest_outcome_at_ms: Option<u64>,
    latest_recon_receipt_id: Option<String>,
    latest_recon_receipt_payload: Option<Value>,
    latest_recon_receipt_at_ms: Option<u64>,
    latest_execution_receipt_id: Option<String>,
    latest_execution_receipt_payload: Option<Value>,
    latest_execution_receipt_at_ms: Option<u64>,
    latest_evidence_snapshot_id: Option<String>,
    latest_evidence_snapshot_payload: Option<Value>,
    latest_evidence_snapshot_at_ms: Option<u64>,
}

impl PostgresExceptionStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ensure_schema(&self) -> Result<(), ExceptionIntelligenceError> {
        let ddl = [
            r#"
            CREATE TABLE IF NOT EXISTS exception_cases (
                case_id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                intent_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                category TEXT NOT NULL,
                severity TEXT NOT NULL,
                state TEXT NOT NULL,
                summary TEXT NOT NULL,
                machine_reason TEXT NOT NULL,
                dedupe_key TEXT NOT NULL,
                cluster_key TEXT NOT NULL,
                first_seen_at_ms BIGINT NOT NULL,
                last_seen_at_ms BIGINT NOT NULL,
                occurrence_count BIGINT NOT NULL DEFAULT 1,
                latest_run_id TEXT NULL,
                latest_outcome_id TEXT NULL,
                latest_recon_receipt_id TEXT NULL,
                latest_execution_receipt_id TEXT NULL,
                latest_evidence_snapshot_id TEXT NULL,
                created_at_ms BIGINT NOT NULL,
                updated_at_ms BIGINT NOT NULL,
                resolved_at_ms BIGINT NULL,
                last_actor TEXT NULL
            )
            "#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS dedupe_key TEXT NOT NULL DEFAULT ''"#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS cluster_key TEXT NOT NULL DEFAULT ''"#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS first_seen_at_ms BIGINT NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS last_seen_at_ms BIGINT NOT NULL DEFAULT 0"#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS occurrence_count BIGINT NOT NULL DEFAULT 1"#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS latest_outcome_id TEXT NULL"#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS latest_recon_receipt_id TEXT NULL"#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS latest_execution_receipt_id TEXT NULL"#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS latest_evidence_snapshot_id TEXT NULL"#,
            r#"ALTER TABLE exception_cases ADD COLUMN IF NOT EXISTS last_actor TEXT NULL"#,
            r#"CREATE UNIQUE INDEX IF NOT EXISTS exception_cases_tenant_dedupe_idx ON exception_cases(tenant_id, dedupe_key)"#,
            r#"CREATE INDEX IF NOT EXISTS exception_cases_tenant_updated_idx ON exception_cases(tenant_id, updated_at_ms DESC, case_id DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS exception_cases_tenant_state_idx ON exception_cases(tenant_id, state, updated_at_ms DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS exception_cases_tenant_severity_idx ON exception_cases(tenant_id, severity, updated_at_ms DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS exception_cases_tenant_category_idx ON exception_cases(tenant_id, category, updated_at_ms DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS exception_cases_tenant_intent_idx ON exception_cases(tenant_id, intent_id, updated_at_ms DESC)"#,
            r#"CREATE INDEX IF NOT EXISTS exception_cases_tenant_cluster_idx ON exception_cases(tenant_id, cluster_key, updated_at_ms DESC)"#,
            r#"
            CREATE TABLE IF NOT EXISTS exception_evidence (
                evidence_id TEXT PRIMARY KEY,
                case_id TEXT NOT NULL REFERENCES exception_cases(case_id) ON DELETE CASCADE,
                evidence_type TEXT NOT NULL,
                source_table TEXT NULL,
                source_id TEXT NULL,
                observed_at_ms BIGINT NULL,
                payload_json JSONB NOT NULL,
                created_at_ms BIGINT NOT NULL
            )
            "#,
            r#"CREATE INDEX IF NOT EXISTS exception_evidence_case_idx ON exception_evidence(case_id, created_at_ms ASC)"#,
            r#"CREATE UNIQUE INDEX IF NOT EXISTS exception_evidence_case_dedupe_idx ON exception_evidence(case_id, evidence_type, COALESCE(source_table, ''), COALESCE(source_id, ''), COALESCE(observed_at_ms, -1))"#,
            r#"
            CREATE TABLE IF NOT EXISTS exception_events (
                event_id TEXT PRIMARY KEY,
                case_id TEXT NOT NULL REFERENCES exception_cases(case_id) ON DELETE CASCADE,
                event_type TEXT NOT NULL,
                from_state TEXT NULL,
                to_state TEXT NULL,
                actor TEXT NOT NULL,
                reason TEXT NOT NULL,
                payload_json JSONB NOT NULL,
                created_at_ms BIGINT NOT NULL
            )
            "#,
            r#"CREATE INDEX IF NOT EXISTS exception_events_case_idx ON exception_events(case_id, created_at_ms ASC)"#,
            r#"
            CREATE TABLE IF NOT EXISTS exception_resolution_history (
                resolution_id TEXT PRIMARY KEY,
                case_id TEXT NOT NULL REFERENCES exception_cases(case_id) ON DELETE CASCADE,
                resolution_state TEXT NOT NULL,
                actor TEXT NOT NULL,
                reason TEXT NOT NULL,
                payload_json JSONB NOT NULL,
                created_at_ms BIGINT NOT NULL
            )
            "#,
            r#"CREATE INDEX IF NOT EXISTS exception_resolution_history_case_idx ON exception_resolution_history(case_id, created_at_ms ASC)"#,
        ];

        for stmt in ddl {
            sqlx::query(stmt)
                .execute(&self.pool)
                .await
                .map_err(sqlx_to_internal)?;
        }

        self.migrate_legacy_schema().await?;
        Ok(())
    }

    pub async fn sync_subject_cases(
        &self,
        tenant_id: &str,
        subject_id: &str,
        intent_id: &str,
        job_id: &str,
        adapter_id: &str,
        latest_run_id: Option<&str>,
        drafts: &[ExceptionDraft],
        now_ms: u64,
    ) -> Result<Vec<ExceptionCase>, ExceptionIntelligenceError> {
        let classifier = ExceptionClassifier;
        let mut tx = self.pool.begin().await.map_err(sqlx_to_internal)?;
        let linkage = self
            .load_linkage(&mut tx, tenant_id, subject_id, latest_run_id)
            .await?;
        let existing_cases = self
            .load_subject_cases(&mut tx, tenant_id, subject_id)
            .await?;
        let existing_map: HashMap<String, ExceptionCase> = existing_cases
            .into_iter()
            .map(|case| (case.dedupe_key.clone(), case))
            .collect();

        let mut active_keys = HashSet::new();
        let mut out = Vec::with_capacity(drafts.len());

        for draft in drafts {
            let classified = classifier.classify(
                &ExceptionContext {
                    tenant_id: tenant_id.to_owned(),
                    subject_id: subject_id.to_owned(),
                    intent_id: intent_id.to_owned(),
                    job_id: job_id.to_owned(),
                    adapter_id: adapter_id.to_owned(),
                    latest_run_id: latest_run_id.map(ToOwned::to_owned),
                    latest_outcome_id: linkage.latest_outcome_id.clone(),
                },
                draft,
            );
            active_keys.insert(classified.dedupe_key.clone());
            let existing = existing_map.get(&classified.dedupe_key);
            let case = self
                .upsert_active_case(
                    &mut tx,
                    existing,
                    tenant_id,
                    subject_id,
                    intent_id,
                    job_id,
                    adapter_id,
                    latest_run_id,
                    &classified,
                    &linkage,
                    now_ms,
                )
                .await?;
            out.push(case);
        }

        for existing in existing_map.values() {
            if active_keys.contains(&existing.dedupe_key) || existing.state.is_terminal() {
                continue;
            }
            self.auto_resolve_case(&mut tx, existing, latest_run_id, &linkage, now_ms)
                .await?;
        }

        tx.commit().await.map_err(sqlx_to_internal)?;
        Ok(out)
    }

    pub async fn list_cases_for_intent(
        &self,
        tenant_id: &str,
        intent_id: &str,
    ) -> Result<Vec<(ExceptionCase, Vec<ExceptionEvidence>)>, ExceptionIntelligenceError> {
        let cases = self
            .list_cases(
                tenant_id,
                &ExceptionSearchQuery {
                    intent_id: Some(intent_id.to_owned()),
                    include_terminal: true,
                    limit: 200,
                    ..ExceptionSearchQuery::default()
                },
            )
            .await?;

        let mut out = Vec::with_capacity(cases.len());
        for case in cases {
            let evidence = self.load_case_evidence(&case.case_id).await?;
            out.push((case, evidence));
        }
        Ok(out)
    }

    pub async fn list_cases(
        &self,
        tenant_id: &str,
        query: &ExceptionSearchQuery,
    ) -> Result<Vec<ExceptionCase>, ExceptionIntelligenceError> {
        let search_like = query
            .search
            .as_ref()
            .map(|value| format!("%{}%", value.trim()));
        let rows = sqlx::query(
            r#"
            SELECT
                case_id, tenant_id, subject_id, intent_id, job_id, adapter_id,
                category, severity, state, summary, machine_reason,
                dedupe_key, cluster_key, first_seen_at_ms, last_seen_at_ms, occurrence_count,
                latest_run_id, latest_outcome_id, latest_recon_receipt_id,
                latest_execution_receipt_id, latest_evidence_snapshot_id,
                created_at_ms, updated_at_ms, resolved_at_ms, last_actor
            FROM exception_cases
            WHERE tenant_id = $1
              AND ($2::text IS NULL OR state = $2)
              AND ($3::text IS NULL OR severity = $3)
              AND ($4::text IS NULL OR category = $4)
              AND ($5::text IS NULL OR adapter_id = $5)
              AND ($6::text IS NULL OR subject_id = $6)
              AND ($7::text IS NULL OR intent_id = $7)
              AND ($8::text IS NULL OR cluster_key = $8)
              AND ($9::boolean OR state NOT IN ('resolved', 'dismissed', 'false_positive'))
              AND (
                    $10::text IS NULL
                    OR case_id ILIKE $10
                    OR summary ILIKE $10
                    OR machine_reason ILIKE $10
                    OR dedupe_key ILIKE $10
                    OR cluster_key ILIKE $10
                  )
            ORDER BY updated_at_ms DESC, case_id DESC
            LIMIT $11 OFFSET $12
            "#,
        )
        .bind(tenant_id)
        .bind(normalize_filter(query.state.as_deref()))
        .bind(normalize_filter(query.severity.as_deref()))
        .bind(normalize_filter(query.category.as_deref()))
        .bind(normalize_filter(query.adapter_id.as_deref()))
        .bind(normalize_filter(query.subject_id.as_deref()))
        .bind(normalize_filter(query.intent_id.as_deref()))
        .bind(normalize_filter(query.cluster_key.as_deref()))
        .bind(query.include_terminal)
        .bind(search_like)
        .bind(normalize_limit(query.limit) as i64)
        .bind(query.offset as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        Ok(rows.into_iter().map(map_case_row).collect())
    }

    pub async fn load_case_detail(
        &self,
        tenant_id: &str,
        case_id: &str,
    ) -> Result<Option<ExceptionCaseDetail>, ExceptionIntelligenceError> {
        let case = self.load_case(tenant_id, case_id).await?;
        let Some(case) = case else {
            return Ok(None);
        };
        let evidence = self.load_case_evidence(case_id).await?;
        let events = self.load_case_events(case_id).await?;
        let resolution_history = self.load_resolution_history(case_id).await?;
        Ok(Some(ExceptionCaseDetail {
            case,
            evidence,
            events,
            resolution_history,
        }))
    }

    pub async fn transition_case_state(
        &self,
        tenant_id: &str,
        case_id: &str,
        to_state: ExceptionState,
        actor: &str,
        reason: &str,
        payload: Value,
        now_ms: u64,
    ) -> Result<ExceptionCase, ExceptionIntelligenceError> {
        let reason = reason.trim();
        if reason.is_empty() {
            return Err(ExceptionIntelligenceError::BadRequest(
                "reason is required".to_owned(),
            ));
        }

        let mut tx = self.pool.begin().await.map_err(sqlx_to_internal)?;
        let Some(case) = self.load_case_tx(&mut tx, tenant_id, case_id).await? else {
            return Err(ExceptionIntelligenceError::NotFound(format!(
                "exception case `{case_id}` not found"
            )));
        };
        if case.state == to_state {
            tx.commit().await.map_err(sqlx_to_internal)?;
            return Ok(case);
        }
        ensure_transition_allowed(case.state, to_state)?;

        let resolved_at_ms = if to_state.is_terminal() {
            Some(now_ms)
        } else {
            None
        };
        sqlx::query(
            r#"
            UPDATE exception_cases
            SET state = $2,
                updated_at_ms = $3,
                resolved_at_ms = $4,
                last_actor = $5
            WHERE case_id = $1
            "#,
        )
        .bind(case_id)
        .bind(to_state.as_str())
        .bind(now_ms as i64)
        .bind(resolved_at_ms.map(|value| value as i64))
        .bind(actor)
        .execute(&mut *tx)
        .await
        .map_err(sqlx_to_internal)?;

        let event_type = if case.state.is_terminal() && !to_state.is_terminal() {
            "reopened"
        } else if to_state.is_terminal() {
            "resolution"
        } else {
            "state_changed"
        };
        self.record_event(
            &mut tx,
            case_id,
            event_type,
            Some(case.state),
            Some(to_state),
            actor,
            reason,
            payload.clone(),
            now_ms,
        )
        .await?;

        if to_state.is_terminal() {
            self.record_resolution(&mut tx, case_id, to_state, actor, reason, payload, now_ms)
                .await?;
        }

        let updated = self
            .load_case_tx(&mut tx, tenant_id, case_id)
            .await?
            .ok_or_else(|| {
                ExceptionIntelligenceError::NotFound(format!(
                    "exception case `{case_id}` disappeared during update"
                ))
            })?;
        tx.commit().await.map_err(sqlx_to_internal)?;
        Ok(updated)
    }

    async fn migrate_legacy_schema(&self) -> Result<(), ExceptionIntelligenceError> {
        if !self.table_exists("exception_intelligence_cases").await? {
            return Ok(());
        }

        sqlx::query(
            r#"
            INSERT INTO exception_cases (
                case_id, tenant_id, subject_id, intent_id, job_id, adapter_id,
                category, severity, state, summary, machine_reason,
                dedupe_key, cluster_key, first_seen_at_ms, last_seen_at_ms, occurrence_count,
                latest_run_id, latest_outcome_id, latest_recon_receipt_id,
                latest_execution_receipt_id, latest_evidence_snapshot_id,
                created_at_ms, updated_at_ms, resolved_at_ms, last_actor
            )
            SELECT
                case_id,
                tenant_id,
                subject_id,
                intent_id,
                job_id,
                adapter_id,
                category,
                severity,
                CASE
                    WHEN lower(state) = 'suppressed' THEN 'dismissed'
                    WHEN lower(state) = 'manual_review_required' THEN 'open'
                    ELSE lower(state)
                END,
                summary,
                machine_reason,
                lower(subject_id || '|' || adapter_id || '|' || category || '|' || machine_reason),
                lower(adapter_id || '|' || category || '|' || machine_reason),
                created_at_ms,
                updated_at_ms,
                1,
                latest_run_id,
                NULL,
                NULL,
                NULL,
                NULL,
                created_at_ms,
                updated_at_ms,
                resolved_at_ms,
                'legacy_migration'
            FROM exception_intelligence_cases
            ON CONFLICT (case_id) DO NOTHING
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;

        if self.table_exists("exception_intelligence_evidence").await? {
            sqlx::query(
                r#"
                INSERT INTO exception_evidence (
                    evidence_id, case_id, evidence_type, source_table, source_id,
                    observed_at_ms, payload_json, created_at_ms
                )
                SELECT
                    evidence_id, case_id, evidence_type, source_table, source_id,
                    observed_at_ms, payload_json, created_at_ms
                FROM exception_intelligence_evidence
                ON CONFLICT (evidence_id) DO NOTHING
                "#,
            )
            .execute(&self.pool)
            .await
            .map_err(sqlx_to_internal)?;
        }

        if self
            .table_exists("exception_intelligence_state_transitions")
            .await?
        {
            sqlx::query(
                r#"
                INSERT INTO exception_events (
                    event_id, case_id, event_type, from_state, to_state,
                    actor, reason, payload_json, created_at_ms
                )
                SELECT
                    state_transition_id,
                    case_id,
                    'legacy_state_transition',
                    CASE
                        WHEN from_state IS NULL THEN NULL
                        WHEN lower(from_state) = 'suppressed' THEN 'dismissed'
                        WHEN lower(from_state) = 'manual_review_required' THEN 'open'
                        ELSE lower(from_state)
                    END,
                    CASE
                        WHEN to_state IS NULL THEN NULL
                        WHEN lower(to_state) = 'suppressed' THEN 'dismissed'
                        WHEN lower(to_state) = 'manual_review_required' THEN 'open'
                        ELSE lower(to_state)
                    END,
                    actor,
                    reason,
                    payload_json,
                    occurred_at_ms
                FROM exception_intelligence_state_transitions
                ON CONFLICT (event_id) DO NOTHING
                "#,
            )
            .execute(&self.pool)
            .await
            .map_err(sqlx_to_internal)?;

            sqlx::query(
                r#"
                INSERT INTO exception_resolution_history (
                    resolution_id, case_id, resolution_state, actor, reason, payload_json, created_at_ms
                )
                SELECT
                    'legacyres_' || state_transition_id,
                    case_id,
                    CASE
                        WHEN lower(to_state) = 'suppressed' THEN 'dismissed'
                        ELSE lower(to_state)
                    END,
                    actor,
                    reason,
                    payload_json,
                    occurred_at_ms
                FROM exception_intelligence_state_transitions
                WHERE lower(to_state) IN ('resolved', 'suppressed', 'false_positive')
                ON CONFLICT (resolution_id) DO NOTHING
                "#,
            )
            .execute(&self.pool)
            .await
            .map_err(sqlx_to_internal)?;
        }

        Ok(())
    }

    async fn upsert_active_case(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        existing: Option<&ExceptionCase>,
        tenant_id: &str,
        subject_id: &str,
        intent_id: &str,
        job_id: &str,
        adapter_id: &str,
        latest_run_id: Option<&str>,
        classified: &ClassifiedExceptionDraft,
        linkage: &ExceptionLinkage,
        now_ms: u64,
    ) -> Result<ExceptionCase, ExceptionIntelligenceError> {
        let case_id = existing
            .map(|case| case.case_id.clone())
            .unwrap_or_else(|| format!("excase_{}", Uuid::new_v4().simple()));
        let prior_state = existing.map(|case| case.state);
        let next_state = next_active_state(existing.map(|case| case.state), classified.state);
        let first_seen_at_ms = existing.map(|case| case.first_seen_at_ms).unwrap_or(now_ms);
        let occurrence_count = match existing {
            Some(case) if case.latest_run_id.as_deref() == latest_run_id => case.occurrence_count,
            Some(case) => case.occurrence_count.saturating_add(1),
            None => 1,
        };
        let event_type = match prior_state {
            None => "created",
            Some(from) if from.is_terminal() && !next_state.is_terminal() => "reopened",
            Some(from) if from != next_state => "state_changed",
            Some(_) => "recon_observed",
        };

        if existing.is_some() {
            sqlx::query(
                r#"
                UPDATE exception_cases
                SET intent_id = $2,
                    job_id = $3,
                    adapter_id = $4,
                    category = $5,
                    severity = $6,
                    state = $7,
                    summary = $8,
                    machine_reason = $9,
                    cluster_key = $10,
                    last_seen_at_ms = $11,
                    occurrence_count = $12,
                    latest_run_id = $13,
                    latest_outcome_id = $14,
                    latest_recon_receipt_id = $15,
                    latest_execution_receipt_id = $16,
                    latest_evidence_snapshot_id = $17,
                    updated_at_ms = $11,
                    resolved_at_ms = NULL,
                    last_actor = 'recon_core'
                WHERE case_id = $1
                "#,
            )
            .bind(&case_id)
            .bind(intent_id)
            .bind(job_id)
            .bind(adapter_id)
            .bind(classified.category.as_str())
            .bind(classified.severity.as_str())
            .bind(next_state.as_str())
            .bind(&classified.summary)
            .bind(&classified.machine_reason)
            .bind(&classified.cluster_key)
            .bind(now_ms as i64)
            .bind(occurrence_count as i64)
            .bind(latest_run_id)
            .bind(linkage.latest_outcome_id.as_deref())
            .bind(linkage.latest_recon_receipt_id.as_deref())
            .bind(linkage.latest_execution_receipt_id.as_deref())
            .bind(linkage.latest_evidence_snapshot_id.as_deref())
            .execute(&mut **tx)
            .await
            .map_err(sqlx_to_internal)?;
        } else {
            sqlx::query(
                r#"
                INSERT INTO exception_cases (
                    case_id, tenant_id, subject_id, intent_id, job_id, adapter_id,
                    category, severity, state, summary, machine_reason,
                    dedupe_key, cluster_key, first_seen_at_ms, last_seen_at_ms, occurrence_count,
                    latest_run_id, latest_outcome_id, latest_recon_receipt_id,
                    latest_execution_receipt_id, latest_evidence_snapshot_id,
                    created_at_ms, updated_at_ms, resolved_at_ms, last_actor
                )
                VALUES (
                    $1,$2,$3,$4,$5,$6,
                    $7,$8,$9,$10,$11,
                    $12,$13,$14,$15,$16,
                    $17,$18,$19,$20,$21,
                    $22,$23,NULL,'recon_core'
                )
                "#,
            )
            .bind(&case_id)
            .bind(tenant_id)
            .bind(subject_id)
            .bind(intent_id)
            .bind(job_id)
            .bind(adapter_id)
            .bind(classified.category.as_str())
            .bind(classified.severity.as_str())
            .bind(next_state.as_str())
            .bind(&classified.summary)
            .bind(&classified.machine_reason)
            .bind(&classified.dedupe_key)
            .bind(&classified.cluster_key)
            .bind(first_seen_at_ms as i64)
            .bind(now_ms as i64)
            .bind(occurrence_count as i64)
            .bind(latest_run_id)
            .bind(linkage.latest_outcome_id.as_deref())
            .bind(linkage.latest_recon_receipt_id.as_deref())
            .bind(linkage.latest_execution_receipt_id.as_deref())
            .bind(linkage.latest_evidence_snapshot_id.as_deref())
            .bind(now_ms as i64)
            .bind(now_ms as i64)
            .execute(&mut **tx)
            .await
            .map_err(sqlx_to_internal)?;
        }

        self.record_event(
            tx,
            &case_id,
            event_type,
            prior_state,
            Some(next_state),
            "recon_core",
            "reconciliation_sync",
            json!({
                "category": classified.category.as_str(),
                "severity": classified.severity.as_str(),
                "machine_reason": classified.machine_reason.clone(),
                "dedupe_key": classified.dedupe_key.clone(),
                "cluster_key": classified.cluster_key.clone(),
                "latest_run_id": latest_run_id,
                "latest_outcome_id": linkage.latest_outcome_id.clone(),
            }),
            now_ms,
        )
        .await?;

        let evidence = build_evidence_records(&case_id, classified, linkage, now_ms);
        self.attach_evidence(tx, &evidence).await?;

        Ok(ExceptionCase {
            case_id,
            tenant_id: tenant_id.to_owned(),
            subject_id: subject_id.to_owned(),
            intent_id: intent_id.to_owned(),
            job_id: job_id.to_owned(),
            adapter_id: adapter_id.to_owned(),
            category: classified.category,
            severity: classified.severity,
            state: next_state,
            summary: classified.summary.clone(),
            machine_reason: classified.machine_reason.clone(),
            dedupe_key: classified.dedupe_key.clone(),
            cluster_key: classified.cluster_key.clone(),
            first_seen_at_ms,
            last_seen_at_ms: now_ms,
            occurrence_count,
            created_at_ms: existing.map(|case| case.created_at_ms).unwrap_or(now_ms),
            updated_at_ms: now_ms,
            resolved_at_ms: None,
            latest_run_id: latest_run_id.map(ToOwned::to_owned),
            latest_outcome_id: linkage.latest_outcome_id.clone(),
            latest_recon_receipt_id: linkage.latest_recon_receipt_id.clone(),
            latest_execution_receipt_id: linkage.latest_execution_receipt_id.clone(),
            latest_evidence_snapshot_id: linkage.latest_evidence_snapshot_id.clone(),
            last_actor: Some("recon_core".to_owned()),
        })
    }

    async fn auto_resolve_case(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        case: &ExceptionCase,
        latest_run_id: Option<&str>,
        linkage: &ExceptionLinkage,
        now_ms: u64,
    ) -> Result<(), ExceptionIntelligenceError> {
        sqlx::query(
            r#"
            UPDATE exception_cases
            SET state = 'resolved',
                updated_at_ms = $2,
                resolved_at_ms = $2,
                latest_run_id = COALESCE($3, latest_run_id),
                latest_outcome_id = COALESCE($4, latest_outcome_id),
                latest_recon_receipt_id = COALESCE($5, latest_recon_receipt_id),
                latest_execution_receipt_id = COALESCE($6, latest_execution_receipt_id),
                latest_evidence_snapshot_id = COALESCE($7, latest_evidence_snapshot_id),
                last_actor = 'recon_core'
            WHERE case_id = $1
            "#,
        )
        .bind(&case.case_id)
        .bind(now_ms as i64)
        .bind(latest_run_id)
        .bind(linkage.latest_outcome_id.as_deref())
        .bind(linkage.latest_recon_receipt_id.as_deref())
        .bind(linkage.latest_execution_receipt_id.as_deref())
        .bind(linkage.latest_evidence_snapshot_id.as_deref())
        .execute(&mut **tx)
        .await
        .map_err(sqlx_to_internal)?;

        self.record_event(
            tx,
            &case.case_id,
            "auto_resolved",
            Some(case.state),
            Some(ExceptionState::Resolved),
            "recon_core",
            "reconciliation_resolved",
            json!({
                "latest_run_id": latest_run_id,
                "latest_outcome_id": linkage.latest_outcome_id.clone(),
            }),
            now_ms,
        )
        .await?;
        self.record_resolution(
            tx,
            &case.case_id,
            ExceptionState::Resolved,
            "recon_core",
            "reconciliation_resolved",
            json!({
                "machine_reason": case.machine_reason.clone(),
                "dedupe_key": case.dedupe_key.clone(),
            }),
            now_ms,
        )
        .await?;
        Ok(())
    }

    async fn load_linkage(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &str,
        subject_id: &str,
        latest_run_id: Option<&str>,
    ) -> Result<ExceptionLinkage, ExceptionIntelligenceError> {
        let mut linkage = ExceptionLinkage::default();
        let subject_row = sqlx::query(
            r#"
            SELECT latest_receipt_id
            FROM recon_core_subjects
            WHERE tenant_id = $1
              AND subject_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(subject_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(sqlx_to_internal)?;

        if let Some(row) = subject_row {
            if let Some(receipt_id) = row
                .try_get::<Option<String>, _>("latest_receipt_id")
                .ok()
                .flatten()
            {
                let execution_row = sqlx::query(
                    r#"
                    SELECT occurred_at_ms, receipt_json
                    FROM execution_core_receipts
                    WHERE receipt_id = $1
                    LIMIT 1
                    "#,
                )
                .bind(&receipt_id)
                .fetch_optional(&mut **tx)
                .await
                .map_err(sqlx_to_internal)?;
                if let Some(exec_row) = execution_row {
                    linkage.latest_execution_receipt_id = Some(receipt_id.clone());
                    linkage.latest_execution_receipt_at_ms =
                        Some(exec_row.get::<i64, _>("occurred_at_ms").max(0) as u64);
                    linkage.latest_execution_receipt_payload = Some(json!({
                        "receipt_id": receipt_id,
                        "receipt": exec_row.get::<Value, _>("receipt_json"),
                    }));
                }
            }
        }

        if let Some(run_id) = latest_run_id {
            if let Some(row) = sqlx::query(
                r#"
                SELECT
                    outcome_id, lifecycle_state, normalized_result, outcome, summary, machine_reason,
                    details_json, exception_case_ids, created_at_ms
                FROM recon_core_outcomes
                WHERE run_id = $1
                ORDER BY created_at_ms DESC, outcome_id DESC
                LIMIT 1
                "#,
            )
            .bind(run_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(sqlx_to_internal)?
            {
                let outcome_id: String = row.get("outcome_id");
                linkage.latest_outcome_id = Some(outcome_id.clone());
                linkage.latest_outcome_at_ms = Some(row.get::<i64, _>("created_at_ms").max(0) as u64);
                linkage.latest_outcome_payload = Some(json!({
                    "outcome_id": outcome_id,
                    "lifecycle_state": row.get::<String, _>("lifecycle_state"),
                    "normalized_result": row.try_get::<Option<String>, _>("normalized_result").ok().flatten(),
                    "outcome": row.get::<String, _>("outcome"),
                    "summary": row.get::<String, _>("summary"),
                    "machine_reason": row.get::<String, _>("machine_reason"),
                    "details": row.get::<Value, _>("details_json"),
                    "exception_case_ids": row.get::<Value, _>("exception_case_ids"),
                }));
            }

            if let Some(row) = sqlx::query(
                r#"
                SELECT recon_receipt_id, outcome, summary, details_json, created_at_ms
                FROM recon_core_receipts
                WHERE run_id = $1
                ORDER BY created_at_ms DESC, recon_receipt_id DESC
                LIMIT 1
                "#,
            )
            .bind(run_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(sqlx_to_internal)?
            {
                let recon_receipt_id: String = row.get("recon_receipt_id");
                linkage.latest_recon_receipt_id = Some(recon_receipt_id.clone());
                linkage.latest_recon_receipt_at_ms =
                    Some(row.get::<i64, _>("created_at_ms").max(0) as u64);
                linkage.latest_recon_receipt_payload = Some(json!({
                    "recon_receipt_id": recon_receipt_id,
                    "outcome": row.get::<String, _>("outcome"),
                    "summary": row.get::<String, _>("summary"),
                    "details": row.get::<Value, _>("details_json"),
                }));
            }

            if let Some(row) = sqlx::query(
                r#"
                SELECT
                    evidence_snapshot_id, context_json, adapter_rows_json, expected_facts_json,
                    observed_facts_json, match_result_json, details_json, exceptions_json, created_at_ms
                FROM recon_core_evidence_snapshots
                WHERE run_id = $1
                ORDER BY created_at_ms DESC, evidence_snapshot_id DESC
                LIMIT 1
                "#,
            )
            .bind(run_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(sqlx_to_internal)?
            {
                let evidence_snapshot_id: String = row.get("evidence_snapshot_id");
                linkage.latest_evidence_snapshot_id = Some(evidence_snapshot_id.clone());
                linkage.latest_evidence_snapshot_at_ms = Some(row.get::<i64, _>("created_at_ms").max(0) as u64);
                linkage.latest_evidence_snapshot_payload = Some(json!({
                    "evidence_snapshot_id": evidence_snapshot_id,
                    "context": row.get::<Value, _>("context_json"),
                    "adapter_rows": row.get::<Value, _>("adapter_rows_json"),
                    "expected_facts": row.get::<Value, _>("expected_facts_json"),
                    "observed_facts": row.get::<Value, _>("observed_facts_json"),
                    "match_result": row.get::<Value, _>("match_result_json"),
                    "details": row.get::<Value, _>("details_json"),
                    "exceptions": row.get::<Value, _>("exceptions_json"),
                }));
            }
        }

        Ok(linkage)
    }

    async fn load_subject_cases(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &str,
        subject_id: &str,
    ) -> Result<Vec<ExceptionCase>, ExceptionIntelligenceError> {
        let rows = sqlx::query(
            r#"
            SELECT
                case_id, tenant_id, subject_id, intent_id, job_id, adapter_id,
                category, severity, state, summary, machine_reason,
                dedupe_key, cluster_key, first_seen_at_ms, last_seen_at_ms, occurrence_count,
                latest_run_id, latest_outcome_id, latest_recon_receipt_id,
                latest_execution_receipt_id, latest_evidence_snapshot_id,
                created_at_ms, updated_at_ms, resolved_at_ms, last_actor
            FROM exception_cases
            WHERE tenant_id = $1
              AND subject_id = $2
            ORDER BY updated_at_ms DESC, case_id DESC
            "#,
        )
        .bind(tenant_id)
        .bind(subject_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(rows.into_iter().map(map_case_row).collect())
    }

    async fn load_case(
        &self,
        tenant_id: &str,
        case_id: &str,
    ) -> Result<Option<ExceptionCase>, ExceptionIntelligenceError> {
        let row = sqlx::query(
            r#"
            SELECT
                case_id, tenant_id, subject_id, intent_id, job_id, adapter_id,
                category, severity, state, summary, machine_reason,
                dedupe_key, cluster_key, first_seen_at_ms, last_seen_at_ms, occurrence_count,
                latest_run_id, latest_outcome_id, latest_recon_receipt_id,
                latest_execution_receipt_id, latest_evidence_snapshot_id,
                created_at_ms, updated_at_ms, resolved_at_ms, last_actor
            FROM exception_cases
            WHERE tenant_id = $1
              AND case_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(case_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(row.map(map_case_row))
    }

    async fn load_case_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        tenant_id: &str,
        case_id: &str,
    ) -> Result<Option<ExceptionCase>, ExceptionIntelligenceError> {
        let row = sqlx::query(
            r#"
            SELECT
                case_id, tenant_id, subject_id, intent_id, job_id, adapter_id,
                category, severity, state, summary, machine_reason,
                dedupe_key, cluster_key, first_seen_at_ms, last_seen_at_ms, occurrence_count,
                latest_run_id, latest_outcome_id, latest_recon_receipt_id,
                latest_execution_receipt_id, latest_evidence_snapshot_id,
                created_at_ms, updated_at_ms, resolved_at_ms, last_actor
            FROM exception_cases
            WHERE tenant_id = $1
              AND case_id = $2
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(case_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(row.map(map_case_row))
    }

    async fn load_case_evidence(
        &self,
        case_id: &str,
    ) -> Result<Vec<ExceptionEvidence>, ExceptionIntelligenceError> {
        let rows = sqlx::query(
            r#"
            SELECT evidence_id, case_id, evidence_type, source_table, source_id, observed_at_ms, payload_json, created_at_ms
            FROM exception_evidence
            WHERE case_id = $1
            ORDER BY created_at_ms ASC, evidence_id ASC
            "#,
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(rows.into_iter().map(map_evidence_row).collect())
    }

    async fn load_case_events(
        &self,
        case_id: &str,
    ) -> Result<Vec<ExceptionEvent>, ExceptionIntelligenceError> {
        let rows = sqlx::query(
            r#"
            SELECT event_id, case_id, event_type, from_state, to_state, actor, reason, payload_json, created_at_ms
            FROM exception_events
            WHERE case_id = $1
            ORDER BY created_at_ms ASC, event_id ASC
            "#,
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(rows.into_iter().map(map_event_row).collect())
    }

    async fn load_resolution_history(
        &self,
        case_id: &str,
    ) -> Result<Vec<ExceptionResolutionRecord>, ExceptionIntelligenceError> {
        let rows = sqlx::query(
            r#"
            SELECT resolution_id, case_id, resolution_state, actor, reason, payload_json, created_at_ms
            FROM exception_resolution_history
            WHERE case_id = $1
            ORDER BY created_at_ms ASC, resolution_id ASC
            "#,
        )
        .bind(case_id)
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(rows.into_iter().map(map_resolution_row).collect())
    }

    async fn attach_evidence(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        evidence: &[ExceptionEvidence],
    ) -> Result<(), ExceptionIntelligenceError> {
        for record in evidence {
            sqlx::query(
                r#"
                INSERT INTO exception_evidence (
                    evidence_id, case_id, evidence_type, source_table, source_id,
                    observed_at_ms, payload_json, created_at_ms
                )
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(&record.evidence_id)
            .bind(&record.case_id)
            .bind(&record.evidence_type)
            .bind(&record.source_table)
            .bind(&record.source_id)
            .bind(record.observed_at_ms.map(|value| value as i64))
            .bind(&record.payload)
            .bind(record.created_at_ms as i64)
            .execute(&mut **tx)
            .await
            .map_err(sqlx_to_internal)?;
        }
        Ok(())
    }

    async fn record_event(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        case_id: &str,
        event_type: &str,
        from_state: Option<ExceptionState>,
        to_state: Option<ExceptionState>,
        actor: &str,
        reason: &str,
        payload: Value,
        created_at_ms: u64,
    ) -> Result<(), ExceptionIntelligenceError> {
        sqlx::query(
            r#"
            INSERT INTO exception_events (
                event_id, case_id, event_type, from_state, to_state, actor, reason, payload_json, created_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
            "#,
        )
        .bind(format!("exevt_{}", Uuid::new_v4().simple()))
        .bind(case_id)
        .bind(event_type)
        .bind(from_state.map(|value| value.as_str().to_owned()))
        .bind(to_state.map(|value| value.as_str().to_owned()))
        .bind(actor)
        .bind(reason)
        .bind(payload)
        .bind(created_at_ms as i64)
        .execute(&mut **tx)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(())
    }

    async fn record_resolution(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        case_id: &str,
        resolution_state: ExceptionState,
        actor: &str,
        reason: &str,
        payload: Value,
        created_at_ms: u64,
    ) -> Result<(), ExceptionIntelligenceError> {
        sqlx::query(
            r#"
            INSERT INTO exception_resolution_history (
                resolution_id, case_id, resolution_state, actor, reason, payload_json, created_at_ms
            )
            VALUES ($1,$2,$3,$4,$5,$6,$7)
            "#,
        )
        .bind(format!("exres_{}", Uuid::new_v4().simple()))
        .bind(case_id)
        .bind(resolution_state.as_str())
        .bind(actor)
        .bind(reason)
        .bind(payload)
        .bind(created_at_ms as i64)
        .execute(&mut **tx)
        .await
        .map_err(sqlx_to_internal)?;
        Ok(())
    }

    async fn table_exists(&self, table_name: &str) -> Result<bool, ExceptionIntelligenceError> {
        let row = sqlx::query("SELECT to_regclass($1)::text AS name")
            .bind(format!("public.{table_name}"))
            .fetch_one(&self.pool)
            .await
            .map_err(sqlx_to_internal)?;
        Ok(row
            .try_get::<Option<String>, _>("name")
            .ok()
            .flatten()
            .is_some())
    }
}

fn build_evidence_records(
    case_id: &str,
    classified: &ClassifiedExceptionDraft,
    linkage: &ExceptionLinkage,
    now_ms: u64,
) -> Vec<ExceptionEvidence> {
    let mut out = Vec::new();

    if let Some(payload) = linkage.latest_execution_receipt_payload.clone() {
        out.push(ExceptionEvidence::new(
            case_id.to_owned(),
            "execution_receipt",
            Some("execution_core_receipts".to_owned()),
            linkage.latest_execution_receipt_id.clone(),
            linkage.latest_execution_receipt_at_ms,
            payload,
            now_ms,
        ));
    }
    if let Some(payload) = linkage.latest_outcome_payload.clone() {
        out.push(ExceptionEvidence::new(
            case_id.to_owned(),
            "recon_outcome",
            Some("recon_core_outcomes".to_owned()),
            linkage.latest_outcome_id.clone(),
            linkage.latest_outcome_at_ms,
            payload,
            now_ms,
        ));
    }
    if let Some(payload) = linkage.latest_recon_receipt_payload.clone() {
        out.push(ExceptionEvidence::new(
            case_id.to_owned(),
            "recon_receipt",
            Some("recon_core_receipts".to_owned()),
            linkage.latest_recon_receipt_id.clone(),
            linkage.latest_recon_receipt_at_ms,
            payload,
            now_ms,
        ));
    }
    if let Some(payload) = linkage.latest_evidence_snapshot_payload.clone() {
        out.push(ExceptionEvidence::new(
            case_id.to_owned(),
            "observed_fact_snapshot",
            Some("recon_core_evidence_snapshots".to_owned()),
            linkage.latest_evidence_snapshot_id.clone(),
            linkage.latest_evidence_snapshot_at_ms,
            payload.clone(),
            now_ms,
        ));
        out.push(ExceptionEvidence::new(
            case_id.to_owned(),
            "adapter_details",
            Some("recon_core_evidence_snapshots".to_owned()),
            linkage.latest_evidence_snapshot_id.clone(),
            linkage.latest_evidence_snapshot_at_ms,
            json!({
                "adapter_rows": payload.get("adapter_rows").cloned().unwrap_or_else(|| Value::Array(Vec::new())),
                "details": payload.get("details").cloned().unwrap_or_else(|| json!({})),
                "context": payload.get("context").cloned().unwrap_or_else(|| json!({})),
            }),
            now_ms,
        ));
    }

    for entry in &classified.evidence {
        out.push(ExceptionEvidence::new(
            case_id.to_owned(),
            entry.evidence_type.clone(),
            entry.source_table.clone(),
            entry.source_id.clone(),
            entry.observed_at_ms,
            entry.payload.clone(),
            now_ms,
        ));
    }

    out
}

fn next_active_state(current: Option<ExceptionState>, suggested: ExceptionState) -> ExceptionState {
    match current {
        Some(ExceptionState::Acknowledged) => ExceptionState::Acknowledged,
        Some(ExceptionState::Investigating) => ExceptionState::Investigating,
        Some(ExceptionState::Resolved)
        | Some(ExceptionState::Dismissed)
        | Some(ExceptionState::FalsePositive)
        | Some(ExceptionState::Open)
        | None => suggested,
    }
}

fn ensure_transition_allowed(
    from_state: ExceptionState,
    to_state: ExceptionState,
) -> Result<(), ExceptionIntelligenceError> {
    let allowed = match from_state {
        ExceptionState::Open => matches!(
            to_state,
            ExceptionState::Acknowledged
                | ExceptionState::Investigating
                | ExceptionState::Resolved
                | ExceptionState::Dismissed
                | ExceptionState::FalsePositive
        ),
        ExceptionState::Acknowledged => matches!(
            to_state,
            ExceptionState::Open
                | ExceptionState::Investigating
                | ExceptionState::Resolved
                | ExceptionState::Dismissed
                | ExceptionState::FalsePositive
        ),
        ExceptionState::Investigating => matches!(
            to_state,
            ExceptionState::Open
                | ExceptionState::Acknowledged
                | ExceptionState::Resolved
                | ExceptionState::Dismissed
                | ExceptionState::FalsePositive
        ),
        ExceptionState::Resolved | ExceptionState::Dismissed | ExceptionState::FalsePositive => {
            matches!(
                to_state,
                ExceptionState::Open | ExceptionState::Investigating
            )
        }
    };

    if allowed {
        Ok(())
    } else {
        Err(ExceptionIntelligenceError::InvalidStateTransition(format!(
            "cannot transition exception case from `{}` to `{}`",
            from_state.as_str(),
            to_state.as_str()
        )))
    }
}

fn normalize_limit(limit: u32) -> u32 {
    if limit == 0 {
        50
    } else {
        limit.clamp(1, 200)
    }
}

fn normalize_filter(value: Option<&str>) -> Option<String> {
    value
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn map_case_row(row: sqlx::postgres::PgRow) -> ExceptionCase {
    ExceptionCase {
        case_id: row.get("case_id"),
        tenant_id: row.get("tenant_id"),
        subject_id: row.get("subject_id"),
        intent_id: row.get("intent_id"),
        job_id: row.get("job_id"),
        adapter_id: row.get("adapter_id"),
        category: ExceptionCategory::parse(row.get::<String, _>("category").as_str())
            .unwrap_or(ExceptionCategory::ManualReviewRequired),
        severity: ExceptionSeverity::parse(row.get::<String, _>("severity").as_str())
            .unwrap_or(ExceptionSeverity::Warning),
        state: ExceptionState::parse(row.get::<String, _>("state").as_str())
            .unwrap_or(ExceptionState::Open),
        summary: row.get("summary"),
        machine_reason: row.get("machine_reason"),
        dedupe_key: row.get("dedupe_key"),
        cluster_key: row.get("cluster_key"),
        first_seen_at_ms: row.get::<i64, _>("first_seen_at_ms").max(0) as u64,
        last_seen_at_ms: row.get::<i64, _>("last_seen_at_ms").max(0) as u64,
        occurrence_count: row.get::<i64, _>("occurrence_count").max(0) as u64,
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
        updated_at_ms: row.get::<i64, _>("updated_at_ms").max(0) as u64,
        resolved_at_ms: row
            .try_get::<Option<i64>, _>("resolved_at_ms")
            .ok()
            .flatten()
            .map(|value| value.max(0) as u64),
        latest_run_id: row
            .try_get::<Option<String>, _>("latest_run_id")
            .ok()
            .flatten(),
        latest_outcome_id: row
            .try_get::<Option<String>, _>("latest_outcome_id")
            .ok()
            .flatten(),
        latest_recon_receipt_id: row
            .try_get::<Option<String>, _>("latest_recon_receipt_id")
            .ok()
            .flatten(),
        latest_execution_receipt_id: row
            .try_get::<Option<String>, _>("latest_execution_receipt_id")
            .ok()
            .flatten(),
        latest_evidence_snapshot_id: row
            .try_get::<Option<String>, _>("latest_evidence_snapshot_id")
            .ok()
            .flatten(),
        last_actor: row
            .try_get::<Option<String>, _>("last_actor")
            .ok()
            .flatten(),
    }
}

fn map_evidence_row(row: sqlx::postgres::PgRow) -> ExceptionEvidence {
    ExceptionEvidence {
        evidence_id: row.get("evidence_id"),
        case_id: row.get("case_id"),
        evidence_type: row.get("evidence_type"),
        source_table: row
            .try_get::<Option<String>, _>("source_table")
            .ok()
            .flatten(),
        source_id: row.try_get::<Option<String>, _>("source_id").ok().flatten(),
        observed_at_ms: row
            .try_get::<Option<i64>, _>("observed_at_ms")
            .ok()
            .flatten()
            .map(|value| value.max(0) as u64),
        payload: row.get::<Value, _>("payload_json"),
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
    }
}

fn map_event_row(row: sqlx::postgres::PgRow) -> ExceptionEvent {
    ExceptionEvent {
        event_id: row.get("event_id"),
        case_id: row.get("case_id"),
        event_type: row.get("event_type"),
        from_state: row
            .try_get::<Option<String>, _>("from_state")
            .ok()
            .flatten()
            .and_then(|value| ExceptionState::parse(&value)),
        to_state: row
            .try_get::<Option<String>, _>("to_state")
            .ok()
            .flatten()
            .and_then(|value| ExceptionState::parse(&value)),
        actor: row.get("actor"),
        reason: row.get("reason"),
        payload: row.get::<Value, _>("payload_json"),
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
    }
}

fn map_resolution_row(row: sqlx::postgres::PgRow) -> ExceptionResolutionRecord {
    ExceptionResolutionRecord {
        resolution_id: row.get("resolution_id"),
        case_id: row.get("case_id"),
        resolution_state: ExceptionState::parse(row.get::<String, _>("resolution_state").as_str())
            .unwrap_or(ExceptionState::Resolved),
        actor: row.get("actor"),
        reason: row.get("reason"),
        payload: row.get::<Value, _>("payload_json"),
        created_at_ms: row.get::<i64, _>("created_at_ms").max(0) as u64,
    }
}

fn sqlx_to_internal(err: sqlx::Error) -> ExceptionIntelligenceError {
    ExceptionIntelligenceError::Backend(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_state_preserves_operator_progress() {
        assert_eq!(
            next_active_state(Some(ExceptionState::Acknowledged), ExceptionState::Open,),
            ExceptionState::Acknowledged
        );
        assert_eq!(
            next_active_state(Some(ExceptionState::Investigating), ExceptionState::Open,),
            ExceptionState::Investigating
        );
        assert_eq!(
            next_active_state(Some(ExceptionState::Resolved), ExceptionState::Open,),
            ExceptionState::Open
        );
    }

    #[test]
    fn operator_transition_matrix_allows_reopen_but_not_terminal_to_terminal_hops() {
        ensure_transition_allowed(ExceptionState::Open, ExceptionState::Acknowledged).unwrap();
        ensure_transition_allowed(ExceptionState::Open, ExceptionState::Resolved).unwrap();
        ensure_transition_allowed(ExceptionState::Acknowledged, ExceptionState::Investigating)
            .unwrap();
        ensure_transition_allowed(ExceptionState::Investigating, ExceptionState::FalsePositive)
            .unwrap();
        ensure_transition_allowed(ExceptionState::Resolved, ExceptionState::Open).unwrap();
        ensure_transition_allowed(ExceptionState::Dismissed, ExceptionState::Investigating)
            .unwrap();

        let err = ensure_transition_allowed(ExceptionState::Resolved, ExceptionState::Dismissed)
            .unwrap_err();
        assert!(matches!(
            err,
            ExceptionIntelligenceError::InvalidStateTransition(_)
        ));
    }

    #[test]
    fn normalize_filter_trims_and_lowercases() {
        assert_eq!(normalize_filter(Some("  HIGH  ")).as_deref(), Some("high"));
        assert_eq!(normalize_filter(Some("   ")), None);
        assert_eq!(normalize_filter(None), None);
    }
}
