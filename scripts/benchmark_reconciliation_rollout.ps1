param(
    [string]$BaseUrl = $(if ($env:STATUS_BASE_URL) { $env:STATUS_BASE_URL } else { "http://127.0.0.1:8082/status" }),
    [string]$TenantId = $(if ($env:TENANT_ID) { $env:TENANT_ID } else { "tenant_demo" }),
    [string]$StatusToken = $(if ($env:STATUS_TOKEN) { $env:STATUS_TOKEN } else { "dev-status-token" }),
    [string]$PrincipalId = $(if ($env:STATUS_PRINCIPAL_ID) { $env:STATUS_PRINCIPAL_ID } else { "demo-operator" }),
    [string]$PrincipalRole = $(if ($env:STATUS_PRINCIPAL_ROLE) { $env:STATUS_PRINCIPAL_ROLE } else { "admin" }),
    [int]$LookbackHours = 168,
    [int]$Iterations = 10,
    [string]$OutputJsonPath = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-TimedGet([string]$Url) {
    $headers = @{
        Authorization      = "Bearer $StatusToken"
        "x-tenant-id"      = $TenantId
        "x-principal-id"   = $PrincipalId
        "x-principal-role" = $PrincipalRole
    }

    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    $response = Invoke-WebRequest -Method Get -Uri $Url -Headers $headers -ErrorAction Stop -SkipHttpErrorCheck
    $watch.Stop()
    if ([int]$response.StatusCode -lt 200 -or [int]$response.StatusCode -ge 300) {
        throw "request failed with status $([int]$response.StatusCode): $($response.Content)"
    }

    [pscustomobject]@{
        Url       = $Url
        LatencyMs = [Math]::Round($watch.Elapsed.TotalMilliseconds, 2)
        Body      = if ([string]::IsNullOrWhiteSpace($response.Content)) { $null } else { $response.Content | ConvertFrom-Json }
    }
}

function Measure-Endpoint([string]$Label, [string]$Url, [int]$Count) {
    $samples = New-Object System.Collections.Generic.List[double]
    for ($i = 0; $i -lt $Count; $i++) {
        $result = Invoke-TimedGet $Url
        $samples.Add([double]$result.LatencyMs)
    }

    $sorted = @($samples | Sort-Object)
    $avg = if ($sorted.Count -eq 0) { 0 } else { [Math]::Round(($sorted | Measure-Object -Average).Average, 2) }
    $p95Index = if ($sorted.Count -eq 0) { 0 } else { [Math]::Min($sorted.Count - 1, [Math]::Ceiling($sorted.Count * 0.95) - 1) }
    $p95 = if ($sorted.Count -eq 0) { 0 } else { [Math]::Round([double]$sorted[$p95Index], 2) }
    $max = if ($sorted.Count -eq 0) { 0 } else { [Math]::Round([double]$sorted[-1], 2) }

    [pscustomobject]@{
        label       = $Label
        iterations  = $Count
        average_ms  = $avg
        p95_ms      = $p95
        max_ms      = $max
        samples_ms  = @($sorted)
    }
}

$base = $BaseUrl.TrimEnd('/')
$lookback = [Math]::Max(1, [Math]::Min(24 * 30, $LookbackHours))
$summary = Invoke-TimedGet "$base/reconciliation/rollout-summary?lookback_hours=$lookback"
$sampledIntent = [string]$summary.Body.queries.sampled_intent_id

if ([string]::IsNullOrWhiteSpace($sampledIntent)) {
    throw "rollout summary did not return a sampled_intent_id; there is not enough recent traffic to benchmark the unified request path"
}

$exceptionMetrics = Measure-Endpoint "exception_index" "$base/exceptions?include_terminal=true&limit=50&offset=0" $Iterations
$unifiedMetrics = Measure-Endpoint "unified_request" "$base/requests/$([Uri]::EscapeDataString($sampledIntent))/unified" $Iterations

$report = [pscustomobject]@{
    tenant_id        = $TenantId
    lookback_hours   = $lookback
    sampled_intent_id = $sampledIntent
    exception_index  = $exceptionMetrics
    unified_request  = $unifiedMetrics
}

$report | ConvertTo-Json -Depth 8

if (-not [string]::IsNullOrWhiteSpace($OutputJsonPath)) {
    $report | ConvertTo-Json -Depth 8 | Set-Content -Path $OutputJsonPath -Encoding utf8
}
