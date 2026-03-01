$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

$BaseUrl = if ($env:BASE_URL) { $env:BASE_URL } else { "http://localhost:8000" }
$TenantId = if ($env:TENANT_ID) { $env:TENANT_ID } else { "tenant_demo" }
$IngressToken = if ($env:INGRESS_TOKEN) { $env:INGRESS_TOKEN } else { "dev-ingress-token" }
$StatusToken = if ($env:STATUS_TOKEN) { $env:STATUS_TOKEN } else { "dev-status-token" }
$WebhookSource = if ($env:WEBHOOK_SOURCE) { $env:WEBHOOK_SOURCE } else { "demo_partner" }
$WebhookIntentKind = if ($env:WEBHOOK_INTENT_KIND) { $env:WEBHOOK_INTENT_KIND } else { "solana.transfer.v1" }
$IngressPrincipalId = if ($env:INGRESS_PRINCIPAL_ID) { $env:INGRESS_PRINCIPAL_ID } else { "ingress-service" }
$WebhookSubmitterKind = if ($env:WEBHOOK_SUBMITTER_KIND) { $env:WEBHOOK_SUBMITTER_KIND } else { "internal_service" }
$StatusPrincipalId = if ($env:STATUS_PRINCIPAL_ID) { $env:STATUS_PRINCIPAL_ID } else { "demo-operator" }
$StatusPrincipalRole = if ($env:STATUS_PRINCIPAL_ROLE) { $env:STATUS_PRINCIPAL_ROLE } else { "admin" }
$WebhookPayloadPath = if ($env:WEBHOOK_PAYLOAD) { $env:WEBHOOK_PAYLOAD } else { Join-Path $ScriptDir "webhook-payload.json" }
$WebhookSecret = if ($env:WEBHOOK_SECRET) { $env:WEBHOOK_SECRET } else { "" }
$WebhookSignature = if ($env:WEBHOOK_SIGNATURE) { $env:WEBHOOK_SIGNATURE } else { "" }
$WebhookId = if ($env:WEBHOOK_ID) { $env:WEBHOOK_ID } else { "webhook-example-{0}" -f [DateTimeOffset]::UtcNow.ToUnixTimeSeconds() }

$WebhookBody = Get-Content -Raw $WebhookPayloadPath

if ([string]::IsNullOrWhiteSpace($WebhookSignature) -and -not [string]::IsNullOrWhiteSpace($WebhookSecret)) {
    $payloadBytes = [System.Text.Encoding]::UTF8.GetBytes($WebhookBody)
    $secretBytes = [System.Text.Encoding]::UTF8.GetBytes($WebhookSecret)
    $hmac = [System.Security.Cryptography.HMACSHA256]::new($secretBytes)
    try {
        $hash = $hmac.ComputeHash($payloadBytes)
        $hex = -join ($hash | ForEach-Object { $_.ToString("x2") })
        $WebhookSignature = "v1=$hex"
    }
    finally {
        $hmac.Dispose()
    }
}

$IngressHeaders = @{
    authorization = "Bearer $IngressToken"
    "x-tenant-id" = $TenantId
    "x-principal-id" = $IngressPrincipalId
    "x-submitter-kind" = $WebhookSubmitterKind
    "x-intent-kind" = $WebhookIntentKind
    "x-webhook-id" = $WebhookId
}
if (-not [string]::IsNullOrWhiteSpace($WebhookSignature)) {
    $IngressHeaders["x-webhook-signature"] = $WebhookSignature
}

$StatusHeaders = @{
    authorization = "Bearer $StatusToken"
    "x-tenant-id" = $TenantId
    "x-principal-id" = $StatusPrincipalId
    "x-principal-role" = $StatusPrincipalRole
}

Write-Host "Submitting webhook intent..."
$submitResponse = Invoke-RestMethod `
    -Method Post `
    -Uri "$BaseUrl/webhooks/$WebhookSource" `
    -Headers $IngressHeaders `
    -ContentType "application/json" `
    -Body $WebhookBody
$submitResponse | ConvertTo-Json -Depth 20

$intentId = $submitResponse.intent_id
if ([string]::IsNullOrWhiteSpace($intentId)) {
    throw "Could not extract intent_id from webhook submit response."
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
