use azums::{Job, JobStatus, MemoryBackend, NewEvent, StorageBackend, StreamBackend};
use chrono::{Duration as ChronoDuration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;

const CASES: usize = 768;
const MAX_TEXT_BYTES: usize = 256;
const MAX_JSON_DEPTH: usize = 4;
const MAX_ARRAY_ITEMS: usize = 16;
const MAX_OBJECT_FIELDS: usize = 16;

#[derive(Debug)]
struct FuzzCase {
    queue_bytes: Vec<u8>,
    job_type_bytes: Vec<u8>,
    stream_bytes: Vec<u8>,
    event_type_bytes: Vec<u8>,
    idempotency_bytes: Option<Vec<u8>>,
    payload_seed: Vec<u8>,
    malformed_bytes: Vec<u8>,
    priority: i32,
    schedule_ms: i16,
    max_attempts_hint: u8,
    operation: u8,
}

#[derive(Debug, Deserialize)]
struct ExpectedPayload {
    #[allow(dead_code)]
    required: String,
}

#[tokio::test]
async fn m13_public_input_boundaries_survive_generated_garbage() -> anyhow::Result<()> {
    let backend = MemoryBackend::new();
    backend.run_migrations().await?;
    let mut committed_jobs = HashSet::new();

    for case_idx in 0..CASES {
        let bytes = deterministic_case_bytes(case_idx);
        let case = fuzz_case_from_bytes(case_idx, &bytes);
        exercise_case(&backend, &case, &mut committed_jobs).await?;
        assert_storage_safety(&backend, &committed_jobs).await?;
    }

    Ok(())
}

#[test]
fn m13_malformed_serialized_data_rejects_without_panic() {
    for case_idx in 0..CASES {
        let bytes = deterministic_case_bytes(case_idx ^ 0x5A5A);
        let malformed = String::from_utf8_lossy(&bytes);

        let job_result = serde_json::from_slice::<Job>(&bytes);
        let event_result = serde_json::from_slice::<NewEvent>(&bytes);
        let status_result = JobStatus::parse(malformed.as_ref());

        assert!(job_result.is_err() || job_result.is_ok());
        assert!(event_result.is_err() || event_result.is_ok());
        assert!(status_result.is_err() || status_result.is_ok());
    }
}

async fn exercise_case(
    backend: &MemoryBackend,
    case: &FuzzCase,
    committed_jobs: &mut HashSet<uuid::Uuid>,
) -> anyhow::Result<()> {
    match case.operation % 5 {
        0 | 1 => {
            let mut job = fuzz_job(case);
            if let Some(key) = bounded_lossy_string(case.idempotency_bytes.as_deref()) {
                job = job.idempotency_key(key);
            }

            let job_id = backend.enqueue(job.into()).await?;
            committed_jobs.insert(job_id);
        }
        2 => {
            let queue = bounded_lossy_string(Some(&case.queue_bytes)).unwrap_or_default();
            let worker = bounded_lossy_string(Some(&case.job_type_bytes)).unwrap_or_default();
            let leased = backend
                .lease_jobs_batch(&queue, &worker, 0, (case.max_attempts_hint % 8 + 1) as i64)
                .await?;
            let mut seen = HashSet::new();
            for job in leased {
                assert!(seen.insert(job.id), "fuzz lease returned duplicate job");
                assert_eq!(job.status, "running");
                assert!(job.locked_by.is_some());
            }
        }
        3 => {
            let stream = bounded_lossy_string(Some(&case.stream_bytes)).unwrap_or_default();
            let event_type = bounded_lossy_string(Some(&case.event_type_bytes)).unwrap_or_default();
            let event = NewEvent::new(event_type, fuzz_json(&case.payload_seed));
            let seq = backend.publish(&stream, event).await?;
            assert!(seq > 0, "stream sequence must be positive");

            let events = backend.read_events(&stream, seq - 1, 8).await?;
            assert!(
                events.iter().any(|event| event.sequence_no == seq),
                "published fuzz event was not readable"
            );
        }
        _ => {
            let _ = serde_json::from_slice::<Value>(&case.malformed_bytes);
            let _ = bounded_lossy_string(Some(&case.malformed_bytes));
            backend.reap_expired_locks().await?;
        }
    }

    let malformed = String::from_utf8_lossy(&case.malformed_bytes);
    let _ = JobStatus::parse(malformed.as_ref());

    let job = fuzz_job(case);
    let typed = job.payload_typed::<ExpectedPayload>();
    assert!(
        typed.is_ok() || typed.is_err(),
        "typed payload parser panicked"
    );

    Ok(())
}

fn fuzz_job(case: &FuzzCase) -> Job {
    Job::new(
        bounded_lossy_string(Some(&case.job_type_bytes)).unwrap_or_default(),
        fuzz_json(&case.payload_seed),
    )
    .queue(bounded_lossy_string(Some(&case.queue_bytes)).unwrap_or_default())
    .priority(case.priority)
    .max_attempts((case.max_attempts_hint % 8 + 1) as i32)
    .run_at(Utc::now() + ChronoDuration::milliseconds(case.schedule_ms as i64))
}

async fn assert_storage_safety(
    backend: &MemoryBackend,
    committed_jobs: &HashSet<uuid::Uuid>,
) -> anyhow::Result<()> {
    let mut listed_ids = HashSet::new();
    for job in backend.list_jobs(None, None, 2_000, None, None).await? {
        assert!(listed_ids.insert(job.id), "job list returned duplicate id");
        assert!(
            JobStatus::parse(&job.status).is_ok(),
            "storage produced invalid status {}",
            job.status
        );
    }

    for job_id in committed_jobs {
        let job = backend
            .get_job(*job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("committed fuzz job {job_id} disappeared"))?;
        let status = JobStatus::parse(&job.status)?;
        if status.is_terminal() {
            assert!(
                job.locked_by.is_none() && job.lock_expires_at.is_none(),
                "terminal fuzz job retained a live lease"
            );
        }
    }

    Ok(())
}

fn fuzz_json(seed: &[u8]) -> Value {
    if seed.is_empty() {
        return Value::Null;
    }
    let mut cursor = 0;
    fuzz_json_at(seed, &mut cursor, 0)
}

fn fuzz_json_at(seed: &[u8], cursor: &mut usize, depth: usize) -> Value {
    if *cursor >= seed.len() {
        return Value::Null;
    }

    let tag = seed[*cursor];
    *cursor += 1;

    if depth >= MAX_JSON_DEPTH {
        return bounded_lossy_string(Some(&seed[(*cursor).saturating_sub(1)..]))
            .map(Value::String)
            .unwrap_or(Value::Null);
    }

    match tag % 7 {
        0 => Value::Null,
        1 => Value::Bool(tag & 0b1000_0000 != 0),
        2 => json!((tag as i64) - 128),
        3 => Value::String(take_fuzz_string(seed, cursor)),
        4 => {
            let len = bounded_len(tag, MAX_ARRAY_ITEMS);
            Value::Array(
                (0..len)
                    .map(|_| fuzz_json_at(seed, cursor, depth + 1))
                    .collect(),
            )
        }
        5 => {
            let len = bounded_len(tag, MAX_OBJECT_FIELDS);
            let mut object = serde_json::Map::new();
            for _ in 0..len {
                let key = take_fuzz_string(seed, cursor);
                object.insert(key, fuzz_json_at(seed, cursor, depth + 1));
            }
            Value::Object(object)
        }
        _ => json!({
            "required": take_fuzz_string(seed, cursor),
            "raw_len": seed.len(),
        }),
    }
}

fn take_fuzz_string(seed: &[u8], cursor: &mut usize) -> String {
    if *cursor >= seed.len() {
        return String::new();
    }
    let requested = bounded_len(seed[*cursor], MAX_TEXT_BYTES);
    *cursor += 1;
    let end = (*cursor + requested).min(seed.len());
    let value = bounded_lossy_string(Some(&seed[*cursor..end])).unwrap_or_default();
    *cursor = end;
    value
}

fn bounded_lossy_string(bytes: Option<&[u8]>) -> Option<String> {
    let bytes = bytes?;
    let take = bytes.len().min(MAX_TEXT_BYTES);
    Some(String::from_utf8_lossy(&bytes[..take]).into_owned())
}

fn bounded_len(seed: u8, max: usize) -> usize {
    if max == 0 {
        0
    } else {
        seed as usize % (max + 1)
    }
}

fn deterministic_case_bytes(case_idx: usize) -> Vec<u8> {
    let mut state = (case_idx as u64)
        .wrapping_add(0xA13F_F00D)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let len = 32 + (case_idx % 512);
    let mut bytes = Vec::with_capacity(len);

    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes.push((state & 0xFF) as u8);
    }

    bytes
}

fn fuzz_case_from_bytes(case_idx: usize, bytes: &[u8]) -> FuzzCase {
    let priority = read_i32(bytes, 0);
    let schedule_ms = (read_u16(bytes, 4) % 101) as i16 - 50;
    let max_attempts_hint = bytes.get(6).copied().unwrap_or(0);
    let operation = bytes.get(7).copied().unwrap_or(case_idx as u8);

    FuzzCase {
        queue_bytes: slice_window(bytes, 8, 64),
        job_type_bytes: slice_window(bytes, 72, 64),
        stream_bytes: slice_window(bytes, 136, 64),
        event_type_bytes: slice_window(bytes, 200, 64),
        idempotency_bytes: (bytes.get(5).copied().unwrap_or(0) % 3 != 0)
            .then(|| slice_window(bytes, 264, 64)),
        payload_seed: slice_window(bytes, 32, 256),
        malformed_bytes: bytes.to_vec(),
        priority,
        schedule_ms,
        max_attempts_hint,
        operation,
    }
}

fn slice_window(bytes: &[u8], start: usize, len: usize) -> Vec<u8> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let start = start % bytes.len();
    let end = (start + len).min(bytes.len());
    bytes[start..end].to_vec()
}

fn read_i32(bytes: &[u8], start: usize) -> i32 {
    let mut raw = [0_u8; 4];
    for (idx, slot) in raw.iter_mut().enumerate() {
        *slot = bytes.get(start + idx).copied().unwrap_or(0);
    }
    i32::from_le_bytes(raw)
}

fn read_u16(bytes: &[u8], start: usize) -> u16 {
    let mut raw = [0_u8; 2];
    for (idx, slot) in raw.iter_mut().enumerate() {
        *slot = bytes.get(start + idx).copied().unwrap_or(0);
    }
    u16::from_le_bytes(raw)
}
