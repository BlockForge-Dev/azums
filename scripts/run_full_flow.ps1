$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

# End-to-end A/B/C/D flow verification for local compose stack.
# Run from anywhere: pwsh ./scripts/run_full_flow.ps1

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$ComposeDir = if ($env:COMPOSE_DIR) {
    Resolve-Path $env:COMPOSE_DIR
}
else {
    Resolve-Path (Join-Path $RepoRoot "deployments/compose")
}

$Base = if ($env:BASE_URL) { $env:BASE_URL } else { "http://localhost:8000" }
$ToWallet = if ($env:TO_WALLET) { $env:TO_WALLET } else { "GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC" }

$ing = @{
    authorization      = if ($env:INGRESS_TOKEN) { "Bearer $($env:INGRESS_TOKEN)" } else { "Bearer dev-ingress-token" }
    "x-tenant-id"      = if ($env:TENANT_ID) { $env:TENANT_ID } else { "tenant_demo" }
    "x-principal-id"   = if ($env:INGRESS_PRINCIPAL_ID) { $env:INGRESS_PRINCIPAL_ID } else { "ingress-service" }
    "x-submitter-kind" = "internal_service"
}

$st = @{
    authorization      = if ($env:STATUS_TOKEN) { "Bearer $($env:STATUS_TOKEN)" } else { "Bearer dev-status-token" }
    "x-tenant-id"      = if ($env:TENANT_ID) { $env:TENANT_ID } else { "tenant_demo" }
    "x-principal-id"   = if ($env:STATUS_PRINCIPAL_ID) { $env:STATUS_PRINCIPAL_ID } else { "demo-operator" }
    "x-principal-role" = if ($env:STATUS_PRINCIPAL_ROLE) { $env:STATUS_PRINCIPAL_ROLE } else { "admin" }
}

function Submit-Intent([hashtable]$payload) {
    $body = @{
        intent_kind = "solana.transfer.v1"
        payload     = $payload
    } | ConvertTo-Json -Depth 8

    Invoke-RestMethod -Method Post -Uri "$Base/api/requests" -Headers $ing -ContentType "application/json" -Body $body
}

function Get-Status([string]$executionIntent) {
    Invoke-RestMethod -Method Get -Uri "$Base/status/requests/$executionIntent" -Headers $st
}

function Wait-Terminal([string]$executionIntent, [int]$timeoutSec = 180) {
    $deadline = (Get-Date).AddSeconds($timeoutSec)
    do {
        $s = Get-Status $executionIntent
        if ($s.state -in @("succeeded", "failed_terminal", "dead_lettered", "finalized")) {
            return $s
        }
        Start-Sleep -Seconds 2
    } while ((Get-Date) -lt $deadline)
    return $s
}

function Get-FlowHistory([string]$executionIntent) {
    Invoke-RestMethod -Method Get -Uri "$Base/status/requests/$executionIntent/history" -Headers $st
}

function Get-FlowReceipt([string]$executionIntent) {
    Invoke-RestMethod -Method Get -Uri "$Base/status/requests/$executionIntent/receipt" -Headers $st
}

function Get-SolanaFinalErrText([string]$payloadIntentId) {
    $query = "select coalesce(final_err_json::text,'') from solana.tx_intents where id='$payloadIntentId';"
    $raw = docker compose exec -T postgres psql -U app -d azums -t -A -c $query
    ($raw | Out-String).Trim()
}

Write-Host "Using compose dir: $ComposeDir"
Write-Host "Using base url   : $Base"
Write-Host "Using to wallet  : $ToWallet"
Write-Host

$results = @()

Push-Location $ComposeDir
try {
    # Flow A: Happy path
    $payloadA = "intent_flowA_{0}" -f [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $rA = Submit-Intent @{ intent_id = $payloadA; type = "transfer"; to_addr = $ToWallet; amount = 1 }
    $sA = Wait-Terminal $rA.intent_id 120
    $hA = Get-FlowHistory $rA.intent_id

    $okA = ($sA.state -eq "succeeded") -and
    (@($hA.transitions | Where-Object reason_code -eq "request_received").Count -gt 0) -and
    (@($hA.transitions | Where-Object reason_code -eq "adapter_routed").Count -gt 0) -and
    (@($hA.transitions | Where-Object reason_code -eq "dispatch_started").Count -gt 0)

    $results += [pscustomobject]@{
        Flow            = "A"
        Pass            = $okA
        ExecutionIntent = $rA.intent_id
        State           = $sA.state
        Classification  = $sA.classification
        Notes           = "Inbound -> adapter -> success path"
    }

    # Flow B: Retry path via forced transport failure.
    $payloadB = "intent_flowB_{0}" -f [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $rB = Submit-Intent @{ intent_id = $payloadB; type = "transfer"; to_addr = $ToWallet; amount = 1; rpc_url = "https://127.0.0.1:1" }
    $sB = Wait-Terminal $rB.intent_id 240
    $recB = Get-FlowReceipt $rB.intent_id

    $hasRetryScheduled = @($recB.entries | Where-Object state -eq "retry_scheduled").Count -gt 0
    $hasRetryDue = @($recB.entries | Where-Object { $_.details.reason_code -eq "retry_due" }).Count -gt 0
    $okB = ($sB.state -eq "dead_lettered" -or $sB.last_failure.code -eq "retry_exhausted") -and $hasRetryScheduled -and $hasRetryDue

    $results += [pscustomobject]@{
        Flow            = "B"
        Pass            = $okB
        ExecutionIntent = $rB.intent_id
        State           = $sB.state
        Classification  = $sB.classification
        Notes           = "Retry scheduled + due observed"
    }

    # Flow C: Terminal failure path with invalid destination.
    $payloadC = "intent_flowC_{0}" -f [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
    $rC = Submit-Intent @{ intent_id = $payloadC; type = "transfer"; to_addr = "11111111111111111111111111111111"; amount = 1 }
    $sC = Wait-Terminal $rC.intent_id 120
    $dbC = Get-SolanaFinalErrText $payloadC

    $okC = ($sC.state -eq "failed_terminal") -and ($dbC -match "ReadonlyLamportChange|InstructionError|AccountNotFound")

    $results += [pscustomobject]@{
        Flow            = "C"
        Pass            = $okC
        ExecutionIntent = $rC.intent_id
        State           = $sC.state
        Classification  = $sC.classification
        Notes           = "Terminal failure classification observed"
    }

    # Flow D: Replay path from Flow C.
    $replay = Invoke-RestMethod -Method Post -Uri "$Base/status/requests/$($rC.intent_id)/replay" -Headers $st -ContentType "application/json" -Body '{"reason":"flow-D self-test"}'
    $sD = Wait-Terminal $rC.intent_id 120
    $hD = Get-FlowHistory $rC.intent_id
    $hasReplayStarted = @($hD.transitions | Where-Object reason_code -eq "replay_started").Count -gt 0
    $hasReplayQueued = @($hD.transitions | Where-Object reason_code -eq "replay_queued").Count -gt 0

    $okD = ($null -ne $replay.replay_job_id) -and $hasReplayStarted -and $hasReplayQueued

    $results += [pscustomobject]@{
        Flow            = "D"
        Pass            = $okD
        ExecutionIntent = $rC.intent_id
        State           = $sD.state
        Classification  = $sD.classification
        Notes           = "Replay lineage events observed"
    }

    Write-Host
    Write-Host "=== Flow Verification ==="
    $results | Format-Table -AutoSize

    $failed = @($results | Where-Object { -not $_.Pass })
    if ($failed.Count -gt 0) {
        throw "One or more flow checks failed."
    }

    Write-Host
    Write-Host "All flow checks passed."
}
finally {
    Pop-Location
}



# K8s deployment is correct

# secrets wiring is correct

# runtime dependencies are correct

# health probes are correct

# scaling behavior is correct

# networking/ingress behavior is correct under load