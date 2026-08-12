# Introduction to PostgresFlow

**PostgresFlow** is a modern, high-performance background job queue engine built entirely on **PostgreSQL**, **SQLx**, and **Tokio**.

```
+-------------------------------------------------------------------------+
|                              PostgresFlow                               |
|                                                                         |
|  +--------------------+  +----------------------+  +-----------------+  |
|  | Transactional      |  | Time-Partitioned     |  | Built-in Admin  |  |
|  | ACID Enqueue       |  | Dataset Storage      |  | HTTP Web UI     |  |
|  +--------------------+  +----------------------+  +-----------------+  |
|  | FOR UPDATE         |  | Auto Retries &       |  | Prometheus      |  |
|  | SKIP LOCKED        |  | Dead-Letter Queue    |  | Metrics         |  |
|  +--------------------+  +----------------------+  +-----------------+  |
+-------------------------------------------------------------------------+
```

## Why Postgres-Only Background Jobs?

Traditional background job systems force application developers to introduce secondary stateful infrastructure (such as Redis, RabbitMQ, Sidekiq, or Celery).

Introducing a second broker creates dual-write consistency problems:
1. Your application commits a database transaction (e.g. `users.insert(...)`).
2. Your application tries to publish a job to Redis/RabbitMQ.
3. If the network fails or the server crashes between step 1 and step 2, your database contains records without corresponding background jobs, leading to silent state drift and data loss.

**PostgresFlow solves this by allowing jobs to be enqueued directly inside your application's Postgres transactions.** If your SQL transaction rolls back, the background job never exists. If your SQL transaction commits, the job exists atomically with the rest of that transaction.

## Key Features

- ⚡ **Zero-Config Quickstart**: Go from zero to a running job worker in under 2 minutes.
- **Transactional Enqueue**: Enqueue jobs inside SQL transactions when using a SQL backend.
- ⚡ **Concurrent Leasing**: Multi-worker parallel processing powered by Postgres `FOR UPDATE SKIP LOCKED`.
- 📁 **Dataset Time Partitioning**: Bounded table sizes via monthly table partitioning and archiving.
- 🔄 **Retries & Backoff**: Configurable exponential backoff with random jitter and DLQ routing.
- 📊 **Visual Web UI**: Built-in HTTP dashboard and Prometheus metrics endpoint.
