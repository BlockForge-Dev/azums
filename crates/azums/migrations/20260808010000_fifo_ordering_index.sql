-- Index for fast FIFO queue scanning and leasing
CREATE INDEX IF NOT EXISTS jobs_fifo_queue_created_idx
  ON jobs (queue, status, run_at, created_at ASC)
  WHERE status = 'queued';
