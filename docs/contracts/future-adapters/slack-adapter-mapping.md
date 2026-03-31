# Slack Adapter Mapping

## First plausible intents

- send channel message
- send direct message
- append to thread

## Execution adapter mapping

| Field | Mapping |
|---|---|
| `adapter_id` | `slack` |
| likely normalized intent kinds | `slack.message.channel`, `slack.message.dm`, `slack.message.thread_reply` |
| stable execution references | team id, channel id, message ts |
| likely `fetch_status(...)` | optional |

## Expected facts

- workspace/team id
- channel or user destination
- template id or content hash
- thread target
- message reference

## Observed facts

- observed channel/user
- observed message ts
- content hash or template fingerprint
- thread ts
- Slack API outcome

## Mismatch mapping

| Subcode | Generic category | Baseline severity | Default operator path |
|---|---|---|---|
| `channel_not_found` | `destination_mismatch` | `high` | `investigate` |
| `destination_mismatch` | `destination_mismatch` | `high` | `investigate` |
| `thread_mismatch` | `state_mismatch` | `warning` | `investigate` |
| `message_reference_missing` | `observation_missing` | `warning` | `investigate` |
| `content_hash_mismatch` | `state_mismatch` | `high` | `investigate` |
| `permission_denied` | `policy_violation` | `high` | `investigate` |

## Evidence mapping

| Evidence source | Purpose |
|---|---|
| adapter-local message intent row | expected destination, content fingerprint, thread target |
| adapter-local API attempt row | Slack API response, remote identifiers, retry posture |
| durable message lookup snapshot | observed channel, ts, content fingerprint, team context |

## Non-goals for v1

- rich block-kit semantic comparison beyond message fingerprinting
- Slack admin policy reconciliation outside the direct message send path
