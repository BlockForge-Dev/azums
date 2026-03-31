param(
    [string]$Namespace = "azums",
    [string]$ImageNamespace = "ghcr.io/blockforge-dev/azums",
    [string]$Tag = "main",
    [switch]$BuildLocalImages,
    [switch]$PushLocalImages,
    [switch]$VerifyBilling,
    [string]$BillingVerifyBaseUrl = "http://127.0.0.1:18083"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($BuildLocalImages) {
    Write-Host "Building local images..."
    & pwsh -File deployments/docker/build-images.ps1 -ImageNamespace $ImageNamespace -Tag $Tag
}

if ($PushLocalImages) {
    $map = @{
        "azums/ingress_api:local"      = "$ImageNamespace/ingress_api:$Tag"
        "azums/status_api:local"       = "$ImageNamespace/status_api:$Tag"
        "azums/execution_worker:local" = "$ImageNamespace/execution_worker:$Tag"
        "azums/operator_ui:local"      = "$ImageNamespace/operator_ui:$Tag"
        "azums/operator_ui_next:local" = "$ImageNamespace/operator_ui_next:$Tag"
        "azums/reverse_proxy:local"    = "$ImageNamespace/reverse_proxy:$Tag"
    }
    foreach ($source in $map.Keys) {
        $target = $map[$source]
        Write-Host "Tag + push: $source -> $target"
        & docker tag $source $target
        & docker push $target
    }
}

$images = @{
    "ingress-api"        = "$ImageNamespace/ingress_api:$Tag"
    "status-api"         = "$ImageNamespace/status_api:$Tag"
    "execution-worker"   = "$ImageNamespace/execution_worker:$Tag"
    "execution-callback-worker" = "$ImageNamespace/execution_worker:$Tag"
    "operator-ui-backend" = "$ImageNamespace/operator_ui:$Tag"
    "operator-ui"        = "$ImageNamespace/operator_ui_next:$Tag"
    "reverse-proxy"      = "$ImageNamespace/reverse_proxy:$Tag"
}

foreach ($deployment in $images.Keys) {
    $image = $images[$deployment]
    Write-Host "Setting image: $deployment -> $image"
    & kubectl -n $Namespace set image "deploy/$deployment" "*=$image"
}

foreach ($deployment in $images.Keys) {
    Write-Host "Waiting for rollout: $deployment"
    & kubectl -n $Namespace rollout status "deploy/$deployment" --timeout=240s
}

if ($VerifyBilling) {
    Write-Host "Running billing verification..."
    & pwsh -File scripts/verify_billing_endpoints.ps1 -OperatorUiBaseUrl $BillingVerifyBaseUrl
}

Write-Host "Redeploy complete."
