param(
    [string]$BaseUrl = $(if ($env:BASE_URL) { $env:BASE_URL } else { "http://127.0.0.2:8000" }),
    [string]$Namespace = "azums",
    [ValidateSet("auto", "k8s", "compose")]
    [string]$Runtime = "auto",
    [string]$ComposeProject = "azums-proof",
    [string]$ComposeFile = $(Join-Path $PSScriptRoot "..\\deployments\\docker\\docker-compose.images.yml"),
    [string]$OutputDirectory = $(Join-Path $env:TEMP "azums-crash-injection"),
    [string]$IngressToken = $(if ($env:INGRESS_TOKEN) { $env:INGRESS_TOKEN } else { "dev-ingress-token" }),
    [string]$StatusToken = $(if ($env:STATUS_TOKEN) { $env:STATUS_TOKEN } else { "dev-status-token" }),
    [string]$IngressPrincipalId = $(if ($env:INGRESS_PRINCIPAL_ID) { $env:INGRESS_PRINCIPAL_ID } else { "ingress-service" }),
    [string]$StatusPrincipalId = $(if ($env:STATUS_PRINCIPAL_ID) { $env:STATUS_PRINCIPAL_ID } else { "demo-operator" }),
    [string]$StatusPrincipalRole = $(if ($env:STATUS_PRINCIPAL_ROLE) { $env:STATUS_PRINCIPAL_ROLE } else { "admin" }),
    [string]$ToAddr = $(if ($env:TO_WALLET) { $env:TO_WALLET } else { "GK8jAw6oibNGWT7WRwh2PCKSTb1XGQSiuPZdCaWRpqRC" }),
    [switch]$KeepArtifacts
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$benchmarkProfile = [ordered]@{
    EXECUTION_RETRY_MAX_ATTEMPTS        = "3"
    EXECUTION_RETRY_BASE_DELAY_MS       = "250"
    EXECUTION_RETRY_MAX_DELAY_MS        = "1500"
    EXECUTION_RETRY_JITTER_PERCENT      = "0"
    EXECUTION_QUEUE_JOB_MAX_ATTEMPTS    = "4"
    EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS = "1"
    EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS  = "3"
}

$composeManagedServices = @(
    "ingress_api",
    "status_api",
    "execution_worker",
    "execution_worker_replica",
    "execution_callback_worker",
    "reverse_proxy"
)

function Get-PortFromBaseUrl([string]$Url) {
    $uri = [Uri]$Url
    if (-not $uri.IsDefaultPort) {
        return $uri.Port
    }
    if ($uri.Scheme -eq "https") {
        return 443
    }
    return 80
}

function Get-ComposeServiceContainer {
    param([Parameter(Mandatory = $true)][string]$Service)

    $json = docker ps --filter "label=com.docker.compose.project=$ComposeProject" --filter "label=com.docker.compose.service=$Service" --format "{{json .}}" 2>$null
    if ($LASTEXITCODE -ne 0) {
        return $null
    }
    $first = @($json | Select-Object -First 1)
    if ($first.Count -eq 0 -or [string]::IsNullOrWhiteSpace([string]$first[0])) {
        return $null
    }
    $record = $first[0] | ConvertFrom-Json
    $ports = @()
    foreach ($part in ([string]$record.Ports).Split(',')) {
        $trimmed = $part.Trim()
        if ($trimmed -match '(?<published>\d+)->(?<target>\d+)/tcp') {
            $ports += [pscustomobject]@{
                PublishedPort = [int]$Matches.published
                TargetPort    = [int]$Matches.target
            }
        }
    }
    [pscustomobject]@{
        Id     = [string]$record.ID
        Names  = [string]$record.Names
        Ports  = $ports
        Status = [string]$record.Status
    }
}

function Resolve-RuntimeTarget {
    if ($Runtime -ne "auto") {
        return $Runtime
    }

    $container = Get-ComposeServiceContainer -Service "reverse_proxy"
    if ($null -ne $container) {
        $basePort = Get-PortFromBaseUrl $BaseUrl
        foreach ($publishedPort in @($container.Ports)) {
            if ($publishedPort.PublishedPort -eq $basePort -and $publishedPort.TargetPort -eq 8000) {
                return "compose"
            }
        }
    }

    return "k8s"
}

function New-CrashTenant {
    param([Parameter(Mandatory = $true)][string]$Prefix)
    "tenant_ws_crash_{0}_{1}" -f $Prefix, ([guid]::NewGuid().ToString("N").Substring(0, 8))
}

function Invoke-HealthCheck {
    $args = @(
        "-File", (Join-Path $PSScriptRoot "check_platform_health.ps1"),
        "-Runtime", $resolvedRuntime,
        "-BaseUrl", $BaseUrl,
        "-ComposeProject", $ComposeProject
    )
    if ($resolvedRuntime -eq "k8s") {
        $args += @("-Namespace", $Namespace)
    }
    & pwsh @args 2>&1 | Out-String
}

function Start-BenchmarkBackground {
    param(
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$ScenarioName,
        [Parameter(Mandatory = $true)][string]$Scenario,
        [Parameter(Mandatory = $true)][string]$TenantId,
        [Parameter(Mandatory = $true)][int]$RequestCount,
        [Parameter(Mandatory = $true)][int]$SubmitConcurrency,
        [Parameter(Mandatory = $true)][int]$TerminalTimeoutSec,
        [Parameter(Mandatory = $true)][string]$OutputJsonPath
    )

    $jobScript = {
        param(
            $RepoRoot,
            $BaseUrl,
            $Scenario,
            $TenantId,
            $RequestCount,
            $SubmitConcurrency,
            $TerminalTimeoutSec,
            $OutputJsonPath,
            $Namespace,
            $Runtime,
            $ComposeProject,
            $IngressToken,
            $StatusToken,
            $IngressPrincipalId,
            $StatusPrincipalId,
            $StatusPrincipalRole,
            $ToAddr
        )

        Set-Location $RepoRoot
        $benchmarkArgs = @(
            "-File", "scripts/benchmark_platform.ps1",
            "-BaseUrl", $BaseUrl,
            "-TenantId", $TenantId,
            "-Scenario", $Scenario,
            "-RequestCount", $RequestCount,
            "-SubmitConcurrency", $SubmitConcurrency,
            "-TerminalTimeoutSec", $TerminalTimeoutSec,
            "-Runtime", $Runtime,
            "-ComposeProject", $ComposeProject,
            "-IngressToken", $IngressToken,
            "-StatusToken", $StatusToken,
            "-IngressPrincipalId", $IngressPrincipalId,
            "-StatusPrincipalId", $StatusPrincipalId,
            "-StatusPrincipalRole", $StatusPrincipalRole,
            "-ToAddr", $ToAddr,
            "-OutputJsonPath", $OutputJsonPath
        )
        if (-not [string]::IsNullOrWhiteSpace($Namespace)) {
            $benchmarkArgs += @("-Namespace", $Namespace)
        }
        & pwsh @benchmarkArgs
        if ($LASTEXITCODE -ne 0) {
            exit $LASTEXITCODE
        }
    }

    Start-Job -Name $ScenarioName -ScriptBlock $jobScript -ArgumentList @(
        $RepoRoot,
        $BaseUrl,
        $Scenario,
        $TenantId,
        $RequestCount,
        $SubmitConcurrency,
        $TerminalTimeoutSec,
        $OutputJsonPath,
        $Namespace,
        $resolvedRuntime,
        $ComposeProject,
        $IngressToken,
        $StatusToken,
        $IngressPrincipalId,
        $StatusPrincipalId,
        $StatusPrincipalRole,
        $ToAddr
    )
}

function Invoke-ComposeUp {
    param([string[]]$Services)

    $args = @("-p", $ComposeProject, "-f", $ComposeFile, "up", "-d", "--force-recreate")
    $args += $Services

    $output = & docker compose @args 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        if (-not [string]::IsNullOrWhiteSpace($output)) {
            Write-Host $output.Trim()
        }
        throw "docker compose up failed"
    }
}

function Invoke-ComposeKill {
    param(
        [Parameter(Mandatory = $true)][string]$Service,
        [Parameter(Mandatory = $true)][string]$Signal
    )

    $output = & docker compose -p $ComposeProject -f $ComposeFile kill -s $Signal $Service 2>&1 | Out-String
    if ($LASTEXITCODE -eq 0) {
        return $true
    }
    if (-not [string]::IsNullOrWhiteSpace($output)) {
        Write-Host $output.Trim()
    }
    return $false
}

function Invoke-DockerRmForce {
    param([Parameter(Mandatory = $true)][string]$ContainerId)

    $output = & docker rm -f $ContainerId 2>&1 | Out-String
    if ($LASTEXITCODE -eq 0) {
        return
    }
    if (-not [string]::IsNullOrWhiteSpace($output)) {
        Write-Host $output.Trim()
    }
    throw "failed to remove container $ContainerId"
}

function Wait-ForComposeService {
    param(
        [Parameter(Mandatory = $true)][string]$Service,
        [int]$TimeoutSec = 180
    )

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $container = Get-ComposeServiceContainer -Service $Service
        if ($null -ne $container -and $container.Status -match '^Up\b') {
            return
        }
        Start-Sleep -Seconds 2
    }
    throw "timed out waiting for compose service $Service"
}

function Wait-ForK8sSelector {
    param([string]$Selector, [int]$TimeoutSec = 300)

    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $pods = kubectl -n $Namespace get pods -l $Selector -o json | ConvertFrom-Json
        foreach ($item in @($pods.items)) {
            if ([string]$item.status.phase -ne "Running") {
                continue
            }
            foreach ($containerStatus in @($item.status.containerStatuses)) {
                if ($containerStatus.ready) {
                    return
                }
            }
        }
        Start-Sleep -Seconds 2
    }

    throw "timed out waiting for selector $Selector"
}

function Apply-BenchmarkRetryProfile {
    if ($resolvedRuntime -eq "compose") {
        foreach ($entry in $benchmarkProfile.GetEnumerator()) {
            Set-Item -Path "Env:$($entry.Key)" -Value $entry.Value
        }
        Invoke-ComposeUp -Services $composeManagedServices
        Wait-ForComposeService -Service "ingress_api"
        Wait-ForComposeService -Service "status_api"
        Wait-ForComposeService -Service "execution_worker"
        Wait-ForComposeService -Service "execution_worker_replica"
        Wait-ForComposeService -Service "execution_callback_worker"
        Wait-ForComposeService -Service "reverse_proxy"
        return
    }

    $retryVars = @(
        "EXECUTION_RETRY_MAX_ATTEMPTS=$($benchmarkProfile.EXECUTION_RETRY_MAX_ATTEMPTS)",
        "EXECUTION_RETRY_BASE_DELAY_MS=$($benchmarkProfile.EXECUTION_RETRY_BASE_DELAY_MS)",
        "EXECUTION_RETRY_MAX_DELAY_MS=$($benchmarkProfile.EXECUTION_RETRY_MAX_DELAY_MS)",
        "EXECUTION_RETRY_JITTER_PERCENT=$($benchmarkProfile.EXECUTION_RETRY_JITTER_PERCENT)"
    )
    $queueVars = @(
        "EXECUTION_QUEUE_JOB_MAX_ATTEMPTS=$($benchmarkProfile.EXECUTION_QUEUE_JOB_MAX_ATTEMPTS)",
        "EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS=$($benchmarkProfile.EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS)",
        "EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS=$($benchmarkProfile.EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS)"
    )

    & kubectl -n $Namespace set env deploy/ingress-api @retryVars
    & kubectl -n $Namespace set env deploy/status-api @retryVars
    & kubectl -n $Namespace set env deploy/execution-worker @retryVars @queueVars
    if ($LASTEXITCODE -ne 0) {
        throw "failed to apply benchmark retry profile to k8s"
    }
    & kubectl -n $Namespace rollout status deploy/ingress-api --timeout=300s
    & kubectl -n $Namespace rollout status deploy/status-api --timeout=300s
    & kubectl -n $Namespace rollout status deploy/execution-worker --timeout=300s
}

function Restore-BenchmarkRetryProfile {
    if ($resolvedRuntime -eq "compose") {
        foreach ($name in @($benchmarkProfile.Keys)) {
            Remove-Item -Path "Env:$name" -ErrorAction SilentlyContinue
        }
        Invoke-ComposeUp -Services $composeManagedServices
        return
    }

    & kubectl -n $Namespace set env deploy/ingress-api EXECUTION_RETRY_MAX_ATTEMPTS- EXECUTION_RETRY_BASE_DELAY_MS- EXECUTION_RETRY_MAX_DELAY_MS- EXECUTION_RETRY_JITTER_PERCENT-
    & kubectl -n $Namespace set env deploy/status-api EXECUTION_RETRY_MAX_ATTEMPTS- EXECUTION_RETRY_BASE_DELAY_MS- EXECUTION_RETRY_MAX_DELAY_MS- EXECUTION_RETRY_JITTER_PERCENT-
    & kubectl -n $Namespace set env deploy/execution-worker EXECUTION_RETRY_MAX_ATTEMPTS- EXECUTION_RETRY_BASE_DELAY_MS- EXECUTION_RETRY_MAX_DELAY_MS- EXECUTION_RETRY_JITTER_PERCENT- EXECUTION_QUEUE_JOB_MAX_ATTEMPTS- EXECUTION_QUEUE_RETRY_BASE_DELAY_SECS- EXECUTION_QUEUE_RETRY_MAX_DELAY_SECS-
    & kubectl -n $Namespace rollout status deploy/ingress-api --timeout=300s
    & kubectl -n $Namespace rollout status deploy/status-api --timeout=300s
    & kubectl -n $Namespace rollout status deploy/execution-worker --timeout=300s
}

function Invoke-WorkerCrash {
    if ($resolvedRuntime -eq "compose") {
        if (-not (Invoke-ComposeKill -Service "execution_worker" -Signal "SIGKILL")) {
            $container = Get-ComposeServiceContainer -Service "execution_worker"
            if ($null -eq $container) {
                throw "failed to SIGKILL execution_worker"
            }
            Invoke-DockerRmForce -ContainerId $container.Id
        }
        Invoke-ComposeUp -Services @("execution_worker")
        Wait-ForComposeService -Service "execution_worker"
        return
    }

    $pod = (& kubectl -n $Namespace get pod -l app=execution-worker -o jsonpath='{.items[0].metadata.name}').Trim()
    if ([string]::IsNullOrWhiteSpace($pod)) {
        throw "execution-worker pod not found"
    }
    & kubectl -n $Namespace delete ("pod/{0}" -f $pod) --grace-period=0 --force
    Wait-ForK8sSelector -Selector "app=execution-worker"
}

function Invoke-PostgresCrash {
    if ($resolvedRuntime -eq "compose") {
        if (-not (Invoke-ComposeKill -Service "postgres" -Signal "SIGKILL")) {
            $container = Get-ComposeServiceContainer -Service "postgres"
            if ($null -eq $container) {
                throw "failed to SIGKILL postgres"
            }
            Invoke-DockerRmForce -ContainerId $container.Id
        }
        Invoke-ComposeUp -Services @("postgres")
        Wait-ForComposeService -Service "postgres"
        return
    }

    & kubectl -n $Namespace delete pod/postgres-0 --grace-period=0 --force
    Wait-ForK8sSelector -Selector "app=postgres"
}

function Invoke-IngressCrash {
    if ($resolvedRuntime -eq "compose") {
        if (-not (Invoke-ComposeKill -Service "ingress_api" -Signal "SIGKILL")) {
            throw "failed to SIGKILL ingress_api"
        }
        Invoke-ComposeUp -Services @("ingress_api")
        Wait-ForComposeService -Service "ingress_api"
        return
    }

    & kubectl -n $Namespace rollout restart deploy/ingress-api
    & kubectl -n $Namespace rollout status deploy/ingress-api --timeout=300s
    if ($LASTEXITCODE -ne 0) {
        throw "ingress-api rollout did not complete"
    }
}

function Invoke-Scenario {
    param(
        [Parameter(Mandatory = $true)][hashtable]$Spec,
        [Parameter(Mandatory = $true)][string]$RepoRoot
    )

    $tenantId = New-CrashTenant -Prefix $Spec.Name
    $outputJson = Join-Path $OutputDirectory ("{0}.json" -f $Spec.Name)
    $job = Start-BenchmarkBackground -RepoRoot $RepoRoot -ScenarioName $Spec.Name -Scenario $Spec.BenchmarkScenario -TenantId $tenantId -RequestCount $Spec.RequestCount -SubmitConcurrency $Spec.SubmitConcurrency -TerminalTimeoutSec $Spec.TerminalTimeoutSec -OutputJsonPath $outputJson

    Start-Sleep -Seconds $Spec.InjectAfterSec

    switch ($Spec.Injector) {
        "worker" { Invoke-WorkerCrash }
        "postgres" { Invoke-PostgresCrash }
        "ingress" { Invoke-IngressCrash }
        default { throw "unknown injector $($Spec.Injector)" }
    }

    $waitTimeoutSec = $Spec.TerminalTimeoutSec + 120
    $completedJob = Wait-Job -Job $job -Timeout $waitTimeoutSec
    if ($null -eq $completedJob) {
        Stop-Job -Job $job | Out-Null
        $jobOutputText = Receive-Job -Job $job -Keep | Out-String
        Remove-Job -Job $job -Force | Out-Null
        if (-not [string]::IsNullOrWhiteSpace($jobOutputText)) {
            Write-Host $jobOutputText.Trim()
        }
        throw "benchmark job for $($Spec.Name) did not finish within ${waitTimeoutSec}s"
    }

    $jobOutput = Receive-Job -Job $job -Keep | Out-String
    $jobState = [string]$job.State
    Remove-Job -Job $job -Force | Out-Null

    if (-not (Test-Path $outputJson)) {
        throw "benchmark output was not written for $($Spec.Name)"
    }

    $result = Get-Content $outputJson -Raw | ConvertFrom-Json
    $postHealthOutput = Invoke-HealthCheck
    $postHealthOk = ($LASTEXITCODE -eq 0)
    $summary = $result.Summary
    $pass = ($summary.AcceptedCount -eq $Spec.RequestCount -and $summary.PendingCount -eq 0 -and $postHealthOk)

    [pscustomobject]@{
        ScenarioName                 = $Spec.Name
        Runtime                      = $resolvedRuntime
        Injector                     = $Spec.Injector
        BenchmarkScenario            = $Spec.BenchmarkScenario
        TenantId                     = $tenantId
        AcceptedCount                = [int]$summary.AcceptedCount
        UniqueAcceptedExecutionCount = [int]$summary.UniqueAcceptedExecutionCount
        RejectedCount                = [int]$summary.RejectedCount
        TerminalCount                = [int]$summary.TerminalCount
        UniqueTerminalExecutionCount = [int]$summary.UniqueTerminalExecutionCount
        PendingCount                 = [int]$summary.PendingCount
        WallClockMs                  = [double]$summary.WallClockMs
        JobState                     = $jobState
        Pass                         = $pass
        BenchmarkJsonPath            = $outputJson
        PostHealthOk                 = $postHealthOk
        PostHealthSummary            = $postHealthOutput.Trim()
        FinalStateCounts             = @($result.FinalStateCounts)
        SubmitStatusCounts           = @($result.SubmitStatusCounts)
        JobOutput                    = $jobOutput.Trim()
    }
}

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$resolvedRuntime = Resolve-RuntimeTarget

$scenarioSpecs = @(
    @{
        Name = "worker_during_execution"
        Injector = "worker"
        BenchmarkScenario = "synthetic_success"
        RequestCount = 4
        SubmitConcurrency = 2
        TerminalTimeoutSec = 180
        InjectAfterSec = 4
    },
    @{
        Name = "postgres_during_processing"
        Injector = "postgres"
        BenchmarkScenario = "retry_then_success"
        RequestCount = 2
        SubmitConcurrency = 1
        TerminalTimeoutSec = 180
        InjectAfterSec = 4
    },
    @{
        Name = "ingress_during_intake"
        Injector = "ingress"
        BenchmarkScenario = "synthetic_success"
        RequestCount = 12
        SubmitConcurrency = 6
        TerminalTimeoutSec = 180
        InjectAfterSec = 6
    }
)

$results = @()
try {
    Apply-BenchmarkRetryProfile

    foreach ($scenarioSpec in $scenarioSpecs) {
        Write-Host ("Running crash scenario: {0} ({1})" -f $scenarioSpec.Name, $resolvedRuntime)
        $scenarioResult = Invoke-Scenario -Spec $scenarioSpec -RepoRoot $repoRoot
        if ($null -ne $scenarioResult -and $scenarioResult.PSObject.Properties["Pass"]) {
            $results += $scenarioResult
        }
    }
}
finally {
    Restore-BenchmarkRetryProfile
}

Write-Host ""
Write-Host "=== Crash Injection Summary ==="
$results |
    Select-Object ScenarioName, Runtime, Injector, BenchmarkScenario, AcceptedCount, RejectedCount, UniqueAcceptedExecutionCount, UniqueTerminalExecutionCount, PendingCount, PostHealthOk, Pass, BenchmarkJsonPath |
    Format-Table -AutoSize

$failed = @($results | Where-Object { $_.PSObject.Properties["Pass"] -and -not $_.Pass })
if ($failed.Count -gt 0) {
    Write-Host ""
    Write-Host "Crash scenarios with unresolved pending work, rejected requests, or failed health checks:"
    $failed | Select-Object ScenarioName, AcceptedCount, RejectedCount, PendingCount, PostHealthOk, BenchmarkJsonPath | Format-Table -AutoSize
    if (-not $KeepArtifacts) {
        Write-Host ("Artifacts kept in {0} for debugging." -f $OutputDirectory)
    }
    throw "one or more crash-injection scenarios failed"
}

Write-Host ""
Write-Host "Crash-injection suite passed."
