param(
    [string]$Namespace = "azums",
    [string]$ConfigMapName = "azums-platform-config",
    [string]$SecretName = "azums-platform-secrets",
    [string]$IngressName = "azums-public",
    [string]$TlsSecretName = "azums-public-tls",
    [string]$ExpectedHost = "",
    [switch]$AllowBillingOff,
    [switch]$AllowRecoveryOff
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-Json([string[]]$Command) {
    $commandName = $Command[0]
    $commandArgs = @()
    if ($Command.Length -gt 1) {
        $commandArgs = $Command[1..($Command.Length - 1)]
    }
    $json = & $commandName @commandArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed: $($Command -join ' ')"
    }
    return $json | ConvertFrom-Json
}

function Get-ConfigValue($Map, [string]$Key) {
    if ($Map -is [System.Collections.IDictionary]) {
        if ($Map.Contains($Key)) {
            return [string]$Map[$Key]
        }
        return ""
    }
    $prop = $Map.PSObject.Properties[$Key]
    if ($null -eq $prop) { return "" }
    return [string]$prop.Value
}

function Get-SecretValue($Map, [string]$Key) {
    if ($Map -is [System.Collections.IDictionary]) {
        if (-not $Map.Contains($Key)) { return "" }
        $encoded = [string]$Map[$Key]
        if ([string]::IsNullOrWhiteSpace($encoded)) { return "" }
        return [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($encoded))
    }
    $prop = $Map.PSObject.Properties[$Key]
    if ($null -eq $prop) { return "" }
    $encoded = [string]$prop.Value
    if ([string]::IsNullOrWhiteSpace($encoded)) { return "" }
    return [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($encoded))
}

function Is-NonEmpty([string]$Value) {
    return -not [string]::IsNullOrWhiteSpace($Value)
}

function Normalize-String([string]$Value) {
    if ([string]::IsNullOrWhiteSpace($Value)) {
        return ""
    }
    return $Value.Trim()
}

function Is-TrueLike([string]$Value) {
    return (Normalize-String $Value).ToLowerInvariant() -eq "true"
}

function Parse-RpcList([string[]]$Values) {
    $out = New-Object System.Collections.Generic.List[string]
    foreach ($value in @($Values)) {
        if (-not (Is-NonEmpty $value)) {
            continue
        }
        foreach ($part in ($value -split ",|;|`r|`n")) {
            $trimmed = $part.Trim()
            if (-not (Is-NonEmpty $trimmed)) {
                continue
            }
            if (-not $out.Contains($trimmed)) {
                $out.Add($trimmed)
            }
        }
    }
    return @($out.ToArray())
}

function RpcListContainsDevnet([string[]]$RpcUrls) {
    foreach ($rpcUrl in @($RpcUrls)) {
        if ([string]$rpcUrl -match "devnet") {
            return $true
        }
    }
    return $false
}

function Get-DeploymentByName($Deployments, [string]$Name) {
    foreach ($deployment in $Deployments.items) {
        if ([string]$deployment.metadata.name -eq $Name) {
            return $deployment
        }
    }
    return $null
}

function Get-DeploymentEnvValue($Deployment, [string]$EnvName, $ConfigMapData, $SecretData) {
    if ($null -ne $Deployment) {
        $container = $Deployment.spec.template.spec.containers | Select-Object -First 1
        if ($null -ne $container) {
            $envProp = $container.PSObject.Properties["env"]
            if ($null -ne $envProp) {
                foreach ($env in @($envProp.Value)) {
                    if ([string]$env.name -ne $EnvName) {
                        continue
                    }
                    if ($null -ne $env.value) {
                        return [string]$env.value
                    }
                    if ($null -ne $env.valueFrom) {
                        if ($null -ne $env.valueFrom.configMapKeyRef) {
                            return Get-ConfigValue $ConfigMapData ([string]$env.valueFrom.configMapKeyRef.key)
                        }
                        if ($null -ne $env.valueFrom.secretKeyRef) {
                            return Get-SecretValue $SecretData ([string]$env.valueFrom.secretKeyRef.key)
                        }
                    }
                }
            }
        }
    }
    return Get-ConfigValue $ConfigMapData $EnvName
}

$failures = New-Object System.Collections.Generic.List[string]
$warnings = New-Object System.Collections.Generic.List[string]

$pods = Get-Json @("kubectl", "-n", $Namespace, "get", "pods", "-o", "json")
$deployments = Get-Json @("kubectl", "-n", $Namespace, "get", "deploy", "-o", "json")
$configMap = Get-Json @("kubectl", "-n", $Namespace, "get", "configmap", $ConfigMapName, "-o", "json")
$secret = Get-Json @("kubectl", "-n", $Namespace, "get", "secret", $SecretName, "-o", "json")
$ingress = Get-Json @("kubectl", "-n", $Namespace, "get", "ingress", $IngressName, "-o", "json")

$tlsSecretExists = $true
try {
    $null = & kubectl -n $Namespace get secret $TlsSecretName -o name 2>$null
    if ($LASTEXITCODE -ne 0) {
        $tlsSecretExists = $false
    }
}
catch {
    $tlsSecretExists = $false
}

if (-not $tlsSecretExists) {
    $failures.Add("TLS secret '$TlsSecretName' is missing.")
}

foreach ($pod in $pods.items) {
    if ($pod.status.phase -ne "Running") {
        $failures.Add("Pod $($pod.metadata.name) phase=$($pod.status.phase)")
    }
    foreach ($cs in $pod.status.containerStatuses) {
        if (-not $cs.ready) {
            $failures.Add("Container $($pod.metadata.name)/$($cs.name) is not Ready.")
        }
        $waiting = $null
        if ($null -ne $cs.state) {
            $waitingProp = $cs.state.PSObject.Properties["waiting"]
            if ($null -ne $waitingProp) {
                $waiting = $waitingProp.Value
            }
        }
        if ($null -ne $waiting -and ($waiting.reason -eq "ErrImagePull" -or $waiting.reason -eq "ImagePullBackOff")) {
            $failures.Add("Container $($pod.metadata.name)/$($cs.name) image pull failure: $($waiting.reason)")
        }
    }
}

$localImages = @()
foreach ($deployment in $deployments.items) {
    foreach ($container in $deployment.spec.template.spec.containers) {
        if ([string]$container.image -match ":local$") {
            $localImages += "$($deployment.metadata.name)=$($container.image)"
        }
    }
}
if ($localImages.Count -gt 0) {
    $failures.Add("Local images are still deployed: $($localImages -join '; ')")
}

$config = $configMap.data
$secrets = $secret.data
$ingressDeployment = Get-DeploymentByName $deployments "ingress-api"
$executionWorkerDeployment = Get-DeploymentByName $deployments "execution-worker"
$operatorUiBackendDeployment = Get-DeploymentByName $deployments "operator-ui-backend"
$reverseProxyDeployment = Get-DeploymentByName $deployments "reverse-proxy"

$publicBaseUrl = Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_PUBLIC_BASE_URL" $config $secrets
$publicHost = ""
if ($ingress.spec.rules.Count -gt 0) {
    $publicHost = [string]$ingress.spec.rules[0].host
}
$publicHost = Normalize-String $publicHost
$expectedHostNormalized = Normalize-String $ExpectedHost
$publicBaseUrl = Normalize-String $publicBaseUrl
if (-not (Is-NonEmpty $publicHost)) {
    $failures.Add("Public ingress host is empty.")
} elseif ($publicHost -eq "azums.local" -or $publicHost -like "*.example.com" -or $publicHost -eq "example.com") {
    $failures.Add("Public ingress host is still a placeholder value: '$publicHost'.")
}
if ((Is-NonEmpty $expectedHostNormalized) -and (-not [string]::Equals($publicHost, $expectedHostNormalized, [System.StringComparison]::OrdinalIgnoreCase))) {
    $failures.Add("Public ingress host mismatch: expected '$expectedHostNormalized' got '$publicHost'.")
}
if (-not $publicBaseUrl.StartsWith("https://")) {
    $failures.Add("OPERATOR_UI_PUBLIC_BASE_URL must be https://... in production.")
}
if ($publicBaseUrl -like "*example.com*") {
    $failures.Add("OPERATOR_UI_PUBLIC_BASE_URL is still using a placeholder domain.")
}

$executionPolicy = Get-DeploymentEnvValue $ingressDeployment "INGRESS_DEFAULT_EXECUTION_POLICY" $config $secrets
if ($executionPolicy -ne "customer_signed") {
    $failures.Add("INGRESS_DEFAULT_EXECUTION_POLICY should be 'customer_signed' for production. Current: '$executionPolicy'.")
}
if ((Get-DeploymentEnvValue $ingressDeployment "INGRESS_EXECUTION_POLICY_ENFORCEMENT_ENABLED" $config $secrets) -ne "true") {
    $failures.Add("INGRESS_EXECUTION_POLICY_ENFORCEMENT_ENABLED must be true.")
}
if ((Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_REQUIRE_DURABLE_METERING" $config $secrets) -ne "true") {
    $failures.Add("OPERATOR_UI_REQUIRE_DURABLE_METERING must be true.")
}
if ((Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_ENFORCE_WORKSPACE_SOLANA_RPC" $config $secrets) -ne "true") {
    $failures.Add("OPERATOR_UI_ENFORCE_WORKSPACE_SOLANA_RPC must be true.")
}
if ((Get-DeploymentEnvValue $executionWorkerDeployment "SOLANA_PLATFORM_SIGNING_ENABLED" $config $secrets) -ne "false") {
    $failures.Add("SOLANA_PLATFORM_SIGNING_ENABLED must be false in production customer-signed mode.")
}
$workerSolanaRpcPrimary = Get-DeploymentEnvValue $executionWorkerDeployment "SOLANA_RPC_PRIMARY_URL" $config $secrets
$workerSolanaRpcList = @(Parse-RpcList @(
    $workerSolanaRpcPrimary,
    (Get-DeploymentEnvValue $executionWorkerDeployment "SOLANA_RPC_URLS" $config $secrets),
    (Get-DeploymentEnvValue $executionWorkerDeployment "SOLANA_RPC_FALLBACK_URLS" $config $secrets),
    (Get-DeploymentEnvValue $executionWorkerDeployment "SOLANA_RPC_URL" $config $secrets)
))
if ($workerSolanaRpcList.Count -eq 0) {
    $failures.Add("Execution worker Solana RPC is not configured.")
} elseif (RpcListContainsDevnet $workerSolanaRpcList) {
    $failures.Add("Execution worker Solana RPC list still contains a devnet endpoint.")
} elseif ($workerSolanaRpcList.Count -lt 2) {
    $warnings.Add("Execution worker Solana RPC failover is not configured; only one endpoint is present.")
}

$productionWorkspaceRpcList = @(Parse-RpcList @(
    Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_PRODUCTION_SOLANA_RPC_URL" $config $secrets
))
if ($productionWorkspaceRpcList.Count -eq 0) {
    $failures.Add("OPERATOR_UI_PRODUCTION_SOLANA_RPC_URL is not configured.")
} elseif (RpcListContainsDevnet $productionWorkspaceRpcList) {
    $failures.Add("OPERATOR_UI_PRODUCTION_SOLANA_RPC_URL still contains a devnet endpoint.")
} elseif ($productionWorkspaceRpcList.Count -lt 2) {
    $warnings.Add("Workspace production RPC is configured as a single endpoint; hybrid failover is not configured.")
}

if ((Get-DeploymentEnvValue $reverseProxyDeployment "REVERSE_PROXY_PUBLIC_PATH_ACL_ENABLED" $config $secrets) -ne "true") {
    $failures.Add("REVERSE_PROXY_PUBLIC_PATH_ACL_ENABLED must be true.")
}
if ((Get-DeploymentEnvValue $reverseProxyDeployment "REVERSE_PROXY_RATE_LIMIT_ENABLED" $config $secrets) -ne "true") {
    $failures.Add("REVERSE_PROXY_RATE_LIMIT_ENABLED must be true.")
}
if ((Get-DeploymentEnvValue $reverseProxyDeployment "REVERSE_PROXY_SECURITY_AUDIT_ENABLED" $config $secrets) -ne "true") {
    $failures.Add("REVERSE_PROXY_SECURITY_AUDIT_ENABLED must be true.")
}
if ((Get-DeploymentEnvValue $reverseProxyDeployment "REVERSE_PROXY_ENFORCE_SECURITY_BASELINE" $config $secrets) -ne "true") {
    $failures.Add("REVERSE_PROXY_ENFORCE_SECURITY_BASELINE must be true.")
}

$callbackAllowPrivate = Get-DeploymentEnvValue $executionWorkerDeployment "EXECUTION_CALLBACK_ALLOW_PRIVATE_DESTINATIONS" $config $secrets
$callbackAllowedHosts = Get-DeploymentEnvValue $executionWorkerDeployment "EXECUTION_CALLBACK_ALLOWED_HOSTS" $config $secrets
if ($callbackAllowPrivate -ne "false") {
    $failures.Add("EXECUTION_CALLBACK_ALLOW_PRIVATE_DESTINATIONS must be false for production.")
}
if (-not (Is-NonEmpty $callbackAllowedHosts) -or $callbackAllowedHosts -eq "callbacks.example.com") {
    $failures.Add("EXECUTION_CALLBACK_ALLOWED_HOSTS is not set to a real approved host list.")
}

$smtpHost = Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_SMTP_HOST" $config $secrets
$smtpPort = Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_SMTP_PORT" $config $secrets
$emailFrom = Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_EMAIL_FROM" $config $secrets
$smtpUsername = Get-SecretValue $secrets "OPERATOR_UI_SMTP_USERNAME"
$smtpPassword = Get-SecretValue $secrets "OPERATOR_UI_SMTP_PASSWORD"
$emailVerificationEnabledRaw = Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_REQUIRE_EMAIL_VERIFICATION" $config $secrets
$passwordResetEnabledRaw = Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_PASSWORD_RESET_ENABLED" $config $secrets
$emailVerificationEnabled = Is-TrueLike $emailVerificationEnabledRaw
$passwordResetEnabled = Is-TrueLike $passwordResetEnabledRaw
$smtpConfigured = (Is-NonEmpty $smtpHost) -and (Is-NonEmpty $smtpPort) -and (Is-NonEmpty $emailFrom) -and (Is-NonEmpty $smtpUsername) -and (Is-NonEmpty $smtpPassword)
$recoveryEnabled = $emailVerificationEnabled -or $passwordResetEnabled
if ($emailVerificationEnabled -ne $passwordResetEnabled) {
    $failures.Add("Email verification and password reset flags must be enabled or disabled together.")
}

if ($recoveryEnabled -and -not $smtpConfigured) {
    $failures.Add("Account recovery/email verification is enabled but SMTP is not fully configured.")
}
if (-not $AllowRecoveryOff) {
    if (-not $smtpConfigured) {
        $failures.Add("SMTP is not fully configured.")
    }
    if (-not $recoveryEnabled) {
        $failures.Add("Recovery mode is disabled. Configure SMTP or re-run with -AllowRecoveryOff if intentional.")
    }
} elseif (-not $smtpConfigured) {
    $warnings.Add("SMTP is not configured; recovery mode is intentionally off.")
}

$flutterwaveSecret = Get-SecretValue $secrets "OPERATOR_UI_FLUTTERWAVE_SECRET_KEY"
$flutterwaveWebhookHash = Get-SecretValue $secrets "OPERATOR_UI_FLUTTERWAVE_WEBHOOK_HASH"
$flutterwaveReady = (Is-NonEmpty $flutterwaveSecret) -and (Is-NonEmpty $flutterwaveWebhookHash)
if ((Is-NonEmpty $flutterwaveSecret) -xor (Is-NonEmpty $flutterwaveWebhookHash)) {
    $failures.Add("Flutterwave config is partial; both secret key and webhook hash are required.")
}
if (-not $AllowBillingOff) {
    if (-not $flutterwaveReady) {
        $failures.Add("Flutterwave is not fully configured.")
    }
} elseif (-not $flutterwaveReady) {
    $warnings.Add("Flutterwave is not configured; billing remains off.")
}

$metricsBearer = Get-SecretValue $secrets "REVERSE_PROXY_METRICS_BEARER_TOKEN"
if (-not (Is-NonEmpty $metricsBearer)) {
    $warnings.Add("REVERSE_PROXY_METRICS_BEARER_TOKEN is not configured.")
}

$sandboxRpc = Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_SANDBOX_SOLANA_RPC_URL" $config $secrets
if (-not ($sandboxRpc -match "devnet")) {
    $warnings.Add("Sandbox Playground RPC is not pointing to devnet.")
}

Write-Host "Running platform health probe..."
& pwsh -File (Join-Path $PSScriptRoot "check_platform_health.ps1") -Namespace $Namespace
if ($LASTEXITCODE -ne 0) {
    $failures.Add("check_platform_health.ps1 failed.")
}

Write-Host ""
Write-Host "=== Production Readiness Summary ==="
[PSCustomObject]@{
    PublicHost                 = $publicHost
    PublicBaseUrl              = $publicBaseUrl
    TlsSecretPresent           = $tlsSecretExists
    ExecutionPolicy            = $executionPolicy
    DurableMeteringRequired    = (Get-DeploymentEnvValue $operatorUiBackendDeployment "OPERATOR_UI_REQUIRE_DURABLE_METERING" $config $secrets)
    WorkerSolanaRpcUrls        = ($workerSolanaRpcList -join ",")
    ProductionSolanaRpcUrls    = ($productionWorkspaceRpcList -join ",")
    SmtpConfigured             = $smtpConfigured
    RecoveryEnabled            = $recoveryEnabled
    FlutterwaveReady           = $flutterwaveReady
    CallbackAllowedHosts       = $callbackAllowedHosts
    LocalImagesPresent         = ($localImages.Count -gt 0)
} | Format-List

if ($warnings.Count -gt 0) {
    Write-Host ""
    Write-Host "Warnings:"
    foreach ($warning in $warnings) {
        Write-Host " - $warning"
    }
}

if ($failures.Count -gt 0) {
    Write-Host ""
    Write-Host "Production readiness failed:"
    foreach ($failure in $failures) {
        Write-Host " - $failure"
    }
    exit 2
}

Write-Host "Production readiness passed."
