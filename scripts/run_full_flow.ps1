param(
    [string]$BaseUrl = $(if ($env:BASE_URL) { $env:BASE_URL } else { "http://localhost:18000" }),
    [string]$TenantId = $(if ($env:TENANT_ID) { $env:TENANT_ID } else { "tenant_demo" }),
    [string]$IngressToken = $(if ($env:INGRESS_TOKEN) { $env:INGRESS_TOKEN } else { "dev-ingress-token" }),
    [string]$StatusToken = $(if ($env:STATUS_TOKEN) { $env:STATUS_TOKEN } else { "dev-status-token" }),
    [string]$IngressPrincipalId = $(if ($env:INGRESS_PRINCIPAL_ID) { $env:INGRESS_PRINCIPAL_ID } else { "ingress-service" }),
    [string]$StatusPrincipalId = $(if ($env:STATUS_PRINCIPAL_ID) { $env:STATUS_PRINCIPAL_ID } else { "demo-operator" }),
    [string]$StatusPrincipalRole = $(if ($env:STATUS_PRINCIPAL_ROLE) { $env:STATUS_PRINCIPAL_ROLE } else { "admin" }),
    [string]$SuccessToAddr = $(if ($env:TO_WALLET) { $env:TO_WALLET } else { "GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC" }),
    [string]$CallbackDeliveryUrl = $(if ($env:CALLBACK_DELIVERY_URL) { $env:CALLBACK_DELIVERY_URL } else { "http://reverse-proxy:8000/healthz" }),
    [switch]$SkipCallbackDestinationSetup
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ing = @{
    authorization      = "Bearer $IngressToken"
    "x-tenant-id"      = $TenantId
    "x-principal-id"   = $IngressPrincipalId
    "x-submitter-kind" = "internal_service"
}

$st = @{
    authorization      = "Bearer $StatusToken"
    "x-tenant-id"      = $TenantId
    "x-principal-id"   = $StatusPrincipalId
    "x-principal-role" = $StatusPrincipalRole
}

function Submit-Intent([hashtable]$payload) {
    $body = @{
        intent_kind = "solana.transfer.v1"
        payload     = $payload
    }

    if ($payload.ContainsKey("__metadata")) {
        $body.metadata = $payload["__metadata"]
        $payload.Remove("__metadata") | Out-Null
        $body.payload = $payload
    }

    $body = $body | ConvertTo-Json -Depth 8

    Invoke-RestMethod -Method Post -Uri "$BaseUrl/api/requests" -Headers $ing -ContentType "application/json" -Body $body
}

function Get-RequestStatus([string]$intentId) {
    Invoke-RestMethod -Method Get -Uri "$BaseUrl/status/requests/$intentId" -Headers $st
}

function Get-RequestReceipt([string]$intentId) {
    Invoke-RestMethod -Method Get -Uri "$BaseUrl/status/requests/$intentId/receipt" -Headers $st
}

function Get-RequestHistory([string]$intentId) {
    Invoke-RestMethod -Method Get -Uri "$BaseUrl/status/requests/$intentId/history" -Headers $st
}

function Get-RequestCallbacks([string]$intentId, [int]$attemptLimit = 25) {
    Invoke-RestMethod -Method Get -Uri "$BaseUrl/status/requests/$intentId/callbacks?include_attempts=true&attempt_limit=$attemptLimit" -Headers $st
}

function Wait-Terminal([string]$intentId, [int]$timeoutSec = 180) {
    $deadline = (Get-Date).AddSeconds($timeoutSec)
    $last = $null
    do {
        $last = Get-RequestStatus $intentId
        if ($last.state -in @("succeeded", "failed_terminal", "dead_lettered", "finalized")) {
            return $last
        }
        Start-Sleep -Seconds 2
    } while ((Get-Date) -lt $deadline)
    return $last
}

function Wait-CallbackCount([string]$intentId, [int]$minCount = 1, [int]$timeoutSec = 60) {
    $deadline = (Get-Date).AddSeconds($timeoutSec)
    $last = $null
    do {
        $last = Get-RequestCallbacks $intentId
        if (@($last.callbacks).Count -ge $minCount) {
            return $last
        }
        Start-Sleep -Seconds 2
    } while ((Get-Date) -lt $deadline)
    return $last
}

Write-Host "Using base url          : $BaseUrl"
Write-Host "Using tenant            : $TenantId"
Write-Host "Using callback url      : $CallbackDeliveryUrl"
Write-Host "Using status role       : $StatusPrincipalRole"
Write-Host

$health = Invoke-RestMethod -Method Get -Uri "$BaseUrl/healthz"
$ready = Invoke-RestMethod -Method Get -Uri "$BaseUrl/readyz"

if (-not $SkipCallbackDestinationSetup) {
    $callbackBody = @{
        delivery_url                = $CallbackDeliveryUrl
        allow_private_destinations  = $true
        timeout_ms                  = 3000
        enabled                     = $true
    } | ConvertTo-Json -Depth 6

    Invoke-RestMethod -Method Post -Uri "$BaseUrl/status/tenant/callback-destination" -Headers $st -ContentType "application/json" -Body $callbackBody | Out-Null
}

$results = @()

# Flow A: inbound -> queued -> leased -> executing -> terminal success
$intentA = "intent_flowA_{0}" -f [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$a = Submit-Intent @{
    intent_id = $intentA
    type = "transfer"
    to_addr = $SuccessToAddr
    amount = 1
    __metadata = @{
        "metering.scope" = "playground"
        "ui.surface" = "playground"
        "playground.demo_scenario" = "success"
    }
}
$aStatus = Wait-Terminal $a.intent_id 180
$aReceipt = Get-RequestReceipt $a.intent_id
$aCallbacks = Wait-CallbackCount $a.intent_id 1 90

$aHasReceived = @($aReceipt.entries | Where-Object { $_.state -eq "received" }).Count -gt 0
$aHasValidated = @($aReceipt.entries | Where-Object { $_.state -eq "validated" }).Count -gt 0
$aHasQueued = @($aReceipt.entries | Where-Object { $_.state -eq "queued" }).Count -gt 0
$aHasLeased = @($aReceipt.entries | Where-Object { $_.state -eq "leased" }).Count -gt 0
$aHasExecuting = @($aReceipt.entries | Where-Object { $_.state -eq "executing" }).Count -gt 0
$aPass = ($aStatus.state -eq "succeeded") -and $aHasReceived -and $aHasValidated -and $aHasQueued -and $aHasLeased -and $aHasExecuting

$results += [pscustomobject]@{
    Flow = "A"
    Pass = $aPass
    IntentId = $a.intent_id
    ReplayJobId = ""
    FinalState = [string]$aStatus.state
    Classification = [string]$aStatus.classification
    FailureCode = ""
    CallbackCount = @($aCallbacks.callbacks).Count
    Notes = "Inbound and success path states observed"
}

# Flow B: retryable failure -> retry_scheduled -> retry_due -> dead_lettered
$intentB = "intent_flowB_{0}" -f [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$b = Submit-Intent @{
    intent_id = $intentB
    type = "transfer"
    to_addr = $SuccessToAddr
    amount = 1
    rpc_url = "https://127.0.0.1:1"
    __metadata = @{
        "metering.scope" = "playground"
        "ui.surface" = "playground"
    }
}
$bStatus = Wait-Terminal $b.intent_id 360
$bReceipt = Get-RequestReceipt $b.intent_id
$bCallbacks = Wait-CallbackCount $b.intent_id 1 120

$bHasRetryScheduled = @($bReceipt.entries | Where-Object { $_.state -eq "retry_scheduled" }).Count -gt 0
$bHasRetryDue = @($bReceipt.entries | Where-Object { $_.details.reason_code -eq "retry_due" }).Count -gt 0
$bPass = ($bStatus.state -eq "dead_lettered") -and ([string]$bStatus.last_failure.code -eq "retry_exhausted") -and $bHasRetryScheduled -and $bHasRetryDue

$results += [pscustomobject]@{
    Flow = "B"
    Pass = $bPass
    IntentId = $b.intent_id
    ReplayJobId = ""
    FinalState = [string]$bStatus.state
    Classification = [string]$bStatus.classification
    FailureCode = if ($bStatus.last_failure) { [string]$bStatus.last_failure.code } else { "" }
    CallbackCount = @($bCallbacks.callbacks).Count
    Notes = "Retry path and exhaustion observed"
}

# Flow C: terminal failure classification and receipt details
$intentC = "intent_flowC_{0}" -f [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
$c = Submit-Intent @{
    intent_id = $intentC
    type = "transfer"
    to_addr = $SuccessToAddr
    amount = 1
    __metadata = @{
        "metering.scope" = "playground"
        "ui.surface" = "playground"
        "playground.demo_scenario" = "terminal_failure"
    }
}
$cStatus = Wait-Terminal $c.intent_id 180
$cReceipt = Get-RequestReceipt $c.intent_id
$cHasFailedTerminal = @($cReceipt.entries | Where-Object { $_.state -eq "failed_terminal" }).Count -gt 0
$cPass = ($cStatus.state -eq "failed_terminal") -and $cHasFailedTerminal -and ($null -ne $cStatus.last_failure)

$results += [pscustomobject]@{
    Flow = "C"
    Pass = $cPass
    IntentId = $c.intent_id
    ReplayJobId = ""
    FinalState = [string]$cStatus.state
    Classification = [string]$cStatus.classification
    FailureCode = if ($cStatus.last_failure) { [string]$cStatus.last_failure.code } else { "" }
    CallerCanFix = if ($cStatus.last_failure) { [string]$cStatus.last_failure.caller_can_fix } else { "" }
    OperatorCanFix = if ($cStatus.last_failure) { [string]$cStatus.last_failure.operator_can_fix } else { "" }
    Notes = "Terminal failure and failure metadata observed"
}

# Flow D: replay request -> replay lineage -> replay execution transitions
$replayBody = @{ reason = "flow-D replay validation" } | ConvertTo-Json -Depth 4
$dReplay = Invoke-RestMethod -Method Post -Uri "$BaseUrl/status/requests/$($c.intent_id)/replay" -Headers $st -ContentType "application/json" -Body $replayBody
$dStatus = Wait-Terminal $c.intent_id 180
$dHistory = Get-RequestHistory $c.intent_id
$dCallbacks = Wait-CallbackCount $c.intent_id 2 90

$dHasReplayStarted = @($dHistory.transitions | Where-Object { $_.reason_code -eq "replay_started" }).Count -gt 0
$dHasReplayQueued = @($dHistory.transitions | Where-Object { $_.reason_code -eq "replay_queued" }).Count -gt 0
$dReplayJobTransitions = @($dHistory.transitions | Where-Object { $_.job_id -eq $dReplay.replay_job_id })
$dReplayLeased = @($dReplayJobTransitions | Where-Object { $_.to_state -eq "leased" }).Count -gt 0
$dReplayExecuting = @($dReplayJobTransitions | Where-Object { $_.to_state -eq "executing" }).Count -gt 0
$dPass = ($null -ne $dReplay.replay_job_id) -and $dHasReplayStarted -and $dHasReplayQueued -and $dReplayLeased -and $dReplayExecuting

$results += [pscustomobject]@{
    Flow = "D"
    Pass = $dPass
    IntentId = $c.intent_id
    ReplayJobId = $dReplay.replay_job_id
    FinalState = [string]$dStatus.state
    Classification = [string]$dStatus.classification
    FailureCode = if ($dStatus.last_failure) { [string]$dStatus.last_failure.code } else { "" }
    CallbackCount = @($dCallbacks.callbacks).Count
    Notes = "Replay lineage and replay execution observed (replay_count=$($dReplay.replay_count))"
}

Write-Host
Write-Host "=== Environment ==="
[pscustomobject]@{
    Health = $health
    Ready = $ready
    BaseUrl = $BaseUrl
    TenantId = $TenantId
} | Format-Table -AutoSize

Write-Host
Write-Host "=== Flow Verification (A/B/C/D) ==="
$results | Format-Table Flow, Pass, IntentId, ReplayJobId, FinalState, Classification, FailureCode, CallbackCount, Notes -AutoSize -Wrap

$failed = @($results | Where-Object { -not $_.Pass })
if ($failed.Count -gt 0) {
    throw "One or more flow checks failed."
}

Write-Host
Write-Host "All flow checks passed."
