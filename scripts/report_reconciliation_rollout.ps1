param(
    [string]$BaseUrl = $(if ($env:STATUS_BASE_URL) { $env:STATUS_BASE_URL } else { "http://127.0.0.1:8082/status" }),
    [string]$TenantId = $(if ($env:TENANT_ID) { $env:TENANT_ID } else { "tenant_demo" }),
    [string]$StatusToken = $(if ($env:STATUS_TOKEN) { $env:STATUS_TOKEN } else { "dev-status-token" }),
    [string]$PrincipalId = $(if ($env:STATUS_PRINCIPAL_ID) { $env:STATUS_PRINCIPAL_ID } else { "demo-operator" }),
    [string]$PrincipalRole = $(if ($env:STATUS_PRINCIPAL_ROLE) { $env:STATUS_PRINCIPAL_ROLE } else { "admin" }),
    [int]$LookbackHours = 168,
    [string]$OutputJsonPath = "",
    [string]$OutputMarkdownPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-JsonRequest([string]$Url) {
    $headers = @{
        Authorization    = "Bearer $StatusToken"
        "x-tenant-id"    = $TenantId
        "x-principal-id" = $PrincipalId
        "x-principal-role" = $PrincipalRole
    }

    $response = Invoke-WebRequest -Method Get -Uri $Url -Headers $headers -ErrorAction Stop -SkipHttpErrorCheck
    if ([int]$response.StatusCode -lt 200 -or [int]$response.StatusCode -ge 300) {
        throw "request failed with status $([int]$response.StatusCode): $($response.Content)"
    }
    return ($response.Content | ConvertFrom-Json)
}

$base = $BaseUrl.TrimEnd('/')
$lookback = [Math]::Max(1, [Math]::Min(24 * 30, $LookbackHours))
$summary = Invoke-JsonRequest "$base/reconciliation/rollout-summary?lookback_hours=$lookback"

$markdown = @"
# Reconciliation Rollout Report

- Tenant: $($summary.tenant_id)
- Window hours: $($summary.window.lookback_hours)
- Window start: $([DateTimeOffset]::FromUnixTimeMilliseconds([int64]$summary.window.started_at_ms).ToString("u"))
- Generated at: $([DateTimeOffset]::FromUnixTimeMilliseconds([int64]$summary.window.generated_at_ms).ToString("u"))

## Intake

- Eligible execution receipts: $($summary.intake.eligible_execution_receipts)
- Intake signals: $($summary.intake.intake_signals)
- Subjects total: $($summary.intake.subjects_total)
- Dirty subjects: $($summary.intake.dirty_subjects)
- Retry-scheduled subjects: $($summary.intake.retry_scheduled_subjects)

## Outcomes

- Matched: $($summary.outcomes.matched)
- Partially matched: $($summary.outcomes.partially_matched)
- Unmatched: $($summary.outcomes.unmatched)
- Pending observation: $($summary.outcomes.pending_observation)
- Stale: $($summary.outcomes.stale)
- Manual review required: $($summary.outcomes.manual_review_required)

## Exceptions

- Total cases: $($summary.exceptions.total_cases)
- Unresolved cases: $($summary.exceptions.unresolved_cases)
- High/critical unresolved: $($summary.exceptions.high_or_critical_cases)
- False positive cases: $($summary.exceptions.false_positive_cases)
- Exception rate: $([Math]::Round([double]$summary.exceptions.exception_rate * 100, 2))%
- False positive rate: $([Math]::Round([double]$summary.exceptions.false_positive_rate * 100, 2))%
- Stale rate: $([Math]::Round([double]$summary.exceptions.stale_rate * 100, 2))%

## Latency

- Average recon latency ms: $($summary.latency.avg_recon_latency_ms)
- P95 recon latency ms: $($summary.latency.p95_recon_latency_ms)
- Max recon latency ms: $($summary.latency.max_recon_latency_ms)
- Average operator handling ms: $($summary.latency.avg_operator_handling_ms)
- P95 operator handling ms: $($summary.latency.p95_operator_handling_ms)

## Query Samples

- Sampled intent: $($summary.queries.sampled_intent_id)
- Exception index query ms: $($summary.queries.exception_index_query_ms)
- Unified request query ms: $($summary.queries.unified_request_query_ms)
"@

Write-Host $markdown

if (-not [string]::IsNullOrWhiteSpace($OutputJsonPath)) {
    $summary | ConvertTo-Json -Depth 8 | Set-Content -Path $OutputJsonPath -Encoding utf8
}

if (-not [string]::IsNullOrWhiteSpace($OutputMarkdownPath)) {
    $markdown | Set-Content -Path $OutputMarkdownPath -Encoding utf8
}
