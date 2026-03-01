$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

$BaseUrl = if ($env:BASE_URL) { $env:BASE_URL } else { "http://localhost:8000" }
$TenantId = if ($env:TENANT_ID) { $env:TENANT_ID } else { "tenant_demo" }
$IngressToken = if ($env:INGRESS_TOKEN) { $env:INGRESS_TOKEN } else { "dev-ingress-token" }
$StatusToken = if ($env:STATUS_TOKEN) { $env:STATUS_TOKEN } else { "dev-status-token" }
$IngressPrincipalId = if ($env:INGRESS_PRINCIPAL_ID) { $env:INGRESS_PRINCIPAL_ID } else { "ingress-service" }
$StatusPrincipalId = if ($env:STATUS_PRINCIPAL_ID) { $env:STATUS_PRINCIPAL_ID } else { "demo-operator" }
$StatusPrincipalRole = if ($env:STATUS_PRINCIPAL_ROLE) { $env:STATUS_PRINCIPAL_ROLE } else { "admin" }
$ApplyCallbackDestination = if ($env:APPLY_CALLBACK_DESTINATION) { $env:APPLY_CALLBACK_DESTINATION } else { "false" }

$SubmitPayloadPath = if ($env:SUBMIT_PAYLOAD) { $env:SUBMIT_PAYLOAD } else { Join-Path $ScriptDir "submit-request.json" }
$ReplayPayloadPath = if ($env:REPLAY_PAYLOAD) { $env:REPLAY_PAYLOAD } else { Join-Path $ScriptDir "replay-request.json" }
$CallbackPayloadPath = if ($env:CALLBACK_PAYLOAD) { $env:CALLBACK_PAYLOAD } else { Join-Path $ScriptDir "callback-destination.json" }

$IngressHeaders = @{
    authorization   = "Bearer $IngressToken"
    "x-tenant-id"   = $TenantId
    "x-principal-id" = $IngressPrincipalId
    "x-submitter-kind" = "internal_service"
}

$StatusHeaders = @{
    authorization    = "Bearer $StatusToken"
    "x-tenant-id"    = $TenantId
    "x-principal-id" = $StatusPrincipalId
    "x-principal-role" = $StatusPrincipalRole
}

if ($ApplyCallbackDestination.ToLowerInvariant() -eq "true") {
    Write-Host "Configuring callback destination..."
    $callbackBody = Get-Content -Raw $CallbackPayloadPath
    $callbackResponse = Invoke-RestMethod `
        -Method Post `
        -Uri "$BaseUrl/status/tenant/callback-destination" `
        -Headers $StatusHeaders `
        -ContentType "application/json" `
        -Body $callbackBody
    $callbackResponse | ConvertTo-Json -Depth 20
    Write-Host
}

Write-Host "Submitting Solana intent..."
$submitBody = Get-Content -Raw $SubmitPayloadPath
$submitResponse = Invoke-RestMethod `
    -Method Post `
    -Uri "$BaseUrl/api/requests" `
    -Headers $IngressHeaders `
    -ContentType "application/json" `
    -Body $submitBody
$submitResponse | ConvertTo-Json -Depth 20

$intentId = $submitResponse.intent_id
if ([string]::IsNullOrWhiteSpace($intentId)) {
    throw "Could not extract intent_id from submit response."
}

Write-Host
Write-Host "Intent ID: $intentId"
Write-Host

Write-Host "Request status:"
$statusResponse = Invoke-RestMethod `
    -Method Get `
    -Uri "$BaseUrl/status/requests/$intentId" `
    -Headers $StatusHeaders
$statusResponse | ConvertTo-Json -Depth 20

Write-Host
Write-Host "Receipt:"
$receiptResponse = Invoke-RestMethod `
    -Method Get `
    -Uri "$BaseUrl/status/requests/$intentId/receipt" `
    -Headers $StatusHeaders
$receiptResponse | ConvertTo-Json -Depth 20

Write-Host
Write-Host "History:"
$historyResponse = Invoke-RestMethod `
    -Method Get `
    -Uri "$BaseUrl/status/requests/$intentId/history" `
    -Headers $StatusHeaders
$historyResponse | ConvertTo-Json -Depth 20

Write-Host
Write-Host "Replay:"
$replayBody = Get-Content -Raw $ReplayPayloadPath
$replayResponse = Invoke-RestMethod `
    -Method Post `
    -Uri "$BaseUrl/status/requests/$intentId/replay" `
    -Headers $StatusHeaders `
    -ContentType "application/json" `
    -Body $replayBody
$replayResponse | ConvertTo-Json -Depth 20
