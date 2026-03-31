param(
    [string]$Namespace = "azums",
    [string]$ConfigMapName = "azums-platform-config",
    [string]$SecretName = "azums-platform-secrets",
    [string]$IngressName = "azums-public",
    [string]$TlsSecretName = "azums-public-tls",
    [string]$PublicHost = "",
    [string]$PublicBaseUrl = "",
    [string]$TlsCertPath = "",
    [string]$TlsKeyPath = "",
    [string]$FlutterwaveSecretKey = "",
    [string]$FlutterwaveWebhookHash = "",
    [string]$FlutterwaveExpectedCurrency = "NGN",
    [string]$FlutterwaveFxRatesUsd = "USD=1;NGN=0.00066;GBP=1.27;CAD=0.74;JPY=0.0067",
    [string]$SmtpHost = "",
    [int]$SmtpPort = 587,
    [string]$SmtpUsername = "",
    [string]$SmtpPassword = "",
    [string]$EmailFrom = "",
    [string]$ProductionSolanaRpcUrl = "",
    [string]$ProductionSolanaRpcPrimaryUrl = "",
    [string[]]$ProductionSolanaRpcFallbackUrls = @(),
    [string]$StagingSolanaRpcUrl = "",
    [string]$SandboxSolanaRpcUrl = "https://api.devnet.solana.com",
    [string]$ExecutionPolicy = "customer_signed",
    [int]$SponsoredMonthlyCapRequests = 10000,
    [string[]]$CallbackAllowedHosts = @(),
    [bool]$CallbackAllowPrivateDestinations = $false,
    [string]$CallbackDeliveryToken = "",
    [string]$CallbackSigningSecret = "",
    [string]$MetricsBearerToken = "",
    [switch]$DisableRecovery,
    [switch]$Apply
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Require-Command([string]$Name) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found: $Name"
    }
}

function Mask([string]$Value) {
    if ([string]::IsNullOrWhiteSpace($Value)) { return "<empty>" }
    if ($Value.Length -le 8) { return ("*" * $Value.Length) }
    return "{0}...{1}" -f $Value.Substring(0, 4), $Value.Substring($Value.Length - 4)
}

function To-BoolString([bool]$Value) {
    if ($Value) { return "true" }
    return "false"
}

function Normalize-RpcList([string[]]$Values) {
    $out = New-Object System.Collections.Generic.List[string]
    foreach ($value in @($Values)) {
        if ([string]::IsNullOrWhiteSpace($value)) {
            continue
        }
        foreach ($part in ($value -split ",|;|`r|`n")) {
            $trimmed = $part.Trim()
            if ([string]::IsNullOrWhiteSpace($trimmed)) {
                continue
            }
            if (-not $out.Contains($trimmed)) {
                $out.Add($trimmed)
            }
        }
    }
    return @($out.ToArray())
}

function Assert-LastExit([string]$Context) {
    if ($LASTEXITCODE -ne 0) {
        throw "$Context (exit code $LASTEXITCODE)"
    }
}

Require-Command "kubectl"

$hasTls = (-not [string]::IsNullOrWhiteSpace($TlsCertPath)) -or (-not [string]::IsNullOrWhiteSpace($TlsKeyPath))
if ($hasTls) {
    if ([string]::IsNullOrWhiteSpace($TlsCertPath) -or [string]::IsNullOrWhiteSpace($TlsKeyPath)) {
        throw "TLS requires both -TlsCertPath and -TlsKeyPath."
    }
    if (-not (Test-Path $TlsCertPath)) {
        throw "TLS cert file not found: $TlsCertPath"
    }
    if (-not (Test-Path $TlsKeyPath)) {
        throw "TLS key file not found: $TlsKeyPath"
    }
}

if ([string]::IsNullOrWhiteSpace($PublicBaseUrl) -and -not [string]::IsNullOrWhiteSpace($PublicHost)) {
    $PublicBaseUrl = "https://$PublicHost"
}

$hasFlutterwave = (-not [string]::IsNullOrWhiteSpace($FlutterwaveSecretKey)) -or (-not [string]::IsNullOrWhiteSpace($FlutterwaveWebhookHash))
if ($hasFlutterwave) {
    if ([string]::IsNullOrWhiteSpace($FlutterwaveSecretKey) -or [string]::IsNullOrWhiteSpace($FlutterwaveWebhookHash)) {
        throw "Flutterwave requires both -FlutterwaveSecretKey and -FlutterwaveWebhookHash."
    }
}

$smtpInputs = @(@($SmtpHost, $SmtpUsername, $SmtpPassword, $EmailFrom) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
$hasSmtp = $smtpInputs.Count -gt 0
if ($hasSmtp) {
    if ([string]::IsNullOrWhiteSpace($SmtpHost) -or
        [string]::IsNullOrWhiteSpace($SmtpUsername) -or
        [string]::IsNullOrWhiteSpace($SmtpPassword) -or
        [string]::IsNullOrWhiteSpace($EmailFrom)) {
        throw "SMTP requires -SmtpHost, -SmtpUsername, -SmtpPassword, and -EmailFrom."
    }
    if ([string]::IsNullOrWhiteSpace($PublicBaseUrl)) {
        throw "SMTP requires -PublicBaseUrl so email links resolve correctly."
    }
}

if ([string]::IsNullOrWhiteSpace($PublicHost) -and $Apply) {
    throw "-PublicHost is required when -Apply is used."
}

$productionRpcCandidates = @()
if (-not [string]::IsNullOrWhiteSpace($ProductionSolanaRpcPrimaryUrl)) {
    $productionRpcCandidates += $ProductionSolanaRpcPrimaryUrl
}
$productionRpcCandidates += @($ProductionSolanaRpcFallbackUrls)
if (-not [string]::IsNullOrWhiteSpace($ProductionSolanaRpcUrl)) {
    $productionRpcCandidates += $ProductionSolanaRpcUrl
}
$productionRpcList = @(Normalize-RpcList $productionRpcCandidates)
if ($productionRpcList.Count -eq 0 -and $Apply) {
    throw "Production Solana RPC is required when -Apply is used. Provide -ProductionSolanaRpcPrimaryUrl plus optional -ProductionSolanaRpcFallbackUrls, or use -ProductionSolanaRpcUrl."
}
if ($productionRpcList.Count -gt 0 -and [string]::IsNullOrWhiteSpace($ProductionSolanaRpcPrimaryUrl)) {
    $ProductionSolanaRpcPrimaryUrl = $productionRpcList[0]
}
if ($productionRpcList.Count -eq 0 -and -not [string]::IsNullOrWhiteSpace($ProductionSolanaRpcUrl)) {
    $productionRpcList = @($ProductionSolanaRpcUrl)
}
$productionRpcPrimary = if ($productionRpcList.Count -gt 0) { $productionRpcList[0] } else { "" }
$productionRpcFallbacks = if ($productionRpcList.Count -gt 1) { @($productionRpcList[1..($productionRpcList.Count - 1)]) } else { @() }
$productionRpcListCsv = $productionRpcList -join ","
$productionRpcFallbacksCsv = $productionRpcFallbacks -join ","

$stagingRpcList = @(Normalize-RpcList @($StagingSolanaRpcUrl))
$stagingRpcListCsv = $stagingRpcList -join ","
$sandboxRpcList = @(Normalize-RpcList @($SandboxSolanaRpcUrl))
$sandboxRpcListCsv = if ($sandboxRpcList.Count -gt 0) { $sandboxRpcList -join "," } else { "https://api.devnet.solana.com" }

if ($productionRpcList.Count -gt 0 -and $productionRpcPrimary -match "devnet") {
    throw "Production Solana RPC primary must not point to devnet."
}
if ($Apply -and $CallbackAllowedHosts.Count -eq 0) {
    throw "-CallbackAllowedHosts is required when -Apply is used."
}
if ((-not [string]::IsNullOrWhiteSpace($PublicBaseUrl)) -and (-not $PublicBaseUrl.StartsWith("https://"))) {
    throw "-PublicBaseUrl must start with https:// for production."
}

$enableRecovery = $hasSmtp -and (-not $DisableRecovery)

$configPatch = @{
    data = @{
        OBS_ENV = "prod"
        INGRESS_DEFAULT_EXECUTION_POLICY = $ExecutionPolicy
        INGRESS_DEFAULT_SPONSORED_MONTHLY_CAP_REQUESTS = [string]$SponsoredMonthlyCapRequests
        INGRESS_EXECUTION_POLICY_ENFORCEMENT_ENABLED = "true"
        EXECUTION_CALLBACK_ALLOW_PRIVATE_DESTINATIONS = (To-BoolString $CallbackAllowPrivateDestinations)
        EXECUTION_CALLBACK_ALLOWED_HOSTS = ($CallbackAllowedHosts -join ",")
        SOLANA_PLATFORM_SIGNING_ENABLED = "false"
        SOLANA_RPC_URL = $productionRpcPrimary
        SOLANA_RPC_PRIMARY_URL = $productionRpcPrimary
        SOLANA_RPC_URLS = $productionRpcListCsv
        SOLANA_RPC_FALLBACK_URLS = $productionRpcFallbacksCsv
        OPERATOR_UI_REQUIRE_DURABLE_METERING = "true"
        OPERATOR_UI_ENFORCE_WORKSPACE_SOLANA_RPC = "true"
        OPERATOR_UI_SANDBOX_SOLANA_RPC_URL = $sandboxRpcListCsv
        OPERATOR_UI_STAGING_SOLANA_RPC_URL = $stagingRpcListCsv
        OPERATOR_UI_PRODUCTION_SOLANA_RPC_URL = $productionRpcListCsv
        OPERATOR_UI_PUBLIC_BASE_URL = $PublicBaseUrl
        OPERATOR_UI_REQUIRE_EMAIL_VERIFICATION = (To-BoolString $enableRecovery)
        OPERATOR_UI_PASSWORD_RESET_ENABLED = (To-BoolString $enableRecovery)
        OPERATOR_UI_EMAIL_FROM = $EmailFrom
        OPERATOR_UI_SMTP_HOST = $SmtpHost
        OPERATOR_UI_SMTP_PORT = [string]$SmtpPort
        OPERATOR_UI_FLUTTERWAVE_EXPECTED_CURRENCY = $FlutterwaveExpectedCurrency
        OPERATOR_UI_FLUTTERWAVE_FX_RATES_USD = $FlutterwaveFxRatesUsd
        REVERSE_PROXY_PUBLIC_PATH_ACL_ENABLED = "true"
        REVERSE_PROXY_RATE_LIMIT_ENABLED = "true"
        REVERSE_PROXY_SECURITY_AUDIT_ENABLED = "true"
        REVERSE_PROXY_ENFORCE_SECURITY_BASELINE = "true"
    }
}

$secretPatch = @{
    stringData = @{}
}

if (-not [string]::IsNullOrWhiteSpace($FlutterwaveSecretKey)) {
    $secretPatch.stringData.OPERATOR_UI_FLUTTERWAVE_SECRET_KEY = $FlutterwaveSecretKey
    $secretPatch.stringData.OPERATOR_UI_FLUTTERWAVE_WEBHOOK_HASH = $FlutterwaveWebhookHash
}
if ($hasSmtp) {
    $secretPatch.stringData.OPERATOR_UI_SMTP_USERNAME = $SmtpUsername
    $secretPatch.stringData.OPERATOR_UI_SMTP_PASSWORD = $SmtpPassword
}
if (-not [string]::IsNullOrWhiteSpace($CallbackDeliveryToken)) {
    $secretPatch.stringData.EXECUTION_CALLBACK_DELIVERY_TOKEN = $CallbackDeliveryToken
}
if (-not [string]::IsNullOrWhiteSpace($CallbackSigningSecret)) {
    $secretPatch.stringData.EXECUTION_CALLBACK_SIGNING_SECRET = $CallbackSigningSecret
}
if (-not [string]::IsNullOrWhiteSpace($MetricsBearerToken)) {
    $secretPatch.stringData.REVERSE_PROXY_METRICS_BEARER_TOKEN = $MetricsBearerToken
}

$configPatchJson = $configPatch | ConvertTo-Json -Compress
$secretPatchJson = $secretPatch | ConvertTo-Json -Compress

Write-Host "Namespace                         : $Namespace"
Write-Host "ConfigMap                         : $ConfigMapName"
Write-Host "Secret                            : $SecretName"
Write-Host "Ingress                           : $IngressName"
Write-Host "TLS secret                        : $TlsSecretName"
Write-Host "Public host                       : $PublicHost"
Write-Host "Public base URL                   : $PublicBaseUrl"
Write-Host "TLS files supplied                : $hasTls"
Write-Host "Flutterwave supplied              : $hasFlutterwave"
Write-Host "SMTP supplied                     : $hasSmtp"
Write-Host "Recovery enabled                  : $enableRecovery"
Write-Host "Execution policy                  : $ExecutionPolicy"
Write-Host "Production Solana RPC primary     : $productionRpcPrimary"
Write-Host "Production Solana RPC fallbacks   : $($productionRpcFallbacks -join ',')"
Write-Host "Production Solana RPC ordered     : $productionRpcListCsv"
Write-Host "Callback allow private            : $CallbackAllowPrivateDestinations"
Write-Host "Callback allowed hosts            : $($CallbackAllowedHosts -join ',')"
Write-Host "Metrics bearer token supplied     : $(-not [string]::IsNullOrWhiteSpace($MetricsBearerToken))"
Write-Host "Flutterwave key                   : $(Mask $FlutterwaveSecretKey)"
Write-Host "SMTP username                     : $(Mask $SmtpUsername)"

if (-not $Apply) {
    Write-Warning "Dry run only. Re-run with -Apply to patch the cluster."
    return
}

if ($hasTls) {
    Write-Host "Applying TLS secret..."
    $tlsYaml = & kubectl -n $Namespace create secret tls $TlsSecretName --cert=$TlsCertPath --key=$TlsKeyPath --dry-run=client -o yaml
    Assert-LastExit "kubectl create secret tls dry-run failed"
    $tlsYaml | & kubectl apply -f -
    Assert-LastExit "kubectl apply TLS secret failed"
}

Write-Host "Patching config map..."
& kubectl -n $Namespace patch configmap $ConfigMapName --type merge -p $configPatchJson
Assert-LastExit "config map patch failed"

if ($secretPatch.stringData.Count -gt 0) {
    Write-Host "Patching secret..."
    & kubectl -n $Namespace patch secret $SecretName --type merge -p $secretPatchJson
    Assert-LastExit "secret patch failed"
}

if (-not [string]::IsNullOrWhiteSpace($PublicHost)) {
    Write-Host "Patching public ingress host..."
    $ingressPatch = @(
        @{
            op = "replace"
            path = "/spec/rules/0/host"
            value = $PublicHost
        },
        @{
            op = "replace"
            path = "/spec/tls/0/hosts/0"
            value = $PublicHost
        },
        @{
            op = "replace"
            path = "/spec/tls/0/secretName"
            value = $TlsSecretName
        }
    ) | ConvertTo-Json -Compress

    & kubectl -n $Namespace patch ingress $IngressName --type json -p $ingressPatch
    Assert-LastExit "public ingress patch failed"
}

Write-Host "Syncing operator-ui-backend runtime env overrides..."
& kubectl -n $Namespace set env deploy/operator-ui-backend `
    OPERATOR_UI_ENFORCE_WORKSPACE_SOLANA_RPC=true `
    OPERATOR_UI_SANDBOX_SOLANA_RPC_URL=$sandboxRpcListCsv `
    OPERATOR_UI_REQUIRE_EMAIL_VERIFICATION=$(To-BoolString $enableRecovery) `
    OPERATOR_UI_FLUTTERWAVE_FX_RATES_USD=$FlutterwaveFxRatesUsd `
    OPERATOR_UI_PASSWORD_RESET_ENABLED=$(To-BoolString $enableRecovery)
Assert-LastExit "operator-ui-backend env sync failed"

Write-Host "Restarting workloads..."
& kubectl -n $Namespace rollout restart deploy/reverse-proxy deploy/ingress-api deploy/status-api deploy/execution-worker deploy/execution-callback-worker deploy/operator-ui-backend deploy/operator-ui
Assert-LastExit "rollout restart failed"

foreach ($deployment in @("reverse-proxy", "ingress-api", "status-api", "execution-worker", "execution-callback-worker", "operator-ui-backend", "operator-ui")) {
    Write-Host "Waiting for rollout: $deployment"
    & kubectl -n $Namespace rollout status "deploy/$deployment" --timeout=300s
    Assert-LastExit "rollout failed for $deployment"
}

Write-Host "Running health check..."
& pwsh -File (Join-Path $PSScriptRoot "check_platform_health.ps1") -Namespace $Namespace
Assert-LastExit "post-apply health check failed"

Write-Host "Production runtime apply complete."
Write-Host "Next verification:"
Write-Host " - Run scripts/check_production_readiness.ps1 -Namespace $Namespace -ExpectedHost $PublicHost"
if ($hasFlutterwave) {
    Write-Host " - Run scripts/verify_billing_endpoints.ps1 with a real Flutterwave transaction id."
}
