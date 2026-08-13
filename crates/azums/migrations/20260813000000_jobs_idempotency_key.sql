ALTER TABLE jobs
ADD COLUMN IF NOT EXISTS idempotency_key TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS jobs_idempotency_key_uq
ON jobs (dataset_id, idempotency_key)
WHERE idempotency_key IS NOT NULL;
