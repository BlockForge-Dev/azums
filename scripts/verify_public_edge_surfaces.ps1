param(
    [string]$Namespace = "azums",
    [string]$ImageNamespace = "ghcr.io/blockforge-dev/azums",
    [string]$Tag = "",
    [switch]$DeployKnownGoodImages,
    [switch]$BuildLocalImages,
    [switch]$PushLocalImages,
    [string]$ExpectedHost = "",
    [string]$PublicBaseUrl = "",
    [string]$ExternalCallbackUrl = "",
    [string]$TenantId = "tenant_demo",
    [string]$IngressToken = "dev-ingress-token",
    [string]$StatusToken = "dev-status-token",
    [string]$IngressPrincipalId = "ingress-service",
    [string]$StatusPrincipalId = "demo-operator",
    [string]$StatusPrincipalRole = "admin",
    [string]$OperatorUiBaseUrl = "",
    [string]$Email = "",
    [string]$Password = "",
    [string]$FlutterwaveTransactionId = "",
    [switch]$AttemptBillingWebhook,
    [string]$FlutterwaveWebhookHash = "",
    [string]$SignupEmail = "",
    [string]$SignupPassword = "",
    [switch]$ExercisePasswordReset,
    [switch]$AllowBillingOff,
    [switch]$AllowRecoveryOff,
    [switch]$SkipHttpsChecks,
    [switch]$SkipExternalCallback,
    [switch]$SkipBilling,
    [switch]$SkipUiSmoke
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-LastExit([string]$Context) {
    if ($LASTEXITCODE -ne 0) {
        throw "$Context (exit code $LASTEXITCODE)"
    }
}

function Invoke-EndpointCheck {
    param(
        [Parameter(Mandatory = $true)][string]$Url,
        [int[]]$ExpectedStatus = @(200)
    )

    $response = Invoke-WebRequest `
        -Uri $Url `
        -Method "GET" `
        -SkipHttpErrorCheck `
        -MaximumRedirection 0 `
        -ErrorAction "Stop"

    [pscustomobject]@{
        Url        = $Url
        StatusCode = [int]$response.StatusCode
        Ok         = (@($ExpectedStatus) -contains [int]$response.StatusCode)
    }
}

if ([string]::IsNullOrWhiteSpace($PublicBaseUrl) -and -not [string]::IsNullOrWhiteSpace($ExpectedHost)) {
    $PublicBaseUrl = "https://$ExpectedHost"
}

if ($DeployKnownGoodImages) {
    if ([string]::IsNullOrWhiteSpace($Tag)) {
        throw "-Tag is required with -DeployKnownGoodImages."
    }
    Write-Host "Deploying known-good image set..."
    $deployArgs = @(
        "-File", (Join-Path $PSScriptRoot "redeploy_latest_images.ps1"),
        "-Namespace", $Namespace,
        "-ImageNamespace", $ImageNamespace,
        "-Tag", $Tag
    )
    if ($BuildLocalImages) { $deployArgs += "-BuildLocalImages" }
    if ($PushLocalImages) { $deployArgs += "-PushLocalImages" }
    & pwsh @deployArgs
    Assert-LastExit "known-good image deployment failed"
}

Write-Host "1) Runtime posture..."
$readinessArgs = @(
    "-File", (Join-Path $PSScriptRoot "check_production_readiness.ps1"),
    "-Namespace", $Namespace,
    "-ExpectedHost", $ExpectedHost
)
if ($AllowBillingOff) { $readinessArgs += "-AllowBillingOff" }
if ($AllowRecoveryOff) { $readinessArgs += "-AllowRecoveryOff" }
& pwsh @readinessArgs
Assert-LastExit "production readiness check failed"

$httpsChecks = @()
if (-not $SkipHttpsChecks) {
    if ([string]::IsNullOrWhiteSpace($PublicBaseUrl)) {
        throw "Public HTTPS checks require -PublicBaseUrl or -ExpectedHost."
    }
    $public = $PublicBaseUrl.TrimEnd("/")
    Write-Host "2) Public HTTPS..."
    $httpsChecks = @(
        Invoke-EndpointCheck -Url "$public/healthz" -ExpectedStatus @(200)
        Invoke-EndpointCheck -Url "$public/readyz" -ExpectedStatus @(200)
        Invoke-EndpointCheck -Url "$public/status/health" -ExpectedStatus @(200)
        Invoke-EndpointCheck -Url "$public/api/ui/health" -ExpectedStatus @(200)
    )
    $failedHttps = @($httpsChecks | Where-Object { -not $_.Ok })
    if ($failedHttps.Count -gt 0) {
        $failedHttps | Format-Table -AutoSize | Out-String | Write-Host
        throw "public HTTPS verification failed"
    }
}

$callbackRan = $false
if (-not $SkipExternalCallback) {
    if ([string]::IsNullOrWhiteSpace($PublicBaseUrl)) {
        throw "External callback verification requires -PublicBaseUrl."
    }
    if ([string]::IsNullOrWhiteSpace($ExternalCallbackUrl)) {
        throw "External callback verification requires -ExternalCallbackUrl."
    }
    Write-Host "3) External callback receiver..."
    & pwsh -File (Join-Path $PSScriptRoot "run_full_flow.ps1") `
        -BaseUrl $PublicBaseUrl `
        -TenantId $TenantId `
        -IngressToken $IngressToken `
        -StatusToken $StatusToken `
        -IngressPrincipalId $IngressPrincipalId `
        -StatusPrincipalId $StatusPrincipalId `
        -StatusPrincipalRole $StatusPrincipalRole `
        -CallbackDeliveryUrl $ExternalCallbackUrl
    Assert-LastExit "external callback verification failed"
    $callbackRan = $true
}

$billingRan = $false
if (-not $SkipBilling) {
    $billingBase = if (-not [string]::IsNullOrWhiteSpace($OperatorUiBaseUrl)) { $OperatorUiBaseUrl } else { $PublicBaseUrl }
    if ([string]::IsNullOrWhiteSpace($billingBase)) {
        throw "Billing verification requires -OperatorUiBaseUrl or -PublicBaseUrl."
    }
    if ([string]::IsNullOrWhiteSpace($Email) -or [string]::IsNullOrWhiteSpace($Password)) {
        throw "Billing verification requires -Email and -Password."
    }
    Write-Host "4) Billing provider..."
    $billingArgs = @(
        "-File", (Join-Path $PSScriptRoot "verify_billing_endpoints.ps1"),
        "-OperatorUiBaseUrl", $billingBase,
        "-Email", $Email,
        "-Password", $Password
    )
    if (-not [string]::IsNullOrWhiteSpace($FlutterwaveTransactionId)) {
        $billingArgs += @("-FlutterwaveTransactionId", $FlutterwaveTransactionId)
    }
    if ($AttemptBillingWebhook) {
        $billingArgs += "-AttemptWebhook"
        $billingArgs += @("-FlutterwaveWebhookHash", $FlutterwaveWebhookHash)
    }
    & pwsh @billingArgs
    Assert-LastExit "billing verification failed"
    $billingRan = $true
}

$uiSmokeRan = $false
if (-not $SkipUiSmoke) {
    if ([string]::IsNullOrWhiteSpace($PublicBaseUrl)) {
        throw "UI smoke verification requires -PublicBaseUrl."
    }
    Write-Host "5) UI smoke flow..."
    $uiArgs = @(
        "-File", (Join-Path $PSScriptRoot "verify_ui_smoke.ps1"),
        "-BaseUrl", $PublicBaseUrl
    )
    if (-not [string]::IsNullOrWhiteSpace($Email)) {
        $uiArgs += @("-Email", $Email)
    }
    if (-not [string]::IsNullOrWhiteSpace($Password)) {
        $uiArgs += @("-Password", $Password)
    }
    if (-not [string]::IsNullOrWhiteSpace($SignupEmail)) {
        $uiArgs += @("-SignupEmail", $SignupEmail)
    }
    if (-not [string]::IsNullOrWhiteSpace($SignupPassword)) {
        $uiArgs += @("-SignupPassword", $SignupPassword)
    }
    if ($ExercisePasswordReset) {
        $uiArgs += "-ExercisePasswordReset"
    }
    & pwsh @uiArgs
    Assert-LastExit "ui smoke verification failed"
    $uiSmokeRan = $true
}

Write-Host ""
Write-Host "=== Public Edge Verification Summary ==="
[pscustomobject]@{
    Namespace                    = $Namespace
    ImageNamespace               = $ImageNamespace
    Tag                          = $Tag
    DeployedKnownGoodImages      = [bool]$DeployKnownGoodImages
    PublicBaseUrl                = $PublicBaseUrl
    RuntimePostureChecked        = $true
    HttpsChecked                 = (-not $SkipHttpsChecks)
    ExternalCallbackChecked      = $callbackRan
    BillingChecked               = $billingRan
    UiSmokeChecked               = $uiSmokeRan
    ManualEmailInboxProofRequired = $true
} | Format-List

Write-Host "Manual follow-up still required:"
Write-Host " - confirm verification email arrives and verify-email link works"
Write-Host " - confirm password reset email arrives and reset-password link works"
Write-Host " - confirm interactive browser flows match backend truth"
