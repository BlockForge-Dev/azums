use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
/// Durable record explaining a queue-policy decision for one job.
pub struct PolicyDecisionRow {
    /// Unique decision identifier.
    pub id: Uuid,
    /// Job affected by the decision.
    pub job_id: Uuid,
    /// Decision name such as `THROTTLED` or `DELAYED`.
    pub decision: String, // THROTTLED / DELAYED / QUARANTINED
    /// Machine-readable reason for the decision.
    pub reason_code: String, // IN_FLIGHT_EXCEEDED / RETRY_RATE_EXCEEDED ...
    /// Structured measurements and policy context.
    pub details_json: Value,
    /// Time at which the policy decision was recorded.
    pub created_at: DateTime<Utc>,
}

#[derive(Clone)]
/// PostgreSQL repository for durable policy-decision history.
pub struct PolicyDecisionsRepo {
    pool: PgPool,
}

impl PolicyDecisionsRepo {
    /// Creates a policy-decision repository backed by `pool`.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts one policy decision and returns its identifier.
    pub async fn insert_decision(
        &self,
        job_id: Uuid,
        decision: &str,
        reason_code: &str,
        details_json: Value,
    ) -> anyhow::Result<Uuid> {
        let id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO policy_decisions (
              id, dataset_id, job_id, decision, reason_code, details_json
            )
            VALUES (
              $1,
              (SELECT dataset_id FROM jobs WHERE id = $2 LIMIT 1),
              $2, $3, $4, $5
            )
            "#,
        )
        .bind(id)
        .bind(job_id)
        .bind(decision)
        .bind(reason_code)
        .bind(details_json)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    /// Lists policy decisions for a job in chronological order.
    pub async fn list_for_job(&self, job_id: Uuid) -> anyhow::Result<Vec<PolicyDecisionRow>> {
        let rows = sqlx::query_as::<_, PolicyDecisionRow>(
            r#"
            SELECT id, job_id, decision, reason_code, details_json, created_at
            FROM policy_decisions
            WHERE job_id = $1
            ORDER BY created_at ASC
            "#,
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }
}
