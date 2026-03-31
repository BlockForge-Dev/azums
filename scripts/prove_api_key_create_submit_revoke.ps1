param(
    [string]$OperatorUiBaseUrl = $(if ($env:OPERATOR_UI_BASE_URL) { $env:OPERATOR_UI_BASE_URL } else { "http://127.0.0.2:8083" }),
    [string]$IngressBaseUrl = $(if ($env:INGRESS_BASE_URL) { $env:INGRESS_BASE_URL } else { "http://127.0.0.2:8000" }),
    [string]$Email = $(if ($env:OPERATOR_UI_EMAIL) { $env:OPERATOR_UI_EMAIL } else { "demo@azums.dev" }),
    [string]$Password = $(if ($env:OPERATOR_UI_PASSWORD) { $env:OPERATOR_UI_PASSWORD } else { "dev-password" }),
    [string]$ToAddr = $(if ($env:TO_WALLET) { $env:TO_WALLET } else { "GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC" }),
    [string]$PrincipalId = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Convert-Body([string]$raw) {
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

function Resolve-OperatorUiBaseUrl([string]$rawBaseUrl) {
    $candidate = $rawBaseUrl.TrimEnd("/")
    try {
        $candidateUri = [Uri]$candidate
    }
    catch {
        return $candidate
    }

    $poweredBy = $null
    try {
        $probe = Invoke-WebRequest `
            -Method "GET" `
            -Uri "$candidate/" `
            -TimeoutSec 4 `
            -MaximumRedirection 0 `
            -ErrorAction "Stop"
        $poweredBy = [string]$probe.Headers["X-Powered-By"]
    }
    catch {
        return $candidate
    }

    if (($poweredBy -notmatch "Next\.js") -or
        ($candidateUri.Host -ne "localhost" -and $candidateUri.Host -ne "127.0.0.1")) {
        return $candidate
    }

    $fallbackPort = if ($candidateUri.IsDefaultPort) {
        if ($candidateUri.Scheme -eq "https") { 443 } else { 80 }
    }
    else {
        $candidateUri.Port
    }
    $fallback = "{0}://127.0.0.2:{1}" -f $candidateUri.Scheme, $fallbackPort

    try {
        $probeFallback = Invoke-WebRequest `
            -Method "GET" `
            -Uri "$fallback/" `
            -TimeoutSec 4 `
            -MaximumRedirection 0 `
            -ErrorAction "Stop"
        $fallbackPoweredBy = [string]$probeFallback.Headers["X-Powered-By"]
        if ($fallbackPoweredBy -notmatch "Next\.js") {
            Write-Host "Detected Next.js at $candidate; using operator_ui backend endpoint $fallback instead."
            return $fallback.TrimEnd("/")
        }
    }
    catch {
        # Fall through to warning and keep candidate.
    }

    Write-Warning "Detected Next.js at $candidate. If proof fails, use -OperatorUiBaseUrl with the backend endpoint directly."
    return $candidate
}

function Invoke-JsonApi {
    param(
        [Parameter(Mandatory = $true)][string]$Method,
        [Parameter(Mandatory = $true)][string]$Url,
        [hashtable]$Headers,
        [object]$Body,
        [Microsoft.PowerShell.Commands.WebRequestSession]$WebSession
    )

    $invokeArgs = @{
        Method      = $Method
        Uri         = $Url
        ErrorAction = "Stop"
    }
    if ($null -ne $Headers) {
        $invokeArgs.Headers = $Headers
    }
    if ($null -ne $WebSession) {
        $invokeArgs.WebSession = $WebSession
    }
    if ($null -ne $Body) {
        $invokeArgs.ContentType = "application/json"
        $invokeArgs.Body = ($Body | ConvertTo-Json -Depth 12)
    }

    try {
        $respBody = Invoke-RestMethod @invokeArgs
        return [pscustomobject]@{
            Ok     = $true
            Status = 200
            Body   = $respBody
            Error  = $null
        }
    }
    catch {
        $status = 0
        $rawBody = $null

        if ($_.Exception.Response -and $_.Exception.Response.StatusCode) {
            $status = [int]$_.Exception.Response.StatusCode
        }
        if ($_.ErrorDetails -and $_.ErrorDetails.Message) {
            $rawBody = $_.ErrorDetails.Message
        }
        elseif ($_.Exception.Response) {
            $response = $_.Exception.Response
            if ($response -is [System.Net.Http.HttpResponseMessage]) {
                try {
                    $rawBody = $response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
                }
                catch {
                    $rawBody = $null
                }
            }
            else {
                try {
                    $stream = $response.GetResponseStream()
                    if ($null -ne $stream) {
                        $reader = [System.IO.StreamReader]::new($stream)
                        $rawBody = $reader.ReadToEnd()
                        $reader.Dispose()
                        $stream.Dispose()
                    }
                }
                catch {
                    $rawBody = $null
                }
            }
        }

        return [pscustomobject]@{
            Ok     = $false
            Status = $status
            Body   = (Convert-Body $rawBody)
            Error  = $_.Exception.Message
        }
    }
}

function Ensure-Ok([object]$result, [string]$context) {
    if (-not $result.Ok) {
        $message = if ($null -ne $result.Body) {
            if ($result.Body -is [string]) { $result.Body } else { ($result.Body | ConvertTo-Json -Depth 10) }
        }
        else {
            $result.Error
        }
        throw "$context failed (status=$($result.Status)): $message"
    }
}

$ui = Resolve-OperatorUiBaseUrl $OperatorUiBaseUrl
$ingress = $IngressBaseUrl.TrimEnd("/")
$session = [Microsoft.PowerShell.Commands.WebRequestSession]::new()
$runId = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()

Write-Host "Operator UI : $ui"
Write-Host "Ingress API : $ingress"
Write-Host "Email       : $Email"
Write-Host

Write-Host "1) Login..."
$login = Invoke-JsonApi `
    -Method "POST" `
    -Url "$ui/api/ui/account/login" `
    -Body @{
        email    = $Email
        password = $Password
    } `
    -WebSession $session
Ensure-Ok $login "Login"

Write-Host "2) Resolve session/tenant..."
$sessionResp = Invoke-JsonApi `
    -Method "GET" `
    -Url "$ui/api/ui/account/session" `
    -WebSession $session
Ensure-Ok $sessionResp "Session lookup"
if (-not $sessionResp.Body.authenticated) {
    throw "Session is not authenticated after login."
}

$tenantId = [string]$sessionResp.Body.session.tenant_id
if ([string]::IsNullOrWhiteSpace($tenantId)) {
    throw "Session did not return tenant_id."
}
Write-Host "   tenant_id=$tenantId"

Write-Host "3) Create API key from UI..."
$createKey = Invoke-JsonApi `
    -Method "POST" `
    -Url "$ui/api/ui/account/api-keys" `
    -Body @{
        name = "proof-$runId"
    } `
    -WebSession $session
Ensure-Ok $createKey "Create API key"

$keyId = [string]$createKey.Body.key.id
$apiToken = [string]$createKey.Body.token
if ([string]::IsNullOrWhiteSpace($keyId) -or [string]::IsNullOrWhiteSpace($apiToken)) {
    throw "Create API key response missing key id/token."
}
Write-Host "   key_id=$keyId"

$submitHeaders = @{
    "x-tenant-id"      = $tenantId
    "x-submitter-kind" = "api_key_holder"
    "x-api-key"        = $apiToken
}
if (-not [string]::IsNullOrWhiteSpace($PrincipalId)) {
    $submitHeaders["x-principal-id"] = $PrincipalId
}

$intentOk = "intent_api_key_proof_${runId}_ok"
$intentBlocked = "intent_api_key_proof_${runId}_revoked"

Write-Host "4) Submit with new key (should succeed)..."
$submitOk = Invoke-JsonApi `
    -Method "POST" `
    -Url "$ingress/api/requests" `
    -Headers $submitHeaders `
    -Body @{
        intent_kind = "solana.transfer.v1"
        payload     = @{
            intent_id = $intentOk
            type      = "transfer"
            to_addr   = $ToAddr
            amount    = 1
        }
    }
Ensure-Ok $submitOk "Submit with newly created API key"
Write-Host "   intent_id=$($submitOk.Body.intent_id)"

Write-Host "5) Revoke API key from UI..."
$revoke = Invoke-JsonApi `
    -Method "POST" `
    -Url "$ui/api/ui/account/api-keys/$keyId/revoke" `
    -WebSession $session
Ensure-Ok $revoke "Revoke API key"

Write-Host "6) Submit with revoked key (should fail)..."
$submitRevoked = Invoke-JsonApi `
    -Method "POST" `
    -Url "$ingress/api/requests" `
    -Headers $submitHeaders `
    -Body @{
        intent_kind = "solana.transfer.v1"
        payload     = @{
            intent_id = $intentBlocked
            type      = "transfer"
            to_addr   = $ToAddr
            amount    = 1
        }
    }

$revokedBlocked = (-not $submitRevoked.Ok) -and ($submitRevoked.Status -in @(401, 403))
if (-not $revokedBlocked) {
    $bodyText = if ($null -eq $submitRevoked.Body) { "" } elseif ($submitRevoked.Body -is [string]) { $submitRevoked.Body } else { ($submitRevoked.Body | ConvertTo-Json -Depth 10) }
    throw "Expected revoked key submit to fail with 401/403. got status=$($submitRevoked.Status), ok=$($submitRevoked.Ok), body=$bodyText"
}

Write-Host
Write-Host "=== Proof Summary ==="
[pscustomobject]@{
    LoginOk                    = $login.Ok
    TenantId                   = $tenantId
    CreatedKeyId               = $keyId
    SubmitWithNewKeyStatus     = $submitOk.Status
    SubmitWithNewKeyIntentId   = [string]$submitOk.Body.intent_id
    RevokeStatus               = $revoke.Status
    SubmitWithRevokedKeyStatus = $submitRevoked.Status
    RevokedKeyBlocked          = $revokedBlocked
} | Format-Table -AutoSize

Write-Host
Write-Host "PASS: create -> submit works, revoke -> submit is blocked."
