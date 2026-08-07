# Dataset Partitioning Strategy

High-volume queues quickly produce millions of job records, causing index degradation and bloated `VACUUM` execution times. PostgresFlow prevents table bloat using time-partitioned datasets and automatic archiving.

## Time Partitioning Strategy

Jobs are automatically routed into hourly/monthly dataset partitions named according to the queue and scheduled timestamp (`run_at`):

```text
jobs (Declarative Partitioned Table by RANGE)
 ├── jobs_default_20260201_00
 ├── jobs_default_20260201_01
 ├── jobs_emails_20260201_00
 └── jobs_default_overflow (DEFAULT Partition)
```

```mermaid
graph TD
    Enqueue["Enqueue Job (queue='emails', run_at='2026-08-07')"] --> Hash["Compute dataset_id: emails_20260807_14"]
    Hash --> CheckPart{"Partition Exists?"}
    CheckPart -- No --> CreatePart["Call ensure_jobs_dataset_partition()"]
    CheckPart -- Yes --> Insert
    CreatePart --> Insert["INSERT INTO jobs (dataset_id, queue, ...)"]
    Insert --> Subtable[("Insert into jobs_emails_20260807_14 subtable")]
```

## Automatic Archiving & Maintenance

To keep the primary `jobs` table small and fast:
1. **Succeeded Jobs Archive**: Background maintenance tasks move `status='succeeded'` jobs older than N days into `jobs_archive`.
2. **Attempt History Pruning**: Old attempt audit logs (`job_attempts`) and policy decision records (`policy_decisions`) are automatically pruned.
3. **Partition Detach**: Aged partitions can be detached or dropped rapidly without full table locks.
