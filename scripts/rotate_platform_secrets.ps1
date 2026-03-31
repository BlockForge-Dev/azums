param(
    [string]$Namespace = "azums",
    [string]$SecretName = "azums-platform-secrets",
    [string]$Tenant = "tenant_demo",
    [string]$DbHost = "postgres",
    [int]$DbPort = 5432,
    [string]$DbName = "azums",
    [string]$DbUser = "app",
    [string]$DbPassword = "",
    [string]$FlutterwaveSecretKey = "",
    [string]$FlutterwaveWebhookHash = "",
    [string]$SmtpUsername = "",
    [string]$SmtpPassword = "",
    [string]$CallbackDeliveryToken = "",
    [string]$CallbackSigningSecret = "",
    [string]$MetricsBearerToken = "",
    [bool]$RotateDatabasePassword = $true,
    [switch]$Apply
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function New-Secret([int]$Bytes = 48) {
    $buffer = New-Object byte[] $Bytes
    [System.Security.Cryptography.RandomNumberGenerator]::Fill($buffer)
    ([Convert]::ToBase64String($buffer)).TrimEnd("=").Replace("+", "-").Replace("/", "_")
}

function Mask([string]$Value) {
    if ([string]::IsNullOrWhiteSpace($Value)) { return "<empty>" }
    if ($Value.Length -le 8) { return ("*" * $Value.Length) }
    return "{0}...{1}" -f $Value.Substring(0, 4), $Value.Substring($Value.Length - 4)
}

if ([string]::IsNullOrWhiteSpace($DbPassword)) {
    $DbPassword = New-Secret 32
}

$statusBearer = New-Secret 48
$ingressBearer = New-Secret 48
$ingressApiKey = "azk_{0}" -f (New-Secret 32)
$tenantApiKey = "azk_{0}" -f (New-Secret 24)
$webhookSecret = New-Secret 32
$statusTenantToken = New-Secret 32

$secretPatch = @{
    stringData = @{
        POSTGRES_PASSWORD = $DbPassword
        DATABASE_URL = "postgres://${DbUser}:$DbPassword@$DbHost`:$DbPort/$DbName"

        INGRESS_BEARER_TOKEN = $ingressBearer
        STATUS_API_BEARER_TOKEN = $statusBearer
        OPERATOR_UI_STATUS_BEARER_TOKEN = $statusBearer
        OPERATOR_UI_INGRESS_BEARER_TOKEN = $ingressBearer

        INGRESS_API_KEY = $ingressApiKey
        INGRESS_TENANT_API_KEYS = "${Tenant}:$tenantApiKey"
        INGRESS_WEBHOOK_SIGNATURE_SECRETS = "${Tenant}:$webhookSecret"
        STATUS_API_TENANT_TOKENS = "${Tenant}:$statusTenantToken"
    }
}

if (-not [string]::IsNullOrWhiteSpace($FlutterwaveSecretKey)) {
    $secretPatch.stringData.OPERATOR_UI_FLUTTERWAVE_SECRET_KEY = $FlutterwaveSecretKey
}
if (-not [string]::IsNullOrWhiteSpace($FlutterwaveWebhookHash)) {
    $secretPatch.stringData.OPERATOR_UI_FLUTTERWAVE_WEBHOOK_HASH = $FlutterwaveWebhookHash
}
if (-not [string]::IsNullOrWhiteSpace($SmtpUsername)) {
    $secretPatch.stringData.OPERATOR_UI_SMTP_USERNAME = $SmtpUsername
}
if (-not [string]::IsNullOrWhiteSpace($SmtpPassword)) {
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

$patchJson = $secretPatch | ConvertTo-Json -Compress

Write-Host "Namespace               : $Namespace"
Write-Host "Secret                  : $SecretName"
Write-Host "Tenant                  : $Tenant"
Write-Host "Rotate DB password      : $RotateDatabasePassword"
Write-Host "New DATABASE_URL        : postgres://${DbUser}:$(Mask $DbPassword)@$DbHost`:$DbPort/$DbName"
Write-Host "INGRESS_BEARER_TOKEN    : $(Mask $ingressBearer)"
Write-Host "STATUS_API_BEARER_TOKEN : $(Mask $statusBearer)"
Write-Host "INGRESS_API_KEY         : $(Mask $ingressApiKey)"
Write-Host "TENANT API KEY          : $(Mask $tenantApiKey)"
Write-Host "WEBHOOK SECRET          : $(Mask $webhookSecret)"
Write-Host "STATUS TENANT TOKEN     : $(Mask $statusTenantToken)"
Write-Host "Flutterwave key supplied: $(-not [string]::IsNullOrWhiteSpace($FlutterwaveSecretKey))"
Write-Host "SMTP password supplied  : $(-not [string]::IsNullOrWhiteSpace($SmtpPassword))"
Write-Host "Callback token supplied : $(-not [string]::IsNullOrWhiteSpace($CallbackDeliveryToken))"
Write-Host "Callback secret supplied: $(-not [string]::IsNullOrWhiteSpace($CallbackSigningSecret))"
Write-Host "Metrics token supplied  : $(-not [string]::IsNullOrWhiteSpace($MetricsBearerToken))"

if (-not $Apply) {
    Write-Warning "Dry run only. Re-run with -Apply to execute rotation."
    return
}

if ($RotateDatabasePassword) {
    Write-Host "Rotating Postgres user password first..."
    $escapedPassword = $DbPassword.Replace("'", "''")
    $alter = "ALTER USER ${DbUser} WITH PASSWORD '$escapedPassword';"
    & kubectl -n $Namespace exec statefulset/postgres -- psql -U $DbUser -d $DbName -v ON_ERROR_STOP=1 -c $alter
    if ($LASTEXITCODE -ne 0) {
        throw "failed to rotate postgres password for user $DbUser"
    }
}

Write-Host "Patching Kubernetes secret..."
& kubectl -n $Namespace patch secret $SecretName --type merge -p $patchJson
if ($LASTEXITCODE -ne 0) {
    throw "failed to patch secret $SecretName"
}

Write-Host "Restarting workloads..."
& kubectl -n $Namespace rollout restart deploy/ingress-api deploy/status-api deploy/execution-worker deploy/operator-ui-backend deploy/reverse-proxy
if ($LASTEXITCODE -ne 0) {
    throw "failed to restart deployments"
}

foreach ($deployment in @("ingress-api", "status-api", "execution-worker", "operator-ui-backend", "reverse-proxy")) {
    Write-Host "Waiting for rollout: $deployment"
    & kubectl -n $Namespace rollout status "deploy/$deployment" --timeout=180s
    if ($LASTEXITCODE -ne 0) {
        throw "rollout failed for $deployment"
    }
}

Write-Host "Rotation complete."
