$ErrorActionPreference = "Stop"

$root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
Push-Location $root
try {
    $images = @(
        @{ manifest = "apps/ingress_api/Cargo.toml";    bin = "ingress_api";    tag = "azums/ingress_api:local" },
        @{ manifest = "crates/status_api/Cargo.toml";   bin = "status_api";     tag = "azums/status_api:local" },
        @{ manifest = "apps/admin_cli/Cargo.toml";      bin = "admin_cli";      tag = "azums/execution_worker:local" },
        @{ manifest = "apps/operator_ui/Cargo.toml";    bin = "operator_ui";    tag = "azums/operator_ui:local" },
        @{ manifest = "crates/reverse-proxy/Cargo.toml"; bin = "reverse_proxy"; tag = "azums/reverse_proxy:local" }
    )

    foreach ($img in $images) {
        Write-Host "Building $($img.tag) ..."
        docker build `
            -f deployments/docker/Dockerfile `
            --build-arg APP_MANIFEST="$($img.manifest)" `
            --build-arg BIN_NAME="$($img.bin)" `
            -t "$($img.tag)" `
            .
    }
}
finally {
    Pop-Location
}
