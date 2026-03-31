param(
    [string]$OperatorUiBaseUrl = "http://127.0.0.1:18083",
    [string]$Email = "demo@azums.dev",
    [string]$Password = "dev-password",
    [string]$FlutterwaveTransactionId = "",
    [switch]$AttemptWebhook,
    [string]$FlutterwaveWebhookHash = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Read-OptionalProperty {
    param(
        [object]$Object,
        [string]$Name,
        [object]$Default = $null
    )
    if ($null -eq $Object) { return $Default }
    $prop = $Object.PSObject.Properties[$Name]
    if ($null -eq $prop) { return $Default }
    return $prop.Value
}

function Invoke-Json {
    param(
        [string]$Method,
        [string]$Url,
        [object]$Body = $null,
        [Microsoft.PowerShell.Commands.WebRequestSession]$Session = $null,
        [hashtable]$Headers = @{}
    )

    $params = @{
        Method      = $Method
        Uri         = $Url
        Headers     = $Headers
        ErrorAction = "Stop"
    }
    if ($Session) {
        $params.WebSession = $Session
    }
    if ($null -ne $Body) {
        $params.ContentType = "application/json"
        $params.Body = ($Body | ConvertTo-Json -Depth 16 -Compress)
    }
    Invoke-RestMethod @params
}

function Invoke-JsonWebRequest {
    param(
        [string]$Method,
        [string]$Url,
        [object]$Body = $null,
        [Microsoft.PowerShell.Commands.WebRequestSession]$Session = $null,
        [hashtable]$Headers = @{}
    )

    $params = @{
        Method             = $Method
        Uri                = $Url
        Headers            = $Headers
        SkipHttpErrorCheck = $true
        ErrorAction        = "Stop"
    }
    if ($Session) {
        $params.WebSession = $Session
    }
    if ($null -ne $Body) {
        $params.ContentType = "application/json"
        $params.Body = ($Body | ConvertTo-Json -Depth 16 -Compress)
    }

    $response = Invoke-WebRequest @params
    $parsed = $null
    if (-not [string]::IsNullOrWhiteSpace($response.Content)) {
        try {
            $parsed = $response.Content | ConvertFrom-Json
        }
        catch {
            $parsed = $response.Content
        }
    }
    [pscustomobject]@{
        StatusCode = [int]$response.StatusCode
        Body       = $parsed
        Raw        = [string]$response.Content
    }
}

$base = $OperatorUiBaseUrl.TrimEnd("/")
$session = New-Object Microsoft.PowerShell.Commands.WebRequestSession

Write-Host "Operator UI backend : $base"
Write-Host "Email               : $Email"

Write-Host "1) Login..."
$loginResponse = Invoke-JsonWebRequest -Method "POST" -Url "$base/api/ui/account/login" -Session $session -Body @{
    email    = $Email
    password = $Password
}
$login = $loginResponse.Body
if ($loginResponse.StatusCode -lt 200 -or $loginResponse.StatusCode -ge 300 -or $null -eq $login -or -not $login.ok) {
    $message = ""
    if ($null -ne $login -and $login.PSObject.Properties["error"]) {
        $message = [string]$login.error
    } elseif (-not [string]::IsNullOrWhiteSpace($loginResponse.Raw)) {
        $message = $loginResponse.Raw
    }
    if ($message -like "*No account found*") {
        throw "Billing verification requires an existing verified account. Sign up first and complete email verification, or rerun the public-edge verifier with -SkipBilling."
    }
    if ($message -like "*Email is not verified yet*") {
        throw "Billing verification requires a verified account. Complete the verify-email flow for '$Email' first, then rerun billing verification."
    }
    throw "Login failed: $message"
}

Write-Host "2) Read billing provider config..."
$providers = Invoke-Json -Method "GET" -Url "$base/api/ui/account/billing/providers" -Session $session

Write-Host "3) Read billing profile..."
$billing = Invoke-Json -Method "GET" -Url "$base/api/ui/account/billing" -Session $session

$verifyResult = $null
if (-not [string]::IsNullOrWhiteSpace($FlutterwaveTransactionId)) {
    Write-Host "4) Verify Flutterwave transaction id..."
    $verifyResult = Invoke-Json -Method "PUT" -Url "$base/api/ui/account/billing" -Session $session -Body @{
        flutterwave_transaction_id = $FlutterwaveTransactionId
    }
}

$webhookResult = $null
if ($AttemptWebhook) {
    if ([string]::IsNullOrWhiteSpace($FlutterwaveWebhookHash)) {
        throw "-AttemptWebhook requires -FlutterwaveWebhookHash"
    }
    if ([string]::IsNullOrWhiteSpace($FlutterwaveTransactionId)) {
        throw "-AttemptWebhook requires -FlutterwaveTransactionId"
    }

    $payload = @{
        event = "charge.completed"
        data  = @{
            id       = $FlutterwaveTransactionId
            tx_ref   = "azums-$FlutterwaveTransactionId"
            customer = @{
                email = $Email
            }
        }
    }
    Write-Host "5) Trigger webhook endpoint..."
    $webhookResult = Invoke-Json -Method "POST" -Url "$base/api/ui/billing/flutterwave/webhook" -Body $payload -Headers @{
        "verif-hash" = $FlutterwaveWebhookHash
    }
}

Write-Host ""
Write-Host "=== Billing Verification Summary ==="
$supportedCurrencies = Read-OptionalProperty -Object $providers.flutterwave -Name "supported_currencies" -Default @()
$paymentCurrency = Read-OptionalProperty -Object $billing.profile -Name "payment_currency"
$paymentAmount = Read-OptionalProperty -Object $billing.profile -Name "payment_amount"
$paymentAmountUsd = Read-OptionalProperty -Object $billing.profile -Name "payment_amount_usd"
$summary = [PSCustomObject]@{
    LoginOk                    = [bool]$login.ok
    ProviderReady              = [bool]$providers.flutterwave.ready
    ProviderHasSecretKey       = [bool]$providers.flutterwave.has_secret_key
    ProviderHasWebhookHash     = [bool]$providers.flutterwave.has_webhook_hash
    SupportedCurrencies        = ($supportedCurrencies -join ",")
    BillingPlan                = $billing.profile.plan
    BillingAccessMode          = $billing.profile.access_mode
    BillingPaymentProvider     = $billing.profile.payment_provider
    BillingPaymentReference    = $billing.profile.payment_reference
    BillingPaymentCurrency     = $paymentCurrency
    BillingPaymentAmount       = $paymentAmount
    BillingPaymentAmountUsd    = $paymentAmountUsd
    VerificationAttempted      = (-not [string]::IsNullOrWhiteSpace($FlutterwaveTransactionId))
    VerificationUpdatedBilling = if ($verifyResult) { [bool]$verifyResult.ok } else { $false }
    WebhookAttempted           = [bool]$AttemptWebhook
    WebhookAccepted            = if ($webhookResult) { [bool]$webhookResult.ok } else { $false }
}
$summary | Format-List
