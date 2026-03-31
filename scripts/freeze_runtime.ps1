param(
    [string]$Namespace = "azums",
    [string]$ImageNamespace = "azums",
    [string]$Tag = "",
    [switch]$SkipBuild,
    [switch]$SkipRolloutWait
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Tag)) {
    $Tag = "freeze-{0}" -f (Get-Date -Format "yyyyMMdd-HHmmss")
}

$images = @(
    @{ Deployment = "ingress-api";         Container = "ingress-api";         Image = "$ImageNamespace/ingress_api:$Tag" },
    @{ Deployment = "status-api";          Container = "status-api";          Image = "$ImageNamespace/status_api:$Tag" },
    @{ Deployment = "execution-worker";    Container = "execution-worker";    Image = "$ImageNamespace/execution_worker:$Tag" },
    @{ Deployment = "execution-callback-worker"; Container = "execution-callback-worker"; Image = "$ImageNamespace/execution_worker:$Tag" },
    @{ Deployment = "operator-ui-backend"; Container = "operator-ui-backend"; Image = "$ImageNamespace/operator_ui:$Tag" },
    @{ Deployment = "operator-ui";         Container = "operator-ui";         Image = "$ImageNamespace/operator_ui_next:$Tag" },
    @{ Deployment = "reverse-proxy";       Container = "reverse-proxy";       Image = "$ImageNamespace/reverse_proxy:$Tag" }
)

if (-not $SkipBuild) {
    Write-Host "Building consistent runtime images with tag $Tag ..."
    & pwsh -File deployments/docker/build-images.ps1 -ImageNamespace $ImageNamespace -Tag $Tag
    if ($LASTEXITCODE -ne 0) {
        throw "image build failed"
    }
}

foreach ($spec in $images) {
    Write-Host "Deploying $($spec.Deployment) -> $($spec.Image)"
    & kubectl -n $Namespace set image "deploy/$($spec.Deployment)" "$($spec.Container)=$($spec.Image)"
    if ($LASTEXITCODE -ne 0) {
        throw "kubectl set image failed for $($spec.Deployment)"
    }
}

if (-not $SkipRolloutWait) {
    foreach ($spec in $images) {
        Write-Host "Waiting for rollout: $($spec.Deployment)"
        & kubectl -n $Namespace rollout status "deploy/$($spec.Deployment)" --timeout=300s
        if ($LASTEXITCODE -ne 0) {
            throw "rollout failed for $($spec.Deployment)"
        }
    }
}

$verification = foreach ($spec in $images) {
    $deployed = (& kubectl -n $Namespace get "deploy/$($spec.Deployment)" -o jsonpath="{.spec.template.spec.containers[?(@.name=='$($spec.Container)')].image}").Trim()
    [pscustomobject]@{
        Deployment    = $spec.Deployment
        Container     = $spec.Container
        ExpectedImage = $spec.Image
        DeployedImage = $deployed
        Match         = ($deployed -eq $spec.Image)
    }
}

$mismatches = @($verification | Where-Object { -not $_.Match })
if ($mismatches.Count -gt 0) {
    $mismatches | Format-Table -AutoSize | Out-String | Write-Host
    throw "runtime freeze verification failed: one or more deployments still reference different images"
}

Write-Host ""
Write-Host "=== Frozen Runtime Images ==="
$verification | Format-Table -AutoSize
Write-Host ""
Write-Host "Runtime freeze complete. All workloads reference tag $Tag."
