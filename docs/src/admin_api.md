# Admin API & Web UI

PostgresFlow comes with an embedded, zero-dependency Axum REST API and visual web console accessible at `http://localhost:3003/`.

## Web Console Features

Accessing `GET /` in your browser opens the built-in HTML/JS administration dashboard:

- **Metrics Panel**: Real-time queue depth, jobs per second, success rate, retry rate, and mean execution latency.
- **Enqueue Job Panel**: Test job enqueueing directly from the web browser.
- **Job List & Filter**: Search and inspect jobs by queue, status (`queued`, `running`, `succeeded`, `failed`, `dlq`), and pagination cursor.
- **DLQ Inspector**: Filter and view jobs currently in Dead-Letter Queue.
- **Job Timeline & Replay**: Inspect full execution history of individual jobs and trigger atomic job replays (`POST /jobs/:id/replay`).

## REST API Endpoints

| Method | Endpoint | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Health check endpoint (always unauthenticated for k8s probes) |
| `GET` | `/` | Single-file embedded HTML/JS Web Console UI |
| `GET` | `/jobs` | List jobs with cursor pagination (`queue`, `status`, `limit`) |
| `POST` | `/jobs` | Enqueue a new job |
| `GET` | `/jobs/:id/timeline` | Get attempt history and state timeline for a job |
| `GET` | `/jobs/:id/explain` | Explain status, retry count, error codes, and suggested remediation |
| `POST` | `/jobs/:id/replay` | Atomically replay a failed or DLQ job |
| `GET` | `/dlq` | List jobs currently in Dead-Letter Queue |
| `GET` | `/metrics` | JSON snapshot of queue depths and throughput rates |
| `GET` | `/metrics/prom` | Native Prometheus text metrics format |

## Security & Authentication

Authentication can be enforced by setting the `PGFLOW_API_TOKEN` environment variable. When set, all requests to administrative endpoints must supply the secret token via `x-api-key` or `Authorization: Bearer <token>`.
