param(
    [string]$IngressBaseUrl = $(if ($env:INGRESS_BASE_URL) { $env:INGRESS_BASE_URL } else { "http://127.0.0.2:8000" }),
    [string]$IngressToken = $(if ($env:INGRESS_TOKEN) { $env:INGRESS_TOKEN } else { "dev-ingress-token" }),
    [string]$TenantId = "",
    [string]$PrincipalId = $(if ($env:INGRESS_PRINCIPAL_ID) { $env:INGRESS_PRINCIPAL_ID } else { "ingress-service" }),
    [int]$FreePlayLimit = 1,
    [string]$ToAddr = $(if ($env:TO_WALLET) { $env:TO_WALLET } else { "GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC" })
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Parse-Body([string]$raw) {
    if ([string]::IsNullOrWhiteSpace($raw)) {
        return $null
    }
    try {
        return $raw | ConvertFrom-Json
    }
    catch {
        return $raw
    }
}

function Invoke-JsonRequest {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)][string]$Url,
        [hashtable]$Headers,
        [object]$Body
    )

    $invokeArgs = @{
        Method            = $Method
        Uri               = $Url
        ErrorAction       = "Stop"
        SkipHttpErrorCheck = $true
    }
    if ($null -ne $Headers) {
        $invokeArgs.Headers = $Headers
    }
    if ($null -ne $Body) {
        $invokeArgs.ContentType = "application/json"
        $invokeArgs.Body = ($Body | ConvertTo-Json -Depth 12)
    }

    $resp = Invoke-WebRequest @invokeArgs
    [pscustomobject]@{
        Status = [int]$resp.StatusCode
        Body   = Parse-Body $resp.Content
        Raw    = $resp.Content
    }
}

$base = $IngressBaseUrl.TrimEnd("/")
if ([string]::IsNullOrWhiteSpace($TenantId)) {
    $TenantId = "tenant_ws_quota_$([guid]::NewGuid().ToString('N').Substring(0, 8))"
}
if ($FreePlayLimit -lt 1) {
    throw "FreePlayLimit must be at least 1."
}

$headers = @{
    authorization      = "Bearer $IngressToken"
    "x-tenant-id"      = $TenantId
    "x-principal-id"   = $PrincipalId
    "x-submitter-kind" = "internal_service"
}

$intent1 = "intent_quota_$([guid]::NewGuid().ToString('N'))"
$intent2 = "intent_quota_$([guid]::NewGuid().ToString('N'))"

Write-Host "Ingress API : $base"
Write-Host "Tenant      : $TenantId"
Write-Host "Limit       : $FreePlayLimit"
Write-Host

Write-Host "1) Upsert tenant quota profile..."
$quotaResp = Invoke-JsonRequest `
    -Method "PUT" `
    -Url "$base/api/internal/tenants/$TenantId/quota" `
    -Headers $headers `
    -Body @{
        plan                    = "developer"
        access_mode             = "free_play"
        free_play_limit         = $FreePlayLimit
        updated_by_principal_id = "proof:quota_submit_enforcement"
    }
if ($quotaResp.Status -lt 200 -or $quotaResp.Status -ge 300) {
    throw "Quota upsert failed (status=$($quotaResp.Status)): $($quotaResp.Raw)"
}

Write-Host "2) Submit first request (should be accepted)..."
$firstResp = Invoke-JsonRequest `
    -Method "POST" `
    -Url "$base/api/requests" `
    -Headers $headers `
    -Body @{
        intent_kind = "solana.transfer.v1"
        payload     = @{
            intent_id = $intent1
            type      = "transfer"
            to_addr   = $ToAddr
            amount    = 1
        }
    }
if ($firstResp.Status -lt 200 -or $firstResp.Status -ge 300) {
    throw "First submit expected success but got status=$($firstResp.Status): $($firstResp.Raw)"
}

Write-Host "3) Submit second request (should be quota blocked)..."
$secondResp = Invoke-JsonRequest `
    -Method "POST" `
    -Url "$base/api/requests" `
    -Headers $headers `
    -Body @{
        intent_kind = "solana.transfer.v1"
        payload     = @{
            intent_id = $intent2
            type      = "transfer"
            to_addr   = $ToAddr
            amount    = 1
        }
    }
if ($secondResp.Status -ne 429) {
    throw "Second submit expected 429 but got status=$($secondResp.Status): $($secondResp.Raw)"
}

Write-Host
Write-Host "=== Proof Summary ==="
[pscustomobject]@{
    TenantId                    = $TenantId
    FreePlayLimit               = $FreePlayLimit
    FirstSubmitStatus           = $firstResp.Status
    FirstSubmitIntentId         = if ($firstResp.Body -and $firstResp.Body.intent_id) { [string]$firstResp.Body.intent_id } else { "" }
    SecondSubmitStatus          = $secondResp.Status
    SecondSubmitQuotaBlocked    = $true
} | Format-Table -AutoSize

Write-Host
Write-Host "PASS: submit_request enforces tenant free_play quota."
