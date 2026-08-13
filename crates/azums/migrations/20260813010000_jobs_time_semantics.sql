ALTER TABLE jobs
  ADD COLUMN IF NOT EXISTS deadline_at timestamptz NULL,
  ADD COLUMN IF NOT EXISTS timeout_seconds bigint NULL,
  ADD COLUMN IF NOT EXISTS recurring_interval_seconds bigint NULL;

ALTER TABLE jobs_archive
  ADD COLUMN IF NOT EXISTS deadline_at timestamptz NULL,
  ADD COLUMN IF NOT EXISTS timeout_seconds bigint NULL,
  ADD COLUMN IF NOT EXISTS recurring_interval_seconds bigint NULL;

CREATE INDEX IF NOT EXISTS jobs_deadline_idx
  ON jobs(queue, status, deadline_at)
  WHERE deadline_at IS NOT NULL;
