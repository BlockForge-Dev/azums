param(
    [string]$ImageNamespace = "azums",
    [string]$Tag = "local"
)

$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Push-Location $root
try {
    $images = @(
        @{ manifest = "apps/ingress_api/Cargo.toml";     bin = "ingress_api";           tag = "$ImageNamespace/ingress_api:$Tag";      include_signer = $false },
        @{ manifest = "crates/status_api/Cargo.toml";    bin = "status_api";            tag = "$ImageNamespace/status_api:$Tag";       include_signer = $false },
        @{ manifest = "apps/admin_cli/Cargo.toml";       bin = "execution_core_worker"; tag = "$ImageNamespace/execution_worker:$Tag"; include_signer = $true  },
        @{ manifest = "apps/operator_ui/Cargo.toml";     bin = "operator_ui";           tag = "$ImageNamespace/operator_ui:$Tag";      include_signer = $false },
        @{ manifest = "crates/reverse-proxy/Cargo.toml"; bin = "reverse_proxy";         tag = "$ImageNamespace/reverse_proxy:$Tag";    include_signer = $false }
    )

    foreach ($img in $images) {
        Write-Host "Building $($img.tag) ..."
        docker build `
            -f deployments/docker/Dockerfile `
            --build-arg APP_MANIFEST="$($img.manifest)" `
            --build-arg BIN_NAME="$($img.bin)" `
            --build-arg INCLUDE_SOLANA_SIGNER="$($img.include_signer.ToString().ToLowerInvariant())" `
            -t "$($img.tag)" `
            .
        if ($LASTEXITCODE -ne 0) {
            throw "docker build failed for $($img.tag) (manifest=$($img.manifest), bin=$($img.bin))"
        }
    }

    $nextTag = "$ImageNamespace/operator_ui_next:$Tag"
    Write-Host "Building $nextTag ..."
    docker build `
        -f deployments/docker/Dockerfile.next `
        -t $nextTag `
        .
    if ($LASTEXITCODE -ne 0) {
        throw "docker build failed for $nextTag"
    }
}
finally {
    Pop-Location
}
